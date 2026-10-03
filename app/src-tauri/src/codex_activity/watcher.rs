use super::{
    parser,
    reducer::{self, Context, Tasks},
};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::SystemTime,
};

const MAX_LINE: usize = 1024 * 1024;

#[derive(Default)]
struct Cursor {
    offset: u64,
    pending: Vec<u8>,
    oversized: bool,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    context: Context,
}

pub struct Reader {
    files: HashMap<PathBuf, Cursor>,
    pub malformed: u64,
    pub unknown: u64,
    started_at: i64,
}

impl Default for Reader {
    fn default() -> Self {
        Self {
            files: HashMap::new(),
            malformed: 0,
            unknown: 0,
            started_at: chrono::Utc::now().timestamp_millis(),
        }
    }
}

impl Reader {
    /// Reconcile metadata after missed/coalesced native notifications. Unchanged
    /// rollouts are never opened or replayed, including long-running turns.
    pub fn reconcile(&mut self, root: &Path, tasks: &mut Tasks) -> std::io::Result<()> {
        for path in session_files(root, true) {
            let meta = fs::metadata(&path)?;
            let changed = self.files.get(&path).is_none_or(|cursor| {
                cursor.offset != meta.len() || cursor.modified != meta.modified().ok()
                    || cursor.created != meta.created().ok()
            });
            if changed { self.read(&path, tasks, false)?; }
        }
        self.forget_missing(tasks);
        Ok(())
    }

    pub fn read(&mut self, path: &Path, tasks: &mut Tasks, baseline: bool) -> std::io::Result<()> {
        let meta = fs::metadata(path)?;
        if !meta.is_file() {
            return Ok(());
        }
        let cursor = self.files.entry(path.to_path_buf()).or_default();
        let modified = meta.modified().ok();
        let created = meta.created().ok();
        if meta.len() < cursor.offset
            || (cursor.offset > 0 && meta.len() == cursor.offset && modified != cursor.modified)
            || (cursor.created.is_some() && created != cursor.created)
        {
            tasks.retain(|_, t| t.thread_id != cursor.context.thread);
            *cursor = Cursor::default();
        }
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(cursor.offset))?;
        // Read only bytes present when the event arrived, even if the producer keeps writing.
        let mut remaining = meta.len().saturating_sub(cursor.offset);
        let mut buffer = [0u8; 64 * 1024];
        while remaining > 0 {
            let limit = remaining.min(buffer.len() as u64) as usize;
            let count = file.read(&mut buffer[..limit])?;
            if count == 0 {
                break;
            }
            remaining -= count as u64;
            cursor.offset += count as u64;
            for byte in &buffer[..count] {
                if *byte == b'\n' {
                    if !cursor.oversized && !cursor.pending.is_empty() {
                        if let Some(record) = parser::decode(&cursor.pending) {
                            if parser::unknown_event(&record) {
                                self.unknown += 1;
                            }
                            let historical = baseline
                                && parser::timestamp(&record.timestamp)
                                    .is_none_or(|t| t < self.started_at);
                            reducer::apply(&mut cursor.context, tasks, record, historical);
                        } else {
                            self.malformed += 1;
                        }
                    }
                    cursor.pending.clear();
                    cursor.oversized = false;
                } else if !cursor.oversized {
                    if cursor.pending.len() == MAX_LINE {
                        cursor.pending.clear();
                        cursor.oversized = true;
                        self.malformed += 1;
                    } else {
                        cursor.pending.push(*byte);
                    }
                }
            }
        }
        cursor.modified = modified;
        cursor.created = created;
        Ok(())
    }

    pub fn forget_missing(&mut self, tasks: &mut Tasks) {
        self.files.retain(|path, cursor| {
            if path.exists() {
                return true;
            }
            tasks.retain(|_, t| t.thread_id != cursor.context.thread || !t.status.active());
            false
        });
    }
}

