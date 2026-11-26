//! Filesystem safety helpers: reparse points, canonicalize, system-root protection.

use std::path::{Component, Path, PathBuf};

/// Detect Windows reparse points (junctions / symlinks). Never follow for delete.
pub fn is_reparse_point(path: &Path) -> bool {
    #[cfg(windows)]
    {
        is_reparse_point_windows(path)
    }
    #[cfg(not(windows))]
    {
        path.symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    }
}

#[cfg(windows)]
fn is_reparse_point_windows(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    match std::fs::symlink_metadata(path) {
        Ok(meta) => (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0,
        Err(_) => false,
    }
}

/// True if any component of `path` (as walked from root) is a reparse point.
pub fn path_contains_reparse(path: &Path) -> bool {
    let mut acc = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(p) => {
                acc.push(p.as_os_str());
            }
            Component::RootDir => {
                acc.push(std::path::MAIN_SEPARATOR_STR);
            }
            Component::Normal(c) => {
                acc.push(c);
                if is_reparse_point(&acc) {
                    return true;
                }
            }
            Component::CurDir => {
                acc.push(".");
                if is_reparse_point(&acc) {
                    return true;
                }
            }
            Component::ParentDir => {
                acc.push("..");
                if is_reparse_point(&acc) {
                    return true;
                }
            }
        }
    }
    false
}

/// Canonicalize without following the final reparse if present; falls back to absolute.
pub fn canonicalize_safe(path: &Path) -> std::io::Result<PathBuf> {
    if is_reparse_point(path) {
        return Ok(path.to_path_buf());
    }
    match dunce_canonicalize(path) {
        Ok(p) => Ok(p),
        Err(_) => {
            if path.is_absolute() {
                Ok(path.to_path_buf())
            } else {
                Ok(std::env::current_dir()?.join(path))
            }
        }
    }
}

fn dunce_canonicalize(path: &Path) -> std::io::Result<PathBuf> {
    let p = std::fs::canonicalize(path)?;
    // Strip Windows \\?\ prefix when possible
    let s = p.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        if let Some(rest) = stripped.strip_prefix("UNC\\") {
            Ok(PathBuf::from(format!(r"\\{rest}")))
        } else {
            Ok(PathBuf::from(stripped))
        }
    } else {
        Ok(p)
    }
}

/// Refuse deletes that would wipe drive roots or critical Windows directories.
pub fn is_system_root_protected(path: &Path) -> bool {
    // Raw forms before canonicalize (Windows treats `C:` as cwd on that drive).
    let raw = path.to_string_lossy();
    let raw_trim = raw.trim_end_matches(['\\', '/']);
    if raw_trim.len() == 2 && raw_trim.as_bytes()[1] == b':' {
        return true;
    }
    if matches!(raw.as_ref(), r"C:\" | r"c:\" | r"D:\" | r"d:\") {
        return true;
    }

    let Ok(canon) = canonicalize_safe(path) else {
        return true;
    };
    let s = canon.to_string_lossy().to_lowercase();
    let trimmed = s.trim_end_matches(['\\', '/']);

    if trimmed.len() == 2 && trimmed.as_bytes()[1] == b':' {
        return true;
    }

    let forbidden = [
        r"c:\windows",
        r"c:\windows\system32",
        r"c:\windows\syswow64",
        r"c:\program files",
        r"c:\program files (x86)",
        r"c:\users",
        r"c:\programdata",
    ];
    for f in forbidden {
        if trimmed == f {
            return true;
        }
    }

    let components: Vec<_> = canon.components().collect();
    if matches!(
        components.as_slice(),
        [Component::Prefix(_)] | [Component::Prefix(_), Component::RootDir]
    ) {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protects_drive_roots() {
        assert!(is_system_root_protected(Path::new(r"C:\")));
        assert!(is_system_root_protected(Path::new(r"C:")));
    }

    #[test]
    fn allows_temp_subdir() {
        let temp = std::env::temp_dir().join("recleaner_test_ok");
        let _ = std::fs::create_dir_all(&temp);
        assert!(!is_system_root_protected(&temp));
        let _ = std::fs::remove_dir_all(&temp);
    }
}
