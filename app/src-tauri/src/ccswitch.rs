//! Launch supported CLIs against the same real configuration managed by CC Switch.
//! Credentials stay in provider files, never in shell text or command arguments.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, WebviewWindow};

const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
const START_CLI: &str = "& $env:MOLLY_CLI_PROGRAM; if ($LASTEXITCODE -ne 0) { Write-Host ('CLI exited with code ' + $LASTEXITCODE) }";

// Inherited API credentials could take precedence over the selected provider.
const AUTH_ENV: &[&str] = &[
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "OPENAI_ORG_ID",
    "OPENAI_ORGANIZATION",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
    "ANTHROPIC_FOUNDRY_API_KEY",
    "ANTHROPIC_FOUNDRY_BASE_URL",
    "ANTHROPIC_FOUNDRY_RESOURCE",
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "GOOGLE_GEMINI_BASE_URL",
    "GOOGLE_GENAI_USE_VERTEXAI",
    "GOOGLE_GENAI_USE_GCA",
    "GOOGLE_APPLICATION_CREDENTIALS",
];

fn cli_name(app_type: &str) -> Result<&'static str, String> {
    match app_type {
        "codex" => Ok("codex"),
        "claude" => Ok("claude"),
        "gemini" => Ok("gemini"),
        _ => Err("支持启动 Claude Code、Codex 和 Gemini CLI".into()),
    }
}

