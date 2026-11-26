//! System Restore checkpoint creation (PowerShell / WMI fallback).

use anyhow::{bail, Result};

/// Create a System Restore checkpoint.
///
/// Prefers `Checkpoint-Computer`; falls back to WMI `SystemRestore.CreateRestorePoint`.
/// Requires elevation and System Restore enabled for the system drive.
pub fn create_restore_point(description: &str) -> Result<()> {
    #[cfg(windows)]
    {
        create_restore_point_windows(description)
    }
    #[cfg(not(windows))]
    {
        let _ = description;
        bail!("System Restore is only available on Windows");
    }
}

#[cfg(windows)]
fn create_restore_point_windows(description: &str) -> Result<()> {
    // Prefer PowerShell/WMI one-liner with CREATE_NO_WINDOW for reliability across SKUs.
    // Checkpoint-Computer requires admin; Begin-SystemRestore also available via COM.
    let script = format!(
        "Checkpoint-Computer -Description '{}' -RestorePointType 'MODIFY_SETTINGS'",
        description.replace('\'', "''")
    );
    let result = crate::run::run_hidden(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", &script],
    )?;
    if !result.success() {
        // Fallback: WMIC-style via powershell COM
        let com = format!(
            "$s=Get-CimInstance -Namespace root/default -ClassName SystemRestore -ErrorAction SilentlyContinue; \
             if(-not $s){{ $null = [wmiclass]'root\\default:SystemRestore' }}; \
             ([wmiclass]'root\\default:SystemRestore').CreateRestorePoint('{}', 12, 100)",
            description.replace('\'', "''")
        );
        let result2 = crate::run::run_hidden(
            "powershell",
            &["-NoProfile", "-NonInteractive", "-Command", &com],
        )?;
        if !result2.success() {
            bail!(
                "CreateRestorePoint failed: {} / {}",
                result.stderr,
                result2.stderr
            );
        }
    }
    Ok(())
}
