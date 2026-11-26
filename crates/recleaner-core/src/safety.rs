//! Per-path delete gates used by analyze/clean.

use recleaner_win::{is_reparse_point, is_system_root_protected, path_contains_reparse};
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Why a path was refused (or [`SafetyReason::Ok`] if delete is allowed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafetyReason {
    Ok,
    SystemRoot,
    ReparsePoint,
    TooYoung { min_age_hours: u64 },
    UserExcluded,
    LockedOrIo,
}

/// Decide whether a concrete file path may be deleted.
pub fn is_safe_to_delete(
    path: &Path,
    min_age_hours: u64,
    user_exclusions: &[String],
) -> SafetyReason {
    if is_system_root_protected(path) {
        return SafetyReason::SystemRoot;
    }
    if is_reparse_point(path) || path_contains_reparse(path) {
        return SafetyReason::ReparsePoint;
    }
    for ex in user_exclusions {
        let expanded = recleaner_win::expand_env(ex);
        if path.starts_with(&expanded) || path.to_string_lossy().contains(&expanded) {
            return SafetyReason::UserExcluded;
        }
    }
    if min_age_hours > 0 {
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            if let Ok(modified) = meta.modified() {
                let age = SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or(Duration::ZERO);
                if age < Duration::from_secs(min_age_hours.saturating_mul(3600)) {
                    return SafetyReason::TooYoung { min_age_hours };
                }
            }
        }
    }
    SafetyReason::Ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn skips_symlink() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("real.txt");
        fs::write(&target, b"hi").unwrap();
        let link = dir.path().join("link.txt");
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(&target, &link).ok();
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, &link).unwrap();
        }
        if link.exists() || is_reparse_point(&link) {
            let reason = is_safe_to_delete(&link, 0, &[]);
            assert_eq!(reason, SafetyReason::ReparsePoint);
        }
    }

    #[test]
    fn age_filter() {
        let dir = tempdir().unwrap();
        let f = dir.path().join("fresh.txt");
        fs::write(&f, b"x").unwrap();
        assert_eq!(
            is_safe_to_delete(&f, 24, &[]),
            SafetyReason::TooYoung { min_age_hours: 24 }
        );
        assert_eq!(is_safe_to_delete(&f, 0, &[]), SafetyReason::Ok);
    }

    #[test]
    fn root_protection() {
        assert_eq!(
            is_safe_to_delete(Path::new(r"C:\"), 0, &[]),
            SafetyReason::SystemRoot
        );
    }
}
