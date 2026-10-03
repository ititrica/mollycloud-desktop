//! Optional upstream taskbar/FPS helpers: fixed bundle paths, pinned hashes,
//! no shell, and a kill-on-close job owned by MollyCloud.
use std::{path::PathBuf, process::{Child, Command}, sync::{Mutex, OnceLock}};
use std::os::windows::{io::AsRawHandle, process::CommandExt};
use sha2::{Digest, Sha256};
use tauri::{Emitter, Manager};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use windows::Win32::{Foundation::{CloseHandle, HANDLE}, System::JobObjects::{CreateJobObjectW, SetInformationJobObject, AssignProcessToJobObject, JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE}};

struct OwnedProcess { child: Child, job: HANDLE }
unsafe impl Send for OwnedProcess {}
impl Drop for OwnedProcess {
    fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); unsafe { let _ = CloseHandle(self.job); } }
}
static TASKBAR: Mutex<Option<OwnedProcess>> = Mutex::new(None);
static FPS: Mutex<Option<OwnedProcess>> = Mutex::new(None);
static TASKBAR_SENDER: OnceLock<broadcast::Sender<String>> = OnceLock::new();
static FPS_STARTED: Mutex<bool> = Mutex::new(false);

const HELPERS: [(&str, &str); 2] = [
    ("NSD_Taskbar_Plugin.exe", "94c56b721c807461f7d886ab4ce99e80a0d0058cd22de1c90763da7209b88a3b"),
    ("NSD_Fps_Plugin.exe", "60c86f785a63f86d1e614a33676e20bbe54969657662c758a69cfe8b862bb810"),
];
fn helper_path(app: &tauri::AppHandle, name: &str) -> Result<PathBuf, String> {
    let (_, expected) = HELPERS.iter().find(|(n, _)| *n == name).ok_or("未知组件")?;
    let path = if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../helpers").join(name)
    } else {
        app.path().resource_dir().map_err(|e| e.to_string())?.join("_up_/_up_/vendor/netspeed-dynamic/helpers").join(name)
    };
    let bytes = std::fs::read(&path).map_err(|_| "内置辅助组件缺失，请重新安装 MollyCloud".to_string())?;
    if format!("{:x}", Sha256::digest(bytes)) != *expected { return Err("内置辅助组件校验失败".into()); }
    Ok(path)
}
fn launch(path: PathBuf) -> Result<OwnedProcess, String> {
    unsafe {
        let job = CreateJobObjectW(None, None).map_err(|e| e.to_string())?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if let Err(e) = SetInformationJobObject(job, JobObjectExtendedLimitInformation, &limits as *const _ as _, std::mem::size_of_val(&limits) as u32) {
            let _ = CloseHandle(job); return Err(e.to_string());
        }
        let mut child = match Command::new(&path).current_dir(path.parent().unwrap()).creation_flags(0x08000000).spawn() {
            Ok(child) => child, Err(e) => { let _ = CloseHandle(job); return Err(e.to_string()); }
        };
        if let Err(e) = AssignProcessToJobObject(job, HANDLE(child.as_raw_handle())) {
            let _ = child.kill(); let _ = child.wait(); let _ = CloseHandle(job); return Err(e.to_string());
        }
        Ok(OwnedProcess { child, job })
    }
}
fn start_taskbar_server() -> Result<(), String> {
    if TASKBAR_SENDER.get().is_some() { return Ok(()); }
    let listener = std::net::TcpListener::bind("127.0.0.1:47291").map_err(|_| "任务栏组件端口 47291 已被占用，请关闭独立版 NetSpeed 后重试")?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let (tx, _) = broadcast::channel::<String>(16);
    let _ = TASKBAR_SENDER.set(tx.clone());
    tauri::async_runtime::spawn(async move {
        let Ok(listener) = tokio::net::TcpListener::from_std(listener) else { return; };
        while let Ok((stream, _)) = listener.accept().await {
            let tx = tx.clone();
            tauri::async_runtime::spawn(async move {
                let Ok(ws) = tokio_tungstenite::accept_hdr_async(stream, |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response: tokio_tungstenite::tungstenite::handshake::server::Response| {
                    if request.headers().contains_key("Origin") {
                        let mut denied = tokio_tungstenite::tungstenite::handshake::server::ErrorResponse::new(Some("仅允许本机原生组件".into()));
                        *denied.status_mut() = tokio_tungstenite::tungstenite::http::StatusCode::FORBIDDEN;
                        return Err(denied);
                    }
                    Ok(response)
                }).await else { return; };
                let (mut writer, _) = ws.split();
                let mut rx = tx.subscribe();
                while let Ok(msg) = rx.recv().await {
                    if writer.send(tokio_tungstenite::tungstenite::Message::Text(msg)).await.is_err() { break; }
                }
            });
        }
    });
    Ok(())
}
#[tauri::command]
pub fn toggle_taskbar_plugin(app: tauri::AppHandle, enable: bool) -> Result<bool, String> {
    let mut process = TASKBAR.lock().map_err(|e| e.to_string())?;
    if !enable { *process = None; return Ok(false); }
    if process.as_mut().is_some_and(|p| p.child.try_wait().ok().flatten().is_some()) { *process = None; }
    if process.is_none() { let path = helper_path(&app, HELPERS[0].0)?; start_taskbar_server()?; *process = Some(launch(path)?); }
    Ok(true)
}
#[tauri::command]
pub fn toggle_fps_plugin(app: tauri::AppHandle, enable: bool) -> Result<bool, String> {
    let mut process = FPS.lock().map_err(|e| e.to_string())?;
    if !enable { *process = None; return Ok(false); }
    if process.as_mut().is_some_and(|p| p.child.try_wait().ok().flatten().is_some()) { *process = None; }
    if process.is_none() {
        let path = helper_path(&app, HELPERS[1].0)?;
        let mut started = FPS_STARTED.lock().map_err(|e| e.to_string())?;
        if !*started {
            let socket = std::net::UdpSocket::bind("127.0.0.1:47292").map_err(|_| "FPS 端口 47292 已被占用，请关闭独立版 NetSpeed 后重试")?;
            std::thread::spawn(move || {
                let mut buf = [0; 16];
                while let Ok((len, _)) = socket.recv_from(&mut buf) {
                    if let Ok(fps) = std::str::from_utf8(&buf[..len]).unwrap_or("").trim().parse::<u32>() {
                        let _ = app.emit("molly-netspeed:fps-event", serde_json::json!({ "fps": fps }));
                    }
                }
            });
            *started = true;
        }
        *process = Some(launch(path)?);
    }
    Ok(true)
}
#[tauri::command]
pub fn sync_to_taskbar(up: String, down: String, lyric: String, mode: String, is_playing: bool, cover: String, msg_title: String, msg_body: String, msg_icon: String, cpu: u8, ram: u8) {
    if let Some(tx) = TASKBAR_SENDER.get() { let _ = tx.send(serde_json::json!({ "up": up, "down": down, "lyric": lyric, "mode": mode, "is_playing": is_playing, "cover": cover, "msg_title": msg_title, "msg_body": msg_body, "msg_icon": msg_icon, "cpu": cpu, "ram": ram }).to_string()); }
}
pub fn shutdown() { if let Ok(mut p) = TASKBAR.lock() { *p = None; } if let Ok(mut p) = FPS.lock() { *p = None; } }
