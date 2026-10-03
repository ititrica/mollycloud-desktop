//! Account-key integration only changes the private provider library.
use crate::app_config::AppType;
use crate::{AppState, Database, Provider};
use std::str::FromStr;
use tauri::Manager;

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
    let state = app
        .try_state::<AppState>()
        .ok_or("内置 CC Switch 尚未初始化")?;
    let id = crate::embedded::molly_provider_id(account, key, agent);
    Ok(state
        .db
        .get_provider_by_id(&id, agent)
        .map_err(|e| e.to_string())?
        .map(|p| (id, model(&p, agent))))
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
