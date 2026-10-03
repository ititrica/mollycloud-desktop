use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const HELPER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/molly-audio-capture"));
const CHUNK: usize = 1024;

fn install_helper(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("native");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let target = directory.join("molly-audio-capture");
    if std::fs::read(&target).ok().as_deref() != Some(HELPER) {
        let mut temporary =
            tempfile::NamedTempFile::new_in(&directory).map_err(|e| e.to_string())?;
        temporary.write_all(HELPER).map_err(|e| e.to_string())?;
        temporary
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        temporary.persist(&target).map_err(|e| e.to_string())?;
    }
    Ok(target)
}

fn capture(app: &AppHandle, enabled: &AtomicBool) -> Result<(), String> {
    let helper = install_helper(app)?;
    let mut child = Command::new(helper)
        .arg(std::process::id().to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("无法启动 macOS 音频组件：{e}"))?;
    let mut stdout = child.stdout.take().ok_or("音频组件没有数据通道")?;
    let stderr = child.stderr.take().ok_or("音频组件没有错误通道")?;
    let error_reader = std::thread::spawn(move || {
        let mut message = String::new();
        let _ = stderr.take(8192).read_to_string(&mut message);
        message
    });
    let (sender, receiver) = mpsc::sync_channel(4);
    std::thread::spawn(move || {
        let mut bytes = [0u8; CHUNK * 4];
        while stdout.read_exact(&mut bytes).is_ok() {
            let values: Vec<f32> = bytes
                .chunks_exact(4)
                .map(|sample| f32::from_le_bytes(sample.try_into().unwrap()))
                .collect();
            if sender.send(values).is_err() {
                break;
            }
        }
    });
    let mut failed = false;
    while enabled.load(Ordering::SeqCst) {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(values) => {
                let _ = app.emit("audio:pcm", values);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                failed = true;
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if child.try_wait().ok().flatten().is_some() {
            failed = true;
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    drop(receiver);
    let message = error_reader.join().unwrap_or_default();
    if failed && enabled.load(Ordering::SeqCst) {
        Err(if message.is_empty() {
            "系统音频捕获已停止，请重新开启音频互动".into()
        } else {
            message
        })
    } else {
        Ok(())
    }
}

/// Starting the app does not request capture permissions. A stopped or denied
/// stream is retried only after the user turns audio interaction off and on.
pub fn start_loopback_capture(app: AppHandle, enabled: Arc<AtomicBool>) {
    loop {
        while !enabled.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(100));
        }
        if let Err(error) = capture(&app, &enabled) {
            let _ = app.emit("audio:error", error);
            while enabled.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}
