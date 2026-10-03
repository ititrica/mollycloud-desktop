//! Launch supported CLIs against the same real configuration managed by CC Switch.
//! Credentials stay in provider files, never in shell text or command arguments.

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, WebviewWindow};

#[cfg(windows)]
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
#[cfg(windows)]
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
        #[cfg(windows)]
        for extension in ["exe", "cmd", "bat", "ps1"] {
            let candidate = directory.join(format!("{name}.{extension}"));
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::fs::PermissionsExt;
            let candidate = directory.join(name);
            if candidate.metadata().is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            }) {
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

#[cfg(windows)]
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

#[cfg(target_os = "macos")]
fn cli_search_path() -> std::ffi::OsString {
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let mut directories: Vec<PathBuf> = std::env::split_paths(&inherited)
        .filter(|path| path.is_absolute())
        .collect();
    for directory in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ] {
        directories.push(PathBuf::from(directory));
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for relative in [
            ".local/bin",
            ".npm-global/bin",
            ".volta/bin",
            "Library/pnpm",
            ".asdf/shims",
            ".cargo/bin",
        ] {
            directories.push(home.join(relative));
        }
        if let Ok(versions) = std::fs::read_dir(home.join(".nvm/versions/node")) {
            let mut versions: Vec<_> = versions
                .flatten()
                .map(|entry| entry.path().join("bin"))
                .collect();
            versions.sort_by(|a, b| b.cmp(a));
            directories.extend(versions);
        }
    }
    std::env::join_paths(directories).unwrap_or(inherited)
}

#[cfg(target_os = "macos")]
fn shell_quote(value: &std::ffi::OsStr) -> String {
    format!("'{}'", value.to_string_lossy().replace('\'', "'\\''"))
}

#[cfg(target_os = "macos")]
fn terminal_script(
    executable: &Path,
    config_env: &[(String, Option<PathBuf>)],
    cwd: &Path,
    path: &std::ffi::OsStr,
) -> Result<String, String> {
    // The same Terminal instance may inherit old provider credentials. Clear
    // overrides in the actual shell that starts the CLI, not just /usr/bin/open.
    let mut script = String::from("#!/bin/zsh\n/bin/rm -f -- \"$0\"\n");
    script.push_str(&format!(
        "cd -- {} || exit 1\nexport PATH={}\n",
        shell_quote(cwd.as_os_str()),
        shell_quote(path)
    ));
    for name in AUTH_ENV {
        script.push_str(&format!("unset {name}\n"));
    }
    for (name, value) in config_env {
        if !matches!(
            name.as_str(),
            "CODEX_HOME" | "CLAUDE_CONFIG_DIR" | "GEMINI_CLI_HOME"
        ) {
            return Err("不支持的 CLI 配置目录变量".into());
        }
        match value {
            Some(value) => script.push_str(&format!(
                "export {name}={}\n",
                shell_quote(value.as_os_str())
            )),
            None => script.push_str(&format!("unset {name}\n")),
        }
    }
    script.push_str(&format!("{}\nmolly_cli_status=$?\nif (( molly_cli_status != 0 )); then printf '\\nCLI exited with code %s\\n' \"$molly_cli_status\"; fi\n", shell_quote(executable.as_os_str())));
    // Keep a normal interactive terminal available after the CLI exits.
    script.push_str("exec /bin/zsh -i\n");
    Ok(script)
}

