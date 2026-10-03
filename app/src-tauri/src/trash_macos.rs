use objc2_foundation::{NSFileManager, NSString, NSURL};
use std::path::{Component, Path, PathBuf};

fn is_system_path(path: &Path, home: Option<&Path>) -> bool {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return true;
    }
    if ["/", "/Users", "/Volumes", "/private"]
        .iter()
        .any(|root| path == Path::new(root))
    {
        return true;
    }
    if [
        "/System",
        "/Library",
        "/Applications",
        "/bin",
        "/sbin",
        "/usr",
        "/dev",
        "/private/etc",
        "/private/var",
    ]
    .iter()
    .any(|root| path.starts_with(root))
    {
        return true;
    }
    if let Some(home) = home {
        if home.starts_with(path) || path.starts_with(home.join("Library")) {
            return true;
        }
    }
    // A mounted volume's root is not an ordinary folder.
    path.parent()
        .is_some_and(|parent| parent == Path::new("/Volumes"))
}

fn checked_paths(paths: &[String], home: Option<&Path>) -> Result<Vec<PathBuf>, String> {
    if paths.is_empty() {
        return Err("没有选中要移入废纸篓的文件".into());
    }
    let mut allowed = Vec::with_capacity(paths.len());
    for raw in paths {
        if raw.chars().any(char::is_control) || is_system_path(Path::new(raw), home) {
            return Err("被拒绝：不能将系统目录、应用目录或用户主目录移入废纸篓".into());
        }
        let path = std::fs::canonicalize(raw).map_err(|e| format!("无法读取文件 {raw}：{e}"))?;
        if is_system_path(&path, home) {
            return Err("被拒绝：文件路径指向受保护的系统目录".into());
        }
        // Trash the user's entry rather than following a symlink into another folder.
        let original = PathBuf::from(raw);
        if !allowed.contains(&original) {
            allowed.push(original);
        }
    }
    Ok(allowed)
}

/// NSFileManager moves the items to the correct per-volume Trash and preserves
/// Finder's restore behavior. It never falls back to permanent deletion.
pub fn move_to_recycle_bin(paths: &[String]) -> Result<usize, String> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let allowed = checked_paths(paths, home.as_deref())?;
    let manager = NSFileManager::defaultManager();
    let mut count = 0;
    for path in allowed {
        let value = path.to_str().ok_or("文件路径不是有效的 Unicode")?;
        let url = NSURL::fileURLWithPath(&NSString::from_str(value));
        manager
            .trashItemAtURL_resultingItemURL_error(&url, None)
            .map_err(|error| {
                format!(
                    "已移入废纸篓 {count} 项；无法处理 {}：{}",
                    path.display(),
                    error.localizedDescription()
                )
            })?;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protects_macos_system_folders_and_home() {
        let home = Path::new("/Users/test");
        for path in [
            "/",
            "/Users",
            "/Users/test",
            "/Users/test/Library",
            "/System/Library",
            "/usr/bin/git",
            "/Applications/Test.app",
            "/Volumes/Backup",
            "/Users/test/Desktop/../../test",
        ] {
            assert!(is_system_path(Path::new(path), Some(home)), "{path}");
        }
        assert!(!is_system_path(
            Path::new("/Users/test/Desktop/report.txt"),
            Some(home)
        ));
        assert!(!is_system_path(
            Path::new("/Volumes/Backup/Documents/report.txt"),
            Some(home)
        ));
    }
    #[test]
    fn missing_files_are_rejected_before_any_trash_operation() {
        let fixture = tempfile::tempdir().unwrap();
        let missing = fixture.path().join("missing").display().to_string();
        assert!(checked_paths(&[missing], None).is_err());
    }
}
