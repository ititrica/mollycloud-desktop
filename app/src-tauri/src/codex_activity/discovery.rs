use std::{collections::BTreeSet, path::PathBuf};

pub fn roots(extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut homes = extra.to_vec();
    if let Some(home) = std::env::var_os("USERPROFILE") {
        homes.push(PathBuf::from(home).join(".codex"));
    }
    #[cfg(not(windows))]
    if let Some(home) = std::env::var_os("HOME") {
        homes.push(PathBuf::from(home).join(".codex"));
    }
    if let Some(home) = std::env::var_os("CODEX_HOME").filter(|h| !h.is_empty()) {
        homes.push(PathBuf::from(home));
    }
    // Resolves the current embedded CC Switch override/environment without opening auth.json.
    if let Ok(home) = molly_ccswitch::cli_config_dir("codex") {
        homes.push(home);
    }
    homes
        .into_iter()
        .filter(|p| p.is_absolute())
        .map(|p| p.join("sessions"))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
