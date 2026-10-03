mod discovery;
mod parser;
mod progress;
mod reducer;
mod watcher;

use notify::{RecursiveMode, Watcher};
use reducer::{Status, Task, Tasks};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::PathBuf,
    sync::{mpsc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

#[derive(Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    tasks: Vec<Task>,
    status: Option<Status>,
    warning: Option<String>,
}

#[derive(Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Saved {
    #[serde(default)]
    unread: Tasks,
    #[serde(default)]
    extra_homes: Vec<PathBuf>,
}

enum Message {
    Fs(notify::Result<notify::Event>),
    Acknowledge(Task),
    AddHome(PathBuf, mpsc::Sender<Result<(), String>>),
}
pub struct ActivityState {
    snapshot: Mutex<Snapshot>,
    sender: mpsc::Sender<Message>,
}

fn require_host(window: &WebviewWindow) -> Result<(), String> {
    if matches!(window.label(), "main" | "console") {
        Ok(())
    } else {
        Err("仅 MollyCloud 窗口可访问任务状态".into())
    }
}

#[tauri::command]
pub fn get_codex_activity(
    window: WebviewWindow,
    state: State<'_, ActivityState>,
) -> Result<Snapshot, String> {
    require_host(&window)?;
    state
        .snapshot
        .lock()
        .map(|s| s.clone())
        .map_err(|_| "任务状态暂不可用".into())
}

#[tauri::command]
pub fn open_codex_activity(
    window: WebviewWindow,
    state: State<'_, ActivityState>,
    key: String,
) -> Result<(), String> {
    require_host(&window)?;
    let task = state
        .snapshot
        .lock()
        .map_err(|_| "任务状态暂不可用")?
        .tasks
        .iter()
        .find(|t| t.key == key)
        .cloned()
        .ok_or("任务已更新，请重试")?;
    if !parser::valid_thread_id(&task.thread_id) {
        return Err("任务标识无效".into());
    }
    // A fixed scheme and validated UUID; never execute a URL supplied by a session log.
    crate::launch::open_codex_thread(&task.thread_id)?;
    state
        .sender
        .send(Message::Acknowledge(task))
        .map_err(|_| "任务监听已停止".into())
}

#[tauri::command]
pub async fn add_codex_activity_home(
    window: WebviewWindow,
    state: State<'_, ActivityState>,
    path: String,
) -> Result<(), String> {
    require_host(&window)?;
    let home = PathBuf::from(path.trim());
    if !home.is_absolute() || !home.join("sessions").is_dir() {
        return Err("请选择包含 sessions 文件夹的 Codex 目录".into());
    }
    let home = home.canonicalize().map_err(|_| "无法访问该目录")?;
    let (reply, receiver) = mpsc::channel();
    state
        .sender
        .send(Message::AddHome(home, reply))
        .map_err(|_| "任务监听已停止")?;
    tauri::async_runtime::spawn_blocking(move || {
        receiver
            .recv_timeout(Duration::from_secs(30))
            .map_err(|_| "添加目录超时，请重试".to_string())
    })
    .await
    .map_err(|_| "添加目录失败".to_string())??
}

fn save(path: &PathBuf, saved: &Saved) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("missing parent"))?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut temp, saved)?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

