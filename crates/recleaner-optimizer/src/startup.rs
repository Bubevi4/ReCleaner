//! Startup enumeration and enable/disable (Run keys, folders, StartupApproved).

use anyhow::Result;
use recleaner_win::{reg_enum_values, RegHive};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Where a startup entry was discovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartupKind {
    HkcuRun,
    HkcuRunOnce,
    HklmRun,
    HklmRunOnce,
    StartupFolderUser,
    StartupFolderCommon,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupEntry {
    pub id: String,
    pub name: String,
    pub command: String,
    pub kind: StartupKind,
    pub enabled: bool,
    pub location: String,
}

const HKCU_RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const HKCU_RUNONCE: &str = r"Software\Microsoft\Windows\CurrentVersion\RunOnce";
const HKLM_RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const HKLM_RUNONCE: &str = r"Software\Microsoft\Windows\CurrentVersion\RunOnce";
const APPROVED_CU: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const APPROVED_LM: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32";

fn approved_enabled(hive: RegHive, key: &str, name: &str) -> bool {
    // StartupApproved stores binary; if missing, treat as enabled.
    // Value first byte 0x02/0x03 often means disabled on Win10+.
    let root = match hive {
        RegHive::CurrentUser => winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER),
        RegHive::LocalMachine => winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE),
        _ => return true,
    };
    let Ok(k) = root.open_subkey(key) else {
        return true;
    };
    match k.get_raw_value(name) {
        Ok(raw) => {
            if raw.bytes.is_empty() {
                true
            } else {
                // Typical: 0x02 = enabled, 0x03 = disabled (Task Manager / StartupApproved)
                !matches!(raw.bytes[0], 0x03 | 0x07)
            }
        }
        Err(_) => true,
    }
}

fn from_run_key(
    hive: RegHive,
    subkey: &str,
    kind: StartupKind,
    approved: Option<(&str, RegHive)>,
) -> Vec<StartupEntry> {
    let Ok(vals) = reg_enum_values(hive, subkey) else {
        return Vec::new();
    };
    vals.into_iter()
        .map(|(name, command)| {
            let enabled = approved
                .map(|(ak, ah)| approved_enabled(ah, ak, &name))
                .unwrap_or(true);
            StartupEntry {
                id: format!("{kind:?}:{name}"),
                name: name.clone(),
                command,
                kind,
                enabled,
                location: format!("{hive:?}\\{subkey}"),
            }
        })
        .collect()
}

fn folder_entries(dir: PathBuf, kind: StartupKind) -> Vec<StartupEntry> {
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    rd.filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("lnk") || x.eq_ignore_ascii_case("url"))
                .unwrap_or(false)
        })
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            StartupEntry {
                id: format!("{kind:?}:{name}"),
                name: name.clone(),
                command: e.path().display().to_string(),
                kind,
                enabled: true,
                location: dir.display().to_string(),
            }
        })
        .collect()
}

pub fn list_startup_entries() -> Result<Vec<StartupEntry>> {
    let mut out = Vec::new();
    out.extend(from_run_key(
        RegHive::CurrentUser,
        HKCU_RUN,
        StartupKind::HkcuRun,
        Some((APPROVED_CU, RegHive::CurrentUser)),
    ));
    out.extend(from_run_key(
        RegHive::CurrentUser,
        HKCU_RUNONCE,
        StartupKind::HkcuRunOnce,
        None,
    ));
    out.extend(from_run_key(
        RegHive::LocalMachine,
        HKLM_RUN,
        StartupKind::HklmRun,
        Some((APPROVED_LM, RegHive::LocalMachine)),
    ));
    out.extend(from_run_key(
        RegHive::LocalMachine,
        HKLM_RUNONCE,
        StartupKind::HklmRunOnce,
        None,
    ));

    if let Some(appdata) = std::env::var_os("APPDATA") {
        let p = PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs\Startup");
        out.extend(folder_entries(p, StartupKind::StartupFolderUser));
    }
    if let Some(pd) = std::env::var_os("ProgramData") {
        let p = PathBuf::from(pd).join(r"Microsoft\Windows\Start Menu\Programs\StartUp");
        out.extend(folder_entries(p, StartupKind::StartupFolderCommon));
    }

    Ok(out)
}

/// Enable/disable via StartupApproved binary flag (Run keys) or rename .lnk (folders).
pub fn set_startup_enabled(entry: &StartupEntry, enabled: bool) -> Result<()> {
    match entry.kind {
        StartupKind::HkcuRun => {
            set_approved(RegHive::CurrentUser, APPROVED_CU, &entry.name, enabled)
        }
        StartupKind::HklmRun => {
            set_approved(RegHive::LocalMachine, APPROVED_LM, &entry.name, enabled)
        }
        StartupKind::StartupFolderUser | StartupKind::StartupFolderCommon => {
            let path = PathBuf::from(&entry.command);
            if enabled {
                let disabled = path.with_extension("lnk.disabled");
                if disabled.exists() {
                    let _ = std::fs::rename(&disabled, &path);
                }
            } else if path.exists() {
                let disabled = path.with_extension("lnk.disabled");
                let _ = std::fs::rename(&path, disabled);
            }
            Ok(())
        }
        StartupKind::HkcuRunOnce | StartupKind::HklmRunOnce => {
            // RunOnce entries: delete to disable
            if !enabled {
                let hive = match entry.kind {
                    StartupKind::HkcuRunOnce => RegHive::CurrentUser,
                    _ => RegHive::LocalMachine,
                };
                let key = match entry.kind {
                    StartupKind::HkcuRunOnce => HKCU_RUNONCE,
                    _ => HKLM_RUNONCE,
                };
                recleaner_win::reg_delete_value(hive, key, &entry.name)?;
            }
            Ok(())
        }
    }
}

fn set_approved(hive: RegHive, key: &str, name: &str, enabled: bool) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;
    let root = match hive {
        RegHive::CurrentUser => RegKey::predef(HKEY_CURRENT_USER),
        RegHive::LocalMachine => RegKey::predef(HKEY_LOCAL_MACHINE),
        _ => return Ok(()),
    };
    let (k, _) = root.create_subkey(key)?;
    // 12-byte typical StartupApproved value: status + FILETIME
    let mut bytes = vec![0u8; 12];
    bytes[0] = if enabled { 0x02 } else { 0x03 };
    // Actually: 0x02 = enabled on some builds, 0x03 = disabled. Win10 uses:
    // disabled: 03 00 00 00 ... enabled: 02 00 00 00 ...
    // Correct mapping used by Task Manager: enabled=0x02, disabled=0x03 is inverted on some docs.
    // Microsoft: first DWORD — 2 = enabled, 3 = disabled.
    bytes[0] = if enabled { 0x02 } else { 0x03 };
    k.set_raw_value(
        name,
        &winreg::RegValue {
            bytes,
            vtype: REG_BINARY,
        },
    )?;
    Ok(())
}
