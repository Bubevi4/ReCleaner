//! Process listing / terminate via `sysinfo` (used when cleanup is blocked by open apps).

use anyhow::{bail, Result};
use sysinfo::{ProcessesToUpdate, Signal, System};

/// Minimal process snapshot (pid + image name).
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
}

fn normalize_name(name: &str) -> (String, String) {
    let target = name.trim().to_lowercase();
    let target_exe = if target.ends_with(".exe") {
        target.clone()
    } else {
        format!("{target}.exe")
    };
    (target, target_exe)
}

fn name_matches(proc_name: &str, target: &str, target_exe: &str) -> bool {
    let n = proc_name.to_lowercase();
    n == target || n == target_exe
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
    let (target, target_exe) = normalize_name(name);
    list_processes()
        .into_iter()
        .any(|p| name_matches(&p.name, &target, &target_exe))
}

/// Terminate every process whose image name matches `name` (with or without `.exe`).
/// Returns how many processes were signaled.
pub fn kill_processes_by_name(name: &str) -> Result<usize> {
    let (target, target_exe) = normalize_name(name);
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let mut killed = 0usize;
    let mut failures = Vec::new();
    for proc in sys.processes().values() {
        let proc_name = proc.name().to_string_lossy();
        if !name_matches(proc_name.as_ref(), &target, &target_exe) {
            continue;
        }
        match proc.kill_with(Signal::Kill).or_else(|| {
            // Some platforms only expose kill().
            Some(proc.kill())
        }) {
            Some(true) => killed += 1,
            Some(false) => failures.push(format!("{proc_name} (pid {})", proc.pid())),
            None => failures.push(format!("{proc_name} (pid {}): unsupported", proc.pid())),
        }
    }

    if killed == 0 && !failures.is_empty() {
        bail!("failed to close {name}: {}", failures.join(", "));
    }
    if killed == 0 {
        // Already gone: treat as success.
        return Ok(0);
    }

    // Brief settle so file locks can release before cleanup continues.
    std::thread::sleep(std::time::Duration::from_millis(400));
    Ok(killed)
}
