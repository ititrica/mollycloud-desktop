mod activity_pool;
mod audio_spectrum;
mod music_controller;
mod notification;
mod system_events;

#[cfg(feature = "test-hooks")]
pub use activity_pool::smoke_activity_api;

use std::net::{IpAddr, Ipv4Addr};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use sysinfo::Networks;
use tauri::{Emitter, Manager, State};
use tokio::sync::Mutex as TokioMutex;

use surge_ping::{Client, Config, PingIdentifier, PingSequence};
mod helpers;


// 全功能灵动岛智能双模动画锁
static ANIMATION_ID: AtomicU32 = AtomicU32::new(0);

// 将分散的坐标合并为一个结构体，并附带所有权 ID 防止误删
struct AnchorState {
    center_x: i32,
    origin_y: i32,
    left_x: i32,
    bottom_y: i32,
    active_id: u32,
}
static ANIMATION_ANCHOR: Mutex<Option<AnchorState>> = Mutex::new(None);

// Tauri holds its plugin-store mutex while dispatching a synchronous command.
// Win32 window operations can immediately re-enter the window event callback,
// which needs that same mutex. Keep ALL window commands asynchronous, including
// getters that wait for the UI thread (as Tauri's own window plugin does).
#[tauri::command]
async fn show_window_no_activate(app: tauri::AppHandle, label: String) -> Result<(), String> {
    if label != "netspeed-widget" { return Ok(()); }
    if let Some(win) = app.get_webview_window("netspeed-widget") {
        #[cfg(target_os = "windows")]
        {
            let (done, completion) = tokio::sync::oneshot::channel();
            app.run_on_main_thread(move || {
                let result = (|| -> Result<(), String> {
                    let hwnd = win.hwnd().map_err(|e| e.to_string())?;
                unsafe {
                    use winapi::um::winuser::{GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE};
                    // Update Tao's visibility state too. Raw ShowWindow alone
                    // leaves it "hidden", so later setAlwaysOnTop/style changes
                    // hide the island again. Suppress activation during show.
                    let style = GetWindowLongPtrW(hwnd.0 as _, GWL_EXSTYLE);
                    SetWindowLongPtrW(hwnd.0 as _, GWL_EXSTYLE, style | WS_EX_NOACTIVATE as isize);
                    let shown = win.show().map_err(|e| e.to_string());
                    SetWindowLongPtrW(hwnd.0 as _, GWL_EXSTYLE, style);
                    shown?;
                    // 强制回到置顶带最顶端：防止被其他置顶窗口（全屏应用/视频播放器悬浮窗等）盖住
                    winapi::um::winuser::SetWindowPos(
                        hwnd.0 as _,
                        winapi::um::winuser::HWND_TOPMOST,
                        0,
                        0,
                        0,
                        0,
                        // SWP_NOSIZE(0x0001) | SWP_NOMOVE(0x0002) | SWP_NOACTIVATE(0x0010)
                        0x0001 | 0x0002 | 0x0010,
                    );
                }
                    Ok(())
                })();
                let _ = done.send(result);
            }).map_err(|e| e.to_string())?;
            completion.await.map_err(|_| "灵动岛窗口操作已取消".to_string())??;
        }
        #[cfg(not(target_os = "windows"))]
        {
            win.show().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

// 动态切换窗口的“不可激活”扩展样式 (WS_EX_NOACTIVATE)。
// 全屏悬停唤起期间开启：点击灵动岛不再抢占焦点/激活，前台始终留给全屏应用，
// 从而避免 Windows 因前台变成非全屏小窗而弹出任务栏；退出全屏后恢复可激活。
#[tauri::command]
async fn set_window_no_activate(app: tauri::AppHandle, label: String, enabled: bool) {
    if label != "netspeed-widget" { return; }
    if let Some(win) = app.get_webview_window("netspeed-widget") {
        #[cfg(target_os = "windows")]
        {
            if let Ok(hwnd) = win.hwnd() {
                unsafe {
                    // GetWindowLongPtrW 返回 LONG_PTR，全程按其位宽运算，避免 64 位下截断
                    let ex_style = winapi::um::winuser::GetWindowLongPtrW(
                        hwnd.0 as _,
                        winapi::um::winuser::GWL_EXSTYLE,
                    );
                    let no_activate = winapi::um::winuser::WS_EX_NOACTIVATE
                        as winapi::shared::basetsd::LONG_PTR;
                    let new_style = if enabled {
                        ex_style | no_activate
                    } else {
                        ex_style & !no_activate
                    };
                    winapi::um::winuser::SetWindowLongPtrW(
                        hwnd.0 as _,
                        winapi::um::winuser::GWL_EXSTYLE,
                        new_style,
                    );
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (label, enabled);
        }
    }
}

// 新增：底层原子化窗口调整指令，彻底消除位移闪烁
#[tauri::command]
async fn set_window_bounds(app: tauri::AppHandle, x: i32, y: i32, width: i32, height: i32) {
    #[cfg(target_os = "windows")]
    {
        if let Some(win) = app.get_webview_window("netspeed-widget") {
            if let Ok(hwnd) = win.hwnd() {
                unsafe {
                    // 0x0014 = SWP_NOACTIVATE (0x0010) | SWP_NOZORDER (0x0004)
                    // 确保同时修改坐标和尺寸时，不抢占用户焦点，不打乱窗口层级
                    winapi::um::winuser::SetWindowPos(
                        hwnd.0 as _,
                        std::ptr::null_mut(),
                        x,
                        y,
                        width,
                        height,
                        0x0014,
                    );
                }
            }
        }
    }
}

#[tauri::command]
async fn start_island_animation(
    window: tauri::WebviewWindow,
    start_width: f64,
    start_height: f64,
    target_width: f64,
    target_height: f64,
    spring_style: String,
) -> Result<(), String> {
    if window.label() != "netspeed-widget" { return Err("只能调整灵动岛窗口".into()); }
    if ![start_width, start_height, target_width, target_height].iter().all(|v| v.is_finite() && *v >= 1.0 && *v <= 2000.0) { return Err("无效窗口尺寸".into()); }
    let id = ANIMATION_ID.fetch_add(1, Ordering::SeqCst) + 1;
    let scale_factor = window.scale_factor().unwrap_or(1.0);

    #[cfg(target_os = "windows")]
    {
        if let Ok(hwnd) = window.hwnd() {
            use winapi::shared::windef::RECT;
            use winapi::um::winuser::{GetWindowRect, SetWindowPos};

            let mut rect: RECT = unsafe { std::mem::zeroed() };
            unsafe {
                GetWindowRect(hwnd.0 as _, &mut rect);
            }

            let (anchor_cx, anchor_cy, _anchor_lx, _anchor_by) = {
                let mut anchor_guard = ANIMATION_ANCHOR.lock().unwrap_or_else(|e| e.into_inner());

                if let Some(anchor) = anchor_guard.as_mut() {
                    anchor.active_id = id;
                    (
                        anchor.center_x,
                        anchor.origin_y,
                        anchor.left_x,
                        anchor.bottom_y,
                    )
                } else {
                    let cx = rect.left + (rect.right - rect.left) / 2;
                    let cy = rect.top;
                    let lx = rect.left;
                    let by = rect.bottom;
                    *anchor_guard = Some(AnchorState {
                        center_x: cx,
                        origin_y: cy,
                        left_x: lx,
                        bottom_y: by,
                        active_id: id,
                    });
                    (cx, cy, lx, by)
                }
            };

            let window_clone = window.clone();
            let hwnd_raw = hwnd.0 as isize;

            std::thread::spawn(move || {
                let start_time = std::time::Instant::now();

                // 2. 👈 根据参数动态匹配弹性物理常数
                // Stiff (克制): 提高频率，大幅拉高阻尼，使其快准狠
                // Bouncy (Q弹): 保持原本欢快的震喜感
                let (freq, decay, duration_ms) = if spring_style == "stiff" {
                    (3.8, 22.0, 250)
                } else {
                    (2.4, 12.0, 400)
                };

                let duration = std::time::Duration::from_millis(duration_ms);

                while start_time.elapsed() < duration {
                    std::thread::sleep(std::time::Duration::from_millis(8));

                    if ANIMATION_ID.load(Ordering::SeqCst) != id {
                        return;
                    }

                    let elapsed = start_time.elapsed().as_secs_f64();
                    let progress = elapsed / (duration_ms as f64 / 1000.0);
                    if progress >= 1.0 {
                        break;
                    }

                    let spring = 1.0
                        - (freq * elapsed * 2.0 * std::f64::consts::PI).cos()
                            * (-decay * elapsed).exp();
                    let current_w = start_width + (target_width - start_width) * spring;
                    let current_h = start_height + (target_height - start_height) * spring;

                    // 1. 保留这俩变量，SetWindowPos 必须用到它们作为宽高参数
                    let phys_window_w = (current_w * scale_factor).round() as i32;
                    let phys_window_h = (current_h * scale_factor).round() as i32;

                    // 2. 坐标计算：直接用浮点数除以 2，避免 i32 除法丢失 0.5 像素导致漂移
                    let final_x = (anchor_cx as f64 - (current_w * scale_factor) / 2.0).round() as i32;
                    let final_y = anchor_cy;

                    unsafe {
                        SetWindowPos(
                            hwnd_raw as _,
                            std::ptr::null_mut(),
                            final_x,
                            final_y,
                            phys_window_w,
                            phys_window_h,
                            0x0014,
                        );
                    }
                }

                // 动画结束定格的那一帧，也要使用相同的浮点计算逻辑
                if ANIMATION_ID.load(Ordering::SeqCst) == id {
                    let phys_target_w = (target_width * scale_factor).round() as i32;
                    let phys_target_h = (target_height * scale_factor).round() as i32;

                    let final_x = (anchor_cx as f64 - (target_width * scale_factor) / 2.0).round() as i32;
                    let final_y = anchor_cy;

                    unsafe {
                        SetWindowPos(
                            hwnd_raw as _,
                            std::ptr::null_mut(),
                            final_x,
                            final_y,
                            phys_target_w,
                            phys_target_h,
                            0x0014,
                        );
                    }
                    let _ = window_clone.emit("molly-netspeed:island-resize", vec![target_width, target_height]);

                    if let Ok(mut guard) = ANIMATION_ANCHOR.lock() {
                        if let Some(anchor) = guard.as_ref() {
                            if anchor.active_id == id {
                                *guard = None;
                            }
                        }
                    }
                }

                if ANIMATION_ID.load(Ordering::SeqCst) == id {
                    let phys_target_w = (target_width * scale_factor).round() as i32;
                    let phys_target_h = (target_height * scale_factor).round() as i32;

                    let final_x = anchor_cx - phys_target_w / 2;
                    let final_y = anchor_cy;

                    unsafe {
                        SetWindowPos(
                            hwnd_raw as _,
                            std::ptr::null_mut(),
                            final_x,
                            final_y,
                            phys_target_w,
                            phys_target_h,
                            0x0014,
                        );
                    }
                    let _ = window_clone.emit("molly-netspeed:island-resize", vec![target_width, target_height]);

                    if let Ok(mut guard) = ANIMATION_ANCHOR.lock() {
                        if let Some(anchor) = guard.as_ref() {
                            if anchor.active_id == id {
                                *guard = None;
                            }
                        }
                    }
                }
            });
        }
    }
    Ok(())
}

pub struct AppState {
    pub networks: Mutex<Networks>,
    pub ws_task: TokioMutex<Option<tokio::task::JoinHandle<()>>>,
}

#[tauri::command]
fn sync_tray_menu() {}

#[tauri::command]
fn get_network_stats(state: State<'_, AppState>) -> (u64, u64) {
    let mut networks = state.networks.lock().unwrap();
    networks.refresh_list();
    networks.refresh();

    let mut total_rx = 0;
    let mut total_tx = 0;

    for (_interface_name, data) in networks.iter() {
        total_rx += data.total_received();
        total_tx += data.total_transmitted();
    }

    (total_rx, total_tx)
}

#[tauri::command]
async fn get_network_latency() -> Result<u128, String> {
    let timeout = Duration::from_millis(1500);
    let client = Client::new(&Config::default())
        .map_err(|e| format!("Failed to create ping client: {}", e))?;

    // 多目标 ICMP ping：避免单一 IP 被网络环境拦截导致误判断网
    let targets = [
        IpAddr::V4(Ipv4Addr::new(223, 5, 5, 5)),
        IpAddr::V4(Ipv4Addr::new(223, 6, 6, 6)),
        IpAddr::V4(Ipv4Addr::new(119, 29, 29, 29)),
        IpAddr::V4(Ipv4Addr::new(1, 0, 0, 1)),
    ];

    for ip in targets {
        let mut pinger = client.pinger(ip, PingIdentifier(rand::random::<u16>())).await;
        pinger.timeout(timeout);
        let attempt_start = Instant::now();
        if pinger.ping(PingSequence(0), &[0u8; 16]).await.is_ok() {
            return Ok(attempt_start.elapsed().as_millis());
        }
    }
    Err("Timeout".to_string())
}

#[tauri::command]
async fn is_widget_visible(app: tauri::AppHandle) -> bool {
    match app.get_webview_window("netspeed-widget") {
        Some(win) => win.is_visible().unwrap_or(false),
        None => false,
    }
}

/// 读取系统剪贴板文本（Windows 专用），供灵动岛检测复制到链接
#[cfg(target_os = "windows")]
#[tauri::command]
fn get_clipboard_text() -> Result<String, String> {
    unsafe {
        use winapi::um::winbase::{GlobalLock, GlobalUnlock};
        use winapi::um::winuser::{CF_UNICODETEXT, CloseClipboard, GetClipboardData, OpenClipboard};

        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err("无法打开剪贴板".to_string());
        }

        // 若剪贴板为空或格式不含文本，全局句柄为 NULL
        let handle = GetClipboardData(CF_UNICODETEXT as u32);
        if handle.is_null() {
            CloseClipboard();
            return Ok(String::new());
        }

        let ptr = GlobalLock(handle) as *const u16;
        if ptr.is_null() {
            CloseClipboard();
            return Ok(String::new());
        }

        // 统计字符串长度直到遇到结尾空字符
        let mut len = 0usize;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));

        GlobalUnlock(handle);
        CloseClipboard();
        Ok(text)
    }
}

// 非 Windows 平台空实现，避免编译报错
#[cfg(not(target_os = "windows"))]
#[tauri::command]
fn get_clipboard_text() -> Result<String, String> {
    Ok(String::new())
}

/// 获取浏览器实时活动标签页（当前窗口标题，即活动标签标题）
/// Windows 上通过 Win32 API（EnumWindows）枚举 msedge/chrome 进程的可见顶层窗口标题实现，
/// 替代原来的 PowerShell 轮询方案：不再每 2s 冷启动一个 PowerShell 子进程，开销更低、响应更快，
/// 且 GetWindowTextW 直接返回 UTF-16，中文标题天然正确，无需再处理控制台编码。
/// 注意：必须 async + spawn_blocking 放到阻塞线程池执行——Tauri v2 里不带 async 的同步命令
/// 会直接跑在主线程，阻塞 UI（频谱/动画掉帧）。
#[cfg(target_os = "windows")]
#[tauri::command]
async fn get_active_browser_tabs() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        use winapi::shared::minwindef::{BOOL, FALSE, LPARAM, TRUE};
        use winapi::shared::windef::HWND;
        use winapi::um::handleapi::CloseHandle;
        use winapi::um::processthreadsapi::OpenProcess;
        use winapi::um::winbase::QueryFullProcessImageNameW;
        use winapi::um::winnt::PROCESS_QUERY_LIMITED_INFORMATION;
        use winapi::um::winuser::{EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible};

        // EnumWindows 回调：lParam 携带结果 Vec 的指针，收集所有可见的 msedge/chrome 窗口标题
        unsafe extern "system" fn enum_browser_windows(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let titles = lparam as *mut Vec<String>;
            // 与旧 PowerShell 的 MainWindowTitle 语义一致：只处理可见窗口
            if IsWindowVisible(hwnd) == FALSE {
                return TRUE;
            }
            // 通过窗口所属进程判断是否为 msedge/chrome（与旧 PowerShell 按进程名匹配一致）
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == 0 {
                return TRUE;
            }
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
            if process.is_null() {
                return TRUE; // 权限不足（如浏览器以管理员运行）时跳过，与 PowerShell 读不到标题时行为一致
            }
            let mut path_buf = [0u16; 1024];
            let mut path_len = path_buf.len() as u32;
            let ok = QueryFullProcessImageNameW(process, 0, path_buf.as_mut_ptr(), &mut path_len);
            CloseHandle(process);
            if ok == FALSE {
                return TRUE;
            }
            let path = String::from_utf16_lossy(&path_buf[..path_len as usize]).to_lowercase();
            let is_browser = path.ends_with("msedge.exe") || path.ends_with("chrome.exe");
            if !is_browser {
                return TRUE;
            }
            // 读取窗口标题（UTF-16，天然支持中文，无需编码转换）
            let mut title_buf = [0u16; 512];
            let n = GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_buf.len() as i32);
            if n > 0 {
                let title = String::from_utf16_lossy(&title_buf[..n as usize]);
                let title = title.trim().to_string();
                if !title.is_empty() {
                    (*titles).push(title);
                }
            }
            TRUE
        }

        let mut titles: Vec<String> = Vec::new();
        unsafe {
            EnumWindows(Some(enum_browser_windows), &mut titles as *mut Vec<String> as isize);
        }
        // 去重：同一标题可能来自多个窗口/进程，保持首次出现顺序
        let mut seen = std::collections::HashSet::new();
        titles.retain(|t| seen.insert(t.clone()));

        Ok(titles)
    })
    .await
    .map_err(|e| format!("获取浏览器标签页任务失败: {}", e))?
}

