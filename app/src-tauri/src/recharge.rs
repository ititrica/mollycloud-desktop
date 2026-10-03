//! Standalone, non-modal payment windows host raw Wry views outside Tauri's IPC registration.
use crate::{api::SERVICE_ORIGIN, state::RuntimeState};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    sync::atomic::{AtomicU64, Ordering},
};
use tauri::{AppHandle, Emitter, Manager, State, Webview, Window, WindowEvent};
use wry::{WebView, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows};

#[cfg(test)]
const PURCHASE_URL: &str = "https://mollycloud.cn/purchase";
const CHECKOUT_PREFIX: &str = "molly-checkout-";
const POPUP_PREFIX: &str = "molly-payment-";
static POPUP_ID: AtomicU64 = AtomicU64::new(0);
#[derive(Default)]
pub struct RechargeState(pub tokio::sync::Mutex<()>);

struct Checkout {
    view: WebView,
    window: Window,
    owner: String,
    // InPrivate isolates website login from both the console and other apps.
    _profile: tempfile::TempDir,
}
struct Popup {
    label: String,
    view: WebView,
    window: Window,
}
thread_local! {
    static CHECKOUT: RefCell<Option<Checkout>> = const { RefCell::new(None) };
    static POPUPS: RefCell<Vec<Popup>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Serialize)]
struct PageStatus {
    owner: String,
    loading: bool,
    host: String,
    error: Option<String>,
}
fn status(app: &AppHandle, owner: &str, loading: bool, url: &str, error: Option<&str>) {
    let host = url::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_default();
    // Payment URLs can contain order tokens. Never forward their query/fragment or log them.
    let _ = app.emit_to(
        "console",
        "recharge-page-status",
        PageStatus {
            owner: owner.to_owned(),
            loading,
            host,
            error: error.map(str::to_owned),
        },
    );
}

pub(crate) fn allowed_navigation(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    match url.host() {
        Some(url::Host::Domain(host)) => {
            host.contains('.')
                && host != "localhost"
                && !host.ends_with(".localhost")
                && !host.ends_with(".local")
                && !host.ends_with(".internal")
        }
        _ => false,
    }
}

fn require_console(webview: &Webview) -> Result<(), String> {
    if webview.label() == "console" {
        Ok(())
    } else {
        Err("请从控制台打开充值页面".into())
    }
}

async fn on_ui<T: Send + 'static>(
    app: &AppHandle,
    operation: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(operation());
    })
    .map_err(|_| "无法访问充值窗口")?;
    rx.await.map_err(|_| "充值窗口任务已结束")?
}

// Only the real MollyCloud main frame receives the access token. No password,
// refresh token, API key, URL parameter, native IPC, or network override is injected.
fn session_script(token: &str, user: &Value, dark: bool, force: bool) -> String {
    let payload =
        json!({"token":token,"user":user,"origin":SERVICE_ORIGIN,"dark":dark,"force":force});
    format!(
        r#"(() => {{
      if (window.top !== window || location.origin !== {origin}) return;
      const p = {payload};
      const account = String(p.user.id);
      const missingSession = !localStorage.getItem('auth_token');
      if (p.force || localStorage.getItem('molly_desktop_account') !== account) {{
        localStorage.setItem('auth_token', p.token);
        localStorage.setItem('auth_user', JSON.stringify(p.user));
        localStorage.setItem('molly_desktop_account', account);
      }}
      localStorage.removeItem('refresh_token');
      localStorage.removeItem('token_expires_at');
      localStorage.setItem('theme', p.dark ? 'dark' : 'light');
      const apply = () => {{ document.documentElement.classList.toggle('dark', p.dark); document.documentElement.style.colorScheme = p.dark ? 'dark' : 'light'; }};
      if (document.documentElement) apply(); else document.addEventListener('DOMContentLoaded', apply, {{once:true}});
      // A mounted site may have cleared its in-memory auth after an expired request.
      // Reinitialize it after restoring access, without interrupting a valid checkout.
      if (p.force && document.readyState !== 'loading' && (missingSession || location.pathname === '/login')) {{
        if (location.pathname !== '/login') location.reload();
      }}
    }})();"#,
        origin = json!(SERVICE_ORIGIN)
    )
}

