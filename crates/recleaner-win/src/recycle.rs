//! Recycle Bin emptying via `SHEmptyRecycleBinW`.

use anyhow::Result;

/// Empty the Recycle Bin for all drives without confirmation UI.
pub fn empty_recycle_bin() -> Result<()> {
    #[cfg(windows)]
    {
        empty_recycle_bin_windows()
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("empty_recycle_bin is only available on Windows");
    }
}

#[cfg(windows)]
fn empty_recycle_bin_windows() -> Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{
        SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
    };

    unsafe {
        let flags = SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND;
        SHEmptyRecycleBinW(None, PCWSTR::null(), flags)
            .map_err(|e| anyhow::anyhow!("SHEmptyRecycleBin failed: {e}"))?;
    }
    Ok(())
}
