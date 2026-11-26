//! Windows helpers for Recleaner.
//!
//! Thin, mostly-safe wrappers around Win32 APIs used by the cleaner and optimizer:
//! environment / known folders, admin detection, reparse-point safety, Recycle Bin,
//! services, registry, System Restore, hidden process spawn, and process listing.
//!
//! Non-Windows builds compile with stub implementations that return errors or
//! conservative defaults so `fmt`/`clippy` can run on Linux CI if needed.

mod env;
mod fs;
mod process;
mod recycle;
mod registry;
mod restore;
mod run;
mod service;

pub use env::{expand_env, known_folder, KnownFolder};
pub use fs::{
    canonicalize_safe, is_reparse_point, is_system_root_protected, path_contains_reparse,
};
pub use process::{is_process_running, list_processes, ProcessInfo};
pub use recycle::empty_recycle_bin;
pub use registry::{
    reg_delete_value, reg_enum_values, reg_get_dword, reg_get_string, reg_set_dword, reg_set_string,
    RegHive,
};
pub use restore::create_restore_point;
pub use run::{run_hidden, run_hidden_output, CommandResult};
pub use service::{
    service_config_start_type, service_set_start_type, service_start, service_status, service_stop,
    ServiceStartType, ServiceStatus,
};

/// Returns true if the current process token has administrators membership (elevated).
pub fn is_admin() -> bool {
    #[cfg(windows)]
    {
        is_admin_windows()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
fn is_admin_windows() -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut size = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        );
        let _ = CloseHandle(token);
        ok.is_ok() && elevation.TokenIsElevated != 0
    }
}
