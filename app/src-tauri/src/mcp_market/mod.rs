mod catalog;
mod process;
mod sources;
#[cfg(test)]
mod tests;
mod uninstall;

use catalog::{validate_id, validate_spec, Entry};
use molly_ccswitch::mcp_market as configs;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager, State, WebviewWindow};

#[derive(Default)]
pub struct MarketState {
    gate: tokio::sync::Mutex<()>,
    plans: std::sync::Mutex<BTreeMap<String, (std::time::Instant, Entry)>>,
    #[cfg(feature = "mcp-market-smoke")]
    test_roots: Option<(PathBuf, PathBuf)>,
}
#[cfg(feature = "mcp-market-smoke")]
impl MarketState {
    pub fn isolated(state: PathBuf, packages: PathBuf) -> Self {
        Self {
            test_roots: Some((state, packages)),
            ..Self::default()
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Installation {
    key: String,
    id: String,
    name: String,
    agent: String,
    path: PathBuf,
    enabled: bool,
    spec: Value,
    target: Value,
    values: BTreeMap<String, String>,
    entry: Option<Entry>,
    version: String,
    check: String,
}
#[derive(Default, Clone, Serialize, Deserialize)]
struct Ledger {
    #[serde(default)]
    revision: u64,
    installations: Vec<Installation>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallRequest {
    pub id: String,
    pub agents: Vec<String>,
    #[serde(default)]
    pub values: BTreeMap<String, String>,
    pub custom: Option<Value>,
    pub plan_id: Option<String>,
}
const AGENTS: [(&str, &str); 4] = [
    ("codex", "Codex"),
    ("claude", "Claude Code"),
    ("opencode", "OpenCode"),
    ("gemini", "Gemini CLI"),
];
fn authorize(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == "console" {
        Ok(())
    } else {
        Err("请从控制台 MCP 市场进行操作".into())
    }
}
fn state_root(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(feature = "mcp-market-smoke")]
    if let Some((path, _)) = &app.state::<MarketState>().test_roots {
        return Ok(path.clone());
    }
    app.path()
        .app_data_dir()
        .map(|p| p.join("mcp-market"))
        .map_err(|_| "无法读取应用目录".into())
}
fn packages_root(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(feature = "mcp-market-smoke")]
    if let Some((_, path)) = &app.state::<MarketState>().test_roots {
        return Ok(path.clone());
    }
    // Outside Molly's identifier/installation directories: no Molly launcher is needed.
    app.path()
        .local_data_dir()
        .map(|p| p.join("MCP/Packages"))
        .map_err(|_| "无法读取本地安装目录".into())
}
fn read_text(path: &Path) -> Result<String, String> {
    if std::fs::metadata(path)
        .map(|m| m.len() > 8_000_000)
        .unwrap_or(false)
    {
        return Err("Agent 配置文件过大，请检查后重试".into());
    }
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(_) => Err(format!("无法读取 {}", path.display())),
    }
}
fn atomic_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    process::ensure_plain_path(path)?;
    let dir = path.parent().ok_or("保存目录无效")?;
    std::fs::create_dir_all(dir).map_err(|_| "无法创建状态目录")?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(|_| "无法创建临时文件")?;
    tmp.write_all(bytes)
        .and_then(|_| tmp.as_file().sync_all())
        .map_err(|_| "无法保存状态")?;
    tmp.persist(path).map_err(|_| "无法替换状态文件")?;
    Ok(())
}
fn save_encrypted<T: Serialize>(path: &Path, data: &T) -> Result<(), String> {
    let raw = serde_json::to_vec(data).map_err(|_| "状态序列化失败")?;
    atomic_private(path, &crate::dpapi_protect(&raw)?)
}
fn load(root: &Path) -> Result<Ledger, String> {
    let path = root.join("state.bin");
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&crate::dpapi_unprotect(&bytes)?)
            .map_err(|_| "安装记录损坏，请先恢复备份".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Ledger::default()),
        Err(_) => Err("无法读取安装记录".into()),
    }
}
#[derive(Serialize, Deserialize)]
struct Change {
    path: PathBuf,
    before: Option<String>,
    after: String,
}
fn snapshot(path: &Path) -> Result<Option<String>, String> {
    if path.exists() {
        read_text(path).map(Some)
    } else {
        Ok(None)
    }
}
fn prepare_change(
    agent: &str,
    path: &Path,
    id: &str,
    expected: Option<&Value>,
    next: Option<&Value>,
) -> Result<Change, String> {
    process::ensure_plain_path(path)?;
    let before = snapshot(path)?;
    let source = before.as_deref().unwrap_or("");
    let entries = configs::entries(agent, source)?;
    if entries.get(id) != expected {
        return Err(format!(
            "{agent} 中的 {id} 已存在或被其他工具修改，请刷新后检查；没有覆盖现有配置。"
        ));
    }
    let after = configs::patch(agent, source, id, next)?;
    Ok(Change {
        path: path.into(),
        before,
        after,
    })
}
fn rollback(
    changes: &[Change],
    write: &impl Fn(&Path, &[u8]) -> Result<(), String>,
) -> Result<(), String> {
    let mut errors = vec![];
    for change in changes.iter().rev() {
        match snapshot(&change.path) {
            Ok(current) if current.as_deref() == Some(&change.after) => {
                let result = if let Some(before) = &change.before {
                    write(&change.path, before.as_bytes())
                } else {
                    std::fs::remove_file(&change.path).map_err(|_| "无法移除新建配置".into())
                };
                if let Err(e) = result {
                    errors.push(e);
                }
            }
            Ok(current) if current == change.before => {}
            _ => errors.push(format!("{} 已被外部修改，保留现场", change.path.display())),
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("；"))
    }
}
fn commit(
    changes: &[Change],
    write: &impl Fn(&Path, &[u8]) -> Result<(), String>,
    save: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let mut applied = 0;
    let result = (|| {
        for change in changes {
            if snapshot(&change.path)? != change.before {
                return Err("Agent 配置在安装期间发生变化，请重试".into());
            }
            applied += 1;
            write(&change.path, change.after.as_bytes())?;
            if read_text(&change.path)? != change.after {
                return Err("Agent 配置写入验证失败".into());
            }
        }
        save()
    })();
    if let Err(error) = result {
        return match rollback(&changes[..applied], write) {
            Ok(()) => Err(format!("{error}；已撤销本次配置修改")),
            Err(rollback) => Err(format!(
                "{error}；{rollback}。原始配置保存在 MCP 市场备份中。"
            )),
        };
    }
    Ok(())
}
fn apply(root: &Path, ledger: &Ledger, changes: &[Change]) -> Result<(), String> {
    let before = load(root)?;
    let mut ledger = ledger.clone();
    ledger.revision = before.revision.checked_add(1).ok_or("安装记录版本过大")?;
    let timestamp = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
    save_encrypted(
        &root.join("backups").join(format!("{timestamp}.bin")),
        &changes,
    )?;
    // Durable journal allows the next operation to finish rollback after an interrupted write.
    save_encrypted(
        &root.join("pending.bin"),
        &(before, changes, ledger.revision),
    )?;
    let result = commit(changes, &configs::write, || {
        save_encrypted(&root.join("state.bin"), &ledger)
    });
    if result.is_ok()
        || changes
            .iter()
            .all(|c| snapshot(&c.path).ok() == Some(c.before.clone()))
    {
        let _ = std::fs::remove_file(root.join("pending.bin"));
    }
    result
}
fn recover(root: &Path) -> Result<(), String> {
    recover_with(root, &configs::write)
}
fn recover_with(
    root: &Path,
    write: &impl Fn(&Path, &[u8]) -> Result<(), String>,
) -> Result<(), String> {
    let path = root.join("pending.bin");
    if !path.exists() {
        return Ok(());
    }
    let bytes = std::fs::read(&path).map_err(|_| "无法读取上次安装记录")?;
    let (ledger, changes, revision): (Ledger, Vec<Change>, u64) =
        serde_json::from_slice(&crate::dpapi_unprotect(&bytes)?).map_err(|_| "上次安装记录无效")?;
    // A crash after the ledger commit must not undo a successful install.
    if load(root)?.revision == revision {
        std::fs::remove_file(path).map_err(|_| "无法结束安装恢复")?;
        return Ok(());
    }
    rollback(&changes, write)?;
    save_encrypted(&root.join("state.bin"), &ledger)?;
    std::fs::remove_file(path).map_err(|_| "无法结束安装恢复")?;
    Ok(())
}
fn status(root: &Path, packages: &Path) -> Result<Value, String> {
    let ledger = load(root)?;
    let mut agents = vec![];
    let mut installations = vec![];
    for (id, label) in AGENTS {
        let path = configs::config_path(id);
        let (path, entries, error) = match path {
            Ok(path) => {
                let parsed = read_text(&path).and_then(|s| configs::entries(id, &s));
                let error = parsed.as_ref().err().cloned();
                (Some(path), parsed.unwrap_or_default(), error)
            }
            Err(e) => (None, serde_json::Map::new(), Some(e)),
        };
        agents.push(json!({"id":id,"name":label,"path":path,"detected":path.as_ref().is_some_and(|p|p.exists()),"error":error}));
        for row in ledger.installations.iter().filter(|row| row.agent == id) {
            let active = entries.get(&row.id);
            let changed = error.is_some()
                || path.as_ref() != Some(&row.path)
                || (if row.enabled {
                    active != Some(&row.target)
                } else {
                    active.is_some()
                });
            installations.push(json!({"key":row.key,"id":row.id,"name":row.name,"agent":row.agent,"path":row.path,"enabled":row.enabled,"managed":true,"changed":changed,"version":row.version,"check":row.check,"canUpdate":row.entry.as_ref().and_then(|e|e.recipe.as_ref()).is_some_and(|r|r.kind!="remote")}));
        }
        for (name, _) in entries.iter().filter(|(name, _)| {
            !ledger
                .installations
                .iter()
                .any(|r| r.agent == id && r.id == **name)
        }) {
            installations.push(json!({"key":format!("external:{id}:{name}"),"id":name,"name":name,"agent":id,"path":path,"enabled":true,"managed":false,"changed":false,"version":"","check":"由其他工具配置","canUpdate":false}));
        }
    }
    Ok(
        json!({"agents":agents,"installations":installations,"packageRoot":packages,"packages":uninstall::inventory(packages,&ledger),"node":process::check_node().is_ok(),"uv":process::find_program("uv").is_some()}),
    )
}
fn install_configuration(
    root: &Path,
    request: &InstallRequest,
    spec: &Value,
    entry: Option<Entry>,
    check: String,
) -> Result<(), String> {
    validate_id(&request.id)?;
    validate_spec(spec)?;
    let mut ledger = load(root)?;
    let mut changes = vec![];
    let mut paths = BTreeSet::new();
    for agent in &request.agents {
        let path = configs::config_path(agent)?;
        if !paths.insert(path.clone()) {
            return Err("多个 Agent 指向同一个配置文件，请调整目录后重试".into());
        }
        let key = format!("{}:{}", agent, request.id);
        if ledger.installations.iter().any(|row| row.key == key) {
            return Err(format!("已为 {agent} 安装此 MCP，请在已安装页面管理"));
        }
        let target = configs::target_spec(agent, spec)?;
        changes.push(prepare_change(
            agent,
            &path,
            &request.id,
            None,
            Some(&target),
        )?);
        ledger.installations.push(Installation {
            key,
            id: request.id.clone(),
            name: entry
                .as_ref()
                .map(|e| e.name.clone())
                .unwrap_or(request.id.clone()),
            agent: agent.clone(),
            path,
            enabled: true,
            spec: spec.clone(),
            target,
            values: request.values.clone(),
            version: entry
                .as_ref()
                .and_then(|e| e.recipe.as_ref())
                .map(|r| r.version.clone())
                .unwrap_or("custom".into()),
            entry: entry.clone(),
            check: check.clone(),
        });
    }
    apply(root, &ledger, &changes)
}
fn preflight_install(
    root: &Path,
    request: &InstallRequest,
) -> Result<BTreeMap<String, PathBuf>, String> {
    recover(root)?;
    let ledger = load(root)?;
    let mut paths = BTreeMap::new();
    let mut unique = BTreeSet::new();
    for agent in &request.agents {
        let path = configs::config_path(agent)?;
        if !unique.insert(path.to_string_lossy().to_lowercase()) {
            return Err("多个 Agent 指向同一个配置文件，请调整目录后重试".into());
        }
        if ledger
            .installations
            .iter()
            .any(|r| r.agent == *agent && r.id == request.id)
        {
            return Err(format!("已为 {agent} 安装此 MCP，请在已安装页面管理"));
        }
        // Validate existing files before any download, subprocess, or credential-bearing request.
        prepare_change(agent, &path, &request.id, None, None)?;
        if let Some(spec) = &request.custom {
            validate_spec(spec)?;
            configs::target_spec(agent, spec)?;
        }
        paths.insert(agent.clone(), path);
    }
    Ok(paths)
}
#[tauri::command]
pub async fn mcp_market_status(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, MarketState>,
) -> Result<Value, String> {
    authorize(&window)?;
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| "MCP 操作正在进行，请稍候")?;
    let root = state_root(&app)?;
    let packages = packages_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        recover(&root)?;
        status(&root, &packages)
    })
    .await
    .map_err(|_| "读取 MCP 状态失败")?
}
#[tauri::command]
pub async fn mcp_market_install(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, MarketState>,
    request: InstallRequest,
) -> Result<Value, String> {
    authorize(&window)?;
    validate_id(&request.id)?;
    if request.agents.is_empty()
        || request.agents.len() > 4
        || request
            .agents
            .iter()
            .any(|a| !AGENTS.iter().any(|(id, _)| id == a))
        || request.agents.iter().collect::<BTreeSet<_>>().len() != request.agents.len()
    {
        return Err("请选择有效的目标 Agent".into());
    }
    if request.values.values().any(|s| s.len() > 8192) {
        return Err("安装参数过长".into());
    }
    let _guard = state.gate.try_lock().map_err(|_| "已有 MCP 安装正在进行")?;
    let root = state_root(&app)?;
    let packages = packages_root(&app)?;
    if request.custom.is_some() && request.plan_id.is_some() {
        return Err("不能同时提交来源安装和手动配置".into());
    }
    let entry = if let Some(plan_id) = &request.plan_id {
        let plans = state.plans.lock().map_err(|_| "安装计划不可用")?;
        let (created, entry) = plans
            .get(plan_id)
            .ok_or("安装计划已失效，请重新从来源读取")?;
        if created.elapsed() > std::time::Duration::from_secs(1800) || entry.id != request.id {
            return Err("安装计划已失效，请重新从来源读取".into());
        }
        Some(entry.clone())
    } else if request.custom.is_some() {
        None
    } else {
        Some(catalog::template(&request.id)?)
    };
    let paths = preflight_install(&root, &request)?;
    let spec = if let Some(spec) = &request.custom {
        validate_spec(spec)?;
        spec.clone()
    } else {
        let e = entry.clone().unwrap();
        let values = request.values.clone();
        let p = packages.clone();
        tauri::async_runtime::spawn_blocking(move || process::build_spec(&e, &values, &p))
            .await
            .map_err(|_| "安装进程未完成")??
    };
    let check = if request.custom.is_some() {
        "配置完成，尚未进行连接测试".into()
    } else if spec["type"] == "http" {
        process::probe_http(&spec)
            .await
            .unwrap_or_else(|e| format!("待检查：{e}"))
    } else {
        let s = spec.clone();
        tauri::async_runtime::spawn_blocking(move || process::probe_stdio(&s))
            .await
            .map_err(|_| "连接测试未完成")??
    };
    tauri::async_runtime::spawn_blocking(move || {
        for (agent, path) in paths {
            if configs::config_path(&agent)? != path {
                return Err("Agent 配置目录在安装期间发生变化，请重试".into());
            }
        }
        install_configuration(&root, &request, &spec, entry, check)?;
        status(&root, &packages)
    })
    .await
    .map_err(|_| "安装未完成")?
}
#[tauri::command]
pub async fn mcp_market_action(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, MarketState>,
    key: String,
    action: String,
) -> Result<Value, String> {
    authorize(&window)?;
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| "MCP 操作正在进行，请稍候")?;
    if !["enable", "disable", "remove", "uninstall", "test", "update"].contains(&action.as_str()) {
        return Err("不支持的 MCP 操作".into());
    }
    let root = state_root(&app)?;
    let packages = packages_root(&app)?;
    recover(&root)?;
    let mut ledger = load(&root)?;
    let index = ledger
        .installations
        .iter()
        .position(|r| r.key == key)
        .ok_or("安装记录不存在，请刷新")?;
    let previous = ledger.installations[index].clone();
    if configs::config_path(&previous.agent)? != previous.path {
        return Err("Agent 配置目录已改变，请恢复原目录后管理旧安装".into());
    }
    let expected = previous.enabled.then_some(&previous.target);
    prepare_change(
        &previous.agent,
        &previous.path,
        &previous.id,
        expected,
        expected,
    )?;
    let mut updated = previous.clone();
    if action == "update" {
        let entry = updated
            .entry
            .as_mut()
            .ok_or("手动配置请通过移除后重新添加更新")?;
        let recipe = entry.recipe.as_mut().ok_or("没有更新模板")?;
        if recipe.kind == "remote" {
            return Err("远程服务由提供方更新".into());
        }
        recipe.version = process::latest_version(recipe).await?;
        if entry.source != "curated" {
            *recipe = sources::pin(recipe.clone()).await?;
        }
        updated.version = recipe.version.clone();
        let e = entry.clone();
        let v = updated.values.clone();
        let p = packages.clone();
        updated.spec =
            tauri::async_runtime::spawn_blocking(move || process::build_spec(&e, &v, &p))
                .await
                .map_err(|_| "更新未完成")??;
        updated.target = configs::target_spec(&updated.agent, &updated.spec)?;
    }
    if action == "test" || action == "update" {
        // Read-only protocol handshake. No tool is executed, and no process is kept alive.
        let result = if updated.spec["type"] == "http" {
            process::probe_http(&updated.spec).await
        } else if updated.spec["type"] == "sse" {
            Err("旧版 SSE 请在目标 Agent 中测试，推荐使用 Streamable HTTP".into())
        } else {
            let spec = updated.spec.clone();
            tauri::async_runtime::spawn_blocking(move || process::probe_stdio(&spec))
                .await
                .map_err(|_| "测试未完成")?
        };
        if action == "update" {
            updated.check = result?;
        } else {
            updated.check = result.unwrap_or_else(|e| format!("待检查：{e}"));
        }
    }
    if action == "enable" {
        updated.enabled = true;
    }
    if action == "disable" {
        updated.enabled = false;
    }
    let expected = previous.enabled.then_some(&previous.target);
    let next = (!matches!(action.as_str(), "remove" | "uninstall") && updated.enabled)
        .then_some(&updated.target);
    let change = prepare_change(
        &previous.agent,
        &previous.path,
        &previous.id,
        expected,
        next,
    )?;
    if matches!(action.as_str(), "remove" | "uninstall") {
        ledger.installations.remove(index);
    } else {
        ledger.installations[index] = updated;
    }
    tauri::async_runtime::spawn_blocking(move || {
        if action == "test" {
            save_encrypted(&root.join("state.bin"), &ledger)?;
        } else {
            apply(&root, &ledger, &[change])?;
        }
        let mut message = None;
        if action == "uninstall" {
            message = Some(
                if previous
                    .entry
                    .as_ref()
                    .and_then(|e| e.recipe.as_ref())
                    .is_some_and(|r| r.kind != "remote")
                {
                    match uninstall::cleanup(&packages, &ledger, &previous.id, &previous.version) {
                        Ok(()) => {
                            "已卸载：Agent 配置已移除，软件包已移入回收站；服务数据保留。".into()
                        }
                        Err(e) => format!("Agent 配置已移除；{e}。可在保留的软件包中再次清理。"),
                    }
                } else {
                    "Agent 连接配置已卸载。远程服务及手动安装的程序由其提供方管理。".into()
                },
            );
        }
        let mut result = status(&root, &packages)?;
        if let Some(message) = message {
            result["message"] = json!(message);
        }
        Ok(result)
    })
    .await
    .map_err(|_| "MCP 操作未完成")?
}
#[tauri::command]
pub async fn mcp_market_search(window: WebviewWindow, query: String) -> Result<Vec<Entry>, String> {
    authorize(&window)?;
    if query.len() > 160 {
        return Err("搜索内容过长".into());
    }
    catalog::registry_search(query.trim()).await
}