fn window_theme(dark: bool) -> tauri::Theme {
    if dark {
        tauri::Theme::Dark
    } else {
        tauri::Theme::Light
    }
}
fn theme(dark: bool) -> wry::Theme {
    if dark {
        wry::Theme::Dark
    } else {
        wry::Theme::Light
    }
}

fn make_popup(
    app: &AppHandle,
    owner: String,
    url: String,
    features: wry::NewWindowFeatures,
    script: String,
    dark: bool,
    visible: bool,
    navigation: fn(&str) -> bool,
) -> wry::NewWindowResponse {
    if !(url == "about:blank" || navigation(&url)) || POPUPS.with(|p| p.borrow().len() >= 4) {
        status(
            app,
            &owner,
            false,
            &url,
            Some("此支付跳转无法在客户端中打开，请使用页面提供的二维码支付。"),
        );
        return wry::NewWindowResponse::Deny;
    }
    let label = format!("{POPUP_PREFIX}{}", POPUP_ID.fetch_add(1, Ordering::Relaxed));
    let result = (|| -> Result<_, String> {
        let window = tauri::window::WindowBuilder::new(app, &label)
            .title("MollyCloud · 支付")
            .theme(Some(window_theme(dark)))
            .minimizable(false)
            .maximizable(false)
            .closable(true)
            .visible(visible)
            .skip_taskbar(!visible)
            .inner_size(920.0, 720.0)
            .min_inner_size(640.0, 480.0)
            .center()
            .build()
            .map_err(|_| "无法创建支付窗口")?;
        let handler_app = app.clone();
        let nav_owner = owner.clone();
        let child_owner = owner.clone();
        let child_app = app.clone();
        let child_script = script.clone();
        let close_window = window.clone();
        let title_window = window.clone();
        let view = WebViewBuilder::new()
            .with_environment(features.opener.environment)
            .with_incognito(true)
            .with_theme(theme(dark))
            .with_devtools(false)
            .with_initialization_script_for_main_only(script, true)
            // Route website close through Tauri so its native window registry is cleaned up.
            // This popup-local signal has no arguments, return data or other capability.
            .with_initialization_script_for_main_only(
                "window.close = () => window.ipc.postMessage('molly-payment-close');",
                true,
            )
            .with_ipc_handler(move |request| {
                if request.body() == "molly-payment-close" {
                    let _ = close_window.close();
                }
            })
            .with_on_page_load_handler(move |_, target| {
                if let Some(host) = url::Url::parse(&target)
                    .ok()
                    .and_then(|url| url.host_str().map(str::to_owned))
                {
                    let _ = title_window.set_title(&format!("MollyCloud · 支付 · {host}"));
                }
            })
            .with_navigation_handler(move |target| {
                let allowed = target == "about:blank" || navigation(&target);
                if !allowed {
                    status(
                        &handler_app,
                        &nav_owner,
                        false,
                        &target,
                        Some("不支持此支付跳转，请使用二维码支付。"),
                    );
                }
                allowed
            })
            .with_download_started_handler(|_, _| false)
            .with_new_window_req_handler(move |target, features| {
                make_popup(
                    &child_app,
                    child_owner.clone(),
                    target,
                    features,
                    child_script.clone(),
                    dark,
                    visible,
                    navigation,
                )
            })
            .build(&window)
            .map_err(|_| {
                let _ = window.close();
                "无法加载支付窗口"
            })?;
        let platform_view = view.webview();
        POPUPS.with(|p| {
            p.borrow_mut().push(Popup {
                label,
                view,
                window,
            })
        });
        Ok(platform_view)
    })();
    match result {
        Ok(webview) => wry::NewWindowResponse::Create { webview },
        Err(error) => {
            status(app, &owner, false, &url, Some(&error));
            wry::NewWindowResponse::Deny
        }
    }
}

