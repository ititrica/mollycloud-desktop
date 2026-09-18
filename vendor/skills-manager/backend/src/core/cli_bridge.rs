//! Agent instructions refer directly to the installed Molly executable.
//! No executable is copied, downloaded, spawned, or published during setup.
use anyhow::{Context, Result};
use std::path::Path;

pub const CLI_FLAG: &str = "--molly-skills-cli";

pub fn agent_document(binary: &Path, library: &Path) -> Result<String> {
    let original = include_str!("../../../skills/manage-skills/SKILL.md");
    let (_, tail) = original.split_once("### When a deployment is refused")
        .context("Missing bundled skill documentation")?;
    let tail = tail.replace("\"$SM\"", "\"$SM\" --molly-skills-cli")
        .replace("~/.skills-manager/skills/", &library.display().to_string());
    Ok(format!("---\nname: manage-molly-skills\ndescription: Manage the user's MollyCloud skill library, presets and Agent deployments using MollyCloud's CLI.\n---\n\n## MollyCloud CLI\n\nThe installed MollyCloud executable is `{}`. Always pass `--molly-skills-cli` as the first argument. Use `--json` when parsing output. Run `--molly-skills-cli --help` for commands. This mode runs without starting the desktop UI.\n\nExamples below use `$SM` as a placeholder for this absolute executable path. Substitute the quoted absolute path in every command. In PowerShell, use `& '<absolute path>' --molly-skills-cli` (escape a literal apostrophe by doubling it); in a POSIX shell, use a correctly quoted absolute path followed by `--molly-skills-cli`. Never execute without this mode argument. If MollyCloud has moved or is missing, ask the user to reinstall this management skill from MollyCloud settings. Never look for or run a private bin copy or the standalone Skills Manager CLI.\n\nLibrary path: `{}`.\n\n### When a deployment is refused{}", binary.display(), library.display(), tail))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn instructions_require_installed_executable_and_explicit_mode() {
        let doc = agent_document(Path::new("C:/Program Files/MollyCloud/mollycloud.exe"), Path::new("C:/Molly/library/skills")).unwrap();
        assert!(doc.contains("C:/Program Files/MollyCloud/mollycloud.exe"));
        assert!(doc.contains("\"$SM\" --molly-skills-cli --json skills search"));
        assert!(!doc.contains(".version"));
        assert!(!doc.contains("BRIDGE_BROKEN"));
        assert!(!doc.contains("~/.skills-manager/skills/"));
        assert!(!doc.contains("\"$SM\" skills"));
    }
}
