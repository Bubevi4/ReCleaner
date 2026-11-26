//! Per-tweak JSON backups and restore-point orchestration.

use crate::tweaks::all_tweaks;
use anyhow::{bail, Context, Result};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// On-disk backup written under `%LOCALAPPDATA%\Recleaner\backups\<timestamp>.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BackupStore {
    pub created_at: String,
    pub restore_point: bool,
    pub tweaks: HashMap<String, serde_json::Value>,
}

pub fn backup_dir() -> PathBuf {
    let base = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from(r"%LOCALAPPDATA%"))
        .join("Recleaner")
        .join("backups");
    let _ = fs::create_dir_all(&base);
    base
}

fn latest_backup_path() -> Option<PathBuf> {
    let dir = backup_dir();
    let mut files: Vec<_> = fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    files.sort();
    files.pop()
}

pub fn save_backup(store: &BackupStore) -> Result<PathBuf> {
    let ts = Local::now().format("%Y%m%d_%H%M%S");
    let path = backup_dir().join(format!("{ts}.json"));
    let data = serde_json::to_string_pretty(store)?;
    fs::write(&path, data).with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

pub fn load_backup(path: &Path) -> Result<BackupStore> {
    let data = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&data)?)
}

/// Create restore point (once), apply selected tweaks, persist per-tweak backups.
pub fn apply_selected(ids: &[String], create_rp: bool) -> Result<PathBuf> {
    let tweaks = all_tweaks();
    let mut store = BackupStore {
        created_at: Local::now().to_rfc3339(),
        restore_point: false,
        tweaks: HashMap::new(),
    };

    if create_rp {
        match recleaner_win::create_restore_point("Recleaner optimizer") {
            Ok(()) => store.restore_point = true,
            Err(e) => tracing::warn!("restore point failed: {e}"),
        }
    }

    for id in ids {
        let Some(t) = tweaks.iter().find(|t| t.info().id == *id) else {
            continue;
        };
        match t.apply() {
            Ok(backup) => {
                store.tweaks.insert(id.clone(), backup);
            }
            Err(e) => {
                tracing::error!("apply {id}: {e}");
                bail!("failed to apply tweak {id}: {e}");
            }
        }
    }

    save_backup(&store)
}

pub fn revert_one(id: &str) -> Result<()> {
    let path = latest_backup_path().context("no backup found")?;
    let mut store = load_backup(&path)?;
    let tweaks = all_tweaks();
    let Some(t) = tweaks.iter().find(|t| t.info().id == id) else {
        bail!("unknown tweak {id}");
    };
    let Some(backup) = store.tweaks.get(id) else {
        bail!("no backup entry for {id}");
    };
    t.revert(backup)?;
    store.tweaks.remove(id);
    let data = serde_json::to_string_pretty(&store)?;
    fs::write(&path, data)?;
    Ok(())
}

pub fn revert_all() -> Result<()> {
    let path = latest_backup_path().context("no backup found")?;
    let store = load_backup(&path)?;
    let tweaks = all_tweaks();
    for (id, backup) in &store.tweaks {
        if let Some(t) = tweaks.iter().find(|t| t.info().id == *id) {
            if let Err(e) = t.revert(backup) {
                tracing::warn!("revert {id}: {e}");
            }
        }
    }
    Ok(())
}

pub fn create_restore_point_only() -> Result<()> {
    recleaner_win::create_restore_point("Recleaner manual")
}