fn build_checkout(
    handle: &AppHandle,
    owner: String,
    script: String,
    dark: bool,
    target: &str,
    navigation: fn(&str) -> bool,
    visible: bool,
) -> Result<Checkout, String> {
    let profile = tempfile::Builder::new()
        .prefix("molly-checkout-")
        .tempdir()
        .map_err(|_| "无法创建充值浏览会话")?;
    // No parent/owner relationship: payment must never disable console input or dragging.
    let label = format!(
        "{CHECKOUT_PREFIX}{}",
        POPUP_ID.fetch_add(1, Ordering::Relaxed)
    );
    let window = tauri::window::WindowBuilder::new(handle, &label)
        .title("MollyCloud · 支付")
        .theme(Some(window_theme(dark)))
        .inner_size(860.0, 680.0)
        .min_inner_size(420.0, 360.0)
        .minimizable(false)
        .maximizable(false)
        .closable(true)
        .visible(false)
        .skip_taskbar(!visible)
        .center()
        .build()
        .map_err(|_| "无法创建支付窗口")?;
    let mut context = wry::WebContext::new(Some(profile.path().to_path_buf()));
    let nav_app = handle.clone();
    let load_app = handle.clone();
    let popup_app = handle.clone();
    let popup_script = script.clone();
    let popup_owner = owner.clone();
    let nav_owner = owner.clone();
    let load_owner = owner.clone();
    let close_window = window.clone();
    let view = WebViewBuilder::new_with_web_context(&mut context)
        .with_additional_browser_args("")
        .with_incognito(true)
        .with_visible(true)
        .with_theme(theme(dark))
        .with_devtools(false)
        .with_initialization_script_for_main_only(script, true)
        .with_initialization_script_for_main_only(
            "window.close = () => window.ipc.postMessage('molly-payment-close');",
            true,
        )
        .with_ipc_handler(move |request| {
            if request.body() == "molly-payment-close" {
                let _ = close_window.close();
            }
        })
        .with_navigation_handler(move |target| {
            let allowed = navigation(&target);
            status(
                &nav_app,
                &nav_owner,
                allowed,
                &target,
                if allowed {
                    None
                } else {
                    Some("此链接无法在充值页打开，请使用页面提供的二维码支付。")
                },
            );
            allowed
        })
        .with_on_page_load_handler(move |event, target| {
            status(
                &load_app,
                &load_owner,
                matches!(event, wry::PageLoadEvent::Started),
                &target,
                None,
            )
        })
        .with_download_started_handler(|_, _| false)
        .with_new_window_req_handler(move |url, features| {
            make_popup(
                &popup_app,
                popup_owner.clone(),
                url,
                features,
                popup_script.clone(),
                dark,
                visible,
                navigation,
            )
        })
        .with_url(target)
        .build(&window)
        .map_err(|_| {
            let _ = window.close();
            "支付页面创建失败，请检查 WebView2 后重试"
        })?;
    Ok(Checkout {
        view,
        window,
        owner,
        _profile: profile,
    })
}

#[tauri::command]
pub async fn open_recharge_view(
    app: AppHandle,
    webview: Webview,
    state: State<'_, RuntimeState>,
    gate: State<'_, RechargeState>,
    dark: bool,
    view_id: String,
) -> Result<(), String> {
    require_console(&webview)?;
    let _guard = gate.0.lock().await;
    let token = state.access_token().await?;
    let user = state.api.get_authenticated("/auth/me", &token).await?;
    if user.get("id").is_none() {
        return Err("无法确认充值账户，请重新登录".into());
    }
    let update = session_script(&token, &user, dark, true);
    on_ui(&app, move || {
        if CHECKOUT.with(|v| v.borrow().as_ref().is_some_and(|v| v.owner == view_id)) {
            let window = CHECKOUT.with(|v| {
                let checkout = v.borrow();
                let checkout = checkout.as_ref().unwrap();
                let _ = checkout.view.set_theme(theme(dark));
                checkout
                    .view
                    .evaluate_script(&update)
                    .map_err(|_| "无法同步充值登录状态")?;
                Ok::<_, String>(checkout.window.clone())
            })?;
            let _ = window.set_theme(Some(window_theme(dark)));
            let windows = POPUPS.with(|p| {
                p.borrow()
                    .iter()
                    .map(|popup| {
                        let _ = popup.view.set_theme(theme(dark));
                        let _ = popup.view.evaluate_script(&update);
                        popup.window.clone()
                    })
                    .collect::<Vec<_>>()
            });
            for window in windows {
                let _ = window.set_theme(Some(window_theme(dark)));
            }
            return Ok(());
        }
        Err("请先选择待支付订单".into())
    })
    .await
}

