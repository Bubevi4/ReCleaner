//! Spawn helper processes with `CREATE_NO_WINDOW` (no console flash).

use anyhow::{Context, Result};
use std::process::{Command, Stdio};

/// Captured exit code and stdio from a hidden process.
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CommandResult {
    pub fn success(&self) -> bool {
        self.status == 0
    }
}

/// Run a process with CREATE_NO_WINDOW (no console flash).
pub fn run_hidden(program: &str, args: &[&str]) -> Result<CommandResult> {
    run_hidden_output(program, args)
}

pub fn run_hidden_output(program: &str, args: &[&str]) -> Result<CommandResult> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let output = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .with_context(|| format!("spawn {program}"))?;
        Ok(CommandResult {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
    #[cfg(not(windows))]
    {
        let output = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| format!("spawn {program}"))?;
        Ok(CommandResult {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}
