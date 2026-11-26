//! Windows Service Control Manager helpers (query / start / stop / start type).

use anyhow::{Context, Result};

/// SCM start type corresponding to `SERVICE_*_START` / `SERVICE_DISABLED`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStartType {
    Boot,
    System,
    Auto,
    Manual,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStatus {
    Stopped,
    StartPending,
    StopPending,
    Running,
    ContinuePending,
    PausePending,
    Paused,
    Unknown,
}

pub fn service_status(name: &str) -> Result<ServiceStatus> {
    #[cfg(windows)]
    {
        service_status_windows(name)
    }
    #[cfg(not(windows))]
    {
        let _ = name;
        anyhow::bail!("services only available on Windows");
    }
}

pub fn service_stop(name: &str) -> Result<()> {
    #[cfg(windows)]
    {
        service_control_windows(name, false)
    }
    #[cfg(not(windows))]
    {
        let _ = name;
        anyhow::bail!("services only available on Windows");
    }
}

pub fn service_start(name: &str) -> Result<()> {
    #[cfg(windows)]
    {
        service_control_windows(name, true)
    }
    #[cfg(not(windows))]
    {
        let _ = name;
        anyhow::bail!("services only available on Windows");
    }
}

pub fn service_config_start_type(name: &str) -> Result<ServiceStartType> {
    #[cfg(windows)]
    {
        service_config_start_type_windows(name)
    }
    #[cfg(not(windows))]
    {
        let _ = name;
        anyhow::bail!("services only available on Windows");
    }
}

pub fn service_set_start_type(name: &str, start: ServiceStartType) -> Result<()> {
    #[cfg(windows)]
    {
        service_set_start_type_windows(name, start)
    }
    #[cfg(not(windows))]
    {
        let _ = name;
        let _ = start;
        anyhow::bail!("services only available on Windows");
    }
}

#[cfg(windows)]
fn open_scm() -> Result<windows::Win32::System::Services::SC_HANDLE> {
    use windows::Win32::System::Services::{OpenSCManagerW, SC_MANAGER_ALL_ACCESS};
    unsafe {
        OpenSCManagerW(None, None, SC_MANAGER_ALL_ACCESS).context("OpenSCManagerW")
    }
}

#[cfg(windows)]
fn service_status_windows(name: &str) -> Result<ServiceStatus> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Services::{
        CloseServiceHandle, OpenServiceW, QueryServiceStatus, SERVICE_QUERY_STATUS,
        SERVICE_STATUS as WinStatus, SERVICE_CONTINUE_PENDING, SERVICE_PAUSE_PENDING,
        SERVICE_PAUSED, SERVICE_RUNNING, SERVICE_START_PENDING, SERVICE_STOP_PENDING,
        SERVICE_STOPPED,
    };

    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let scm = open_scm()?;
        let svc = OpenServiceW(scm, PCWSTR(wide.as_ptr()), SERVICE_QUERY_STATUS)
            .context("OpenServiceW")?;
        let mut st = WinStatus::default();
        let q = QueryServiceStatus(svc, &mut st);
        let _ = CloseServiceHandle(svc);
        let _ = CloseServiceHandle(scm);
        q.context("QueryServiceStatus")?;
        Ok(match st.dwCurrentState {
            SERVICE_STOPPED => ServiceStatus::Stopped,
            SERVICE_START_PENDING => ServiceStatus::StartPending,
            SERVICE_STOP_PENDING => ServiceStatus::StopPending,
            SERVICE_RUNNING => ServiceStatus::Running,
            SERVICE_CONTINUE_PENDING => ServiceStatus::ContinuePending,
            SERVICE_PAUSE_PENDING => ServiceStatus::PausePending,
            SERVICE_PAUSED => ServiceStatus::Paused,
            _ => ServiceStatus::Unknown,
        })
    }
}

