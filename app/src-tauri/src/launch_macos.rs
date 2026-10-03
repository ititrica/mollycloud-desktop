use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize)]
pub struct LaunchResult {
    pub success: bool,
    pub message: String,
    pub resolved: Option<String>,
}

const APP_ALIASES: &[(&[&str], &[&str])] = &[
    (
        &[
            "网易云",
            "网易云音乐",
            "netease",
            "cloudmusic",
            "cloud music",
        ],
        &["网易云音乐", "NeteaseMusic", "CloudMusic"],
    ),
    (&["微信", "wechat", "weixin"], &["WeChat", "微信"]),
    (&["qq", "腾讯qq"], &["QQ"]),
    (
        &["记事本", "notepad", "笔记本", "文本编辑", "textedit"],
        &["TextEdit"],
    ),
    (&["计算器", "calc", "calculator"], &["Calculator"]),
    (
        &["vscode", "vs code", "visual studio code"],
        &["Visual Studio Code"],
    ),
    (&["浏览器", "browser", "safari"], &["Safari"]),
    (
        &["画图", "mspaint", "paint", "预览", "preview"],
        &["Preview"],
    ),
    (
        &[
            "资源管理器",
            "文件管理器",
            "explorer",
            "此电脑",
            "我的电脑",
            "finder",
        ],
        &["Finder"],
    ),
    (
        &["任务管理器", "taskmgr", "活动监视器"],
        &["Activity Monitor"],
    ),
    (
        &["控制面板", "control", "系统设置", "settings"],
        &["System Settings", "System Preferences"],
    ),
    (&["终端", "terminal"], &["Terminal"]),
    (&["截图", "snippingtool"], &["Screenshot"]),
    (&["office", "word"], &["Microsoft Word"]),
    (&["excel"], &["Microsoft Excel"]),
    (&["powerpoint", "ppt"], &["Microsoft PowerPoint"]),
    (&["wps"], &["wpsoffice", "WPS Office"]),
    (&["飞书", "feishu"], &["Feishu", "Lark", "飞书"]),
    (&["钉钉", "dingtalk"], &["DingTalk", "钉钉"]),
    (&["抖音", "douyin"], &["Douyin", "抖音"]),
    (&["哔哩哔哩", "bilibili"], &["bilibili", "哔哩哔哩"]),
];

fn validate_app_name(input: &str) -> Result<&str, String> {
    let name = input.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err("请提供有效的应用名称（最多 64 字）".into());
    }
    if name.starts_with('-')
        || input.chars().any(char::is_control)
        || name.contains(['/', '\\', ';', '|', '&', '`', '$', '>', '<'])
    {
        return Err("应用名包含非法字符".into());
    }
    Ok(name)
}

fn normalized(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn collect_applications(directory: &Path, depth: usize, result: &mut Vec<PathBuf>) {
    if depth > 3 || result.len() >= 600 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "app") {
            result.push(path);
        } else if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            collect_applications(&path, depth + 1, result);
        }
    }
}

fn open_application(path: &std::ffi::OsStr) -> Result<(), String> {
    let output = Command::new("/usr/bin/open")
        .arg("-a")
        .arg(path)
        .output()
        .map_err(|e| format!("无法调用 macOS 应用启动器：{e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err("没有找到该应用，请确认已安装".into())
    }
}

pub fn launch_application_checked(application: String) -> LaunchResult {
    let name = match validate_app_name(&application) {
        Ok(name) => name,
        Err(message) => {
            return LaunchResult {
                success: false,
                message,
                resolved: None,
            }
        }
    };
    if name == "浏览器" || name.eq_ignore_ascii_case("browser") {
        use objc2_app_kit::NSWorkspace;
        use objc2_foundation::{NSString, NSURL};
        // This only asks Launch Services which app handles HTTPS. It performs
        // no network request and does not open a website.
        let url = NSURL::URLWithString(&NSString::from_str("https://example.test"));
        let browser = url
            .as_deref()
            .and_then(|url| NSWorkspace::sharedWorkspace().URLForApplicationToOpenURL(url));
        if let Some(path) = browser
            .and_then(|app| app.path())
            .map(|path| path.to_string())
        {
            if open_application(std::ffi::OsStr::new(&path)).is_ok() {
                return LaunchResult {
                    success: true,
                    message: "已打开默认浏览器".into(),
                    resolved: Some(path),
                };
            }
        }
    }
    let candidates: Vec<&str> = APP_ALIASES
        .iter()
        .find(|(aliases, _)| aliases.iter().any(|alias| alias.eq_ignore_ascii_case(name)))
        .map(|(_, names)| names.to_vec())
        .unwrap_or_else(|| vec![name]);
    let mut apps = Vec::new();
    for root in [
        "/Applications",
        "/System/Applications",
        "/System/Library/CoreServices",
    ] {
        collect_applications(Path::new(root), 0, &mut apps);
    }
    if let Some(home) = std::env::var_os("HOME") {
        collect_applications(&PathBuf::from(home).join("Applications"), 0, &mut apps);
    }
    apps.sort();
    for candidate in &candidates {
        if let Some(path) = apps.iter().find(|path| {
            path.file_stem()
                .is_some_and(|stem| normalized(&stem.to_string_lossy()) == normalized(candidate))
        }) {
            if open_application(path.as_os_str()).is_ok() {
                crate::log_line(&format!("launch: {name} -> {}", path.display()));
                return LaunchResult {
                    success: true,
                    message: format!("已打开 {name}"),
                    resolved: Some(path.display().to_string()),
                };
            }
        }
    }
    // Launch Services also locates applications installed outside standard folders.
    for candidate in candidates {
        if open_application(std::ffi::OsStr::new(candidate)).is_ok() {
            return LaunchResult {
                success: true,
                message: format!("已打开 {name}"),
                resolved: Some(candidate.into()),
            };
        }
    }
    LaunchResult {
        success: false,
        message: format!("没有找到 {name}，请确认已安装对应的 Mac 应用"),
        resolved: None,
    }
}

fn open_target(target: &str) -> Result<(), String> {
    let status = Command::new("/usr/bin/open")
        .arg("--")
        .arg(target)
        .status()
        .map_err(|e| format!("打开失败：{e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("macOS 未能打开链接".into())
    }
}

pub fn open_url(raw: &str) -> Result<(), String> {
    let parsed = url::Url::parse(raw).map_err(|_| "链接格式无效")?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("仅支持有效的 http/https 链接".into());
    }
    open_target(parsed.as_str())
}

pub(crate) fn open_codex_thread(id: &str) -> Result<(), String> {
    if id.len() != 36
        || !id.bytes().enumerate().all(|(index, c)| {
            if [8, 13, 18, 23].contains(&index) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
    {
        return Err("Codex 任务标识无效".into());
    }
    open_target(&format!("codex://threads/{id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn app_names_cannot_be_paths_or_open_options() {
        for name in [
            "-a Terminal",
            "/bin/sh",
            "Calculator; open",
            "Finder\n",
            "$(touch /tmp/x)",
        ] {
            assert!(validate_app_name(name).is_err());
        }
        assert!(validate_app_name("微信").is_ok());
        assert!(validate_app_name("Visual Studio Code").is_ok());
    }
}