#[tauri::command]
pub async fn mcp_market_resolve(
    window: WebviewWindow,
    state: State<'_, MarketState>,
    source: String,
    id: String,
) -> Result<sources::Resolution, String> {
    authorize(&window)?;
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| "MCP 操作正在进行，请稍候")?;
    let mut resolution = sources::resolve(&source, &id).await?;
    let mut plans = state.plans.lock().map_err(|_| "安装计划不可用")?;
    plans.retain(|_, (time, _)| time.elapsed() < std::time::Duration::from_secs(1800));
    if plans.len() + resolution.plans.len() > 60 {
        plans.clear();
    }
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    for plan in &mut resolution.plans {
        plan.plan_id = format!(
            "{}-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        plans.insert(
            plan.plan_id.clone(),
            (std::time::Instant::now(), plan.entry.clone()),
        );
    }
    Ok(resolution)
}

#[tauri::command]
pub async fn mcp_market_cleanup(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, MarketState>,
    id: String,
    version: String,
) -> Result<Value, String> {
    authorize(&window)?;
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| "MCP 操作正在进行，请稍候")?;
    let root = state_root(&app)?;
    let packages = packages_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        recover(&root)?;
        uninstall::cleanup(&packages, &load(&root)?, &id, &version)?;
        let mut result = status(&root, &packages)?;
        result["message"] = json!("软件包已移入回收站，服务数据保留。");
        Ok(result)
    })
    .await
    .map_err(|_| "软件包清理未完成")?
}