pub async fn open_payment_target(
    app: &AppHandle,
    token: &str,
    user: &Value,
    dark: bool,
    target: String,
    recovery: Value,
    view_id: String,
) -> Result<(), String> {
    if !allowed_navigation(&target) {
        return Err("支付地址无效".into());
    }
    let mut script = session_script(token, user, dark, false);
    // Isolated same-origin recovery for the official payment landing pages only.
    // Gateway credentials never enter the console or third-party page storage.
    script.push_str(&format!("if(window.top===window&&location.origin==={}){{localStorage.setItem('payment.recovery.current',{});}}",json!(SERVICE_ORIGIN),json!(recovery.to_string())));
    let handle = app.clone();
    on_ui(app, move || {
        let existing = CHECKOUT.with(|v| {
            v.borrow()
                .as_ref()
                .filter(|c| c.owner == view_id)
                .map(|c| c.window.clone())
        });
        if let Some(window) = existing {
            window.show().map_err(|_| "无法显示支付窗口")?;
            return window.set_focus().map_err(|_| "无法激活支付窗口".into());
        }
        close_on_ui();
        let checkout = build_checkout(
            &handle,
            view_id,
            script,
            dark,
            &target,
            allowed_navigation,
            true,
        )?;
        let window = checkout.window.clone();
        CHECKOUT.with(|v| *v.borrow_mut() = Some(checkout));
        if window.show().and_then(|_| window.set_focus()).is_err() {
            close_on_ui();
            return Err("无法显示支付窗口".into());
        }
        Ok(())
    })
    .await
}

pub async fn close(app: &AppHandle) -> Result<(), String> {
    on_ui(app, || {
        close_on_ui();
        Ok(())
    })
    .await
}
fn close_popups() {
    let popups = POPUPS.with(|p| std::mem::take(&mut *p.borrow_mut()));
    for popup in popups {
        drop(popup.view);
        let _ = popup.window.close();
    }
}
fn dispose_checkout(checkout: Checkout, close_window: bool) {
    close_popups();
    let Checkout {
        view,
        window,
        owner,
        _profile,
    } = checkout;
    let _ = view.clear_all_browsing_data();
    drop(view);
    if close_window {
        let _ = window.close();
    }
    let _ =
        window
            .app_handle()
            .emit_to("console", "recharge-window-closed", json!({"owner":owner}));
}
fn close_on_ui() {
    let checkout = CHECKOUT.with(|v| v.borrow_mut().take());
    if let Some(checkout) = checkout {
        dispose_checkout(checkout, true);
    } else {
        close_popups();
    }
}
#[tauri::command]
pub async fn close_recharge_view(
    app: AppHandle,
    webview: Webview,
    gate: State<'_, RechargeState>,
    view_id: Option<String>,
) -> Result<(), String> {
    require_console(&webview)?;
    let _guard = gate.0.lock().await;
    close_owned(&app, view_id).await
}
async fn close_owned(app: &AppHandle, view_id: Option<String>) -> Result<(), String> {
    on_ui(app, move || {
        if CHECKOUT.with(|v| {
            v.borrow()
                .as_ref()
                .is_some_and(|c| view_id.as_ref().is_none_or(|id| c.owner == *id))
        }) {
            close_on_ui();
        }
        Ok(())
    })
    .await
}

