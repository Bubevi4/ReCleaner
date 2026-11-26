//! Gaming optimizer for Recleaner (Win11-oriented).
//!
//! Exposes a [`Tweak`] trait with Safe / Moderate / Risky presets, System Restore
//! and JSON backups under `%LOCALAPPDATA%\Recleaner\backups`, and a startup manager
//! covering HKCU/HKLM Run/RunOnce, Startup folders, and StartupApproved.

mod backup;
mod startup;
mod tweaks;

pub use backup::{
    apply_selected, backup_dir, create_restore_point_only, revert_all, revert_one, BackupStore,
};
pub use startup::{list_startup_entries, set_startup_enabled, StartupEntry, StartupKind};
pub use tweaks::{all_tweaks, Risk, Tweak, TweakCategory, TweakInfo, TweakState};