// 非 Windows 平台空实现，避免编译报错
#[cfg(not(target_os = "windows"))]
#[tauri::command]
fn get_active_browser_tabs() -> Result<Vec<String>, String> {
    Ok(Vec::new())
}

// 缓存 AppHandle 供剪贴板监听线程的窗口过程使用
#[cfg(target_os = "windows")]
static CLIPBOARD_EMITTER: OnceLock<tauri::AppHandle> = OnceLock::new();

// 剪贴板监听窗口过程：一旦检测到剪贴板内容变化（WM_CLIPBOARDUPDATE），推送事件给前端
#[cfg(target_os = "windows")]
extern "system" fn clipboard_wndproc(
    hwnd: winapi::shared::windef::HWND,
    msg: winapi::shared::minwindef::UINT,
    wparam: winapi::shared::minwindef::WPARAM,
    lparam: winapi::shared::minwindef::LPARAM,
) -> winapi::shared::minwindef::LRESULT {
    use winapi::um::winuser::{
        DefWindowProcW, PostQuitMessage, WM_CLIPBOARDUPDATE, WM_DESTROY,
    };
    unsafe {
        if msg == WM_CLIPBOARDUPDATE {
            if let Some(app) = CLIPBOARD_EMITTER.get() {
                let _ = Emitter::emit(app, "molly-netspeed:clipboard-changed", ());
            }
            return 0;
        }
        if msg == WM_DESTROY {
            PostQuitMessage(0);
            return 0;
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

// 启动剪贴板变更监听（事件驱动，无需轮询）：通过隐藏窗口 + AddClipboardFormatListener
#[cfg(target_os = "windows")]
fn start_clipboard_monitor(app: tauri::AppHandle) {
    use winapi::um::libloaderapi::GetModuleHandleW;
    use winapi::um::winuser::{
        AddClipboardFormatListener, CreateWindowExW, DispatchMessageW, GetMessageW,
        RegisterClassW, TranslateMessage, MSG, WNDCLASSW, WS_OVERLAPPEDWINDOW,
    };

    // 缓存 AppHandle 供窗口过程回调使用
    let _ = CLIPBOARD_EMITTER.set(app);

    std::thread::spawn(|| unsafe {
        let class_name = "NetSpeedClipboardListener"
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<u16>>();
        let hinstance = GetModuleHandleW(std::ptr::null());

        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(clipboard_wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance as _,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        // 注册窗口失败则直接退出监听线程
        if RegisterClassW(&wc) == 0 {
            return;
        }

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            class_name.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance as _,
            std::ptr::null_mut(),
        );
        if hwnd.is_null() {
            return;
        }

        // 注册为剪贴板格式监听者，剪贴板变化时收到 WM_CLIPBOARDUPDATE
        AddClipboardFormatListener(hwnd);

        // 线程消息循环
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}


/// Statically linked built-in service. Not part of the installable feature catalog.
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    let dispatch: Box<dyn Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync> = Box::new(tauri::generate_handler![
            get_network_stats,
            is_widget_visible,
            get_network_latency,
            notification::fetch_latest_notification,
            set_window_bounds,
            start_island_animation,
            show_window_no_activate,
            set_window_no_activate,
            helpers::toggle_taskbar_plugin,
            helpers::sync_to_taskbar,
            audio_spectrum::get_audio_spectrum,
            music_controller::set_target_player,
            music_controller::fetch_netease_music_info,
            music_controller::control_system_media,
            music_controller::get_random_cover_url,
            music_controller::get_smtc_cover,
            music_controller::fetch_netease_lyrics,
            music_controller::fetch_song_meta,
            music_controller::start_websocket_lyrics,
            music_controller::stop_websocket_lyrics,
            helpers::toggle_fps_plugin,
            sync_tray_menu,
            get_clipboard_text,
            get_active_browser_tabs,
        open_console, configure_activity_api]);
    tauri::plugin::Builder::new("molly-netspeed")
        .invoke_handler(move |invoke| {
            if !matches!(invoke.message.webview().label(), "console" | "netspeed-widget") {
                invoke.resolver.reject("此窗口不能调用灵动岛服务");
                return true;
            }
            dispatch(invoke)
        })
        .setup(|app, _| {
            app.manage(AppState { networks: Mutex::new(Networks::new_with_refreshed_list()), ws_task: TokioMutex::new(None) });
            Ok(())
        })
        .on_event(|_, event| { if matches!(event, tauri::RunEvent::Exit) { helpers::shutdown(); } })
        .build()
}

#[tauri::command]
async fn open_console(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("console") {
        let _ = window.show(); let _ = window.unminimize(); let _ = window.set_focus();
        let _ = window.emit("molly:navigate", "netspeed");
    }
}

#[tauri::command]
fn configure_activity_api(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    activity_pool::configure(app, enabled)
}

pub fn start(app: &tauri::AppHandle) {
    system_events::start_monitor(app.clone());
    #[cfg(target_os = "windows")]
    start_clipboard_monitor(app.clone());
            // 全屏应用检测线程
            let app_handle_for_fs = app.clone();
            std::thread::spawn(move || {
                unsafe {
                    let _ = windows::Win32::System::Com::CoInitializeEx(
                        None,
                        windows::Win32::System::Com::COINIT_MULTITHREADED,
                    );
                }

                let mut was_fullscreen = false;
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(600));

                    #[cfg(target_os = "windows")]
                    {
                        unsafe {
                            let mut is_fullscreen = false;
                            let fg_hwnd = winapi::um::winuser::GetForegroundWindow();
                            let shell_hwnd = winapi::um::winuser::GetShellWindow(); // 系统的根：explorer.exe

                            // 过滤掉无焦点窗口、桌面根节点
                            if !fg_hwnd.is_null()
                                && fg_hwnd != winapi::um::winuser::GetDesktopWindow()
                                && fg_hwnd != shell_hwnd
                            {
                                // 获取系统外壳 (explorer.exe) 的进程 ID
                                let mut shell_pid = 0;
                                if !shell_hwnd.is_null() {
                                    winapi::um::winuser::GetWindowThreadProcessId(
                                        shell_hwnd,
                                        &mut shell_pid,
                                    );
                                }

                                // 获取当前前景窗口的进程 ID
                                let mut fg_pid = 0;
                                winapi::um::winuser::GetWindowThreadProcessId(fg_hwnd, &mut fg_pid);

                                // 核心判定：如果抢占焦点的窗口 PID 和任务栏/桌面是一家人
                                // 说明这绝对是任务栏悬浮窗、音量面板或透明防误触层，直接忽略！
                                if shell_pid != 0 && fg_pid == shell_pid {
                                    // 属于系统外壳组件，当做无事发生
                                } else {
                                    // 进一步排除子窗口 (WS_CHILD) 和 鼠标穿透层 (WS_EX_TRANSPARENT)
                                    let style = winapi::um::winuser::GetWindowLongPtrW(
                                        fg_hwnd,
                                        winapi::um::winuser::GWL_STYLE,
                                    ) as u32;
                                    let ex_style = winapi::um::winuser::GetWindowLongPtrW(
                                        fg_hwnd,
                                        winapi::um::winuser::GWL_EXSTYLE,
                                    ) as u32;

                                    if (style & winapi::um::winuser::WS_CHILD) == 0
                                        && (ex_style & winapi::um::winuser::WS_EX_TRANSPARENT) == 0
                                    {
                                        let mut class_name = [0u16; 256];
                                        let len = winapi::um::winuser::GetClassNameW(
                                            fg_hwnd,
                                            class_name.as_mut_ptr(),
                                            class_name.len() as i32,
                                        );
                                        let class_str =
                                            String::from_utf16_lossy(&class_name[..len as usize]);

                                        // 保底黑名单（防一手那些不在 explorer.exe 里的新版 UWP 系统层）
                                        let is_blacklisted = class_str
                                            .contains("Windows.UI.Core.CoreWindow")
                                            || class_str.contains("Xaml_WindowedPopupClass")
                                            || class_str.contains("SearchApp")
                                            || class_str.contains("NotifyIconOverflowWindow");

                                        if !is_blacklisted {
                                            // 几何判定：真正判断它是否铺满了屏幕
                                            let mut rect: winapi::shared::windef::RECT =
                                                std::mem::zeroed();
                                            winapi::um::winuser::GetWindowRect(fg_hwnd, &mut rect);

                                            let monitor = winapi::um::winuser::MonitorFromWindow(
                                                fg_hwnd,
                                                winapi::um::winuser::MONITOR_DEFAULTTONEAREST,
                                            );

                                            // 跨屏判定：只有全屏发生在灵动岛所在的那块显示器上才触发隐藏。
                                            // 否则副屏全屏看片/游戏时，主屏的岛也会被误藏。
                                            // 岛窗口取不到时（启动瞬间/已销毁）回退为旧的“任意屏全屏”行为。
                                            let mut same_monitor_as_island = true;
                                            if let Some(island_win) =
                                                app_handle_for_fs.get_webview_window("netspeed-widget")
                                            {
                                                if let Ok(island_hwnd) = island_win.hwnd() {
                                                    let island_monitor =
                                                        winapi::um::winuser::MonitorFromWindow(
                                                            island_hwnd.0 as _,
                                                            winapi::um::winuser::MONITOR_DEFAULTTONEAREST,
                                                        );
                                                    same_monitor_as_island =
                                                        island_monitor == monitor;
                                                }
                                            }
                                            if !same_monitor_as_island {
                                                // 全屏在另一块屏上：与本应用无关，维持可见
                                            } else {
                                                let mut mi: winapi::um::winuser::MONITORINFO =
                                                    std::mem::zeroed();
                                                mi.cbSize = std::mem::size_of::<
                                                    winapi::um::winuser::MONITORINFO,
                                                >(
                                                )
                                                    as u32;
                                                winapi::um::winuser::GetMonitorInfoW(
                                                    monitor,
                                                    &mut mi,
                                                );

                                                if rect.left <= mi.rcMonitor.left
                                                    && rect.top <= mi.rcMonitor.top
                                                    && rect.right >= mi.rcMonitor.right
                                                    && rect.bottom >= mi.rcMonitor.bottom
                                                {
                                                    is_fullscreen = true;
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // 状态翻转时发送信号
                            if is_fullscreen != was_fullscreen {
                                let _ = app_handle_for_fs.emit("molly-netspeed:fullscreen-changed", is_fullscreen);
                                was_fullscreen = is_fullscreen;
                            }
                        }
                    }
                }
            });


}