pub fn handle_window_event(window: &Window, event: &WindowEvent) {
    if !matches!(event, WindowEvent::Destroyed) {
        return;
    }
    if window.label().starts_with(CHECKOUT_PREFIX) {
        let checkout = CHECKOUT.with(|v| {
            let mut value = v.borrow_mut();
            if value
                .as_ref()
                .is_some_and(|c| c.window.label() == window.label())
            {
                value.take()
            } else {
                None
            }
        });
        if let Some(checkout) = checkout {
            dispose_checkout(checkout, false);
        }
    } else if window.label().starts_with(POPUP_PREFIX) {
        POPUPS.with(|p| p.borrow_mut().retain(|p| p.label != window.label()));
        let owner = CHECKOUT.with(|v| v.borrow().as_ref().map(|c| c.owner.clone()));
        if let Some(owner) = owner {
            let _ =
                window
                    .app_handle()
                    .emit_to("console", "recharge-returned", json!({"owner":owner}));
        }
    } else if window.label() == "console" {
        close_on_ui();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkout_navigation_cannot_reach_local_or_active_content() {
        for url in [
            "javascript:alert(1)",
            "file:///c:/secret",
            "http://mollycloud.cn",
            "https://127.0.0.1/a",
            "https://a.localhost",
            "https://user:pass@example.com",
            "tauri://localhost/index.html",
            "data:text/html,a",
        ] {
            assert!(!allowed_navigation(url), "{url}");
        }
        for url in [
            PURCHASE_URL,
            "https://checkout.stripe.com/c/pay",
            "https://pay.example.com/return",
        ] {
            assert!(allowed_navigation(url));
        }
    }
    #[test]
    fn auth_script_is_origin_and_main_frame_scoped_and_never_sets_refresh_tokens() {
        let script = session_script("mock-access", &json!({"id":7}), false, false);
        assert!(
            script.contains("window.top !== window")
                && script.contains("location.origin !== \"https://mollycloud.cn\"")
        );
        assert!(!script.contains("setItem('refresh_token'"));
        assert!(!script.contains("__TAURI") && !script.contains("invoke"));
        assert!(script.contains("removeItem('refresh_token')"));
    }
}

#[cfg(feature = "recharge-smoke")]
pub fn run_smoke() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{atomic::AtomicBool, Arc},
        time::Duration,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    std::thread::spawn(move || {
        while !stopped.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                    let mut request = [0u8; 4096];
                    let read = stream.read(&mut request).unwrap_or(0);
                    let frame = String::from_utf8_lossy(&request[..read]).starts_with("GET /frame");
                    let body = if frame {
                        "<!doctype html><script>let token=null;try{token=localStorage.getItem('auth_token')}catch{}parent.postMessage({kind:'isolated',token,bridge:!!window.__TAURI_INTERNALS__},'*')</script>".to_string()
                    } else {
                        format!("<!doctype html><title>Checkout fixture</title><script>addEventListener('message',e=>{{if(e.data.kind==='isolated')window.frameProbe=e.data}})</script><body>Local checkout<iframe src='http://localhost:{port}/frame'></iframe></body>")
                    };
                    let _ = write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body);
                }
                Err(_) => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    });
    let temporary = tempfile::tempdir().unwrap();
    let profile = temporary.path().join("host");
    let okay = Arc::new(AtomicBool::new(false));
    let completed = okay.clone();
    let origin = format!("http://127.0.0.1:{port}");
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = "cn.mollycloud.recharge-smoke".into();
    context.config_mut().app.windows.clear();
    tauri::Builder::default()
        .setup(move |app| {
            tauri::WebviewWindowBuilder::new(app,"console",tauri::WebviewUrl::App("recharge-smoke.html".into()))
                .visible(false).skip_taskbar(true).inner_size(960.0,640.0).data_directory(profile)
                .on_web_resource_request(|_,response| { *response.body_mut()=std::borrow::Cow::Owned(b"<!doctype html><body>Checkout smoke host</body>".to_vec()); response.headers_mut().insert("Content-Type","text/html".parse().unwrap()); })
                .build()?;
            let handle = app.handle().clone();
            let watchdog = handle.clone();
            std::thread::spawn(move || { std::thread::sleep(Duration::from_secs(45)); watchdog.exit(1); });
            tauri::async_runtime::spawn(async move {
                let result = smoke_lifecycle(&handle, &origin).await;
                let exit_code = if result.is_ok() { 0 } else { 1 };
                match result { Ok(()) => { completed.store(true,Ordering::SeqCst); println!("Checkout native smoke: independent window, close-only controls, console responsiveness, isolated access-only login, iframe isolation, native popup, console identity and cleanup passed"); }, Err(error) => { eprintln!("Checkout native smoke failed: {error}"); std::process::exit(1); } }
                let _ = close(&handle).await;
                handle.exit(exit_code);
            });
            Ok(())
        })
        .on_window_event(handle_window_event)
        .run(context).unwrap();
    stop.store(true, Ordering::Relaxed);
    assert!(
        okay.load(Ordering::SeqCst),
        "Native checkout smoke did not pass"
    );
}

