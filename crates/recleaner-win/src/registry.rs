//! Registry get/set/delete helpers via `winreg`.

use anyhow::{Context, Result};
use winreg::enums::*;
use winreg::RegKey;

/// Predefined registry hive handle.
#[derive(Debug, Clone, Copy)]
pub enum RegHive {
    CurrentUser,
    LocalMachine,
    ClassesRoot,
    Users,
}

impl RegHive {
    fn root(self) -> RegKey {
        match self {
            RegHive::CurrentUser => RegKey::predef(HKEY_CURRENT_USER),
            RegHive::LocalMachine => RegKey::predef(HKEY_LOCAL_MACHINE),
            RegHive::ClassesRoot => RegKey::predef(HKEY_CLASSES_ROOT),
            RegHive::Users => RegKey::predef(HKEY_USERS),
        }
    }
}

pub fn reg_get_string(hive: RegHive, subkey: &str, name: &str) -> Result<Option<String>> {
    let root = hive.root();
    let key = match root.open_subkey(subkey) {
        Ok(k) => k,
        Err(_) => return Ok(None),
    };
    match key.get_value::<String, _>(name) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("reg_get_string"),
    }
}

pub fn reg_get_dword(hive: RegHive, subkey: &str, name: &str) -> Result<Option<u32>> {
    let root = hive.root();
    let key = match root.open_subkey(subkey) {
        Ok(k) => k,
        Err(_) => return Ok(None),
    };
    match key.get_value::<u32, _>(name) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("reg_get_dword"),
    }
}

pub fn reg_set_string(hive: RegHive, subkey: &str, name: &str, value: &str) -> Result<()> {
    let root = hive.root();
    let (key, _) = root
        .create_subkey(subkey)
        .with_context(|| format!("create_subkey {subkey}"))?;
    key.set_value(name, &value.to_string())
        .context("set_value string")?;
    Ok(())
}

pub fn reg_set_dword(hive: RegHive, subkey: &str, name: &str, value: u32) -> Result<()> {
    let root = hive.root();
    let (key, _) = root
        .create_subkey(subkey)
        .with_context(|| format!("create_subkey {subkey}"))?;
    key.set_value(name, &value).context("set_value dword")?;
    Ok(())
}

pub fn reg_delete_value(hive: RegHive, subkey: &str, name: &str) -> Result<()> {
    let root = hive.root();
    let key = root
        .open_subkey_with_flags(subkey, KEY_SET_VALUE)
        .with_context(|| format!("open_subkey {subkey}"))?;
    match key.delete_value(name) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).context("delete_value"),
    }
}

/// Enumerate value names under a key.
pub fn reg_enum_values(hive: RegHive, subkey: &str) -> Result<Vec<(String, String)>> {
    let root = hive.root();
    let key = match root.open_subkey(subkey) {
        Ok(k) => k,
        Err(_) => return Ok(Vec::new()),
    };
    let mut out = Vec::new();
    for name in key.enum_values().filter_map(|v| v.ok().map(|(n, _)| n)) {
        if let Ok(val) = key.get_value::<String, _>(&name) {
            out.push((name, val));
        } else if let Ok(val) = key.get_value::<u32, _>(&name) {
            out.push((name, val.to_string()));
        } else {
            out.push((name, String::new()));
        }
    }
    Ok(out)
}