#[cfg(windows)]
fn service_control_windows(name: &str, start: bool) -> Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Services::{
        CloseServiceHandle, ControlService, OpenServiceW, StartServiceW, SERVICE_START,
        SERVICE_STATUS as WinStatus, SERVICE_STOP, SERVICE_CONTROL_STOP,
    };

    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let scm = open_scm()?;
        let access = if start { SERVICE_START } else { SERVICE_STOP };
        let svc = OpenServiceW(scm, PCWSTR(wide.as_ptr()), access).context("OpenServiceW")?;
        let result = if start {
            StartServiceW(svc, None).context("StartServiceW")
        } else {
            let mut st = WinStatus::default();
            ControlService(svc, SERVICE_CONTROL_STOP, &mut st).context("ControlService STOP")
        };
        let _ = CloseServiceHandle(svc);
        let _ = CloseServiceHandle(scm);
        result?;
    }
    Ok(())
}

#[cfg(windows)]
fn service_config_start_type_windows(name: &str) -> Result<ServiceStartType> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Services::{
        CloseServiceHandle, OpenServiceW, QueryServiceConfigW, QUERY_SERVICE_CONFIGW,
        SERVICE_QUERY_CONFIG, SERVICE_AUTO_START, SERVICE_BOOT_START, SERVICE_DEMAND_START,
        SERVICE_DISABLED, SERVICE_SYSTEM_START,
    };

    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let scm = open_scm()?;
        let svc = OpenServiceW(scm, PCWSTR(wide.as_ptr()), SERVICE_QUERY_CONFIG)
            .context("OpenServiceW")?;
        let mut needed = 0u32;
        let _ = QueryServiceConfigW(svc, None, 0, &mut needed);
        let mut buf = vec![0u8; needed as usize];
        let cfg = buf.as_mut_ptr() as *mut QUERY_SERVICE_CONFIGW;
        QueryServiceConfigW(svc, Some(cfg), needed, &mut needed).context("QueryServiceConfigW")?;
        let start_type = (*cfg).dwStartType;
        let _ = CloseServiceHandle(svc);
        let _ = CloseServiceHandle(scm);
        Ok(match start_type {
            SERVICE_BOOT_START => ServiceStartType::Boot,
            SERVICE_SYSTEM_START => ServiceStartType::System,
            SERVICE_AUTO_START => ServiceStartType::Auto,
            SERVICE_DEMAND_START => ServiceStartType::Manual,
            SERVICE_DISABLED => ServiceStartType::Disabled,
            _ => ServiceStartType::Manual,
        })
    }
}

#[cfg(windows)]
fn service_set_start_type_windows(name: &str, start: ServiceStartType) -> Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Services::{
        ChangeServiceConfigW, CloseServiceHandle, OpenServiceW, ENUM_SERVICE_TYPE,
        SERVICE_CHANGE_CONFIG, SERVICE_AUTO_START, SERVICE_BOOT_START, SERVICE_DEMAND_START,
        SERVICE_DISABLED, SERVICE_ERROR, SERVICE_SYSTEM_START,
    };

    let dw = match start {
        ServiceStartType::Boot => SERVICE_BOOT_START,
        ServiceStartType::System => SERVICE_SYSTEM_START,
        ServiceStartType::Auto => SERVICE_AUTO_START,
        ServiceStartType::Manual => SERVICE_DEMAND_START,
        ServiceStartType::Disabled => SERVICE_DISABLED,
    };
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let scm = open_scm()?;
        let svc = OpenServiceW(scm, PCWSTR(wide.as_ptr()), SERVICE_CHANGE_CONFIG)
            .context("OpenServiceW")?;
        // SERVICE_NO_CHANGE == 0xFFFFFFFF
        ChangeServiceConfigW(
            svc,
            ENUM_SERVICE_TYPE(0xFFFF_FFFF),
            dw,
            SERVICE_ERROR(0xFFFF_FFFF),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .context("ChangeServiceConfigW")?;
        let _ = CloseServiceHandle(svc);
        let _ = CloseServiceHandle(scm);
    }
    Ok(())
}