#[cfg(feature = "recharge-smoke")]
async fn smoke_eval(app: &AppHandle, script: &str) -> Result<Value, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let sender = RefCell::new(Some(tx));
    let diagnostic = script.chars().take(100).collect::<String>();
    let script = script.to_owned();
    on_ui(app, move || {
        CHECKOUT.with(|v| {
            v.borrow()
                .as_ref()
                .ok_or("Missing checkout")?
                .view
                .evaluate_script_with_callback(&script, move |result| {
                    if let Some(tx) = sender.borrow_mut().take() {
                        let _ = tx.send(result);
                    }
                })
                .map_err(|e| e.to_string())
        })
    })
    .await?;
    let value = tokio::time::timeout(std::time::Duration::from_secs(5), rx)
        .await
        .map_err(|_| format!("Script timed out: {diagnostic}"))?
        .map_err(|_| "Script callback missing")?;
    serde_json::from_str(&value).map_err(|e| e.to_string())
}

#[cfg(feature = "recharge-smoke")]
async fn smoke_lifecycle(app: &AppHandle, origin: &str) -> Result<(), String> {
    let handle = app.clone();
    let target = origin.to_owned();
    // The fixture uses its own loopback origin and mock credentials; production always uses SERVICE_ORIGIN.
    let script = session_script("smoke-access", &json!({"id":7}), false, false)
        .replace(SERVICE_ORIGIN, origin);
    on_ui(app, move || {
        let checkout = build_checkout(
            &handle,
            "smoke-payment".into(),
            script,
            false,
            &target,
            |url| url.starts_with("http://127.0.0.1:") || url.starts_with("http://localhost:"),
            false,
        )?;
        if checkout
            .window
            .is_minimizable()
            .map_err(|e| e.to_string())?
            || checkout
                .window
                .is_maximizable()
                .map_err(|e| e.to_string())?
            || !checkout.window.is_closable().map_err(|e| e.to_string())?
        {
            return Err("Payment window must expose only its close control".into());
        }
        if handle.get_webview_window(checkout.window.label()).is_some() {
            return Err("Payment window must not register a Tauri webview".into());
        }
        use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
        use windows::Win32::UI::WindowsAndMessaging::{GetWindow, GW_OWNER};
        let hwnd = windows::Win32::Foundation::HWND(
            checkout.window.hwnd().map_err(|e| e.to_string())?.0 as _,
        );
        let console = handle
            .get_webview_window("console")
            .ok_or("Missing console")?;
        let console_hwnd =
            windows::Win32::Foundation::HWND(console.hwnd().map_err(|e| e.to_string())?.0 as _);
        if unsafe { GetWindow(hwnd, GW_OWNER).is_ok() || !IsWindowEnabled(console_hwnd).as_bool() }
        {
            return Err("Payment window owns or disables the console".into());
        }
        console
            .set_position(tauri::LogicalPosition::new(80.0, 90.0))
            .map_err(|e| e.to_string())?;
        let position = console
            .outer_position()
            .map_err(|e| e.to_string())?
            .to_logical::<f64>(console.scale_factor().map_err(|e| e.to_string())?);
        if (position.x - 80.0).abs() > 2.0 || (position.y - 90.0).abs() > 2.0 {
            return Err("Console cannot move during payment".into());
        }
        if handle.get_webview_window("console").is_none() {
            return Err("Console identity lost after adding checkout".into());
        }
        CHECKOUT.with(|v| *v.borrow_mut() = Some(checkout));
        Ok(())
    })
    .await?;
    let mut ready = false;
    for _ in 0..60 {
        if smoke_eval(
            app,
            "document.readyState==='complete' && !!window.frameProbe",
        )
        .await?
            == json!(true)
        {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    if !ready {
        let diagnostic = smoke_eval(app,"({ready:document.readyState,url:location.href,body:document.body?.innerHTML,frame:window.frameProbe})").await?;
        return Err(format!(
            "Fixture or isolated frame did not load: {diagnostic}"
        ));
    }
    let probe = smoke_eval(app,"({token:localStorage.getItem('auth_token'),refresh:localStorage.getItem('refresh_token'),bridge:!!window.__TAURI_INTERNALS__,frame:window.frameProbe,user:JSON.parse(localStorage.getItem('auth_user')).id})").await?;
    if probe["token"] != "smoke-access"
        || !probe["refresh"].is_null()
        || probe["bridge"] != false
        || probe["user"] != 7
        || !probe["frame"]["token"].is_null()
        || probe["frame"]["bridge"] != false
    {
        return Err("Access-token or iframe isolation failed".into());
    }
    println!("Checkout fixture and cross-origin isolation passed");
    let refreshed = session_script("rotated-smoke-access", &json!({"id":7}), true, true)
        .replace(SERVICE_ORIGIN, origin);
    smoke_eval(app, &refreshed).await?;
    if smoke_eval(app, "document.documentElement.classList.contains('dark') && localStorage.getItem('auth_token')==='rotated-smoke-access'").await? != true {
        return Err("Session or theme synchronization failed".into());
    }
    smoke_eval(app, "localStorage.removeItem('auth_token')").await?;
    // A reload destroys the evaluating document, so it cannot promise a JS result callback.
    on_ui(app, move || {
        CHECKOUT.with(|v| {
            v.borrow()
                .as_ref()
                .ok_or("Missing checkout")?
                .view
                .evaluate_script(&refreshed)
                .map_err(|e| e.to_string())
        })
    })
    .await?;
    let mut restored = false;
    for _ in 0..60 {
        if smoke_eval(app, "document.readyState==='complete' && !!window.frameProbe && localStorage.getItem('auth_token')==='rotated-smoke-access'").await? == true {
            restored = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    if !restored {
        return Err("Expired website session was not restored after reload".into());
    }
    println!("Checkout theme and expired-session recovery passed");
    smoke_eval(
        app,
        "window.smokePopup=window.open(location.origin+'/popup','paymentPopup'); !!window.smokePopup",
    )
    .await?;
    let mut popup_created = false;
    for _ in 0..50 {
        if on_ui(app, || Ok(POPUPS.with(|p| p.borrow().len() == 1))).await? {
            popup_created = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    if !popup_created {
        return Err("Payment popup was not created inside app".into());
    }
    let mut popup_ready = false;
    for _ in 0..60 {
        if smoke_eval(app, "window.smokePopup.document.readyState==='complete' && window.smokePopup.location.pathname==='/popup'").await? == true { popup_ready=true; break; }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    if !popup_ready {
        return Err("Payment popup fixture did not load".into());
    }
    let isolated = smoke_eval(app, "!window.smokePopup.__TAURI_INTERNALS__").await?;
    if isolated != true {
        return Err("Popup exposes Tauri IPC".into());
    }
    println!("Checkout native popup isolation passed");
    smoke_eval(app, "window.smokePopup.close()").await?;
    let mut popup_closed = false;
    for _ in 0..50 {
        if on_ui(app, || Ok(POPUPS.with(|p| p.borrow().is_empty()))).await? {
            popup_closed = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    if !popup_closed {
        return Err("Payment window.close did not clean up native popup".into());
    }
    smoke_eval(
        app,
        "window.smokePopup=window.open(location.origin+'/popup','paymentPopup2')",
    )
    .await?;
    if !on_ui(app, || Ok(POPUPS.with(|p| p.borrow().len() == 1))).await? {
        return Err("Could not reopen payment popup".into());
    }
    on_ui(app, || {
        CHECKOUT.with(|v| v.borrow_mut().as_mut().unwrap().owner = "current-payment".into());
        Ok(())
    })
    .await?;
    close_owned(app, Some("stale-payment".into())).await?;
    if !on_ui(app, || Ok(CHECKOUT.with(|v| v.borrow().is_some()))).await? {
        return Err("A stale component closed the current payment".into());
    }
    let handle = app.clone();
    on_ui(app, move || {
        let window = CHECKOUT.with(|v| v.borrow().as_ref().unwrap().window.clone());
        if handle.get_webview_window("console").is_none() {
            return Err("Console lost before payment close".into());
        }
        // This is the same native CloseRequested path as clicking the titlebar X.
        window.close().map_err(|e| e.to_string())
    })
    .await?;
    let mut closed = false;
    for _ in 0..50 {
        if on_ui(app, || {
            Ok(CHECKOUT.with(|v| v.borrow().is_none()) && POPUPS.with(|p| p.borrow().is_empty()))
        })
        .await?
        {
            closed = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    if !closed {
        return Err(
            "Native close control did not release payment session and child windows".into(),
        );
    }
    println!("Payment X closes only payment windows; console remains available");
    if app.get_webview_window("console").is_none() {
        return Err("Payment close removed the console".into());
    }

    if !on_ui(app, || {
        Ok(CHECKOUT.with(|v| v.borrow().is_none()) && POPUPS.with(|p| p.borrow().is_empty()))
    })
    .await?
    {
        return Err("Checkout cleanup left a view alive".into());
    }
    Ok(())
}