#[cfg(target_os = "macos")]
fn create_terminal_launcher(script: &str) -> Result<PathBuf, String> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    let mut launcher = tempfile::Builder::new()
        .prefix("MollyCloud-")
        .suffix(".command")
        .tempfile()
        .map_err(|e| e.to_string())?;
    launcher
        .write_all(script.as_bytes())
        .map_err(|e| e.to_string())?;
    launcher
        .as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    let (_, path) = launcher.keep().map_err(|e| e.to_string())?;
    Ok(path)
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
    crate::console_plugins::require(&app_handle, "ccswitch")?;
    let name = cli_name(&app_type)?;
    #[cfg(windows)]
    let path = std::env::var_os("PATH").unwrap_or_default();
    #[cfg(target_os = "macos")]
    let path = cli_search_path();
    let executable = find_cli(name, &path)?;
    tauri::async_runtime::spawn_blocking(move || {
        let system_home = molly_ccswitch::cli_system_home()?;
        let directory = working_directory(&system_home, cwd.as_deref())?;
        // Resolve paths before activation: invalid launch settings cannot change the provider.
        let config_env = molly_ccswitch::cli_launch_environment(&app_type)?;
        #[cfg(windows)]
        {
            molly_ccswitch::prepare_cli_launch(&app_handle, &app_type, &provider_id)?;
            launch_command(&executable, &config_env, &directory)
                .spawn()
                .map_err(|e| format!("无法启动 {name}：{e}"))?;
        }
        #[cfg(target_os = "macos")]
        {
            let launcher = create_terminal_launcher(&terminal_script(
                &executable,
                &config_env,
                &directory,
                &path,
            )?)?;
            if let Err(error) =
                molly_ccswitch::prepare_cli_launch(&app_handle, &app_type, &provider_id)
            {
                let _ = std::fs::remove_file(&launcher);
                return Err(error);
            }
            let status = Command::new("/usr/bin/open")
                .args(["-a", "Terminal"])
                .arg(&launcher)
                .status();
            if !status.is_ok_and(|status| status.success()) {
                let _ = std::fs::remove_file(&launcher);
                return Err(format!("无法在 macOS 终端启动 {name}"));
            }
        }
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
    #[cfg(windows)]
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
    #[cfg(windows)]
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
        #[cfg(windows)]
        let shim = tools_dir.join("codex.cmd");
        #[cfg(target_os = "macos")]
        let shim = tools_dir.join("codex");
        std::fs::write(&shim, b"fixture only; never execute").unwrap();
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
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

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_launcher_quotes_paths_and_clears_credentials() {
        let script = terminal_script(
            Path::new("/tmp/tools 'quoted'/codex"),
            &[(
                "CODEX_HOME".into(),
                Some(PathBuf::from("/tmp/private .codex")),
            )],
            Path::new("/tmp/project $(touch unwanted)"),
            std::ffi::OsStr::new("/usr/bin:/bin"),
        )
        .unwrap();
        assert!(script.contains("'/tmp/tools '\\''quoted'\\''/codex'"));
        assert!(script.contains("cd -- '/tmp/project $(touch unwanted)'"));
        assert!(script.contains("export CODEX_HOME='/tmp/private .codex'"));
        for name in AUTH_ENV {
            assert!(script.contains(&format!("unset {name}\n")));
        }
        assert!(!script.contains("export HOME="));
        assert!(terminal_script(
            Path::new("/tmp/codex"),
            &[("HOME".into(), Some(PathBuf::from("/tmp/other")))],
            Path::new("/tmp"),
            std::ffi::OsStr::new("/bin")
        )
        .is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_launcher_runs_mock_cli_without_shell_expansion_or_auth_override() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = tempfile::tempdir().unwrap();
        let project = fixture.path().join("project 'quoted' $(touch injected)");
        std::fs::create_dir(&project).unwrap();
        let executable = fixture.path().join("mock 'codex'");
        let output = fixture.path().join("observed");
        // Only a mock executable and fake credentials are used; no real CLI
        // configuration is read or switched by this regression test.
        std::fs::write(&executable, "#!/bin/zsh -f\nprintf '%s\\n' \"$PWD\" \"${OPENAI_API_KEY-unset}\" \"$CODEX_HOME\" > \"$MOLLY_MOCK_OUTPUT\"\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let config = fixture.path().join("mock .codex");
        let script = terminal_script(
            &executable,
            &[("CODEX_HOME".into(), Some(config.clone()))],
            &project,
            std::ffi::OsStr::new("/usr/bin:/bin"),
        )
        .unwrap();
        let script = script.replace("exec /bin/zsh -i\n", "exit $molly_cli_status\n");
        let launcher = fixture.path().join("fixture.command");
        std::fs::write(&launcher, script).unwrap();
        let status = Command::new("/bin/zsh")
            .arg("-f")
            .arg(&launcher)
            .env("OPENAI_API_KEY", "mock-do-not-use")
            .env("MOLLY_MOCK_OUTPUT", &output)
            .env("HOME", fixture.path())
            .current_dir(fixture.path())
            .status()
            .unwrap();
        assert!(status.success());
        let observed = std::fs::read_to_string(output).unwrap();
        let values: Vec<_> = observed.lines().collect();
        // /var is a symlink to /private/var on macOS; zsh reports a physical cwd.
        assert_eq!(
            std::fs::canonicalize(values[0]).unwrap(),
            std::fs::canonicalize(&project).unwrap()
        );
        assert_eq!(values[1], "unset");
        assert_eq!(values[2], config.to_str().unwrap());
        assert!(!fixture.path().join("injected").exists());
        assert!(!launcher.exists());
    }
}
