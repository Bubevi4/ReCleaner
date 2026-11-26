//! Process listing via `sysinfo` (used to skip cleanup while apps are open).

use sysinfo::{ProcessesToUpdate, System};

/// Minimal process snapshot (pid + image name).
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
}

pub fn list_processes() -> Vec<ProcessInfo> {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys.processes()
        .iter()
        .map(|(pid, proc)| ProcessInfo {
            pid: pid.as_u32(),
            name: proc.name().to_string_lossy().into_owned(),
        })
        .collect()
}

/// Case-insensitive match on process image name (with or without `.exe`).
pub fn is_process_running(name: &str) -> bool {
    let target = name.trim().to_lowercase();
    let target_exe = if target.ends_with(".exe") {
        target.clone()
    } else {
        format!("{target}.exe")
    };
    list_processes().into_iter().any(|p| {
        let n = p.name.to_lowercase();
        n == target || n == target_exe
    })
}
