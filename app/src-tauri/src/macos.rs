//! macOS assistant tools. Credentials remain native and queries never use a shell.
use std::{process::Command, sync::atomic::{AtomicU64, Ordering}};
use tauri::{AppHandle, Emitter};

pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
    crate::credential_store::store()?.seal(data)
}
pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
    crate::credential_store::store()?.unseal(data)
}

pub fn active_application() -> String {
    // Application identity is available without granting access to other apps' UI.
    objc2_app_kit::NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .and_then(|app| app.localizedName())
        .map(|name| name.to_string())
        .unwrap_or_default()
}

pub fn idle_seconds() -> u64 {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
    }
    let seconds = unsafe { CGEventSourceSecondsSinceLastEventType(1, u32::MAX) };
    if seconds.is_finite() && seconds >= 0.0 {
        seconds as u64
    } else {
        0
    }
}

fn applescript(script: &str) -> Result<(), String> {
    let output = Command::new("/usr/bin/osascript")
        .args(["-e", script])
        .output()
        .map_err(|_| "无法调用 macOS 系统服务")?;
    if output.status.success() {
        Ok(())
    } else {
        Err("macOS 未完成系统操作，请检查系统设置中的自动化权限".into())
    }
}

pub fn set_volume(level: Option<u8>, mute: Option<bool>) -> Result<String, String> {
    let level = level.unwrap_or(50).min(100);
    let script = match mute {
        Some(true) => "set volume output muted true".to_owned(),
        Some(false) => format!("set volume output volume {level} without output muted"),
        None => format!("set volume output volume {level}"),
    };
    applescript(&script)?;
    Ok(if mute == Some(true) {
        "已静音".into()
    } else {
        format!("音量已设为 {level}%")
    })
}

pub fn send_notification(app: &AppHandle, title: &str, body: &str) {
    // Native notification plugin obeys the user's macOS notification preferences.
    use tauri_plugin_notification::NotificationExt;
    if app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .is_err()
    {
        let _ = app.emit(
            "notification:fallback",
            serde_json::json!({"title":title,"body":body}),
        );
    }
}

static SHUTDOWN_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static SHUTDOWN_ACTIVE: AtomicU64 = AtomicU64::new(0);

pub fn schedule_shutdown(app: AppHandle, minutes: u32) -> Result<String, String> {
    if !(1..=1440).contains(&minutes) {
        return Err("时间范围：1~1440 分钟".into());
    }
    let task = SHUTDOWN_SEQUENCE.fetch_add(1, Ordering::SeqCst);
    SHUTDOWN_ACTIVE.store(task, Ordering::SeqCst);
    std::thread::spawn(move || {
        // Cancelled tasks stop within one second; closing MollyCloud cancels the timer.
        for _ in 0..minutes * 60 {
            std::thread::sleep(std::time::Duration::from_secs(1));
            if SHUTDOWN_ACTIVE.load(Ordering::SeqCst) != task {
                return;
            }
        }
        if SHUTDOWN_ACTIVE
            .compare_exchange(task, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            if let Err(error) = applescript("tell application \"System Events\" to shut down") {
                send_notification(&app, "定时关机未完成", &error);
            }
        }
    });
    Ok(format!(
        "已设定 {minutes} 分钟后关机；保持 MollyCloud 运行，说「取消关机」可取消"
    ))
}

pub fn cancel_shutdown() -> Result<String, String> {
    if SHUTDOWN_ACTIVE.swap(0, Ordering::SeqCst) != 0 {
        Ok("已取消定时关机".into())
    } else {
        Err("没有待取消的关机任务".into())
    }
}

pub fn query_command(text: &str) -> Result<Command, String> {
    if text
        .chars()
        .any(|c| c.is_control() || "&|><`$;%^".contains(c))
    {
        return Err("命令被拦截（不允许链式、重定向、变量或控制字符）".into());
    }
    let mut parts = shell_words::split(text).map_err(|_| "命令引号不完整")?;
    if parts.is_empty() {
        return Err("命令为空".into());
    }
    let name = parts.remove(0);
    let program = match name.as_str() {
        "ls" => "/bin/ls",
        "cat" => "/bin/cat",
        "pwd" => "/bin/pwd",
        "echo" => "/bin/echo",
        "ps" => "/bin/ps",
        "df" => "/bin/df",
        "date" => "/bin/date",
        "hostname" => "/bin/hostname",
        "ifconfig" => "/sbin/ifconfig",
        "ping" => "/sbin/ping",
        "netstat" => "/usr/sbin/netstat",
        "traceroute" => "/usr/sbin/traceroute",
        "arp" => "/usr/sbin/arp",
        "nslookup" => "/usr/bin/nslookup",
        "uname" => "/usr/bin/uname",
        "sw_vers" => "/usr/bin/sw_vers",
        "whoami" => "/usr/bin/whoami",
        "head" => "/usr/bin/head",
        "tail" => "/usr/bin/tail",
        "wc" => "/usr/bin/wc",
        "du" => "/usr/bin/du",
        "top" => "/usr/bin/top",
        "uptime" => "/usr/bin/uptime",
        _ => return Err(format!("命令被拦截（不在查询白名单内：{name}）")),
    };
    match name.as_str() {
        "hostname" if !parts.is_empty() => return Err("hostname 只允许查询".into()),
        "date" if !parts.is_empty() && (parts.len() != 1 || !parts[0].starts_with('+')) => {
            return Err("date 只允许查询日期".into())
        }
        "ifconfig" if !parts.is_empty() && parts != ["-a"] && parts != ["-l"] => {
            return Err("ifconfig 只允许查看接口（-a / -l）".into())
        }
        "arp" if parts != ["-a"] => return Err("arp 只允许查看缓存（-a）".into()),
        "ping" if !parts.iter().any(|arg| arg == "-c" || arg.starts_with("-c")) => {
            parts.splice(0..0, ["-c".into(), "4".into()]);
        }
        "top" if !parts.iter().any(|arg| arg == "-l" || arg.starts_with("-l")) => {
            parts.splice(0..0, ["-l".into(), "1".into()]);
        }
        _ => {}
    }
    let mut command = Command::new(program);
    command.args(parts);
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_data_authenticates_and_never_contains_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let store = molly_local_secrets::SecretStore::new(dir.path());
        let input = b"synthetic-private-key";
        let mut encrypted = store.seal(input).unwrap();
        assert!(!encrypted.windows(input.len()).any(|bytes| bytes == input));
        assert_eq!(store.unseal(&encrypted).unwrap(), input);
        let last = encrypted.len() - 1;
        encrypted[last] ^= 1;
        assert!(store.unseal(&encrypted).is_err());

        assert!(store.unseal(b"truncated").is_err());
    }
    #[test]
    fn query_tools_cannot_mutate_settings_or_execute_shell_expressions() {
        for text in [
            "ls; rm -rf /",
            "echo $(touch /tmp/no)",
            "ls\nwhoami",
            "hostname changed",
            "date 10031200",
            "ifconfig en0 down",
            "arp -d -a",
            "/tmp/ls",
            "sh -c ls",
        ] {
            assert!(query_command(text).is_err(), "accepted: {text}");
        }
        assert!(query_command("uname -a").is_ok());
        assert!(query_command("ls \"/tmp/a directory\"").is_ok());
        assert_eq!(
            query_command("ping localhost")
                .unwrap()
                .get_args()
                .collect::<Vec<_>>(),
            ["-c", "4", "localhost"]
        );
    }
}