fn publish(app: &AppHandle, tasks: &Tasks, warning: &Option<String>) {
    let mut visible: Vec<_> = tasks
        .values()
        .filter(|t| t.status.active() || t.unread)
        .cloned()
        .collect();
    visible.sort_by(|a, b| {
        b.status
            .priority()
            .cmp(&a.status.priority())
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
    let snapshot = Snapshot {
        status: visible
            .iter()
            .max_by_key(|t| t.status.priority())
            .map(|t| t.status),
        tasks: visible,
        warning: warning.clone(),
    };
    let state = app.state::<ActivityState>();
    if let Ok(mut current) = state.snapshot.lock() {
        if *current == snapshot {
            return;
        }
        *current = snapshot.clone();
    }
    // Do not broadcast lifecycle metadata to embedded third-party content.
    let _ = app.emit_to("main", "codex-activity-updated", &snapshot);
}

pub fn initialize(app: &AppHandle) {
    let (sender, receiver) = mpsc::channel();
    app.manage(ActivityState {
        snapshot: Mutex::new(Snapshot::default()),
        sender: sender.clone(),
    });
    let app = app.clone();
    std::thread::spawn(move || {
        let Ok(dir) = app.path().app_data_dir() else {
            return;
        };
        let path = dir.join("codex-activity.json");
        let mut saved: Saved = fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        saved
            .unread
            .retain(|_, t| parser::valid_thread_id(&t.thread_id) && t.unread && !t.status.active());
        let mut tasks = saved.unread.clone();
        let mut last_saved = saved.clone();
        let mut reader = watcher::Reader::default();
        let mut warning = None;
        let mut watcher = match notify::recommended_watcher(move |event| {
            let _ = sender.send(Message::Fs(event));
        }) {
            Ok(w) => Some(w),
            Err(_) => {
                warning = Some("Codex 文件通知不可用，已改为定时同步".into());
                None
            }
        };
        let mut watched = BTreeMap::new();
        let mut discovered = BTreeSet::new();
        let mut initial = true;
        loop {
            // Native notifications provide immediate updates; metadata-only
            // reconciliation also covers missed FSEvents and changed homes.
            let mut roots = discovery::roots(&saved.extra_homes);
            roots.extend(discovered.iter().cloned());
            roots.sort();
            roots.dedup();
            watched.retain(|anchor: &PathBuf, _| {
                if anchor.is_dir() {
                    true
                } else {
                    if let Some(watcher) = watcher.as_mut() { let _ = watcher.unwatch(anchor); }
                    false
                }
            });
            for root in &roots {
                if !root.is_dir() {
                    discovered.remove(root);
                }
                let mut anchor = root.clone();
                while !anchor.is_dir() && anchor.pop() {}
                if !anchor.is_dir() {
                    continue;
                }
                let recursive = anchor == *root;
                if watched.get(&anchor) != Some(&recursive) {
                    let mode = if recursive {
                        RecursiveMode::Recursive
                    } else {
                        RecursiveMode::NonRecursive
                    };
                    if watcher.as_mut().is_some_and(|watcher| watcher.watch(&anchor, mode).is_ok()) {
                        watched.insert(anchor, recursive);
                    } else {
                        warning = Some("部分 Codex 文件通知不可用，已改为定时同步".into());
                    }
                }
                if root.is_dir() && discovered.insert(root.clone()) {
                    for file in watcher::session_files(root, initial) {
                        if reader.read(&file, &mut tasks, true).is_err() {
                            warning = Some("部分 Codex 会话暂不可读".into());
                        }
                    }
                }
                if root.is_dir() && reader.reconcile(root, &mut tasks).is_err() {
                    warning = Some("部分 Codex 会话暂不可读".into());
                }
            }
            if initial {
                for (key, old) in &saved.unread {
                    if let Some(task) = tasks.get_mut(key) {
                        if task.status == old.status {
                            task.unread = true;
                        }
                    }
                }
                initial = false;
            }
            saved.unread = tasks
                .iter()
                .filter(|(_, t)| t.unread)
                .map(|(k, t)| (k.clone(), t.clone()))
                .collect();
            if saved != last_saved {
                if save(&path, &saved).is_err() {
                    warning = Some("任务未读状态保存失败".into());
                } else {
                    last_saved = saved.clone();
                }
            }
            publish(&app, &tasks, &warning);
            let message = match receiver.recv_timeout(Duration::from_secs(2)) {
                Ok(m) => m,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => break,
            };
            let mut batch = vec![message];
            // Merge write bursts while bounding delay under sustained output.
            let batch_deadline = Instant::now() + Duration::from_millis(200);
            while batch.len() < 1000 && Instant::now() < batch_deadline {
                let wait = Duration::from_millis(80)
                    .min(batch_deadline.saturating_duration_since(Instant::now()));
                match receiver.recv_timeout(wait) {
                    Ok(m) => batch.push(m),
                    Err(_) => break,
                }
            }
            let mut changed = BTreeSet::new();
            let mut rescan = false;
            let mut check_missing = false;
            for message in batch {
                match message {
                    Message::Acknowledge(old) => {
                        if let Some(t) = tasks.get_mut(&old.key) {
                            if t.status == old.status && t.updated_at == old.updated_at {
                                t.unread = false;
                            }
                        }
                    }
                    Message::AddHome(home, reply) => {
                        let mut next = saved.clone();
                        if !next.extra_homes.contains(&home) {
                            next.extra_homes.push(home);
                        }
                        match save(&path, &next) {
                            Ok(()) => {
                                saved = next;
                                last_saved = saved.clone();
                                let _ = reply.send(Ok(()));
                            }
                            Err(_) => {
                                let _ = reply.send(Err("Codex 目录保存失败，请重试".into()));
                            }
                        }
                    }
                    Message::Fs(Err(_)) => {
                        rescan = true;
                        warning = Some("Codex 文件通知丢失，已重新同步".into());
                    }
                    Message::Fs(Ok(event)) => {
                        check_missing |= matches!(
                            event.kind,
                            notify::EventKind::Remove(_)
                                | notify::EventKind::Modify(notify::event::ModifyKind::Name(_))
                        );
                        if event.need_rescan() {
                            rescan = true;
                        }
                        if matches!(event.kind, notify::EventKind::Access(_)) {
                            continue;
                        }
                        for file in event.paths {
                            if roots.iter().any(|root| file.starts_with(root)) {
                                if file.is_dir() {
                                    changed.extend(watcher::session_files(&file, false));
                                } else if file.extension().is_some_and(|ext| ext == "jsonl") {
                                    changed.insert(file);
                                }
                            }
                        }
                    }
                }
            }
            if rescan {
                for root in &roots {
                    changed.extend(watcher::session_files(root, true));
                }
            }
            if rescan || check_missing {
                reader.forget_missing(&mut tasks);
            }
            for file in changed {
                if file.is_file() && reader.read(&file, &mut tasks, false).is_err() {
                    warning = Some("部分 Codex 会话暂不可读".into());
                }
            }
            // Keep the latest terminal metadata in memory to deduplicate file moves/replays.
            // publish filters read terminals; only unread terminals are persisted.
            if reader.malformed > 0 || reader.unknown > 0 {
                crate::log_line(&format!(
                    "codex activity: skipped {} malformed/oversized records, {} unknown events",
                    reader.malformed, reader.unknown
                ));
                reader.malformed = 0;
                reader.unknown = 0;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_state_roundtrip_does_not_touch_codex_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("codex-activity.json");
        let saved = Saved {
            unread: BTreeMap::from([(
                "test:turn".into(),
                Task {
                    key: "test:turn".into(),
                    thread_id: "01a09809-9dd4-7203-ae17-00575d41934a".into(),
                    turn_id: "turn".into(),
                    model: "test-model".into(),
                    provider: "test-provider".into(),
                    status: Status::Ready,
                    progress: progress::Progress::Ready,
                    progress_title: String::new(),
                    steps: Vec::new(),
                    unread: true,
                    updated_at: "2026-09-16T10:00:00Z".into(),
                },
            )]),
            extra_homes: vec![dir.path().join("test-codex")],
        };
        save(&path, &saved).unwrap();
        save(&path, &saved).unwrap();
        let restored: Saved = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(restored.extra_homes, saved.extra_homes);
        assert_eq!(restored.unread, saved.unread);
        assert!(!dir.path().join("test-codex").exists());
    }
}
