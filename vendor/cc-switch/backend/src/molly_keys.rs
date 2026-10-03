//! Account-key integration only changes the private provider library.
use crate::app_config::AppType;
use crate::{AppState, Database, Provider};
use std::str::FromStr;
use tauri::Manager;

const MOLLY_CODEX_MODELS: &[&str] = &[
    "gpt-5.6-luna",
    "gpt-5.6-terra",
    "gpt-5.6-sol",
    "gpt-6-luna",
    "gpt-6-sol",
    "gpt-6-astra",
    "gpt-6-astra-fast",
    "gpt-6.1-sol",
    "gpt-6.1-sol-fast",
];

/// Exact intersection only: aliases, prefixes and models absent from this key's group are excluded.
fn mapped_models(available: &[String]) -> Vec<&'static str> {
    MOLLY_CODEX_MODELS
        .iter()
        .copied()
        .filter(|id| available.iter().any(|m| m == id))
        .collect()
}

fn update_codex(
    provider: &mut Provider,
    requested: Option<&str>,
    available: Option<&[String]>,
) -> Result<bool, String> {
    let mut next = provider.clone();
    let meta = next.meta.get_or_insert_with(Default::default);
    let previous = meta.molly_codex_mode.as_deref().unwrap_or("native");
    let mode = requested.unwrap_or(previous);
    if !matches!(mode, "native" | "mapped") {
        return Err("配置模式无效".into());
    }
    let matches = if mode == "mapped" {
        let models = mapped_models(available.ok_or("映射模式需要先拉取分组模型")?);
        if models.is_empty() {
            return Err("当前分组没有白名单中的模型，未修改映射配置。".into());
        }
        models
    } else {
        Vec::new()
    };
    let text = next.settings_config["config"]
        .as_str()
        .ok_or("Codex 配置格式无效")?;
    let mut config = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| "Codex TOML 配置无效，请先编辑修复")?;
    let before = next.settings_config.clone();
    let native_context = config
        .get("model_context_window")
        .and_then(|v| v.as_integer());
    let initialized = meta.molly_context_initialized;
    if (!meta.molly_context_initialized && config.get("model_context_window").is_none())
        || mode == "mapped"
    {
        config["model_context_window"] = toml_edit::value(1_050_000i64);
        if !meta.molly_context_initialized && config.get("model_auto_compact_token_limit").is_none()
        {
            config["model_auto_compact_token_limit"] = toml_edit::value(900_000i64);
        }
    }
    meta.molly_context_initialized = true;
    if mode == "mapped" {
        if previous != "mapped" {
            meta.molly_native_context_window = if initialized {
                native_context
            } else {
                native_context.or(Some(1_050_000))
            };
            meta.molly_native_catalog = Some(
                next.settings_config
                    .get("modelCatalog")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            );
            meta.molly_native_model = config
                .get("model")
                .and_then(|v| v.as_str())
                .map(str::to_owned);
            meta.molly_native_catalog_pointer = config
                .get("model_catalog_json")
                .and_then(|v| v.as_str())
                .map(str::to_owned);
        }
        // The generated catalog owns this pointer when explicitly enabled.
        config.as_table_mut().remove("model_catalog_json");
        if !config
            .get("model")
            .and_then(|v| v.as_str())
            .is_some_and(|id| matches.contains(&id))
        {
            config["model"] = toml_edit::value(matches[0]);
        }
        let rows: Vec<_> = matches
            .iter()
            .map(|id| {
                serde_json::json!({
                    "model":id,"displayName":id,"contextWindow":1050000,
                    "reasoningLevels":["low","medium","high","xhigh","max","ultra"],
                    "defaultReasoningLevel":"high"
                })
            })
            .collect();
        next.settings_config["modelCatalog"] = serde_json::json!({"models":rows});
    } else if previous == "mapped" {
        match meta.molly_native_catalog.take() {
            Some(value) if !value.is_null() => {
                next.settings_config["modelCatalog"] = value;
            }
            _ => {
                next.settings_config
                    .as_object_mut()
                    .ok_or("Codex 配置格式无效")?
                    .remove("modelCatalog");
            }
        }
        if let Some(model) = meta.molly_native_model.take() {
            config["model"] = toml_edit::value(model);
        } else {
            config.as_table_mut().remove("model");
        }
        if let Some(context) = meta.molly_native_context_window.take() {
            config["model_context_window"] = toml_edit::value(context);
        } else {
            config.as_table_mut().remove("model_context_window");
        }
        config.as_table_mut().remove("model_catalog_json");
        if let Some(pointer) = meta.molly_native_catalog_pointer.take() {
            config["model_catalog_json"] = toml_edit::value(pointer);
        }
    }
    meta.molly_codex_mode = Some(mode.into());
    next.settings_config["config"] = serde_json::json!(config.to_string());
    if before != next.settings_config {
        meta.molly_pending_apply = true;
    }
    let changed = serde_json::to_value(&next).map_err(|e| e.to_string())?
        != serde_json::to_value(&*provider).map_err(|e| e.to_string())?;
    *provider = next;
    Ok(changed)
}

