//! Environment variable expansion and common Windows known folders.

use std::path::PathBuf;

/// Well-known user/system folders resolved via environment variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnownFolder {
    LocalAppData,
    RoamingAppData,
    ProgramData,
    Windows,
    ProgramFiles,
    ProgramFilesX86,
    UserProfile,
    Desktop,
    Documents,
    Downloads,
    Temp,
}

/// Expand `%VAR%` / `$VAR` style environment variables in a path string.
pub fn expand_env(input: &str) -> String {
    #[cfg(windows)]
    {
        expand_env_windows(input)
    }
    #[cfg(not(windows))]
    {
        let mut out = input.to_string();
        for (key, value) in std::env::vars() {
            let pat = format!("%{key}%");
            out = out.replace(&pat, &value);
        }
        out
    }
}

#[cfg(windows)]
fn expand_env_windows(input: &str) -> String {
    use windows::core::PCWSTR;
    use windows::Win32::System::Environment::ExpandEnvironmentStringsW;

    let wide: Vec<u16> = input.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let needed = ExpandEnvironmentStringsW(PCWSTR(wide.as_ptr()), None);
        if needed == 0 {
            return input.to_string();
        }
        let mut buf = vec![0u16; needed as usize];
        let written = ExpandEnvironmentStringsW(PCWSTR(wide.as_ptr()), Some(&mut buf));
        if written == 0 {
            return input.to_string();
        }
        // written includes trailing NUL
        String::from_utf16_lossy(&buf[..written as usize - 1])
    }
}

pub fn known_folder(folder: KnownFolder) -> Option<PathBuf> {
    match folder {
        KnownFolder::Temp => {
            let t = std::env::temp_dir();
            Some(t)
        }
        KnownFolder::UserProfile => dirs_user_profile(),
        KnownFolder::LocalAppData => std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
        KnownFolder::RoamingAppData => std::env::var_os("APPDATA").map(PathBuf::from),
        KnownFolder::ProgramData => std::env::var_os("ProgramData").map(PathBuf::from),
        KnownFolder::Windows => std::env::var_os("SystemRoot")
            .or_else(|| std::env::var_os("WINDIR"))
            .map(PathBuf::from),
        KnownFolder::ProgramFiles => std::env::var_os("ProgramFiles").map(PathBuf::from),
        KnownFolder::ProgramFilesX86 => std::env::var_os("ProgramFiles(x86)").map(PathBuf::from),
        KnownFolder::Desktop => known_folder(KnownFolder::UserProfile).map(|p| p.join("Desktop")),
        KnownFolder::Documents => {
            known_folder(KnownFolder::UserProfile).map(|p| p.join("Documents"))
        }
        KnownFolder::Downloads => {
            known_folder(KnownFolder::UserProfile).map(|p| p.join("Downloads"))
        }
    }
}

fn dirs_user_profile() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
}
