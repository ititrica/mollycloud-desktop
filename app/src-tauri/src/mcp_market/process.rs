use super::catalog::{Entry, Recipe};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub fn find_program(name: &str) -> Option<PathBuf> {
    let path = Path::new(name);
    if path.is_absolute() {
        return path.is_file().then(|| path.into());
    }
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .filter(|p| p.is_absolute())
        .map(|p| {
            p.join(if name.ends_with(".exe") {
                name.to_owned()
            } else {
                format!("{name}.exe")
            })
        })
        .find(|p| p.is_file())
}
pub fn command(program: &Path) -> Command {
    let mut c = Command::new(program);
    c.creation_flags(0x08000000);
    c
}
// Temporary installation/diagnostic children die together, including on timeout.
struct Job(windows::Win32::Foundation::HANDLE);
impl Job {
    fn assign(child: &Child) -> Result<Self, String> {
        use windows::Win32::System::JobObjects::*;
        unsafe {
            let handle = CreateJobObjectW(None, None).map_err(|_| "无法创建安装进程组")?;
            let job = Self(handle);
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as _,
                std::mem::size_of_val(&info) as u32,
            )
            .map_err(|_| "无法配置安装进程组")?;
            AssignProcessToJobObject(
                handle,
                windows::Win32::Foundation::HANDLE(child.as_raw_handle()),
            )
            .map_err(|_| "无法管理安装进程组")?;
            Ok(job)
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

fn run_limited(
    c: &mut Command,
    timeout: Duration,
    max_bytes: u64,
) -> Result<std::fs::File, String> {
    let output = tempfile::tempfile().map_err(|_| "无法创建安装日志")?;
    c.stdout(output.try_clone().map_err(|_| "无法创建安装日志")?)
        .stderr(output.try_clone().map_err(|_| "无法创建安装日志")?)
        .stdin(Stdio::null());
    let mut child = c.spawn().map_err(|_| "无法启动安装器，请检查运行环境")?;
    let job = match Job::assign(&child) {
        Ok(job) => job,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
    };
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|_| "无法获取安装进度")? {
            drop(job);
            return if status.success() {
                Ok(output)
            } else {
                Err(format!(
                    "依赖安装失败（退出码 {}），请检查网络、运行环境和软件源后重试。",
                    status.code().unwrap_or(-1)
                ))
            };
        }
        if start.elapsed() > timeout
            || output
                .metadata()
                .map(|m| m.len() > max_bytes)
                .unwrap_or(false)
        {
            drop(job);
            let _ = child.kill();
            let _ = child.wait();
            return Err("依赖安装超时，请检查网络后重试".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
fn run(c: &mut Command) -> Result<(), String> {
    run_limited(c, Duration::from_secs(240), 20_000_000).map(|_| ())
}
pub fn check_node() -> Result<PathBuf, String> {
    use std::io::{Read, Seek};
    let node = find_program("node").ok_or("请先安装 Node.js 22.13 或更高版本，并重新打开 Molly")?;
    let mut output = run_limited(
        command(&node).arg("--version"),
        Duration::from_secs(5),
        65_536,
    )?;
    output.rewind().map_err(|_| "无法检查 Node.js")?;
    let mut version = String::new();
    output
        .take(65_536)
        .read_to_string(&mut version)
        .map_err(|_| "无法检查 Node.js")?;
    if semver::Version::parse(version.trim().trim_start_matches('v'))
        .map(|v| v < semver::Version::new(22, 13, 0))
        .unwrap_or(true)
    {
        return Err("请更新到 Node.js 22.13 或更高版本".into());
    }
    Ok(node)
}
pub fn ensure_plain_path(path: &Path) -> Result<(), String> {
    use std::os::windows::fs::MetadataExt;
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("安装目录必须是绝对路径".into());
    }
    for ancestor in path.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(meta) if meta.file_attributes() & 0x400 != 0 => {
                return Err("安装目标不能使用符号链接或目录联接".into())
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("无法访问安装目录".into()),
        }
    }
    Ok(())
}
pub fn install_package(entry: &Entry, root: &Path) -> Result<(String, Vec<String>), String> {
    let recipe = entry.recipe.as_ref().ok_or("没有安装模板")?;
    super::catalog::validate_id(&entry.id)?;
    super::sources::validate_package(&recipe.kind, &recipe.package, &recipe.version)?;
    if recipe.version.is_empty()
        || !recipe
            .version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-+_!".contains(&b))
    {
        return Err("软件包版本无效".into());
    }
    let target = root.join(&entry.id).join(&recipe.version);
    ensure_plain_path(&target)?;
    let marker = target.join("mcp-install.json");
    let complete = std::fs::read(&marker)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .is_some_and(|v| v["package"] == recipe.package && v["version"] == recipe.version);
    if recipe.kind == "npm" {
        super::sources::validate_bin("npm", &recipe.bin)?;
        let node = check_node()?;
        let npm = node
            .parent()
            .ok_or("Node 路径无效")?
            .join("node_modules/npm/bin/npm-cli.js");
        if !npm.is_file() {
            return Err("未找到 npm，请使用包含 npm 的 Node.js 安装包".into());
        }
        let binary = target
            .join("node_modules")
            .join(&recipe.package)
            .join(&recipe.bin);
        if !complete || !binary.is_file() {
            std::fs::create_dir_all(&target).map_err(|_| "无法创建 MCP 安装目录")?;
            run(command(&node)
                .current_dir(&target)
                .arg(npm)
                .args([
                    "install",
                    "--ignore-scripts",
                    "--no-audit",
                    "--no-fund",
                    "--engine-strict",
                    "--registry=https://registry.npmjs.org",
                    "--prefix",
                ])
                .arg(&target)
                .arg("--cache")
                .arg(root.join(".cache/npm"))
                .arg(format!("{}@{}", recipe.package, recipe.version)))?;
        }
        ensure_plain_path(&binary)?;
        if !binary.is_file() {
            return Err("软件包未提供预期的启动入口，未修改 Agent 配置".into());
        }
        std::fs::write(
            marker,
            json!({"package":recipe.package,"version":recipe.version}).to_string(),
        )
        .map_err(|_| "无法保存安装状态")?;
        Ok((
            node.to_string_lossy().into(),
            vec![binary.to_string_lossy().into()],
        ))
    } else if recipe.kind == "pypi" {
        let uv =
            find_program("uv").ok_or("请先安装 uv 并重新打开 Molly；Python 环境由 uv 按需准备")?;
        if !recipe.bin.is_empty() {
            super::sources::validate_bin("pypi", &recipe.bin)?;
        }
        if !complete
            || !target.join("Scripts/python.exe").is_file()
            || (!recipe.bin.is_empty()
                && !target
                    .join("Scripts")
                    .join(format!("{}.exe", recipe.bin))
                    .is_file())
        {
            std::fs::create_dir_all(target.parent().unwrap()).map_err(|_| "无法创建安装目录")?;
            if !target.join("Scripts/python.exe").is_file() {
                run(command(&uv)
                    .current_dir(target.parent().unwrap())
                    .args([
                        "venv",
                        "--no-config",
                        "--allow-existing",
                        "--python",
                        "3.12",
                    ])
                    .arg(&target)
                    .arg("--cache-dir")
                    .arg(root.join(".cache/uv")))?;
            }
            run(command(&uv)
                .current_dir(&target)
                .args([
                    "pip",
                    "install",
                    "--no-config",
                    "--index-url",
                    "https://pypi.org/simple",
                    "--python",
                ])
                .arg(target.join("Scripts/python.exe"))
                .arg("--cache-dir")
                .arg(root.join(".cache/uv"))
                .arg(format!("{}=={}", recipe.package, recipe.version)))?;
        }
        let bin = if recipe.bin.is_empty() {
            use std::io::{Read, Seek};
            let mut output = run_limited(command(&target.join("Scripts/python.exe"))
                .args(["-I", "-c", "import importlib.metadata,json,sys; print(json.dumps([e.name for e in importlib.metadata.distribution(sys.argv[1]).entry_points if e.group == 'console_scripts']))"])
                .arg(&recipe.package), Duration::from_secs(10), 65_536)?;
            output.rewind().map_err(|_| "无法读取 Python 入口")?;
            let mut text = String::new();
            output
                .take(65_536)
                .read_to_string(&mut text)
                .map_err(|_| "无法读取 Python 入口")?;
            let bins: Vec<String> =
                serde_json::from_str(text.trim()).map_err(|_| "无法识别 Python 入口")?;
            if bins.len() != 1 {
                return Err("Python 包有多个入口或没有入口，请按文档手动配置".into());
            }
            bins[0].clone()
        } else {
            recipe.bin.clone()
        };
        super::sources::validate_bin("pypi", &bin)?;
        let binary = target.join("Scripts").join(format!("{bin}.exe"));
        ensure_plain_path(&binary)?;
        if !binary.is_file() {
            return Err("Python 软件包未提供预期启动入口".into());
        }
        std::fs::write(
            marker,
            json!({"package":recipe.package,"version":recipe.version}).to_string(),
        )
        .map_err(|_| "无法保存安装状态")?;
        Ok((binary.to_string_lossy().into(), vec![]))
    } else {
        Err("不支持的软件包类型".into())
    }
}
pub fn build_spec(
    entry: &Entry,
    values: &BTreeMap<String, String>,
    root: &Path,
) -> Result<Value, String> {
    let r = entry.recipe.as_ref().ok_or("没有安装模板")?;
    let mut values = values.clone();
    for field in &entry.fields {
        values
            .entry(field.key.clone())
            .or_insert_with(|| field.default.clone());
    }
    for field in &entry.fields {
        let value = values.get(&field.key).map(String::as_str).unwrap_or("");
        if field.required && value.trim().is_empty() {
            return Err(format!("请填写{}", field.label));
        }
        if !value.is_empty()
            && !field.choices.is_empty()
            && !field.choices.iter().any(|c| c == value)
        {
            return Err(format!("{}必须选择来源支持的值", field.label));
        }
        if value.chars().any(char::is_control) {
            return Err(format!("{}包含无效字符", field.label));
        }
        if field.kind == "directory"
            && (!Path::new(value).is_absolute() || !Path::new(value).is_dir())
        {
            return Err(format!("{}必须是已存在的绝对路径", field.label));
        }
    }
    let data_dir = root.join(&entry.id).join("data");
    let expand = |text: &str| expand_template(text, &values, &data_dir.to_string_lossy());
    let spec = if r.kind == "remote" {
        json!({"type":"http", "url":expand(&r.url),"headers":r.headers.iter().map(|(k,v)|(k.clone(),expand(v))).filter(|(_,v)|!v.is_empty()).collect::<BTreeMap<_,_>>()})
    } else {
        if entry.id == "git" && find_program("git").is_none() {
            return Err("请先安装 Git 并重新打开 Molly".into());
        }
        let (program, mut args) = install_package(entry, root)?;
        args.extend(r.args.iter().map(|s| expand(s)));
        for (key, group) in &r.optional_args {
            if values.get(key).is_some_and(|v| !v.is_empty()) {
                args.extend(group.iter().map(|s| expand(s)));
            }
        }
        ensure_plain_path(&data_dir)?;
        std::fs::create_dir_all(&data_dir).map_err(|_| "无法创建 MCP 数据目录")?;
        json!({"type":"stdio","command":program,"args":args,"env":r.env.iter().map(|(k,v)|(k.clone(),expand(v))).filter(|(_,v)|!v.is_empty()).collect::<BTreeMap<_,_>>()})
    };
    super::catalog::validate_spec(&spec)?;
    Ok(spec)
}

// Substitute only the template, never interpret a placeholder inside a supplied secret.
pub(super) fn expand_template(
    text: &str,
    values: &BTreeMap<String, String>,
    data_dir: &str,
) -> String {
    let mut rest = text;
    let mut result = String::new();
    while let Some(start) = rest.find('{') {
        result.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}').map(|i| i + start) else {
            result.push_str(&rest[start..]);
            return result;
        };
        let key = &rest[start + 1..end];
        result.push_str(if key == "dataDir" {
            data_dir
        } else {
            values
                .get(key)
                .map(String::as_str)
                .unwrap_or(&rest[start..=end])
        });
        rest = &rest[end + 1..];
    }
    result.push_str(rest);
    result
}

