//! WKWebView regression using production image assets and a loopback mock only.
//! cargo run --example image_native_smoke --features image-native-smoke
#[allow(dead_code)]
#[path = "../src/console_plugins.rs"]
mod console_plugins;
#[allow(dead_code)]
#[path = "../src/image_workbench.rs"]
mod image_workbench;

use serde_json::{json, Value};
use std::sync::{atomic::{AtomicUsize, Ordering}, Arc, Mutex};
static BOOTSTRAP_CALLS: AtomicUsize = AtomicUsize::new(0);
#[derive(Clone)]
struct SmokeResult(Arc<Mutex<Option<Value>>>);
#[tauri::command]
fn smoke_report(app: tauri::AppHandle, result: Value, state: tauri::State<'_, SmokeResult>) {
    *state.0.lock().unwrap() = Some(result);
    app.exit(0);
}
#[tauri::command]
fn bootstrap_public() -> Value {
    BOOTSTRAP_CALLS.fetch_add(1, Ordering::SeqCst);
    json!({"service_origin":"https://example.test","health":{"status":"ok"},"settings":{}})
}
#[tauri::command]
fn bootstrap_calls() -> usize { BOOTSTRAP_CALLS.load(Ordering::SeqCst) }
#[tauri::command]
fn restore_session() -> Value { json!({"id":"native-image-test","email":"image@example.test","username":"Image smoke","balance":0}) }
#[tauri::command]
fn fetch_dashboard() -> Value {
    json!({"user":restore_session(),"keys":{"items":[{"id":"sentinel","key":"NEVER-AUTO-IMPORT"}]},"usage":{},"subscriptions":{},"subscription_progress":[]})
}
#[tauri::command]
fn get_assistant_config() -> Value {
    json!({"enabled":false,"provider":"mollycloud","model":"","persona":"","custom_base_url":"","greet_interval":20,"molly_key_id":"","api_key_configured":false})
}
#[tauri::command]
fn is_pet_visible() -> bool { false }
#[tauri::command]
fn set_console_dashboard_active() {}
#[tauri::command]
fn check_desktop_update() -> Option<Value> { None }
#[tauri::command]
fn fetch_account_balance() -> Value { json!({"balance":0}) }

fn main() {
    use std::io::{Read, Write};
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().to_path_buf();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let api = format!("http://{}/stream", listener.local_addr().unwrap());
    let requested = Arc::new(Mutex::new(String::new()));
    let request_copy = requested.clone();
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(50);
        while std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    // Darwin inherits O_NONBLOCK on accepted sockets.
                    stream.set_nonblocking(false).unwrap();
                    stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
                    let mut bytes = Vec::new();
                    let mut part = [0u8; 1024];
                    while !bytes.windows(4).any(|value| value == b"\r\n\r\n") {
                        let count = stream.read(&mut part).unwrap();
                        if count == 0 { break; }
                        bytes.extend_from_slice(&part[..count]);
                    }
                    *request_copy.lock().unwrap() = String::from_utf8_lossy(&bytes).into();
                    let mut payload = vec![b'x'; 3 * 1024 * 1024];
                    payload.extend_from_slice(b"native-stream-end");
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", payload.len()).unwrap();
                    stream.write_all(&payload).unwrap();
                    return;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(std::time::Duration::from_millis(10)),
                Err(error) => panic!("mock listener: {error}"),
            }
        }
    });
    let result = SmokeResult(Arc::new(Mutex::new(None)));
    let output = result.clone();
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = "cn.mollycloud.native-image-smoke".into();
    context.config_mut().app.windows.clear();
    let script = format!("window.__imageSmokeApi={};\n{}", serde_json::to_string(&api).unwrap(), include_str!("image_native_smoke.js"));
    let app = tauri::Builder::default()
        .manage(result).manage(image_workbench::ImageRequests::default())
        .plugin(image_workbench::plugin())
        .invoke_handler(tauri::generate_handler![smoke_report, bootstrap_public, bootstrap_calls, restore_session, fetch_dashboard, get_assistant_config, is_pet_visible, set_console_dashboard_active, check_desktop_update, fetch_account_balance, console_plugins::list_console_plugins, image_workbench::image_request, image_workbench::cancel_image_request])
        .setup(move |app| {
            console_plugins::initialize_at(app.handle(), root.join("plugins"));
            let resources = app.handle().clone();
            tauri::WebviewWindowBuilder::new(app, "console", tauri::WebviewUrl::App("index.html".into()))
                .title("MollyCloud isolated native image verification")
                .inner_size(1180.0, 760.0).visible(true).focused(false).incognito(true)
                .data_directory(root.join("webview"))
                .on_web_resource_request(move |request, response| console_plugins::intercept(&resources, request, response))
                // Test-only instrumentation observes the opaque iframe without
                // granting it same-origin access or changing its sandbox.
                .initialization_script_for_all_frames(script).build()?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {std::thread::sleep(std::time::Duration::from_secs(55));handle.exit(1);});
            Ok(())
        }).build(context).unwrap();
    app.run_return(|_, _| {});
    let outcome = output.0.lock().unwrap().clone().unwrap_or(json!({"ok":false,"error":"watchdog timeout"}));
    println!("{outcome}");
    assert_eq!(outcome["ok"], true);
    server.join().unwrap();
    let received = requested.lock().unwrap().to_ascii_lowercase();
    assert!(received.starts_with("get /stream "));
    assert!(received.contains("authorization: bearer mock-image-manual-only"));
    assert!(!received.contains("cookie:") && !received.contains("never-auto-import"));
    println!("Native image sandbox and complete 3 MB transport passed; no real credentials or paid requests");
}
