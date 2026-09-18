use super::*;

fn version_path(packages: &Path, id: &str, version: &str) -> Result<PathBuf, String> {
    validate_id(id)?;
    if version.len() > 100
        || !version.bytes().next().is_some_and(|b| b.is_ascii_digit())
        || !version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-+_!".contains(&b))
    {
        return Err("软件包版本目录无效".into());
    }
    let path = packages.join(id).join(version);
    process::ensure_plain_path(&path)?;
    let marker = path.join("mcp-install.json");
    process::ensure_plain_path(&marker)?;
    let value: Value = serde_json::from_str(&read_text(&marker)?)
        .map_err(|_| "此目录没有有效的市场安装标记，已保留文件")?;
    if value["version"] != version || !value["package"].as_str().is_some_and(|s| !s.is_empty()) {
        return Err("软件包安装标记不匹配，已保留文件".into());
    }
    Ok(path)
}
fn normalize(text: &str) -> String {
    text.replace('\\', "/").to_lowercase()
}
pub(super) fn references(value: &Value, path: &Path) -> bool {
    match value {
        // Conservative substring matching also recognizes --path=<absolute-path> arguments.
        Value::String(text) => normalize(text).contains(&normalize(&path.to_string_lossy())),
        Value::Array(values) => values.iter().any(|v| references(v, path)),
        Value::Object(values) => values.values().any(|v| references(v, path)),
        _ => false,
    }
}
fn configurations(ledger: &Ledger) -> Result<Vec<Value>, String> {
    let mut paths = BTreeSet::new();
    let mut configs = vec![];
    for (agent, _) in AGENTS {
        paths.insert((agent.to_owned(), configs::config_path(agent)?));
    }
    for row in &ledger.installations {
        paths.insert((row.agent.clone(), row.path.clone()));
    }
    for (agent, path) in paths {
        configs.push(Value::Object(configs::entries(&agent, &read_text(&path)?)?));
    }
    Ok(configs)
}
pub(super) fn ensure_unused(
    ledger: &Ledger,
    configs: &[Value],
    path: &Path,
    id: &str,
    version: &str,
) -> Result<(), String> {
    if ledger
        .installations
        .iter()
        .any(|r| (r.id == id && r.version == version) || references(&r.spec, path))
    {
        return Err("软件包仍被其他 Agent 使用（包括已禁用配置），已保留".into());
    }
    if configs.iter().any(|v| references(v, path)) {
        return Err("检测到其他 Agent 配置引用此软件包，已保留".into());
    }
    Ok(())
}
pub fn inventory(packages: &Path, ledger: &Ledger) -> Vec<Value> {
    if process::ensure_plain_path(packages).is_err() {
        return vec![];
    }
    let Ok(ids) = std::fs::read_dir(packages) else {
        return vec![];
    };
    let configs = configurations(ledger);
    let mut rows = vec![];
    for dir in ids.flatten().take(400) {
        let id = dir.file_name().to_string_lossy().to_string();
        if validate_id(&id).is_err() || process::ensure_plain_path(&dir.path()).is_err() {
            continue;
        }
        let Ok(versions) = std::fs::read_dir(dir.path()) else {
            continue;
        };
        for item in versions.flatten().take(100) {
            let version = item.file_name().to_string_lossy().to_string();
            let Ok(path) = version_path(packages, &id, &version) else {
                continue;
            };
            // Active versions are managed from their Agent cards; list only leftover versions.
            if ledger
                .installations
                .iter()
                .any(|r| r.id == id && r.version == version)
            {
                continue;
            }
            let reason = configs
                .as_ref()
                .map_err(|_| "无法完整读取 Agent 配置，暂不清理".into())
                .and_then(|c| ensure_unused(ledger, c, &path, &id, &version))
                .err();
            rows.push(json!({"id":id,"version":version,"path":path,"reason":reason}));
        }
    }
    rows
}
pub fn cleanup(packages: &Path, ledger: &Ledger, id: &str, version: &str) -> Result<(), String> {
    cleanup_with(
        packages,
        ledger,
        id,
        version,
        &configurations(ledger)?,
        |path| crate::trash::move_to_recycle_bin(&[path.to_string_lossy().to_string()]).map(|_| ()),
    )
}
pub(super) fn cleanup_with(
    packages: &Path,
    ledger: &Ledger,
    id: &str,
    version: &str,
    configs: &[Value],
    recycle: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let path = version_path(packages, id, version)?;
    ensure_unused(ledger, configs, &path, id, version)?;
    recycle(&path)?;
    if path.exists() {
        return Err("文件仍被占用或未清理完成，可关闭目标 Agent 后重试".into());
    }
    Ok(())
}
