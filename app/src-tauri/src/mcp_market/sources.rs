//! Discovery documents become bounded, reviewable plans; never execute documentation commands.
use super::catalog::{self, Entry, Field, Recipe};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolution {
    pub plans: Vec<Plan>,
    pub notes: Vec<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub plan_id: String,
    pub label: String,
    pub source_url: String,
    pub entry: Entry,
}

pub(super) async fn fetch(url: &str) -> Result<Option<String>, String> {
    let mut response = super::process::network_client(url)?
        .get(url)
        .send()
        .await
        .map_err(|_| "来源连接失败，请检查网络后重试")?;
    if response.status() == 404 {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(format!("来源返回 {}，请稍后重试", response.status()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "来源读取失败")? {
        if bytes.len() + chunk.len() > 2_000_000 {
            return Err("来源内容超过 2 MB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| "来源不是 UTF-8 文本".into())
}
async fn get_json(url: &str) -> Result<Value, String> {
    serde_json::from_str(&fetch(url).await?.ok_or("来源未找到")?)
        .map_err(|_| "来源 JSON 格式无法识别".into())
}
fn string(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().into()
}
fn array<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn recipe(kind: &str) -> Recipe {
    serde_json::from_value(json!({"kind":kind,"version":"remote"})).unwrap()
}
fn stable_id(source: &str, kind: &str, identity: &str) -> String {
    // FNV-1a gives a version-independent configuration key (not a security digest).
    let mut hash = 0xcbf29ce484222325_u64;
    for part in [source, kind, identity] {
        for byte in part.bytes().chain(std::iter::once(0)) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    format!("source-{hash:016x}")
}

pub(super) fn validate_package(kind: &str, name: &str, version: &str) -> Result<(), String> {
    let word = |s: &str| {
        !s.is_empty()
            && s.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric())
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    };
    let valid = if kind == "npm" {
        if let Some(scoped) = name.strip_prefix('@') {
            scoped
                .split_once('/')
                .is_some_and(|(a, b)| word(a) && word(b))
        } else {
            word(name)
        }
    } else {
        kind == "pypi" && word(name)
    };
    if !valid || name.len() > 214 {
        return Err("仅支持 npm / PyPI 的包名，不支持脚本、Git 或本地路径依赖".into());
    }
    if !version.is_empty()
        && version != "latest"
        && (version.len() > 100
            || !version.bytes().next().is_some_and(|b| b.is_ascii_digit())
            || !version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-+_!".contains(&b)))
    {
        return Err("来源版本不是固定版本，请手动配置".into());
    }
    Ok(())
}
pub(super) fn validate_bin(kind: &str, bin: &str) -> Result<(), String> {
    let path = bin.replace('\\', "/");
    if path.is_empty()
        || path.starts_with('/')
        || path.contains(':')
        || path.chars().any(char::is_control)
        || path.split('/').any(|s| s == ".." || s.is_empty())
        || (kind == "pypi" && path.contains('/'))
    {
        return Err("软件包启动入口无效".into());
    }
    Ok(())
}

// Registry inputs are translated once. User values are never interpreted as templates.
fn input(v: &Value, label: &str, fields: &mut Vec<Field>) -> Result<String, String> {
    if let Some(value) = v["value"].as_str() {
        let mut substitutions = BTreeMap::new();
        if let Some(vars) = v["variables"].as_object() {
            for (name, definition) in vars {
                substitutions.insert(
                    name.clone(),
                    input(definition, &format!("{label} · {name}"), fields)?,
                );
            }
        }
        return Ok(super::process::expand_template(
            value,
            &substitutions,
            "{dataDir}",
        ));
    }
    if fields.len() >= 40 {
        return Err("来源需要的参数过多，请手动配置".into());
    }
    let key = format!("input{}", fields.len() + 1);
    let default = v["default"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| {
            v.get("default")
                .filter(|v| v.is_boolean() || v.is_number())
                .map(Value::to_string)
        })
        .unwrap_or_default();
    fields.push(Field {
        key: key.clone(),
        label: label.into(),
        kind: if v["isSecret"] == true {
            "secret"
        } else {
            "text"
        }
        .into(),
        required: v["isRequired"] == true,
        default,
        placeholder: string(v, "placeholder"),
        choices: array(v, "choices")
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
    });
    Ok(format!("{{{key}}}"))
}
fn variables(
    value: &str,
    vars: &Value,
    label: &str,
    fields: &mut Vec<Field>,
) -> Result<String, String> {
    input(&json!({"value":value,"variables":vars}), label, fields)
}
fn key_values(
    values: &[Value],
    fields: &mut Vec<Field>,
) -> Result<BTreeMap<String, String>, String> {
    values
        .iter()
        .map(|v| {
            let name = v["name"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("来源参数缺少名称")?;
            Ok((name.into(), input(v, name, fields)?))
        })
        .collect()
}
pub(super) fn registry_recipes(server: &Value) -> (Vec<(Recipe, Vec<Field>)>, Vec<String>) {
    let mut result = vec![];
    let mut notes = vec![];
    for remote in array(server, "remotes").iter().take(12) {
        let parsed = (|| {
            let transport = remote["type"].as_str().unwrap_or_default();
            if transport != "streamable-http" {
                return Err("旧版 SSE 远程连接请通过手动配置添加".into());
            }
            let mut r = recipe("remote");
            let mut fields = vec![];
            r.url = variables(
                remote["url"].as_str().ok_or("来源缺少远程地址")?,
                &remote["variables"],
                "服务地址",
                &mut fields,
            )?;
            r.headers = key_values(array(remote, "headers"), &mut fields)?;
            Ok((r, fields))
        })();
        match parsed {
            Ok(v) => result.push(v),
            Err(e) => notes.push(e),
        }
    }
    for package in array(server, "packages").iter().take(12) {
        let parsed = (|| {
            let kind = package["registryType"].as_str().unwrap_or_default();
            if !["npm", "pypi"].contains(&kind) {
                return Err(format!(
                    "{kind} 软件包暂需手动安装（目前支持 npm、PyPI、HTTP）"
                ));
            }
            let base = string(package, "registryBaseUrl");
            let allowed: &[&str] = if kind == "npm" {
                &["https://registry.npmjs.org"]
            } else {
                &["https://pypi.org", "https://pypi.org/simple"]
            };
            if !base.is_empty() && !allowed.contains(&base.trim_end_matches('/')) {
                return Err("自定义软件源暂需手动配置".into());
            }
            if package["transport"]["type"] != "stdio"
                || !array(package, "runtimeArguments").is_empty()
                || package.get("fileSha256").is_some()
            {
                return Err(
                    "软件包需要额外的运行时参数、校验或传输适配，请按来源文档手动配置".into(),
                );
            }
            let hint = string(package, "runtimeHint");
            if !hint.is_empty() && hint != (if kind == "npm" { "npx" } else { "uvx" }) {
                return Err("来源要求不同的运行时，请手动配置".into());
            }
            let mut r = recipe(kind);
            let mut fields = vec![];
            r.package = string(package, "identifier");
            r.version = string(package, "version");
            validate_package(kind, &r.package, &r.version)?;
            r.env = key_values(array(package, "environmentVariables"), &mut fields)?;
            for arg in array(package, "packageArguments") {
                if arg["isRepeated"] == true {
                    return Err("重复参数暂需手动配置".into());
                }
                let named = arg["type"] == "named";
                let name = if named {
                    string(arg, "name")
                } else {
                    string(arg, "valueHint")
                };
                if named && (!name.starts_with('-') || name.chars().any(char::is_whitespace)) {
                    return Err("命名参数格式无法识别".into());
                }
                let before = fields.len();
                let value = input(
                    arg,
                    if name.is_empty() { "参数" } else { &name },
                    &mut fields,
                )?;
                let mut parts = vec![];
                if named {
                    parts.push(name);
                }
                if !named || !value.is_empty() {
                    parts.push(value);
                }
                if fields.len() == before + 1
                    && arg.get("value").is_none()
                    && !fields[before].required
                    && fields[before].default.is_empty()
                {
                    if !named {
                        return Err("可选位置参数请手动配置".into());
                    }
                    r.optional_args.insert(fields[before].key.clone(), parts);
                } else {
                    r.args.extend(parts);
                }
            }
            Ok((r, fields))
        })();
        match parsed {
            Ok(v) => result.push(v),
            Err(e) => notes.push(e),
        }
    }
    (result, notes)
}

fn placeholder(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    value.is_empty()
        || value.starts_with("${")
        || value.starts_with('<')
        || value.starts_with('{')
        || lower.contains("your_")
        || lower.contains("your-")
        || lower.contains("replace_")
        || lower.contains("replace-")
        || lower.contains("api_key_here")
        || lower.contains("token_here")
        || lower.replace('\\', "/").contains("/path/to/")
        || lower.replace('\\', "/").contains("/users/username/")
}
fn community_value(
    value: &str,
    label: &str,
    secret: bool,
    fields: &mut Vec<Field>,
) -> Result<String, String> {
    if placeholder(value) {
        input(
            &json!({"isRequired":true,"isSecret":secret,"placeholder":value}),
            label,
            fields,
        )
    } else {
        Ok(value.into())
    }
}
pub(super) fn config_recipe(spec: &Value) -> Result<(Recipe, Vec<Field>), String> {
    let mut fields = vec![];
    if let Some(url) = spec["url"].as_str().or(spec["httpUrl"].as_str()) {
        if spec.as_object().is_some_and(|o| {
            o.keys()
                .any(|k| !["url", "httpUrl", "headers", "type", "disabled"].contains(&k.as_str()))
        }) {
            return Err("远程配置包含额外设置，请手动添加".into());
        }
        if spec["type"] == "sse" {
            return Err("SSE 配置需手动添加".into());
        }
        catalog::validate_url(url)?;
        if url.contains("example.") || url.contains("YOUR") || url.contains('{') {
            return Err("文档中的示例地址需要手动补充".into());
        }
        let mut r = recipe("remote");
        r.url = url.into();
        if let Some(headers) = spec["headers"].as_object() {
            for (k, v) in headers {
                let v = v.as_str().ok_or("请求头格式无法识别")?;
                let value = if let Some(token) = v.strip_prefix("Bearer ") {
                    format!("Bearer {}", community_value(token, k, true, &mut fields)?)
                } else {
                    community_value(v, k, true, &mut fields)?
                };
                r.headers.insert(k.clone(), value);
            }
        }
        return Ok((r, fields));
    }
    let command = spec["command"].as_str().unwrap_or("");
    let mut args: Vec<String> = spec["args"]
        .as_array()
        .ok_or("不是可识别的连接配置")?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or("参数必须是字符串".to_owned())
        })
        .collect::<Result<_, _>>()?;
    let kind = match command {
        "npx" | "npx.cmd" => "npm",
        "uvx" | "uvx.exe" => "pypi",
        _ => return Err("仅自动转换 npx / uvx 安装及 HTTP 连接；其他命令需手动配置".into()),
    };
    if kind == "npm" && args.first().is_some_and(|s| s == "-y" || s == "--yes") {
        args.remove(0);
    }
    if args.is_empty() {
        return Err("缺少软件包名".into());
    }
    let token = args.remove(0);
    let mut r = recipe(kind);
    let split = if kind == "npm" {
        token
            .rfind('@')
            .filter(|i| *i > 0)
            .map(|i| (&token[..i], &token[i + 1..]))
    } else {
        token.split_once("==").or_else(|| token.split_once('@'))
    };
    let (name, version) = split.unwrap_or((&token, ""));
    validate_package(kind, name, version)?;
    r.package = name.into();
    r.version = version.into();
    for (i, arg) in args.iter().enumerate() {
        r.args.push(community_value(
            arg,
            &format!("参数 {}", i + 1),
            false,
            &mut fields,
        )?);
    }
    if let Some(env) = spec["env"].as_object() {
        for (k, v) in env {
            r.env.insert(
                k.clone(),
                community_value(v.as_str().ok_or("环境变量格式错误")?, k, true, &mut fields)?,
            );
        }
    }
    // Do not silently discard command execution semantics from a document.
    if spec.as_object().is_some_and(|o| {
        o.keys()
            .any(|k| !["command", "args", "env", "type", "disabled"].contains(&k.as_str()))
    }) {
        return Err("配置包含额外运行设置，请手动添加以保留这些设置".into());
    }
    Ok((r, fields))
}
pub(super) fn document_configs(text: &str) -> Vec<Value> {
    fn collect(v: Value, out: &mut Vec<Value>) {
        if let Some(entries) = v["mcpServers"].as_object() {
            out.extend(entries.values().cloned());
        } else if v.get("command").is_some() || v.get("url").is_some() {
            out.push(v);
        }
    }
    let mut out = vec![];
    if let Ok(v) = serde_json::from_str(text) {
        collect(v, &mut out);
    }
    for block in text.split("```").skip(1).step_by(2).take(60) {
        if let Some((_, body)) = block.split_once('\n') {
            if let Ok(v) = serde_json::from_str(body.trim()) {
                collect(v, &mut out);
            }
        }
    }
    out.truncate(24);
    out
}
pub(super) async fn pin(mut r: Recipe) -> Result<Recipe, String> {
    if r.kind == "remote" {
        // Resolve URL placeholders to harmless dummy values for URL syntax validation.
        let sample = r.url.replace('{', "").replace('}', "");
        catalog::validate_url(&sample)?;
        return Ok(r);
    }
    validate_package(&r.kind, &r.package, &r.version)?;
    let mut url = url::Url::parse(if r.kind == "npm" {
        "https://registry.npmjs.org"
    } else {
        "https://pypi.org"
    })
    .unwrap();
    if r.kind == "npm" {
        url.path_segments_mut()
            .unwrap()
            .push(&r.package)
            .push(if r.version.is_empty() {
                "latest"
            } else {
                &r.version
            });
        let data = get_json(url.as_str()).await?;
        r.version = string(&data, "version");
        r.bin = if let Some(bin) = data["bin"].as_str() {
            bin.into()
        } else if let Some(bins) = data["bin"].as_object().filter(|o| o.len() == 1) {
            bins.values()
                .next()
                .and_then(Value::as_str)
                .unwrap_or("")
                .into()
        } else {
            return Err("软件包提供多个启动入口或没有入口，请按来源文档手动配置".into());
        };
        validate_bin(&r.kind, &r.bin)?;
    } else {
        {
            let mut parts = url.path_segments_mut().unwrap();
            parts.push("pypi").push(&r.package);
            if !r.version.is_empty() && r.version != "latest" {
                parts.push(&r.version);
            }
            parts.push("json");
        }
        let data = get_json(url.as_str()).await?;
        r.version = string(&data["info"], "version");
        // The venv installer reads the installed distribution's console_scripts metadata.
        r.bin.clear();
    }
    if r.version.is_empty() || r.version == "latest" {
        return Err("无法确定固定版本".into());
    }
    validate_package(&r.kind, &r.package, &r.version)?;
    Ok(r)
}

pub async fn resolve(source: &str, id: &str) -> Result<Resolution, String> {
    let mut candidates: Vec<(Recipe, Vec<Field>, String)> = vec![];
    let mut notes = vec![];
    let base: Entry;
    if source == "registry" {
        if id.len() > 300 || !id.contains('/') || id.chars().any(char::is_control) {
            return Err("官方目录标识无效".into());
        }
        let mut url =
            url::Url::parse("https://registry.modelcontextprotocol.io/v0.1/servers/").unwrap();
        url.path_segments_mut()
            .unwrap()
            .pop_if_empty()
            .push(id)
            .push("versions")
            .push("latest");
        let data = get_json(url.as_str()).await?;
        let server = data.get("server").unwrap_or(&data);
        if server["name"] != id {
            return Err("来源标识不一致，请重新搜索".into());
        }
        base = Entry {
            id: id.into(),
            name: server["title"].as_str().unwrap_or(id).into(),
            description: string(server, "description"),
            category: "官方目录".into(),
            homepage: url.to_string(),
            source: source.into(),
            fields: vec![],
            recipe: None,
        };
        let (recipes, warnings) = registry_recipes(server);
        notes.extend(warnings);
        candidates.extend(recipes.into_iter().map(|(r, f)| (r, f, url.to_string())));
    } else if source == "awesome" {
        let directory: Vec<Entry> =
            serde_json::from_str(include_str!("../../../src/mcp/community.json"))
                .map_err(|_| "社区目录损坏")?;
        base = directory
            .into_iter()
            .find(|e| e.id == id)
            .ok_or("社区项目未找到，请刷新目录")?;
        let home = url::Url::parse(&base.homepage).map_err(|_| "仓库地址无效")?;
        let parts: Vec<_> = home
            .path_segments()
            .ok_or("仓库地址无效")?
            .filter(|s| !s.is_empty())
            .collect();
        if home.host_str() != Some("github.com")
            || parts.len() < 2
            || parts.iter().any(|s| {
                *s == ".."
                    || !s
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            })
        {
            return Err("目前支持从 GitHub 仓库提取配置".into());
        }
        let mut raw = None;
        let location = if parts.len() > 2 {
            if parts.get(2) != Some(&"tree") || parts.len() < 4 {
                return Err("仓库子目录链接暂无法识别，请查看来源手动配置".into());
            }
            parts[3..].join("/")
        } else {
            "HEAD".into()
        };
        // HEAD follows the repository's default branch; no API token or API rate quota needed.
        for file in ["server.json", ".mcp.json", "README.md", "readme.md"] {
            let url = format!(
                "https://raw.githubusercontent.com/{}/{}/{location}/{file}",
                parts[0], parts[1]
            );
            match fetch(&url).await {
                Ok(Some(text)) => {
                    if file == "server.json" {
                        if let Ok(server) = serde_json::from_str::<Value>(&text) {
                            let (recipes, warnings) = registry_recipes(&server);
                            notes.extend(warnings);
                            candidates
                                .extend(recipes.into_iter().map(|(r, f)| (r, f, url.clone())));
                        }
                    } else {
                        for config in document_configs(&text) {
                            match config_recipe(&config) {
                                Ok((r, f)) => candidates.push((r, f, url.clone())),
                                Err(e) => notes.push(e),
                            }
                        }
                    }
                    raw = Some(url);
                    if !candidates.is_empty() {
                        break;
                    }
                }
                Ok(None) => (),
                Err(e) => {
                    notes.push(e);
                    break;
                }
            }
        }
        if raw.is_none() && notes.is_empty() {
            notes.push("仓库中未找到公开安装说明".into());
        }
    } else {
        return Err("不支持的目录来源".into());
    }
    let mut plans = vec![];
    let mut seen = std::collections::BTreeSet::new();
    for (r, fields, url) in candidates.into_iter().take(12) {
        let fingerprint = serde_json::to_string(&(&r, &fields)).unwrap_or_default();
        if !seen.insert(fingerprint.clone()) {
            continue;
        }
        match pin(r).await {
            Ok(r) => {
                let label = if r.kind == "remote" {
                    format!("HTTP · {}", r.url)
                } else {
                    format!("{} · {} @ {}", r.kind, r.package, r.version)
                };
                let mut entry = base.clone();
                // Version-independent identity allows updates without changing an Agent key.
                let identity = serde_json::to_string(&(
                    &r.kind,
                    &r.package,
                    &r.url,
                    &r.args,
                    &r.env,
                    &r.headers,
                    &r.optional_args,
                ))
                .unwrap();
                entry.id = stable_id(&format!("{source}:{id}"), &r.kind, &identity);
                entry.fields = fields;
                entry.recipe = Some(r);
                plans.push(Plan {
                    plan_id: String::new(),
                    label,
                    source_url: url,
                    entry,
                });
            }
            Err(e) => notes.push(e),
        }
    }
    notes.sort();
    notes.dedup();
    notes.truncate(8);
    if plans.is_empty() {
        notes.push(
            "未识别到可自动安装的配置。可查看来源并通过“手动添加”补充；不会执行 README 中的脚本。"
                .into(),
        );
    }
    Ok(Resolution { plans, notes })
}