fn initialize() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"Molly MCP installation check","version":"0.1.3"}}})
}
pub fn probe_stdio(spec: &Value) -> Result<String, String> {
    let program = find_program(spec["command"].as_str().ok_or("缺少启动命令")?)
        .ok_or("未找到 MCP 启动程序")?;
    // Executables only: no automatic shell wrapping of a catalog command.
    if program
        .extension()
        .and_then(|v| v.to_str())
        .map(|v| !v.eq_ignore_ascii_case("exe"))
        .unwrap_or(true)
    {
        return Err("连接测试只直接启动 EXE，请在 Agent 中测试脚本命令".into());
    }
    let mut c = command(&program);
    let work = tempfile::tempdir().map_err(|_| "无法创建连接测试目录")?;
    c.current_dir(work.path());
    if let Some(args) = spec["args"].as_array() {
        c.args(args.iter().filter_map(Value::as_str));
    }
    if let Some(env) = spec["env"].as_object() {
        c.envs(env.iter().filter_map(|(k, v)| v.as_str().map(|v| (k, v))));
    }
    c.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = c.spawn().map_err(|_| "MCP 启动失败，请检查运行环境")?;
    let job = match Job::assign(&child) {
        Ok(j) => j,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
    };
    let result = (|| {
        let mut input = child.stdin.take().ok_or("无法连接 MCP 输入")?;
        let output = child.stdout.take().ok_or("无法读取 MCP 输出")?;
        let (tx, rx) = std::sync::mpsc::sync_channel(16);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let mut line = String::new();
                // Bound individual protocol frames as well as message count.
                use std::io::Read;
                let n = reader
                    .by_ref()
                    .take(1_048_576)
                    .read_line(&mut line)
                    .unwrap_or(0);
                if n == 0 || n >= 1_048_576 {
                    break;
                }
                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    if tx.send(value).is_err() {
                        break;
                    }
                }
            }
        });
        writeln!(input, "{}", initialize()).map_err(|_| "MCP 初始化失败")?;
        input.flush().map_err(|_| "MCP 初始化失败")?;
        let started = Instant::now();
        loop {
            if started.elapsed() >= Duration::from_secs(25) {
                return Err("MCP 初始化超时".into());
            }
            let remaining = Duration::from_secs(25).saturating_sub(started.elapsed());
            let reply = rx.recv_timeout(remaining).map_err(|error| match error {
                std::sync::mpsc::RecvTimeoutError::Timeout => {
                    "MCP 未在 25 秒内完成初始化，请检查密钥与依赖"
                }
                std::sync::mpsc::RecvTimeoutError::Disconnected => {
                    "MCP 启动后退出，未完成初始化。请检查该版本的依赖兼容性及必填参数"
                }
            })?;
            if reply["id"] != 1 {
                continue;
            }
            if reply.get("error").is_some() || reply["result"]["protocolVersion"].as_str().is_none()
            {
                return Err("MCP 拒绝初始化，请在 Agent 中检查配置和授权".into());
            }
            return Ok("连接检查通过；重新加载目标 Agent 后使用".into());
        }
    })();
    drop(job);
    let _ = child.kill();
    let _ = child.wait();
    result
}
pub(super) fn network_client(endpoint: &str) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("MollyCloud-MCP/", env!("CARGO_PKG_VERSION")));
    if url::Url::parse(endpoint)
        .ok()
        .is_some_and(|url| matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
    {
        builder = builder.no_proxy();
    } else if let Some(proxy) = crate::proxy::get_system_proxy() {
        builder = builder.proxy(reqwest::Proxy::all(proxy).map_err(|_| "系统代理配置无效")?);
    }
    builder.build().map_err(|_| "网络初始化失败".into())
}
pub async fn probe_http(spec: &Value) -> Result<String, String> {
    let endpoint = spec["url"].as_str().ok_or("缺少地址")?;
    let client = network_client(endpoint)?;
    let mut request = client
        .post(endpoint)
        .header("Accept", "application/json, text/event-stream")
        .json(&initialize());
    if let Some(headers) = spec["headers"].as_object() {
        for (k, v) in headers {
            request = request.header(k, v.as_str().unwrap_or(""));
        }
    }
    let mut response = request
        .send()
        .await
        .map_err(|_| "无法连接 MCP，请检查网络和地址")?;
    if matches!(response.status().as_u16(), 401 | 403) {
        return Ok("等待授权：请在目标 Agent 的 MCP 设置中完成登录或检查令牌".into());
    }
    if !response.status().is_success() {
        return Err(format!(
            "MCP 返回 HTTP {}，请在 Agent 中检查连接",
            response.status().as_u16()
        ));
    }
    let session = response.headers().get("mcp-session-id").cloned();
    let result = tokio::time::timeout(Duration::from_secs(20), async {
        let mut body = Vec::new();
        loop {
            let chunk = response.chunk().await.map_err(|_| "MCP 响应读取失败")?;
            let Some(chunk) = chunk else {
                return Err("服务没有返回有效的 MCP 初始化结果".into());
            };
            if body.len() + chunk.len() > 1_048_576 {
                return Err("MCP 响应过大".into());
            }
            body.extend_from_slice(&chunk);
            let text = String::from_utf8_lossy(&body);
            let reply = serde_json::from_slice::<Value>(&body)
                .ok()
                .filter(|v| v["id"] == 1)
                .or_else(|| {
                    text.lines()
                        .filter_map(|line| line.strip_prefix("data:"))
                        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
                        .find(|v| v["id"] == 1)
                });
            if let Some(reply) = reply {
                return if reply.get("error").is_none()
                    && reply["result"]["protocolVersion"].is_string()
                {
                    Ok("连接检查通过；由目标 Agent 直接连接".into())
                } else {
                    Err("服务拒绝 MCP 初始化".into())
                };
            }
        }
    })
    .await
    .unwrap_or_else(|_| Err("MCP 响应超时".into()));
    drop(response);
    if let Some(session) = session {
        let mut close = client
            .delete(endpoint)
            .header("mcp-session-id", session)
            .header("MCP-Protocol-Version", "2025-03-26");
        if let Some(headers) = spec["headers"].as_object() {
            for (k, v) in headers {
                close = close.header(k, v.as_str().unwrap_or(""));
            }
        }
        let _ = tokio::time::timeout(Duration::from_secs(3), close.send()).await;
    }
    result
}
pub async fn latest_version(r: &Recipe) -> Result<String, String> {
    let url = if r.kind == "npm" {
        format!("https://registry.npmjs.org/{}/latest", r.package)
    } else {
        format!("https://pypi.org/pypi/{}/json", r.package)
    };
    let client = network_client(&url)?;
    let value: Value = client
        .get(url)
        .send()
        .await
        .map_err(|_| "无法检查更新")?
        .error_for_status()
        .map_err(|_| "软件源暂不可用")?
        .json()
        .await
        .map_err(|_| "软件源格式错误")?;
    if value.get("deprecated").is_some() {
        return Err("上游已停止维护此软件包，请选择替代服务".into());
    }
    let version = if r.kind == "npm" {
        &value["version"]
    } else {
        &value["info"]["version"]
    };
    version
        .as_str()
        .map(String::from)
        .ok_or("没有可用版本".into())
}
