//! Pure configuration editing for Molly's installer. No proxy or MCP process.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub fn config_path(agent: &str) -> Result<PathBuf, String> {
    let path = match agent {
        "codex" => crate::codex_config::get_codex_config_path(),
        "claude" => crate::config::get_claude_mcp_path(),
        "gemini" => crate::gemini_config::get_gemini_dir().join("settings.json"),
        "opencode" => {
            if let Some(path) = crate::config::cli_env_path("OPENCODE_CONFIG") {
                path
            } else {
                let dir = crate::opencode_config::get_opencode_dir();
                let json = dir.join("opencode.json");
                let jsonc = dir.join("opencode.jsonc");
                if json.exists() && jsonc.exists() {
                    return Err("OpenCode 同时存在 JSON 与 JSONC 配置，请通过 OPENCODE_CONFIG 指定实际使用的文件。".into());
                }
                if jsonc.exists() {
                    jsonc
                } else {
                    json
                }
            }
        }
        _ => return Err("不支持的 Agent".into()),
    };
    crate::embedded::require_config_path(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn target_spec(agent: &str, spec: &Value) -> Result<Value, String> {
    let mut result = spec.clone();
    match agent {
        "codex" => {
            let object = result.as_object_mut().ok_or("连接配置无效")?;
            if object.remove("type") == Some(json!("sse")) {
                return Err("Codex 请使用 Streamable HTTP 或本地服务".into());
            }
            if let Some(headers) = object.remove("headers") {
                object.insert("http_headers".into(), headers);
            }
        }
        "opencode" => {
            let local = spec.get("command").is_some();
            result = if local {
                let mut args = vec![spec["command"].clone()];
                args.extend(spec["args"].as_array().cloned().unwrap_or_default());
                json!({"type":"local", "command":args, "enabled":true})
            } else {
                json!({"type":"remote", "url":spec["url"], "enabled":true})
            };
            if let Some(env) = spec.get("env") {
                result["environment"] = env.clone();
            }
            if let Some(headers) = spec.get("headers") {
                result["headers"] = headers.clone();
            }
        }
        "gemini" => {
            result.as_object_mut().ok_or("连接配置无效")?.remove("type");
            if spec["type"] == "http" {
                if let Some(url) = result.as_object_mut().unwrap().remove("url") {
                    result["httpUrl"] = url;
                }
            }
        }
        "claude" => {}
        _ => return Err("不支持的 Agent".into()),
    }
    Ok(result)
}

fn key(agent: &str) -> Result<&'static str, String> {
    match agent {
        "codex" => Ok("mcp_servers"),
        "opencode" => Ok("mcp"),
        "claude" | "gemini" => Ok("mcpServers"),
        _ => Err("不支持的 Agent".into()),
    }
}
fn parse(agent: &str, source: &str) -> Result<Value, String> {
    let value = if agent == "codex" {
        let table: toml::Table = source
            .parse()
            .map_err(|_| "现有 TOML 配置格式错误，请先修复配置")?;
        serde_json::to_value(table).map_err(|_| "无法读取配置")?
    } else {
        json5::from_str(if source.trim().is_empty() {
            "{}"
        } else {
            source
        })
        .map_err(|_| "现有 JSON 配置格式错误，请先修复配置")?
    };
    if !value.is_object() {
        return Err("配置根节点必须是对象".into());
    }
    if value.get(key(agent)?).is_some_and(|v| !v.is_object()) {
        return Err("现有 MCP 配置必须是对象，未修改文件".into());
    }
    Ok(value)
}
pub fn entries(agent: &str, source: &str) -> Result<serde_json::Map<String, Value>, String> {
    Ok(parse(agent, source)?
        .get(key(agent)?)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default())
}
pub fn patch(agent: &str, source: &str, id: &str, spec: Option<&Value>) -> Result<String, String> {
    let current = parse(agent, source)?;
    let section = key(agent)?;
    let mut expected = current.clone();
    if expected.get(section).is_none() {
        expected[section] = json!({});
    }
    let servers = expected[section].as_object_mut().unwrap();
    if let Some(spec) = spec {
        servers.insert(id.into(), spec.clone());
    } else {
        servers.remove(id);
    }
    let output = if agent == "codex" {
        let mut doc: toml_edit::DocumentMut = source.parse().map_err(|_| "无法解析 TOML")?;
        if doc.get(section).is_none() {
            doc[section] = toml_edit::table();
        }
        let table = doc[section].as_table_like_mut().ok_or("MCP 配置必须是表")?;
        if let Some(spec) = spec {
            let text = toml::to_string(spec).map_err(|_| "无法转换 MCP 配置")?;
            let entry: toml_edit::DocumentMut = text.parse().map_err(|_| "无法转换 MCP 配置")?;
            table.insert(id, toml_edit::Item::Table(entry.as_table().clone()));
        } else {
            table.remove(id);
        }
        doc.to_string()
    } else {
        let mut doc = json_five::rt::parser::from_str(if source.trim().is_empty() {
            "{}"
        } else {
            source
        })
        .map_err(|_| "无法解析 JSON")?;
        crate::services::omo::merge_rt_value(
            &mut doc.value,
            &current,
            &expected,
            "",
            if source.contains("\r\n") {
                "\r\n"
            } else {
                "\n"
            },
        )
        .map_err(|e| e.to_string())?;
        doc.to_string()
    };
    if parse(agent, &output)? != expected {
        return Err("配置校验不一致，未写入文件".into());
    }
    Ok(output)
}

pub fn write(path: &Path, content: &[u8]) -> Result<(), String> {
    crate::config::atomic_write_private(path, content).map_err(|e| e.to_string())
}