pub fn get_molly_key_provider(
    app: &tauri::AppHandle,
    account: &str,
    key: &str,
    agent: &str,
) -> Result<Option<Provider>, String> {
    let state = app
        .try_state::<AppState>()
        .ok_or("内置 CC Switch 尚未初始化")?;
    if let Some(provider) = state
        .db
        .get_provider_by_id(
            &crate::embedded::molly_provider_id(account, key, agent),
            agent,
        )
        .map_err(|e| e.to_string())?
    {
        return Ok(Some(provider));
    }
    // The original editor permits renaming additive provider IDs.
    Ok(state
        .db
        .get_all_providers(agent)
        .map_err(|e| e.to_string())?
        .values()
        .find(|p| {
            p.meta.as_ref().is_some_and(|m| {
                m.molly_account_id.as_deref() == Some(account)
                    && m.molly_key_id.as_deref() == Some(key)
            })
        })
        .cloned())
}

/// Preserve auth, endpoints, TOML comments and advanced options. Never write live configuration.
pub fn update_molly_codex_provider(
    app: &tauri::AppHandle,
    account: &str,
    key: &str,
    mode: Option<&str>,
    models: Option<&[String]>,
) -> Result<(), String> {
    let state = app
        .try_state::<AppState>()
        .ok_or("内置 CC Switch 尚未初始化")?;
    let mut provider =
        get_molly_key_provider(app, account, key, "codex")?.ok_or("请先编辑此密钥的供应商配置")?;
    if update_codex(&mut provider, mode, models)? {
        state
            .db
            .save_provider("codex", &provider)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) fn mark_applied(
    db: &Database,
    agent: &str,
    id: &str,
    applied_config: &serde_json::Value,
) -> Result<(), crate::error::AppError> {
    if let Some(mut provider) = db.get_provider_by_id(id, agent)? {
        // A refresh may have prepared newer models while the live write was running.
        if &provider.settings_config != applied_config {
            return Ok(());
        }
        if let Some(meta) = provider
            .meta
            .as_mut()
            .filter(|m| m.molly_key_id.is_some() && m.molly_pending_apply)
        {
            meta.molly_pending_apply = false;
            db.save_provider(agent, &provider)?;
        }
    }
    Ok(())
}

fn ensure_default(db: &Database, agent: &str, existing: Option<String>) -> Result<(), String> {
    let id = match agent {
        "claude" => "claude-official",
        "claude-desktop" => "claude-desktop-official",
        "codex" => "codex-official",
        "gemini" => "gemini-official",
        "grokbuild" => "grokbuild-official",
        _ => return Ok(()), // Additive tools never acquire a default.
    };
    let kind = AppType::from_str(agent).map_err(|e| e.to_string())?;
    db.ensure_official_seed_by_id(id, kind)
        .map_err(|e| e.to_string())?;
    let marker = format!("molly_key_official_default_v1:{agent}");
    if db
        .get_setting(&marker)
        .map_err(|e| e.to_string())?
        .is_none()
    {
        if existing.is_none()
            && db
                .get_current_provider(agent)
                .map_err(|e| e.to_string())?
                .is_none()
        {
            db.set_current_provider(agent, id)
                .map_err(|e| e.to_string())?;
        }
        db.set_setting(&marker, "true").map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn ensure_molly_official(app: &tauri::AppHandle, agent: &str) -> Result<(), String> {
    let state = app
        .try_state::<AppState>()
        .ok_or("内置 CC Switch 尚未初始化")?;
    let kind = AppType::from_str(agent).map_err(|e| e.to_string())?;
    let current = if kind.is_additive_mode() {
        None
    } else {
        crate::settings::get_effective_current_provider(&state.db, &kind)
            .map_err(|e| e.to_string())?
    };
    ensure_default(&state.db, agent, current)
}

fn model(provider: &Provider, agent: &str) -> String {
    let config = &provider.settings_config;
    match agent {
        "claude" | "claude-desktop" => config["env"]["ANTHROPIC_MODEL"]
            .as_str()
            .unwrap_or("")
            .into(),
        "gemini" => config["env"]["GEMINI_MODEL"].as_str().unwrap_or("").into(),
        "codex" | "grokbuild" => config["config"]
            .as_str()
            .and_then(|s| s.parse::<toml::Value>().ok())
            .and_then(|v| {
                if agent == "codex" {
                    v.get("model")?.as_str().map(str::to_owned)
                } else {
                    v.get("models")?.get("default")?.as_str().map(str::to_owned)
                }
            })
            .unwrap_or_default(),
        _ => config["models"]
            .as_object()
            .and_then(|m| m.keys().next())
            .cloned()
            .or_else(|| {
                config["models"]
                    .as_array()?
                    .first()?
                    .get("id")?
                    .as_str()
                    .map(str::to_owned)
            })
            .unwrap_or_default(),
    }
}

pub fn find_molly_key_provider(
    app: &tauri::AppHandle,
    account: &str,
    key: &str,
    agent: &str,
) -> Result<Option<(String, String)>, String> {
    Ok(get_molly_key_provider(app, account, key, agent)?.map(|p| (p.id.clone(), model(&p, agent))))
}

fn reconcile(
    db: &Database,
    account: &str,
    agent: &str,
    key_ids: &[String],
) -> Result<bool, String> {
    let mut current_removed = false;
    let current = db.get_current_provider(agent).map_err(|e| e.to_string())?;
    for provider in db
        .get_all_providers(agent)
        .map_err(|e| e.to_string())?
        .values()
    {
        let Some(meta) = &provider.meta else { continue };
        if meta.molly_account_id.as_deref() != Some(account) {
            continue;
        }
        if meta
            .molly_key_id
            .as_ref()
            .is_some_and(|id| !key_ids.contains(id))
        {
            current_removed |= current.as_deref() == Some(&provider.id);
            db.delete_provider(agent, &provider.id)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(current_removed)
}

/// Complete account snapshots only. No live configuration is changed.
pub fn reconcile_molly_key_providers(
    app: &tauri::AppHandle,
    account: &str,
    agent: &str,
    key_ids: &[String],
) -> Result<bool, String> {
    let state = app
        .try_state::<AppState>()
        .ok_or("内置 CC Switch 尚未初始化")?;
    reconcile(&state.db, account, agent, key_ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn codex() -> Provider {
        let mut provider = Provider::with_id(
            "molly-fixture".into(),
            "Fixture".into(),
            serde_json::json!({
                "auth":{"OPENAI_API_KEY":"mock-secret"},
                "config":"# keep comment\nmodel = \"native-model\"\nmodel_provider = \"mollycloud\"\n[model_providers.mollycloud]\nbase_url = \"https://example.invalid/v1\"\nwire_api = \"responses\"\n[features]\ncustom_feature = true\n",
                "custom":{"preserve":true}
            }),
            None,
        );
        let mut meta = crate::provider::ProviderMeta::default();
        meta.molly_key_id = Some("42".into());
        provider.meta = Some(meta);
        provider
    }
    #[test]
    fn native_defaults_preserve_advanced_config_and_explicit_context() {
        let mut provider = codex();
        assert!(update_codex(&mut provider, None, None).unwrap());
        let config: toml::Value = provider.settings_config["config"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(config["model_context_window"].as_integer(), Some(1050000));
        assert!(config["features"]["custom_feature"].as_bool().unwrap());
        assert!(provider.settings_config["config"]
            .as_str()
            .unwrap()
            .contains("# keep comment"));
        assert_eq!(
            provider.settings_config["auth"]["OPENAI_API_KEY"],
            "mock-secret"
        );
        assert!(!update_codex(&mut provider, None, None).unwrap());
        provider.settings_config["config"] =
            serde_json::json!("model = \"chosen\"\nmodel_context_window = 128000\n");
        update_codex(&mut provider, None, None).unwrap();
        assert!(provider.settings_config["config"]
            .as_str()
            .unwrap()
            .contains("128000"));
        provider.settings_config["config"] = serde_json::json!("model = \"chosen\"\n");
        update_codex(&mut provider, None, None).unwrap();
        assert!(!provider.settings_config["config"]
            .as_str()
            .unwrap()
            .contains("model_context_window"));
    }
    #[test]
    fn mapping_uses_exact_available_whitelist_and_restores_native_catalog() {
        let mut provider = codex();
        let native = serde_json::json!({"models":[{"model":"user-native","contextWindow":500000}]});
        provider.settings_config["modelCatalog"] = native.clone();
        let models = vec![
            "gpt-6.1-sol-fast".into(),
            "gpt-6-sol".into(),
            "gpt-6-sol".into(),
            "gpt-6-sol-extra".into(),
            "GPT-6-astra".into(),
            "other".into(),
        ];
        update_codex(&mut provider, Some("mapped"), Some(&models)).unwrap();
        let rows = provider.settings_config["modelCatalog"]["models"]
            .as_array()
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["model"], "gpt-6-sol");
        assert_eq!(rows[1]["model"], "gpt-6.1-sol-fast");
        for row in rows {
            assert_eq!(row["contextWindow"], 1050000);
            assert_eq!(
                row["reasoningLevels"],
                serde_json::json!(["low", "medium", "high", "xhigh", "max", "ultra"])
            );
        }
        assert!(!update_codex(&mut provider, None, Some(&models)).unwrap());
        update_codex(&mut provider, None, Some(&["gpt-6-astra".into()])).unwrap();
        assert_eq!(
            provider.settings_config["modelCatalog"]["models"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        update_codex(&mut provider, Some("native"), None).unwrap();
        assert_eq!(provider.settings_config["modelCatalog"], native);
        assert_eq!(model(&provider, "codex"), "native-model");
        assert!(provider.meta.unwrap().molly_pending_apply);
    }
    #[test]
    fn unavailable_models_invalid_mode_and_bad_toml_leave_provider_untouched() {
        let mut provider = codex();
        let before = serde_json::to_value(&provider).unwrap();
        for available in [vec![], vec!["gpt-5.5".into(), "prefix/gpt-6-sol".into()]] {
            assert!(update_codex(&mut provider, Some("mapped"), Some(&available)).is_err());
            assert_eq!(serde_json::to_value(&provider).unwrap(), before);
        }
        assert!(update_codex(&mut provider, Some("invalid"), None).is_err());
        provider.settings_config["config"] = serde_json::json!("invalid [ TOML");
        let before = serde_json::to_value(&provider).unwrap();
        assert!(update_codex(&mut provider, Some("mapped"), Some(&["gpt-6-sol".into()])).is_err());
        assert_eq!(serde_json::to_value(&provider).unwrap(), before);
    }

    #[test]
    fn switching_modes_preserves_a_manual_native_context_override() {
        let mut provider = codex();
        update_codex(&mut provider, None, None).unwrap();
        for value in [
            "model = \"custom\"\nmodel_context_window = 128000\n",
            "model = \"custom\"\n",
        ] {
            provider.settings_config["config"] = serde_json::json!(value);
            update_codex(&mut provider, Some("mapped"), Some(&["gpt-6-sol".into()])).unwrap();
            update_codex(&mut provider, Some("native"), None).unwrap();
            assert_eq!(provider.settings_config["config"], value);
        }
    }
    #[test]
    fn pending_apply_is_key_specific_and_only_cleared_after_activation() {
        let db = Database::memory().unwrap();
        let mut provider = codex();
        update_codex(&mut provider, Some("mapped"), Some(&["gpt-6-sol".into()])).unwrap();
        db.save_provider("codex", &provider).unwrap();
        mark_applied(&db, "codex", "another-provider", &provider.settings_config).unwrap();
        assert!(
            db.get_provider_by_id(&provider.id, "codex")
                .unwrap()
                .unwrap()
                .meta
                .unwrap()
                .molly_pending_apply
        );
        mark_applied(
            &db,
            "codex",
            &provider.id,
            &serde_json::json!({"stale":"config"}),
        )
        .unwrap();
        assert!(
            db.get_provider_by_id(&provider.id, "codex")
                .unwrap()
                .unwrap()
                .meta
                .unwrap()
                .molly_pending_apply
        );
        mark_applied(&db, "codex", &provider.id, &provider.settings_config).unwrap();
        assert!(
            !db.get_provider_by_id(&provider.id, "codex")
                .unwrap()
                .unwrap()
                .meta
                .unwrap()
                .molly_pending_apply
        );
    }
    #[test]
    fn manually_selected_models_survive_reopening_additive_tools() {
        let array = Provider::with_id(
            "fixture".into(),
            "Fixture".into(),
            serde_json::json!({"models":[{"id":"chosen-model"}]}),
            None,
        );
        for agent in ["openclaw", "hermes", "pi"] {
            assert_eq!(model(&array, agent), "chosen-model");
        }
        let map = Provider::with_id(
            "fixture".into(),
            "Fixture".into(),
            serde_json::json!({"models":{"chosen-model":{}}}),
            None,
        );
        for agent in ["opencode", "mcode"] {
            assert_eq!(model(&map, agent), "chosen-model");
        }
    }
    #[test]
    fn official_default_is_empty_idempotent_and_preserves_existing_choice() {
        let db = Database::memory().unwrap();
        ensure_default(&db, "codex", None).unwrap();
        assert_eq!(
            db.get_current_provider("codex").unwrap().as_deref(),
            Some("codex-official")
        );
        let p = db
            .get_provider_by_id("codex-official", "codex")
            .unwrap()
            .unwrap();
        assert_eq!(
            p.settings_config,
            serde_json::json!({"auth":{},"config":""})
        );
        let mut other = Provider::with_id(
            "custom".into(),
            "Custom".into(),
            serde_json::json!({}),
            None,
        );
        other.category = Some("custom".into());
        db.save_provider("codex", &other).unwrap();
        db.set_current_provider("codex", "custom").unwrap();
        ensure_default(&db, "codex", None).unwrap();
        assert_eq!(
            db.get_current_provider("codex").unwrap().as_deref(),
            Some("custom")
        );
        ensure_default(&db, "claude", Some("already-selected".into())).unwrap();
        assert!(db.get_current_provider("claude").unwrap().is_none());
        ensure_default(&db, "opencode", None).unwrap();
        assert!(db.get_current_provider("opencode").unwrap().is_none());
        assert!(db.get_all_providers("opencode").unwrap().is_empty());
    }
    #[test]
    fn reconciliation_removes_only_missing_keys_of_the_same_account() {
        let db = Database::memory().unwrap();
        for (id, account, key) in [("one", "a", "1"), ("two", "a", "2"), ("other", "b", "1")] {
            let mut p = Provider::with_id(id.into(), id.into(), serde_json::json!({}), None);
            let mut meta = crate::provider::ProviderMeta::default();
            meta.molly_account_id = Some(account.into());
            meta.molly_key_id = Some(key.into());
            p.meta = Some(meta);
            db.save_provider("codex", &p).unwrap();
        }
        db.set_current_provider("codex", "two").unwrap();
        assert!(reconcile(&db, "a", "codex", &["1".into()]).unwrap());
        let items = db.get_all_providers("codex").unwrap();
        assert!(
            items.contains_key("one") && items.contains_key("other") && !items.contains_key("two")
        );
    }
}