fn find_cli(name: &str, search_path: &std::ffi::OsStr) -> Result<PathBuf, String> {
    for directory in std::env::split_paths(search_path) {
        // Never resolve an executable against the user-selected project directory.
        if !directory.is_absolute() {
            continue;
        }
        for extension in ["exe", "cmd", "bat", "ps1"] {
            let candidate = directory.join(format!("{name}.{extension}"));
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(format!(
        "未找到 {name}，请先安装对应 CLI 并将其加入 PATH，再重启 MollyCloud"
    ))
}

fn working_directory(system_home: &Path, requested: Option<&str>) -> Result<PathBuf, String> {
    let directory = match requested.filter(|value| !value.trim().is_empty()) {
        Some(value) => {
            if value.chars().any(char::is_control) || !Path::new(value).is_absolute() {
                return Err("项目目录必须是有效的绝对路径".into());
            }
            PathBuf::from(value)
        }
        None => system_home.to_path_buf(),
    };
    if !directory.is_dir() {
        return Err("项目目录不存在或不是文件夹".into());
    }
    Ok(directory)
}

fn launch_command(
    executable: &Path,
    config_env: &[(String, Option<PathBuf>)],
    cwd: &Path,
) -> Command {
    let system_root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let powershell = system_root.join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let mut command = Command::new(powershell);
    command.args(["-NoLogo", "-NoProfile", "-NoExit", "-Command", START_CLI]);
    command.creation_flags(CREATE_NEW_CONSOLE);
    command.current_dir(cwd);
    command.env("MOLLY_CLI_PROGRAM", executable);
    for (name, path) in config_env {
        if let Some(path) = path {
            command.env(name, path);
        } else {
            command.env_remove(name);
        }
    }
    for variable in AUTH_ENV {
        command.env_remove(variable);
    }
    command
}

#[tauri::command]
pub async fn launch_ccswitch_cli(
    app_handle: AppHandle,
    window: WebviewWindow,
    app_type: String,
    provider_id: String,
    cwd: Option<String>,
) -> Result<bool, String> {
    if window.label() != "console" {
        return Err("只能从控制台启动内置 CLI".into());
    }
    let name = cli_name(&app_type)?;
    let path = std::env::var_os("PATH").unwrap_or_default();
    let executable = find_cli(name, &path)?;
    tauri::async_runtime::spawn_blocking(move || {
        let system_home = molly_ccswitch::cli_system_home()?;
        let directory = working_directory(&system_home, cwd.as_deref())?;
        // Resolve paths before activation: invalid launch settings cannot change the provider.
        let config_env = molly_ccswitch::cli_launch_environment(&app_type)?;
        molly_ccswitch::prepare_cli_launch(&app_handle, &app_type, &provider_id)?;
        launch_command(&executable, &config_env, &directory)
            .spawn()
            .map_err(|e| format!("无法启动 {name}：{e}"))?;
        // This acknowledges the terminal process. CLI startup/runtime errors are
        // reported by that terminal; spawning PowerShell is not a CLI health check.
        Ok(true)
    })
    .await
    .map_err(|_| "启动 CLI 的任务未完成".to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_text_contains_no_interpolated_program_or_project() {
        let command = launch_command(
            Path::new(r"C:\test & space\codex.cmd"),
            &[(
                "CODEX_HOME".into(),
                Some(PathBuf::from(r"C:\Users\Test\.codex")),
            )],
            Path::new(r"C:\project 'quoted'"),
        );
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args.last().unwrap(), START_CLI);
        assert!(!args
            .iter()
            .any(|arg| arg.contains("test & space") || arg.contains("quoted")));
        let environment: std::collections::HashMap<_, _> = command.get_envs().collect();
        assert_eq!(
            environment[std::ffi::OsStr::new("CODEX_HOME")],
            Some(std::ffi::OsStr::new(r"C:\Users\Test\.codex"))
        );
        assert!(!environment.contains_key(std::ffi::OsStr::new("GEMINI_CLI_HOME")));
        assert!(!environment.contains_key(std::ffi::OsStr::new("CLAUDE_CONFIG_DIR")));
        assert_eq!(environment[std::ffi::OsStr::new("OPENAI_API_KEY")], None);
        assert!(!environment.contains_key(std::ffi::OsStr::new("HOME")));
        assert!(!environment.contains_key(std::ffi::OsStr::new("USERPROFILE")));
    }

    #[test]
    fn rejects_arbitrary_program_names_and_relative_workdirs() {
        assert!(cli_name("powershell").is_err());
        assert!(cli_name("codex;calc").is_err());
        assert!(working_directory(Path::new(r"C:\private"), Some("relative")).is_err());
        assert!(working_directory(Path::new(r"C:\private"), Some("C:\\bad\npath")).is_err());
    }

    #[test]
    fn clears_auth_overrides_without_rewriting_other_tools_paths() {
        let command = launch_command(
            Path::new(r"C:\tools\codex.cmd"),
            &[(
                "CODEX_HOME".into(),
                Some(PathBuf::from(r"C:\Users\Test\.codex")),
            )],
            Path::new(r"C:\Molly Private\home\workspace"),
        );
        let environment: std::collections::HashMap<_, _> = command.get_envs().collect();
        for name in AUTH_ENV.iter() {
            assert_eq!(
                environment.get(std::ffi::OsStr::new(name)),
                Some(&None),
                "inherited {name} must be removed from the child"
            );
        }
        // No process-global HOME/USERPROFILE rewrite is required by these CLIs.
        for name in ["HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA"] {
            assert!(!environment.contains_key(std::ffi::OsStr::new(name)));
        }
    }

    #[test]
    fn finds_absolute_cli_shims_without_running_them() {
        let fixture = tempfile::tempdir().unwrap();
        let tools_dir = fixture.path().join("tools & space");
        std::fs::create_dir(&tools_dir).unwrap();
        let shim = tools_dir.join("codex.cmd");
        std::fs::write(&shim, b"fixture only; never execute").unwrap();
        let search_path =
            std::env::join_paths([Path::new("relative-path"), tools_dir.as_path()]).unwrap();
        assert_eq!(find_cli("codex", &search_path).unwrap(), shim);
        assert!(find_cli("gemini", &search_path).is_err());
    }

    #[test]
    fn rejects_missing_workdirs_and_uses_existing_system_home() {
        let fixture = tempfile::tempdir().unwrap();
        let missing = fixture.path().join("missing-project");
        assert!(working_directory(fixture.path(), missing.to_str()).is_err());
        assert!(!missing.exists());
        let default = working_directory(fixture.path(), None).unwrap();
        assert_eq!(default, fixture.path());
        assert!(!fixture.path().join("workspace").exists());
        assert!(default.is_dir());
    }
}
