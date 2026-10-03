//! Trusted, versioned console UI packages over host-owned native adapters.
//! Registry replacement is the commit point; user data is never a package path.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{Manager, State};
const CATALOG: &str = "https://desktop.veriolink.com/plugins/index.json";
const MAX_DOWNLOAD: usize = 128 * 1024 * 1024;
const MAX_UNPACKED: u64 = 512 * 1024 * 1024;
const HOST: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Definition {
    pub id: String,
    pub name: String,
    pub version: String,
    pub upstream_version: String,
    pub repository: String,
    pub asset_dir: String,
    pub adapter_api: String,
    pub host_version: String,
    pub description: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Package {
    pub id: String,
    pub version: String,
    pub upstream_version: String,
    pub adapter_api: String,
    pub host_version: String,
    pub url: String,
    pub sha256: String,
    pub size: usize,
    #[serde(default)]
    pub notes: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Installed {
    package: Package,
    directory: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Record {
    current: Option<Installed>,
    previous: Option<Installed>,
}
type Registry = BTreeMap<String, Record>;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    #[serde(flatten)]
    definition: Definition,
    installed_version: Option<String>,
    upstream_version_installed: Option<String>,
    pub installed: bool,
    pub usable: bool,
    compatible: bool,
    restart_required: bool,
    can_rollback: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    official_version: Option<String>,
    official_error: Option<String>,
    package: Option<Package>,
    compatible: bool,
    update_available: bool,
    catalog_error: Option<String>,
}
#[derive(Deserialize)]
struct Catalog {
    schema: u32,
    plugins: Vec<Package>,
}

pub struct PluginStore {
    root: PathBuf,
    records: Mutex<Registry>,
    operation: tokio::sync::Mutex<()>,
    started: HashSet<String>,
    load_error: Option<String>,
}
pub fn definitions() -> Vec<Definition> {
    serde_json::from_str(
        include_str!("../../plugins/definitions.json").trim_start_matches('\u{feff}'),
    )
    .expect("valid plugin definitions")
}
fn definition(id: &str) -> Result<Definition, String> {
    definitions()
        .into_iter()
        .find(|d| d.id == id)
        .ok_or_else(|| "未知插件".into())
}
fn bundled(d: &Definition) -> Installed {
    Installed {
        directory: None,
        package: Package {
            id: d.id.clone(),
            version: d.version.clone(),
            upstream_version: d.upstream_version.clone(),
            adapter_api: d.adapter_api.clone(),
            host_version: d.host_version.clone(),
            url: String::new(),
            sha256: String::new(),
            size: 0,
            notes: String::new(),
        },
    }
}
fn compatible(p: &Package, d: &Definition) -> bool {
    p.id == d.id
        && p.adapter_api == d.adapter_api
        && semver::VersionReq::parse(&p.host_version)
            .ok()
            .zip(semver::Version::parse(HOST).ok())
            .is_some_and(|(req, host)| req.matches(&host))
}
fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && path.len() < 240
        && !path.contains(['\\', ':', '%', '\0'])
        && !path.starts_with('/')
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with(['.', ' '])
                && !part.chars().any(char::is_control)
                && {
                    let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
                    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                        && !(stem.len() == 4
                            && (stem.starts_with("COM") || stem.starts_with("LPT"))
                            && stem.as_bytes()[3].is_ascii_digit())
                }
        })
}
fn check_package(p: &Package, d: &Definition) -> Result<(), String> {
    if !compatible(p, d) {
        return Err("此插件版本需要更新 MollyCloud 的原生适配，请先升级主程序。".into());
    }
    if semver::Version::parse(&p.version).is_err()
        || p.sha256.len() != 64
        || !p.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || p.size == 0
        || p.size > MAX_DOWNLOAD
    {
        return Err("插件版本或校验信息无效".into());
    }
    let url = url::Url::parse(&p.url).map_err(|_| "插件下载地址无效")?;
    if url.scheme() != "https"
        || url.host_str() != Some("desktop.veriolink.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || !url.path().starts_with("/plugins/")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("插件只能从 Molly 官方插件下载目录安装".into());
    }
    Ok(())
}
impl PluginStore {
    fn open(root: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let path = root.join("installed.json");
        let records: Registry = if path.exists() {
            serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
                .map_err(|_| "插件记录损坏；请保留数据并恢复 installed.json")?
        } else {
            definitions()
                .iter()
                .map(|d| {
                    (
                        d.id.clone(),
                        Record {
                            current: Some(bundled(d)),
                            previous: None,
                        },
                    )
                })
                .collect()
        };
        for (id, record) in &records {
            definition(id)?;
            for item in [&record.current, &record.previous].into_iter().flatten() {
                if item.package.id != *id
                    || item.directory.as_ref().is_some_and(|p| {
                        !safe_relative(p) || !p.starts_with(&format!("packages/{id}/"))
                    })
                {
                    return Err("插件记录路径无效".into());
                }
            }
        }
        let started = definitions()
            .iter()
            .filter(|d| {
                records
                    .get(&d.id)
                    .and_then(|r| r.current.as_ref())
                    .is_some_and(|p| compatible(&p.package, d))
            })
            .map(|d| d.id.clone())
            .collect();
        let store = Self {
            root,
            records: Mutex::new(records),
            operation: tokio::sync::Mutex::new(()),
            started,
            load_error: None,
        };
        if !path.exists() {
            store.persist(&*store.records.lock().map_err(|_| "插件状态不可用")?)?;
        }
        Ok(store)
    }
    fn persist(&self, records: &Registry) -> Result<(), String> {
        let mut temporary =
            tempfile::NamedTempFile::new_in(&self.root).map_err(|e| e.to_string())?;
        temporary
            .write_all(&serde_json::to_vec_pretty(records).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        temporary.as_file().sync_all().map_err(|e| e.to_string())?;
        temporary
            .persist(self.root.join("installed.json"))
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn replace(&self, id: &str, value: Option<Installed>, rollback: bool) -> Result<(), String> {
        if let Some(error) = &self.load_error {
            return Err(error.clone());
        }
        let mut lock = self.records.lock().map_err(|_| "插件状态不可用")?;
        let mut next = lock.clone();
        let record = next.entry(id.into()).or_default();
        let old = record.current.take();
        record.current = if rollback {
            record.previous.take()
        } else {
            value
        };
        // Uninstall drops code references, but never accesses feature data directories.
        record.previous = if record.current.is_some() { old } else { None };
        self.persist(&next)?;
        *lock = next;
        Ok(())
    }
    pub fn enabled(&self, id: &str) -> bool {
        (id == "images" || self.started.contains(id))
            && self
                .records
                .lock()
                .ok()
                .and_then(|r| r.get(id).and_then(|r| r.current.clone()))
                .is_some_and(|p| definition(id).is_ok_and(|d| compatible(&p.package, &d)))
    }
    fn list(&self) -> Result<Vec<Status>, String> {
        if let Some(error) = &self.load_error {
            return Err(error.clone());
        }
        let records = self.records.lock().map_err(|_| "插件状态不可用")?;
        Ok(definitions()
            .into_iter()
            .map(|d| {
                let record = records.get(&d.id);
                let current = record.and_then(|r| r.current.as_ref());
                let ok = current.is_some_and(|p| compatible(&p.package, &d));
                let installed = current.is_some();
                Status {
                    installed_version: current.map(|p| p.package.version.clone()),
                    upstream_version_installed: current.map(|p| p.package.upstream_version.clone()),
                    installed,
                    usable: ok && (d.id == "images" || self.started.contains(&d.id)),
                    compatible: ok,
                    restart_required: d.id != "images" && (ok != self.started.contains(&d.id)),
                    can_rollback: record
                        .and_then(|r| r.previous.as_ref())
                        .is_some_and(|p| compatible(&p.package, &d)),
                    definition: d,
                }
            })
            .collect())
    }
    fn install_archive(&self, p: Package, bytes: Vec<u8>) -> Result<(), String> {
        check_package(&p, &definition(&p.id)?)?;
        let id = p.id.clone();
        if bytes.len() != p.size
            || format!("{:x}", Sha256::digest(&bytes)) != p.sha256.to_ascii_lowercase()
        {
            return Err("插件大小或 SHA-256 校验失败，已保留原版本".into());
        }
        let stage = tempfile::tempdir_in(&self.root).map_err(|e| e.to_string())?;
        unpack(&bytes, stage.path(), &p)?;
        let dir = format!("packages/{}/{}", id, p.sha256.to_ascii_lowercase());
        let destination = self.root.join(&dir);
        if !destination.exists() {
            std::fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
            std::fs::rename(stage.path(), &destination).map_err(|e| e.to_string())?;
        }
        self.replace(
            &id,
            Some(Installed {
                package: p,
                directory: Some(dir),
            }),
            false,
        )?;
        Ok(())
    }
    // Removed/downloaded obsolete code is collected only on next launch: loaded
    // iframes can finish in-flight reads without deleting another plugin's data.
    fn collect_code(&self) -> Result<(), String> {
        if self.load_error.is_some() {
            return Ok(());
        }
        let records = self.records.lock().map_err(|_| "插件状态不可用")?;
        let retained: HashSet<PathBuf> = records
            .values()
            .flat_map(|r| [&r.current, &r.previous])
            .flatten()
            .filter_map(|p| p.directory.as_ref())
            .map(|p| self.root.join(p))
            .collect();
        for d in definitions() {
            let base = self.root.join("packages").join(d.id);
            if !base.exists() {
                continue;
            }
            for entry in std::fs::read_dir(base)
                .map_err(|e| e.to_string())?
                .flatten()
            {
                let path = entry.path();
                if retained.contains(&path) {
                    continue;
                }
                // Never recurse through a link/reparse point.
                if entry.file_type().map_err(|e| e.to_string())?.is_dir()
                    && !entry.file_type().map_err(|e| e.to_string())?.is_symlink()
                {
                    std::fs::remove_dir_all(path).map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(())
    }
}
pub fn initialize(app: &tauri::AppHandle) -> Result<Arc<PluginStore>, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("plugins");
    Ok(initialize_at(app, root))
}
pub fn initialize_at(app: &tauri::AppHandle, root: PathBuf) -> Arc<PluginStore> {
    // Optional plugin metadata cannot prevent login or the pet from starting.
    let store = Arc::new(
        PluginStore::open(root.clone()).unwrap_or_else(|error| PluginStore {
            root,
            records: Mutex::new(Registry::new()),
            operation: tokio::sync::Mutex::new(()),
            started: HashSet::new(),
            load_error: Some(error),
        }),
    );
    let _ = store.collect_code();
    app.manage(store.clone());
    store
}
pub fn require_skills_cli() -> Result<(), String> {
    let data = molly_skills::embedded::private_root();
    let path = data
        .parent()
        .ok_or("无法定位 Molly 插件目录")?
        .join("plugins/installed.json");
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.to_string()),
    };
    let records: Registry = serde_json::from_slice(&bytes).map_err(|_| "插件记录损坏")?;
    let d = definition("skills")?;
    if records
        .get("skills")
        .and_then(|r| r.current.as_ref())
        .is_some_and(|p| compatible(&p.package, &d))
    {
        Ok(())
    } else {
        Err("Skill 管理器插件未安装或不兼容，请在 Molly 控制台中安装。".into())
    }
}
pub fn require(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    if let Some(store) = app.try_state::<Arc<PluginStore>>() {
        if !store.enabled(id) {
            return Err(
                "请先在设置 → 功能插件中安装插件；安装后如有提示请重启 MollyCloud。".into(),
            );
        }
    }
    Ok(()) // Test harnesses without a plugin registry use isolated fixtures.
}
fn console(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == "console" {
        Ok(())
    } else {
        Err("插件管理仅供控制台使用".into())
    }
}
#[tauri::command]
pub fn list_console_plugins(
    window: tauri::WebviewWindow,
    store: State<'_, Arc<PluginStore>>,
) -> Result<Vec<Status>, String> {
    console(&window)?;
    store.list()
}
#[tauri::command]
pub async fn change_console_plugin(
    window: tauri::WebviewWindow,
    store: State<'_, Arc<PluginStore>>,
    id: String,
    action: String,
) -> Result<Vec<Status>, String> {
    console(&window)?;
    let d = definition(&id)?;
    let _guard = store.operation.lock().await;
    if let Some(error) = &store.load_error {
        return Err(error.clone());
    }
    match action.as_str() {
        "install" => store.replace(&id, Some(bundled(&d)), false)?,
        "uninstall" => store.replace(&id, None, false)?,
        "rollback" => {
            let valid = store
                .records
                .lock()
                .map_err(|_| "插件状态不可用")?
                .get(&id)
                .and_then(|r| r.previous.as_ref())
                .is_some_and(|p| compatible(&p.package, &d));
            if !valid {
                return Err("没有可恢复的兼容版本".into());
            }
            store.replace(&id, None, true)?;
        }
        "update" => {
            let catalog = fetch_catalog().await?;
            let p = catalog
                .plugins
                .into_iter()
                .find(|p| p.id == id)
                .ok_or("尚未发布此插件的适配更新")?;
            check_package(&p, &d)?;
            {
                let records = store.records.lock().map_err(|_| "插件状态不可用")?;
                if let Some(old) = records.get(&id).and_then(|r| r.current.as_ref()) {
                    if semver::Version::parse(&p.version).map_err(|e| e.to_string())?
                        <= semver::Version::parse(&old.package.version)
                            .map_err(|e| e.to_string())?
                    {
                        return Err("当前已是此渠道的最新适配版本".into());
                    }
                }
            }
            let bytes = download(&p.url, MAX_DOWNLOAD).await?;
            let installer = store.inner().clone();
            tauri::async_runtime::spawn_blocking(move || installer.install_archive(p, bytes))
                .await
                .map_err(|e| e.to_string())??;
        }
        _ => return Err("不支持的插件操作".into()),
    }
    store.list()
}
async fn download(url: &str, limit: usize) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .connect_timeout(Duration::from_secs(12))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client
        .get(url)
        .header("User-Agent", "MollyCloud-Plugins")
        .send()
        .await
        .map_err(|_| "网络连接失败，请稍后重试")?;
    if !response.status().is_success() {
        return Err(format!("来源返回 HTTP {}", response.status().as_u16()));
    }
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err("来源文件超过大小限制".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "下载中断，已保留原版本")?
    {
        if bytes.len() + chunk.len() > limit {
            return Err("来源文件超过大小限制".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
async fn fetch_catalog() -> Result<Catalog, String> {
    let bytes = download(CATALOG, 1024 * 1024).await?;
    let c: Catalog = serde_json::from_slice(&bytes).map_err(|_| "插件目录格式无效")?;
    let mut seen = HashSet::new();
    if c.schema != 1
        || c.plugins.len() > 16
        || c.plugins
            .iter()
            .any(|p| definition(&p.id).is_err() || !seen.insert(&p.id))
    {
        return Err("插件目录版本或条目无效".into());
    }
    Ok(c)
}
#[tauri::command]
pub async fn check_console_plugin(
    window: tauri::WebviewWindow,
    store: State<'_, Arc<PluginStore>>,
    id: String,
) -> Result<UpdateCheck, String> {
    console(&window)?;
    let d = definition(&id)?;
    let official = async {
        let data = download(
            &format!(
                "https://api.github.com/repos/{}/releases/latest",
                d.repository
            ),
            1024 * 1024,
        )
        .await?;
        let v: serde_json::Value = serde_json::from_slice(&data).map_err(|_| "官方版本格式无效")?;
        v.get("tag_name")
            .and_then(|v| v.as_str())
            .map(|s| s.trim_start_matches('v').to_owned())
            .ok_or_else(|| "未找到官方发行版".to_owned())
    };
    let (official, catalog) = tokio::join!(official, fetch_catalog());
    let (official_version, official_error) = match official {
        Ok(v) => (Some(v), None),
        Err(e) => (None, Some(e)),
    };
    let (package, catalog_error) = match catalog {
        Ok(c) => (c.plugins.into_iter().find(|p| p.id == id), None),
        Err(e) => (None, Some(e)),
    };
    let compatible = package
        .as_ref()
        .is_some_and(|p| check_package(p, &d).is_ok());
    let current = store
        .records
        .lock()
        .map_err(|_| "插件状态不可用")?
        .get(&id)
        .and_then(|r| r.current.as_ref())
        .map(|p| p.package.version.clone());
    let update_available = package
        .as_ref()
        .and_then(|p| semver::Version::parse(&p.version).ok())
        .is_some_and(|v| {
            current
                .as_ref()
                .and_then(|s| semver::Version::parse(s).ok())
                .is_none_or(|old| v > old)
        });
    Ok(UpdateCheck {
        official_version,
        official_error,
        package,
        compatible,
        update_available,
        catalog_error,
    })
}
fn unpack(bytes: &[u8], target: &Path, expected: &Package) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "插件不是有效 ZIP")?;
    if zip.len() > 12000 {
        return Err("插件文件数量超限".into());
    }
    let mut total = 0u64;
    let mut seen = HashSet::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().trim_end_matches('/').to_owned();
        if !safe_relative(&name)
            || !seen.insert(name.to_lowercase())
            || entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
        {
            return Err("插件包含不安全路径或重复文件".into());
        }
        if entry.is_dir() {
            continue;
        }
        let ext = Path::new(&name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(
            ext.as_str(),
            "html"
                | "webmanifest"
                | "js"
                | "mjs"
                | "css"
                | "json"
                | "png"
                | "jpg"
                | "jpeg"
                | "gif"
                | "webp"
                | "avif"
                | "svg"
                | "ico"
                | "woff"
                | "woff2"
                | "ttf"
                | "map"
                | "txt"
                | "md"
                | "wasm"
        ) && name != "LICENSE"
        {
            return Err(format!("插件包含不支持的文件：{name}"));
        }
        total = total.checked_add(entry.size()).ok_or("插件解压大小无效")?;
        if total > MAX_UNPACKED {
            return Err("插件解压大小超限".into());
        }
        let path = target.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let mut data = Vec::new();
        (&mut entry)
            .take(MAX_UNPACKED + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        if data.len() as u64 != entry.size() {
            return Err("插件文件大小无效".into());
        }
        std::fs::write(path, data).map_err(|e| e.to_string())?;
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Manifest {
        id: String,
        version: String,
        adapter_api: String,
        host_version: String,
        upstream_version: String,
    }
    let manifest: Manifest = serde_json::from_slice(
        &std::fs::read(target.join("plugin.json")).map_err(|_| "缺少 plugin.json")?,
    )
    .map_err(|_| "插件清单无效")?;
    if manifest.id != expected.id
        || manifest.version != expected.version
        || manifest.adapter_api != expected.adapter_api
        || manifest.host_version != expected.host_version
        || manifest.upstream_version != expected.upstream_version
        || !target.join("index.html").is_file()
    {
        return Err("插件清单与发布目录不一致".into());
    }
    Ok(())
}
pub fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript",
        "css" => "text/css",
        "json" => "application/json",
        "webmanifest" => "application/manifest+json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "webp" => "image/webp",
        "jpg" | "jpeg" => "image/jpeg",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}
/// None = bundled asset; Some(Err) = fail closed; Some(Ok) = installed file.
pub fn asset<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    id: &str,
    path: &str,
) -> Option<Result<Vec<u8>, String>> {
    let store = app.try_state::<Arc<PluginStore>>()?;
    let records = match store.records.lock() {
        Ok(r) => r,
        Err(_) => return Some(Err("插件状态不可用".into())),
    };
    let current = match records.get(id).and_then(|r| r.current.as_ref()) {
        Some(p) => p,
        None => return Some(Err("插件未安装".into())),
    };
    if !safe_relative(path) || !definition(id).is_ok_and(|d| compatible(&current.package, &d)) {
        return Some(Err("插件路径或版本无效".into()));
    }
    let directory = current.directory.as_ref()?;
    let root = store.root.join(directory);
    let file = root.join(path);
    let valid = file
        .canonicalize()
        .ok()
        .zip(root.canonicalize().ok())
        .is_some_and(|(f, r)| f.starts_with(r));
    Some(if valid {
        std::fs::read(file).map_err(|_| "插件文件不存在".into())
    } else {
        Err("插件文件不存在".into())
    })
}
pub fn intercept(
    app: &tauri::AppHandle,
    request: tauri::http::Request<Vec<u8>>,
    response: &mut tauri::http::Response<std::borrow::Cow<'static, [u8]>>,
) {
    let raw = request.uri().path().trim_start_matches('/');
    for d in definitions() {
        let virtual_prefix = format!("molly-plugins/{}/", d.id);
        let virtual_path = raw.strip_prefix(&virtual_prefix);
        if let Some(path) = virtual_path.or_else(|| raw.strip_prefix(&format!("{}/", d.asset_dir)))
        {
            let result = if request.method() != tauri::http::Method::GET {
                Some(Err("不支持的请求".into()))
            } else {
                asset(app, &d.id, path).or_else(|| {
                    if virtual_path.is_some() {
                        Some(
                            app.asset_resolver()
                                .get(format!("{}/{path}", d.asset_dir))
                                .map(|a| a.bytes)
                                .ok_or_else(|| "插件资源不存在".into()),
                        )
                    } else {
                        None
                    }
                })
            };
            if let Some(result) = result {
                let (status, bytes) = match result {
                    Ok(b) => (200, b),
                    Err(_) => (404, Vec::new()),
                };
                *response.status_mut() = tauri::http::StatusCode::from_u16(status).unwrap();
                response
                    .headers_mut()
                    .insert("Content-Type", mime(path).parse().unwrap());
                response
                    .headers_mut()
                    .insert("Cache-Control", "no-store".parse().unwrap());
                response.headers_mut().remove("Content-Length");
                *response.body_mut() = std::borrow::Cow::Owned(bytes);
            }
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;
    fn package() -> Package {
        let d = definition("ccswitch").unwrap();
        Package {
            version: "3.20.4-molly.2".into(),
            url: "https://desktop.veriolink.com/plugins/test.zip".into(),
            sha256: "a".repeat(64),
            size: 128,
            ..bundled(&d).package
        }
    }
    fn archive(p: &Package, extra: Option<&str>, wrong_id: bool) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name,data) in [("index.html", "<html><body>plugin</body></html>".to_owned()),("plugin.json",serde_json::json!({"id":if wrong_id {"images"} else {&p.id},"version":p.version,"adapterApi":p.adapter_api,"hostVersion":p.host_version,"upstreamVersion":p.upstream_version}).to_string())] {
            zip.start_file(name,SimpleFileOptions::default()).unwrap();zip.write_all(data.as_bytes()).unwrap();
        }
        if let Some(path) = extra {
            zip.start_file(path, SimpleFileOptions::default()).unwrap();
            zip.write_all(b"sentinel").unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
    #[test]
    fn install_rollback_uninstall_retains_user_data_and_survives_restart() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("ccswitch/providers.json");
        std::fs::create_dir_all(data.parent().unwrap()).unwrap();
        std::fs::write(&data, "fake-provider").unwrap();
        let root = temp.path().join("plugins");
        let store = PluginStore::open(root.clone()).unwrap();
        let d = definition("ccswitch").unwrap();
        let mut installed = bundled(&d);
        installed.package.version = "3.20.4-molly.2".into();
        store.replace("ccswitch", Some(installed), false).unwrap();
        assert!(
            store
                .list()
                .unwrap()
                .iter()
                .find(|p| p.definition.id == "ccswitch")
                .unwrap()
                .can_rollback
        );
        store.replace("ccswitch", None, true).unwrap();
        assert_eq!(
            store.list().unwrap()[0].installed_version.as_deref(),
            Some("3.20.4-molly.1")
        );
        store.replace("ccswitch", None, false).unwrap();
        assert!(!store.enabled("ccswitch"));
        assert!(store.list().unwrap()[0].restart_required);
        drop(store);
        let store = PluginStore::open(root).unwrap();
        assert!(!store.list().unwrap()[0].restart_required);
        store.replace("ccswitch", Some(bundled(&d)), false).unwrap();
        assert!(!store.enabled("ccswitch")); // Native adapter starts only after restart.
        assert!(store.list().unwrap()[0].restart_required);
        assert_eq!(std::fs::read_to_string(data).unwrap(), "fake-provider");
    }
    #[test]
    fn rejected_packages_leave_the_current_version_intact() {
        let temp = tempfile::tempdir().unwrap();
        let store = PluginStore::open(temp.path().join("plugins")).unwrap();
        let before = std::fs::read(store.root.join("installed.json")).unwrap();
        let p = package();
        for path in [
            "../outside.txt",
            "C:/outside.txt",
            "assets/CON.txt",
            "assets/trailing.",
            "assets/payload.exe",
            "INDEX.HTML",
            "assets\\escape.txt",
        ] {
            let stage = tempfile::tempdir_in(temp.path()).unwrap();
            assert!(
                unpack(&archive(&p, Some(path), false), stage.path(), &p).is_err(),
                "{path}"
            );
        }
        let stage = tempfile::tempdir_in(temp.path()).unwrap();
        assert!(unpack(&archive(&p, None, true), stage.path(), &p).is_err());
        assert_eq!(
            std::fs::read(store.root.join("installed.json")).unwrap(),
            before
        );
        assert!(!temp.path().join("outside.txt").exists());
    }
    #[test]
    fn compatible_packages_extract_and_code_gc_stays_inside_plugin_tree() {
        let temp = tempfile::tempdir().unwrap();
        let store = PluginStore::open(temp.path().join("plugins")).unwrap();
        let p = package();
        check_package(&p, &definition("ccswitch").unwrap()).unwrap();
        let dir = "packages/ccswitch/kept";
        let target = store.root.join(dir);
        std::fs::create_dir_all(&target).unwrap();
        unpack(&archive(&p, Some("assets/app.js"), false), &target, &p).unwrap();
        store
            .replace(
                "ccswitch",
                Some(Installed {
                    package: p,
                    directory: Some(dir.into()),
                }),
                false,
            )
            .unwrap();
        let unused = store.root.join("packages/ccswitch/unused");
        std::fs::create_dir_all(&unused).unwrap();
        std::fs::create_dir_all(store.root.join("../skills-manager/library")).unwrap();
        store.collect_code().unwrap();
        assert!(!unused.exists());
        assert!(target.join("index.html").is_file());
        assert!(store.root.join("../skills-manager/library").is_dir());
    }
    #[test]
    fn verified_archive_install_and_digest_failure_are_transactional() {
        let temp = tempfile::tempdir().unwrap();
        let store = PluginStore::open(temp.path().join("plugins")).unwrap();
        let mut p = package();
        let bytes = archive(&p, None, false);
        p.size = bytes.len();
        p.sha256 = format!("{:x}", Sha256::digest(&bytes));
        let mut invalid = p.clone();
        invalid.sha256 = "0".repeat(64);
        assert!(store.install_archive(invalid, bytes.clone()).is_err());
        assert_eq!(
            store.list().unwrap()[0].installed_version.as_deref(),
            Some("3.20.4-molly.1")
        );
        store.install_archive(p, bytes).unwrap();
        assert_eq!(
            store.list().unwrap()[0].installed_version.as_deref(),
            Some("3.20.4-molly.2")
        );
        store.replace("ccswitch", None, true).unwrap();
        assert_eq!(
            store.list().unwrap()[0].installed_version.as_deref(),
            Some("3.20.4-molly.1")
        );
    }
    #[test]
    fn release_packages_match_the_native_installer_format() {
        let Some(directory) = std::env::var_os("MOLLY_PLUGIN_FIXTURES") else {
            return;
        };
        let root = PathBuf::from(directory);
        let catalog: Catalog =
            serde_json::from_slice(&std::fs::read(root.join("index.json")).unwrap()).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let store = PluginStore::open(temp.path().join("plugins")).unwrap();
        for p in catalog.plugins {
            let name = p.url.rsplit('/').next().unwrap();
            let bytes = std::fs::read(root.join(name)).unwrap();
            store.install_archive(p, bytes).unwrap();
        }
        assert!(store
            .list()
            .unwrap()
            .iter()
            .all(|p| p.installed && p.usable));
    }
    #[test]
    fn compatibility_and_download_origins_fail_closed() {
        let d = definition("ccswitch").unwrap();
        let mut p = package();
        p.adapter_api = "unknown".into();
        assert!(check_package(&p, &d).is_err());
        p = package();
        p.host_version = ">=99.0.0".into();
        assert!(check_package(&p, &d).is_err());
        for url in [
            "http://desktop.veriolink.com/plugins/a.zip",
            "https://example.com/plugins/a.zip",
            "https://desktop.veriolink.com/other/a.zip",
            "https://user@desktop.veriolink.com/plugins/a.zip",
        ] {
            p = package();
            p.url = url.into();
            assert!(check_package(&p, &d).is_err());
        }
    }
    #[test]
    fn failed_registry_commit_does_not_change_live_state() {
        let temp = tempfile::tempdir().unwrap();
        let store = PluginStore::open(temp.path().join("plugins")).unwrap();
        std::fs::remove_file(store.root.join("installed.json")).unwrap();
        std::fs::create_dir(store.root.join("installed.json")).unwrap();
        assert!(store.replace("ccswitch", None, false).is_err());
        assert!(store.enabled("ccswitch"));
    }
}
#[cfg(feature = "plugin-smoke")]
pub fn initialize_smoke(app: &tauri::AppHandle, root: PathBuf) -> Result<(), String> {
    let store = Arc::new(PluginStore::open(root)?);
    let d = definition("ccswitch")?;
    for (name, version) in [("old", "3.20.4-molly.1"), ("new", "3.20.4-molly.2")] {
        let dir = format!("packages/ccswitch/{name}");
        let path = store.root.join(&dir);
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        std::fs::write(
            path.join("index.html"),
            "<!doctype html><html><body><script src='./probe.js'></script></body></html>",
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(path.join("probe.js"),format!("parent.postMessage({{probe:'{name}',sameOrigin:parent.location.origin===location.origin}},location.origin)" )).map_err(|e|e.to_string())?;
        let mut p = bundled(&d);
        p.package.version = version.into();
        p.directory = Some(dir);
        store.replace("ccswitch", Some(p), false)?;
    }
    app.manage(store);
    Ok(())
}