pub fn session_files(root: &Path, recent_only: bool) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                dirs.push(entry.path());
            } else if kind.is_file() && entry.path().extension().is_some_and(|ext| ext == "jsonl") {
                let recent = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age.as_secs() < 7 * 86400);
                if !recent_only || recent {
                    result.push(entry.path());
                }
            }
        }
    }
    result.sort();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn missed_file_notifications_still_update_a_running_turn_and_completion() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("rollout.jsonl");
        fs::write(&file, "{\"type\":\"session_meta\",\"payload\":{\"id\":\"01a09809-9dd4-7203-ae17-00575d41934a\"}}\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"a\"}}\n").unwrap();
        let mut reader = Reader::default();
        let mut tasks = Tasks::new();
        reader.read(&file, &mut tasks, true).unwrap();
        let mut output = fs::OpenOptions::new().append(true).open(&file).unwrap();
        writeln!(output, "{}", serde_json::json!({"type":"event_msg","payload":{"type":"item_completed","turn_id":"a","item":{"type":"CommandExecution","exit_code":0}}})).unwrap();
        reader.reconcile(dir.path(), &mut tasks).unwrap();
        assert_eq!(tasks.values().next().unwrap().progress, super::super::progress::Progress::CommandComplete);
        writeln!(output, "{}", serde_json::json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"a"}})).unwrap();
        reader.reconcile(dir.path(), &mut tasks).unwrap();
        assert_eq!(tasks.values().next().unwrap().status, reducer::Status::Ready);
        assert!(tasks.values().next().unwrap().unread);
        tasks.values_mut().next().unwrap().unread = false;
        reader.reconcile(dir.path(), &mut tasks).unwrap();
        assert!(!tasks.values().next().unwrap().unread);
    }
    #[test]
    fn completion_during_bootstrap_is_unread_but_old_completion_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("session.jsonl");
        let mut reader = Reader {
            started_at: parser::timestamp("2026-09-16T10:00:00Z").unwrap(),
            ..Default::default()
        };
        let meta = "{\"type\":\"session_meta\",\"payload\":{\"id\":\"01a09809-9dd4-7203-ae17-00575d41934a\"}}\n";
        let records = format!("{meta}{{\"timestamp\":\"2026-09-16T09:00:00Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_started\",\"turn_id\":\"a\"}}}}\n{{\"timestamp\":\"2026-09-16T10:00:01Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_complete\",\"turn_id\":\"a\"}}}}\n");
        fs::write(&file, &records).unwrap();
        let mut tasks = Tasks::new();
        reader.read(&file, &mut tasks, true).unwrap();
        assert!(tasks.values().next().unwrap().unread);
        let old = dir.path().join("old.jsonl");
        fs::write(
            &old,
            records
                .replace("01a09809", "01a09810")
                .replace("10:00:01", "09:01:00"),
        )
        .unwrap();
        reader.read(&old, &mut tasks, true).unwrap();
        assert_eq!(tasks.values().filter(|t| t.unread).count(), 1);
    }
    #[test]
    fn native_notifications_detect_nested_session_creation() {
        use notify::{RecursiveMode, Watcher};
        use std::{
            sync::mpsc,
            time::{Duration, Instant},
        };
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = tx.send(event);
        })
        .unwrap();
        watcher.watch(dir.path(), RecursiveMode::Recursive).unwrap();
        let nested = dir.path().join("2026/09/16");
        fs::create_dir_all(&nested).unwrap();
        let file = nested.join("rollout.jsonl");
        fs::write(&file, b"{}\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut detected = false;
        while Instant::now() < deadline {
            if let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(100)) {
                if event
                    .paths
                    .iter()
                    .any(|p| p.file_name() == file.file_name())
                {
                    detected = true;
                    break;
                }
            }
        }
        assert!(
            detected,
            "native recursive watcher must report new session files"
        );
    }

    #[test]
    fn malformed_lines_do_not_block_later_lifecycle_events() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.jsonl");
        let mut bytes = b"bad json\n".to_vec();
        bytes.extend(vec![b'x'; MAX_LINE + 5]);
        bytes.push(b'\n');
        bytes.extend(b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"01a09809-9dd4-7203-ae17-00575d41934a\"}}\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"ok\"}}\n");
        fs::write(&file, bytes).unwrap();
        let mut reader = Reader::default();
        let mut tasks = Tasks::new();
        reader.read(&file, &mut tasks, false).unwrap();
        assert_eq!(reader.malformed, 2);
        assert_eq!(tasks.len(), 1);
    }
    #[test]
    fn partial_lines_truncation_and_multiple_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        let meta = "{\"type\":\"session_meta\",\"payload\":{\"id\":\"01a09809-9dd4-7203-ae17-00575d41934a\"}}\n";
        fs::write(&path, format!("{meta}{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_started\",\"turn_id\":\"1\"}}}}\n")).unwrap();
        let mut r = Reader::default();
        let mut tasks = Tasks::new();
        r.read(&path, &mut tasks, true).unwrap();
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_")
            .unwrap();
        r.read(&path, &mut tasks, false).unwrap();
        assert_eq!(
            tasks.values().next().unwrap().status,
            reducer::Status::Running
        );
        f.write_all(b"complete\"}}\n").unwrap();
        r.read(&path, &mut tasks, false).unwrap();
        assert!(tasks.values().next().unwrap().unread);
        r.read(&path, &mut tasks, false).unwrap();
        assert_eq!(tasks.len(), 1);
        drop(f);
        fs::write(&path, meta).unwrap();
        r.read(&path, &mut tasks, false).unwrap();
        assert!(tasks.is_empty());
        let other = dir.path().join("other.jsonl");
        fs::write(&other, format!("{}{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_started\",\"turn_id\":\"2\"}}}}\n", meta.replace("01a09809", "01a09810"))).unwrap();
        r.read(&other, &mut tasks, false).unwrap();
        assert_eq!(tasks.len(), 1);
        fs::remove_file(other).unwrap();
        r.forget_missing(&mut tasks);
        assert!(tasks.is_empty());
    }
}
