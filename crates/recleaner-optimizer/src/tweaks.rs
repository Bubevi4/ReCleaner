//! Built-in gaming / system tweaks implementing [`Tweak`].

use anyhow::Result;
use recleaner_win::{
    reg_get_dword, reg_set_dword, run_hidden, service_config_start_type, service_set_start_type,
    service_stop, RegHive, ServiceStartType,
};
use serde::{Deserialize, Serialize};

/// Risk badge for UI confirmations (Safe / Moderate / Risky).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Safe,
    Moderate,
    Risky,
}

impl Risk {
    pub fn as_str(self) -> &'static str {
        match self {
            Risk::Safe => "safe",
            Risk::Moderate => "moderate",
            Risk::Risky => "risky",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TweakCategory {
    Gaming,
    Input,
    Visual,
    Background,
    Power,
    Privacy,
    Security,
    Network,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TweakInfo {
    pub id: String,
    pub name_en: String,
    pub name_ru: String,
    pub description_en: String,
    pub description_ru: String,
    pub category: TweakCategory,
    pub risk: Risk,
    pub requires_reboot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TweakState {
    pub id: String,
    pub applied: bool,
    pub detail: String,
}

/// A reversible system/gaming tweak.
///
/// [`Tweak::apply`] must return a JSON value sufficient for [`Tweak::revert`].
pub trait Tweak: Send + Sync {
    /// Static metadata for the UI.
    fn info(&self) -> TweakInfo;
    /// Probe whether the tweak appears already applied.
    fn detect(&self) -> Result<TweakState>;
    /// Apply the tweak and return a backup blob for later revert.
    fn apply(&self) -> Result<serde_json::Value>;
    /// Restore previous state from a backup blob produced by [`Tweak::apply`].
    fn revert(&self, backup: &serde_json::Value) -> Result<()>;
}

struct FnTweak {
    info: TweakInfo,
    detect: fn() -> Result<TweakState>,
    apply: fn() -> Result<serde_json::Value>,
    revert: fn(&serde_json::Value) -> Result<()>,
}

impl Tweak for FnTweak {
    fn info(&self) -> TweakInfo {
        self.info.clone()
    }
    fn detect(&self) -> Result<TweakState> {
        (self.detect)()
    }
    fn apply(&self) -> Result<serde_json::Value> {
        (self.apply)()
    }
    fn revert(&self, backup: &serde_json::Value) -> Result<()> {
        (self.revert)(backup)
    }
}

fn info(
    id: &str,
    en: &str,
    ru: &str,
    den: &str,
    dru: &str,
    cat: TweakCategory,
    risk: Risk,
    reboot: bool,
) -> TweakInfo {
    TweakInfo {
        id: id.into(),
        name_en: en.into(),
        name_ru: ru.into(),
        description_en: den.into(),
        description_ru: dru.into(),
        category: cat,
        risk,
        requires_reboot: reboot,
    }
}

fn dword_backup(hive: RegHive, key: &str, name: &str) -> Result<serde_json::Value> {
    Ok(serde_json::json!({
        "hive": format!("{hive:?}"),
        "key": key,
        "name": name,
        "value": reg_get_dword(hive, key, name)?,
    }))
}

fn restore_dword(backup: &serde_json::Value) -> Result<()> {
    let key = backup["key"].as_str().unwrap_or("");
    let name = backup["name"].as_str().unwrap_or("");
    let hive = RegHive::CurrentUser; // most gaming tweaks are HKCU; HKLM handled per-tweak
    let hive_s = backup["hive"].as_str().unwrap_or("CurrentUser");
    let hive = if hive_s.contains("LocalMachine") {
        RegHive::LocalMachine
    } else {
        hive
    };
    match backup.get("value") {
        Some(v) if v.is_null() => {
            let _ = recleaner_win::reg_delete_value(hive, key, name);
        }
        Some(v) => {
            if let Some(n) = v.as_u64() {
                reg_set_dword(hive, key, name, n as u32)?;
            }
        }
        None => {}
    }
    Ok(())
}

/// All built-in Safe / Moderate / Risky tweaks (ordered for the Optimize page).
pub fn all_tweaks() -> Vec<Box<dyn Tweak>> {
    vec![
        // —— Safe ——
        Box::new(FnTweak {
            info: info(
                "game-mode",
                "Enable Game Mode",
                "Включить игровой режим",
                "Turns on Windows Game Mode.",
                "Включает игровой режим Windows.",
                TweakCategory::Gaming,
                Risk::Safe,
                false,
            ),
            detect: || {
                let v = reg_get_dword(
                    RegHive::CurrentUser,
                    r"Software\Microsoft\GameBar",
                    "AutoGameModeEnabled",
                )?;
                Ok(TweakState {
                    id: "game-mode".into(),
                    applied: v == Some(1),
                    detail: format!("{v:?}"),
                })
            },
            apply: || {
                let b = dword_backup(
                    RegHive::CurrentUser,
                    r"Software\Microsoft\GameBar",
                    "AutoGameModeEnabled",
                )?;
                reg_set_dword(
                    RegHive::CurrentUser,
                    r"Software\Microsoft\GameBar",
                    "AutoGameModeEnabled",
                    1,
                )?;
                Ok(b)
            },
            revert: restore_dword,
        }),
        Box::new(FnTweak {
            info: info(
                "game-dvr-off",
                "Disable Game DVR / Captures",
                "Отключить Game DVR",
                "Disables background recording overlays.",
                "Отключает фоновую запись и оверлеи.",
                TweakCategory::Gaming,
                Risk::Safe,
                false,
            ),
            detect: || {
                let v = reg_get_dword(
                    RegHive::CurrentUser,
                    r"System\GameConfigStore",
                    "GameDVR_Enabled",
                )?;
                Ok(TweakState {
                    id: "game-dvr-off".into(),
                    applied: v == Some(0),
                    detail: format!("{v:?}"),
                })
            },
            apply: || {
                let b = dword_backup(
                    RegHive::CurrentUser,
                    r"System\GameConfigStore",
                    "GameDVR_Enabled",
                )?;
                reg_set_dword(
                    RegHive::CurrentUser,
                    r"System\GameConfigStore",
                    "GameDVR_Enabled",
                    0,
                )?;
                let _ = reg_set_dword(
                    RegHive::CurrentUser,
                    r"Software\Microsoft\Windows\CurrentVersion\GameDVR",
                    "AppCaptureEnabled",
                    0,
                );
                Ok(b)
            },
            revert: restore_dword,
        }),
        Box::new(FnTweak {
            info: info(
                "mouse-accel-off",
                "Disable mouse acceleration",
                "Отключить ускорение мыши",
                "Sets Enhance pointer precision off (raw-ish).",
                "Отключает повышенную точность указателя.",
                TweakCategory::Input,
                Risk::Safe,
                false,
            ),
            detect: || {
                let v = reg_get_dword(RegHive::CurrentUser, r"Control Panel\Mouse", "MouseSpeed")?;
                Ok(TweakState {
                    id: "mouse-accel-off".into(),
                    applied: v == Some(0),
                    detail: format!("{v:?}"),
                })
            },
            apply: || {
                let key = r"Control Panel\Mouse";
                let backup = serde_json::json!({
                    "MouseSpeed": reg_get_dword(RegHive::CurrentUser, key, "MouseSpeed")?,
                    "MouseThreshold1": reg_get_dword(RegHive::CurrentUser, key, "MouseThreshold1")?,
                    "MouseThreshold2": reg_get_dword(RegHive::CurrentUser, key, "MouseThreshold2")?,
                });
                reg_set_dword(RegHive::CurrentUser, key, "MouseSpeed", 0)?;
                reg_set_dword(RegHive::CurrentUser, key, "MouseThreshold1", 0)?;
                reg_set_dword(RegHive::CurrentUser, key, "MouseThreshold2", 0)?;
                Ok(backup)
            },
            revert: |b| {
                let key = r"Control Panel\Mouse";
                if let Some(v) = b["MouseSpeed"].as_u64() {
                    reg_set_dword(RegHive::CurrentUser, key, "MouseSpeed", v as u32)?;
                }
                if let Some(v) = b["MouseThreshold1"].as_u64() {
                    reg_set_dword(RegHive::CurrentUser, key, "MouseThreshold1", v as u32)?;
                }
                if let Some(v) = b["MouseThreshold2"].as_u64() {
                    reg_set_dword(RegHive::CurrentUser, key, "MouseThreshold2", v as u32)?;
                }
                Ok(())
            },
        }),
        Box::new(FnTweak {
            info: info(
                "sticky-keys-off",
                "Disable Sticky Keys hotkey",
                "Отключить залипание клавиш",
                "Prevents Shift×5 popup during games.",
                "Убирает всплытие по пяти нажатиям Shift.",
                TweakCategory::Input,
                Risk::Safe,
                false,
            ),
            detect: || {
                let v = reg_get_dword(
                    RegHive::CurrentUser,
                    r"Control Panel\Accessibility\StickyKeys",
                    "Flags",
                )?;
                Ok(TweakState {
                    id: "sticky-keys-off".into(),
                    applied: matches!(v, Some(x) if x & 0x200 == 0),
                    detail: format!("{v:?}"),
                })
            },
            apply: || {
                let key = r"Control Panel\Accessibility\StickyKeys";
                let prev = reg_get_dword(RegHive::CurrentUser, key, "Flags")?;
                let backup = serde_json::json!({"Flags": prev});
                let flags = prev.unwrap_or(510) & !0x200;
                reg_set_dword(RegHive::CurrentUser, key, "Flags", flags)?;
                Ok(backup)
            },
            revert: |b| {
                if let Some(v) = b["Flags"].as_u64() {
                    reg_set_dword(
                        RegHive::CurrentUser,
                        r"Control Panel\Accessibility\StickyKeys",
                        "Flags",
                        v as u32,
                    )?;
                }
                Ok(())
            },
        }),
        Box::new(FnTweak {
            info: info(
                "visual-fx-perf",
                "Visual effects: best performance",
                "Визуальные эффекты: производительность",
                "Sets VisualFXSetting to Adjust for best performance.",
                "Режим «Обеспечить наилучшее быстродействие».",
                TweakCategory::Visual,
                Risk::Safe,
                false,
            ),
            detect: || {
                let v = reg_get_dword(
                    RegHive::CurrentUser,
                    r"Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects",
                    "VisualFXSetting",
                )?;
                Ok(TweakState {
                    id: "visual-fx-perf".into(),
                    applied: v == Some(2),
                    detail: format!("{v:?}"),
                })
            },
            apply: || {
                let b = dword_backup(
                    RegHive::CurrentUser,
                    r"Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects",
                    "VisualFXSetting",
                )?;
                reg_set_dword(
                    RegHive::CurrentUser,
                    r"Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects",
                    "VisualFXSetting",
                    2,
                )?;
                Ok(b)
            },
            revert: restore_dword,
        }),
        Box::new(FnTweak {
            info: info(
                "background-apps-off",
                "Limit background apps",
                "Ограничить фоновые приложения",
                "Disables global background apps permission.",
                "Отключает глобальный доступ фоновых приложений.",
                TweakCategory::Background,
                Risk::Safe,
                false,
            ),
            detect: || {
                let v = reg_get_dword(
                    RegHive::CurrentUser,
                    r"Software\Microsoft\Windows\CurrentVersion\BackgroundAccessApplications",
                    "GlobalUserDisabled",
                )?;
                Ok(TweakState {
                    id: "background-apps-off".into(),
                    applied: v == Some(1),
                    detail: format!("{v:?}"),
                })
            },
            apply: || {
                let b = dword_backup(
                    RegHive::CurrentUser,
                    r"Software\Microsoft\Windows\CurrentVersion\BackgroundAccessApplications",
                    "GlobalUserDisabled",
                )?;
                reg_set_dword(
                    RegHive::CurrentUser,
                    r"Software\Microsoft\Windows\CurrentVersion\BackgroundAccessApplications",
                    "GlobalUserDisabled",
                    1,
                )?;
                Ok(b)
            },
            revert: restore_dword,
        }),
        // —— Moderate ——
        Box::new(FnTweak {
            info: info(
                "hags-on",
                "Hardware-accelerated GPU scheduling",
                "Планирование GPU с аппаратным ускорением",
                "Enables HAGS (HwSchMode=2). May need reboot.",
                "Включает HAGS. Может потребоваться перезагрузка.",
                TweakCategory::Gaming,
                Risk::Moderate,
                true,
            ),
            detect: || {
                let v = reg_get_dword(
                    RegHive::LocalMachine,
                    r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers",
                    "HwSchMode",
                )?;
                Ok(TweakState {
                    id: "hags-on".into(),
                    applied: v == Some(2),
                    detail: format!("{v:?}"),
                })
            },
            apply: || {
                let b = dword_backup(
                    RegHive::LocalMachine,
                    r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers",
                    "HwSchMode",
                )?;
                reg_set_dword(
                    RegHive::LocalMachine,
                    r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers",
                    "HwSchMode",
                    2,
                )?;
                Ok(b)
            },
            revert: restore_dword,
        }),
        Box::new(FnTweak {
            info: info(
                "swap-effect-upgrade",
                "SwapEffectUpgrade (optimizations for windowed games)",
                "SwapEffectUpgrade (оконные игры)",
                "Enables Optimizations for windowed games.",
                "Включает оптимизации для оконных игр.",
                TweakCategory::Gaming,
                Risk::Moderate,
                false,
            ),
            detect: || {
                let v = reg_get_dword(
                    RegHive::CurrentUser,
                    r"Software\Microsoft\DirectX\UserGpuPreferences",
                    "DirectXUserGlobalSettings",
                );
                Ok(TweakState {
                    id: "swap-effect-upgrade".into(),
                    applied: false,
                    detail: format!("{v:?}"),
                })
            },
            apply: || {
                let key = r"Software\Microsoft\DirectX\UserGpuPreferences";
                let prev = recleaner_win::reg_get_string(
                    RegHive::CurrentUser,
                    key,
                    "DirectXUserGlobalSettings",
                )?;
                let backup = serde_json::json!({"value": prev});
                let _ = recleaner_win::reg_set_string(
                    RegHive::CurrentUser,
                    key,
                    "DirectXUserGlobalSettings",
                    "SwapEffectUpgradeEnable=1;",
                );
                Ok(backup)
            },
            revert: |b| {
                let key = r"Software\Microsoft\DirectX\UserGpuPreferences";
                match b.get("value") {
                    Some(v) if v.is_null() => {
                        let _ = recleaner_win::reg_delete_value(
                            RegHive::CurrentUser,
                            key,
                            "DirectXUserGlobalSettings",
                        );
                    }
                    Some(v) => {
                        if let Some(s) = v.as_str() {
                            recleaner_win::reg_set_string(
                                RegHive::CurrentUser,
                                key,
                                "DirectXUserGlobalSettings",
                                s,
                            )?;
                        }
                    }
                    None => {}
                }
                Ok(())
            },
        }),
        Box::new(FnTweak {
            info: info(
                "ultimate-performance",
                "Ultimate Performance power plan",
                "Схема Ultimate Performance",
                "Duplicates and activates Ultimate Performance.",
                "Создаёт и активирует схему максимальной производительности.",
                TweakCategory::Power,
                Risk::Moderate,
                false,
            ),
            detect: || {
                let r = run_hidden("powercfg", &["/getactivescheme"])?;
                Ok(TweakState {
                    id: "ultimate-performance".into(),
                    applied: r.stdout.to_lowercase().contains("ultimate"),
                    detail: r.stdout.trim().into(),
                })
            },
            apply: || {
                let prev = run_hidden("powercfg", &["/getactivescheme"])?;
                let backup = serde_json::json!({"active": prev.stdout});
                let _ = run_hidden(
                    "powercfg",
                    &["-duplicatescheme", "e9a42b02-d5df-448d-aa00-03f14749eb61"],
                );
                // activate by GUID search via powercfg /list is brittle; try known GUID
                let _ = run_hidden(
                    "powercfg",
                    &["/setactive", "e9a42b02-d5df-448d-aa00-03f14749eb61"],
                );
                Ok(backup)
            },
            revert: |b| {
                // Best-effort: parse GUID from previous active scheme line
                if let Some(active) = b["active"].as_str() {
                    if let Some(start) = active.find(':') {
                        let rest = active[start + 1..].trim();
                        let guid = rest.split_whitespace().next().unwrap_or("");
                        if !guid.is_empty() {
                            let _ = run_hidden("powercfg", &["/setactive", guid]);
                        }
                    }
                }
                Ok(())
            },
        }),
        Box::new(FnTweak {
            info: info(
                "mmcss-games",
                "MMCSS Games priority",
                "Приоритет MMCSS Games",
                "Raises Multimedia Class Scheduler Games priority.",
                "Повышает приоритет Games в MMCSS.",
                TweakCategory::Gaming,
                Risk::Moderate,
                false,
            ),
            detect: || {
                let v = reg_get_dword(
                    RegHive::LocalMachine,
                    r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games",
                    "Priority",
                )?;
                Ok(TweakState {
                    id: "mmcss-games".into(),
                    applied: v == Some(8),
                    detail: format!("{v:?}"),
                })
            },
            apply: || {
                let key = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games";
                let backup = serde_json::json!({
                    "Priority": reg_get_dword(RegHive::LocalMachine, key, "Priority")?,
                    "GPU Priority": reg_get_dword(RegHive::LocalMachine, key, "GPU Priority")?,
                    "Scheduling Category": recleaner_win::reg_get_string(RegHive::LocalMachine, key, "Scheduling Category")?,
                });
                reg_set_dword(RegHive::LocalMachine, key, "Priority", 8)?;
                reg_set_dword(RegHive::LocalMachine, key, "GPU Priority", 8)?;
                let _ = recleaner_win::reg_set_string(
                    RegHive::LocalMachine,
                    key,
                    "Scheduling Category",
                    "High",
                );
                Ok(backup)
            },
            revert: |b| {
                let key = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games";
                if let Some(v) = b["Priority"].as_u64() {
                    reg_set_dword(RegHive::LocalMachine, key, "Priority", v as u32)?;
                }
                if let Some(v) = b["GPU Priority"].as_u64() {
                    reg_set_dword(RegHive::LocalMachine, key, "GPU Priority", v as u32)?;
                }
                Ok(())
            },
        }),
        Box::new(FnTweak {
            info: info(
                "fso-off",
                "Disable Fullscreen Optimizations (global hint)",
                "Отключить оптимизации полного экрана",
                "Sets GameDVR_FSEBehaviorMode hint.",
                "Глобальная подсказка FSO off.",
                TweakCategory::Gaming,
                Risk::Moderate,
                false,
            ),
            detect: || {
                let v = reg_get_dword(
                    RegHive::CurrentUser,
                    r"System\GameConfigStore",
                    "GameDVR_FSEBehaviorMode",
                )?;
                Ok(TweakState {
                    id: "fso-off".into(),
                    applied: v == Some(2),
                    detail: format!("{v:?}"),
                })
            },
            apply: || {
                let b = dword_backup(
                    RegHive::CurrentUser,
                    r"System\GameConfigStore",
                    "GameDVR_FSEBehaviorMode",
                )?;
                reg_set_dword(
                    RegHive::CurrentUser,
                    r"System\GameConfigStore",
                    "GameDVR_FSEBehaviorMode",
                    2,
                )?;
                Ok(b)
            },
            revert: restore_dword,
        }),
        Box::new(FnTweak {
            info: info(
                "delivery-opt-off",
                "Disable Delivery Optimization (P2P)",
                "Отключить оптимизацию доставки (P2P)",
                "Sets DODownloadMode to 0.",
                "Отключает P2P раздачу обновлений.",
                TweakCategory::Background,
                Risk::Moderate,
                false,
            ),
            detect: || {
                let v = reg_get_dword(
                    RegHive::LocalMachine,
                    r"SOFTWARE\Microsoft\Windows\CurrentVersion\DeliveryOptimization\Config",
                    "DODownloadMode",
                )?;
                Ok(TweakState {
                    id: "delivery-opt-off".into(),
                    applied: v == Some(0),
                    detail: format!("{v:?}"),
                })
            },
            apply: || {
                let b = dword_backup(
                    RegHive::LocalMachine,
                    r"SOFTWARE\Microsoft\Windows\CurrentVersion\DeliveryOptimization\Config",
                    "DODownloadMode",
                )?;
                reg_set_dword(
                    RegHive::LocalMachine,
                    r"SOFTWARE\Microsoft\Windows\CurrentVersion\DeliveryOptimization\Config",
                    "DODownloadMode",
                    0,
                )?;
                Ok(b)
            },
            revert: restore_dword,
        }),
        Box::new(FnTweak {
            info: info(
                "widgets-off",
                "Disable Widgets / News feed",
                "Отключить виджеты",
                "Turns off Windows Widgets taskbar feature.",
                "Отключает виджеты на панели задач.",
                TweakCategory::Background,
                Risk::Moderate,
                false,
            ),
            detect: || {
                let v = reg_get_dword(
                    RegHive::CurrentUser,
                    r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
                    "TaskbarDa",
                )?;
                Ok(TweakState {
                    id: "widgets-off".into(),
                    applied: v == Some(0),
                    detail: format!("{v:?}"),
                })
            },
            apply: || {
                let b = dword_backup(
                    RegHive::CurrentUser,
                    r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
                    "TaskbarDa",
                )?;
                reg_set_dword(
                    RegHive::CurrentUser,
                    r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
                    "TaskbarDa",
                    0,
                )?;
                Ok(b)
            },
            revert: restore_dword,
        }),
        Box::new(FnTweak {
            info: info(
                "copilot-off",
                "Hide Copilot button",
                "Скрыть Copilot",
                "Removes Copilot from taskbar where supported.",
                "Убирает кнопку Copilot с панели задач.",
                TweakCategory::Background,
                Risk::Moderate,
                false,
            ),
            detect: || {
                let v = reg_get_dword(
                    RegHive::CurrentUser,
                    r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
                    "ShowCopilotButton",
                )?;
                Ok(TweakState {
                    id: "copilot-off".into(),
                    applied: v == Some(0),
                    detail: format!("{v:?}"),
                })
            },
            apply: || {
                let b = dword_backup(
                    RegHive::CurrentUser,
                    r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
                    "ShowCopilotButton",
                )?;
                reg_set_dword(
                    RegHive::CurrentUser,
                    r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
                    "ShowCopilotButton",
                    0,
                )?;
                Ok(b)
            },
            revert: restore_dword,
        }),
        Box::new(FnTweak {
            info: info(
                "core-parking-off",
                "Disable CPU core parking (powercfg)",
                "Отключить parking ядер",
                "Sets CPMINCORES to 100% on active plan.",
                "Устанавливает CPMINCORES = 100%.",
                TweakCategory::Power,
                Risk::Moderate,
                false,
            ),
            detect: || {
                Ok(TweakState {
                    id: "core-parking-off".into(),
                    applied: false,
                    detail: String::new(),
                })
            },
            apply: || {
                let prev = run_hidden("powercfg", &["/getactivescheme"])?;
                let backup = serde_json::json!({"active": prev.stdout});
                let _ = run_hidden(
                    "powercfg",
                    &[
                        "-setacvalueindex",
                        "SCHEME_CURRENT",
                        "SUB_PROCESSOR",
                        "CPMINCORES",
                        "100",
                    ],
                );
                let _ = run_hidden("powercfg", &["-setactive", "SCHEME_CURRENT"]);
                Ok(backup)
            },
            revert: |_| Ok(()),
        }),
        // —— Risky ——
        Box::new(FnTweak {
            info: info(
                "telemetry-diagtrack",
                "Disable DiagTrack telemetry service",
                "Отключить службу DiagTrack",
                "Stops and disables Connected User Experiences and Telemetry.",
                "Останавливает и отключает телеметрию DiagTrack.",
                TweakCategory::Privacy,
                Risk::Risky,
                false,
            ),
            detect: || {
                let st = service_config_start_type("DiagTrack").ok();
                Ok(TweakState {
                    id: "telemetry-diagtrack".into(),
                    applied: st == Some(ServiceStartType::Disabled),
                    detail: format!("{st:?}"),
                })
            },
            apply: || {
                let prev = service_config_start_type("DiagTrack").ok();
                let backup = serde_json::json!({"start": format!("{prev:?}")});
                let _ = service_stop("DiagTrack");
                service_set_start_type("DiagTrack", ServiceStartType::Disabled)?;
                Ok(backup)
            },
            revert: |b| {
                let s = b["start"].as_str().unwrap_or("");
                let t = if s.contains("Auto") {
                    ServiceStartType::Auto
                } else if s.contains("Manual") {
                    ServiceStartType::Manual
                } else {
                    ServiceStartType::Manual
                };
                service_set_start_type("DiagTrack", t)?;
                Ok(())
            },
        }),
        Box::new(FnTweak {
            info: info(
                "sysmain-off",
                "Disable SysMain (Superfetch)",
                "Отключить SysMain",
                "Stops SysMain — may help on HDD, mixed on SSD.",
                "Отключает SysMain — эффект зависит от диска.",
                TweakCategory::Background,
                Risk::Risky,
                false,
            ),
            detect: || {
                let st = service_config_start_type("SysMain").ok();
                Ok(TweakState {
                    id: "sysmain-off".into(),
                    applied: st == Some(ServiceStartType::Disabled),
                    detail: format!("{st:?}"),
                })
            },
            apply: || {
                let prev = service_config_start_type("SysMain").ok();
                let backup = serde_json::json!({"start": format!("{prev:?}")});
                let _ = service_stop("SysMain");
                service_set_start_type("SysMain", ServiceStartType::Disabled)?;
                Ok(backup)
            },
            revert: |b| {
                let s = b["start"].as_str().unwrap_or("Auto");
                let t = if s.contains("Auto") {
                    ServiceStartType::Auto
                } else {
                    ServiceStartType::Manual
                };
                service_set_start_type("SysMain", t)?;
                Ok(())
            },
        }),
        Box::new(FnTweak {
            info: info(
                "wsearch-off",
                "Disable Windows Search service",
                "Отключить Windows Search",
                "Stops WSearch — breaks start search indexing.",
                "Отключает индексацию поиска.",
                TweakCategory::Background,
                Risk::Risky,
                false,
            ),
            detect: || {
                let st = service_config_start_type("WSearch").ok();
                Ok(TweakState {
                    id: "wsearch-off".into(),
                    applied: st == Some(ServiceStartType::Disabled),
                    detail: format!("{st:?}"),
                })
            },
            apply: || {
                let prev = service_config_start_type("WSearch").ok();
                let backup = serde_json::json!({"start": format!("{prev:?}")});
                let _ = service_stop("WSearch");
                service_set_start_type("WSearch", ServiceStartType::Disabled)?;
                Ok(backup)
            },
            revert: |b| {
                let s = b["start"].as_str().unwrap_or("Auto");
                let t = if s.contains("Auto") {
                    ServiceStartType::Auto
                } else {
                    ServiceStartType::Manual
                };
                service_set_start_type("WSearch", t)?;
                Ok(())
            },
        }),
        Box::new(FnTweak {
            info: info(
                "vbs-hvci-off",
                "Disable VBS / HVCI (SECURITY WARNING)",
                "Отключить VBS / HVCI (ПРЕДУПРЕЖДЕНИЕ)",
                "Reduces security (Credential Guard / memory integrity). Reboot required.",
                "Снижает безопасность. Нужна перезагрузка.",
                TweakCategory::Security,
                Risk::Risky,
                true,
            ),
            detect: || {
                let v = reg_get_dword(
                    RegHive::LocalMachine,
                    r"SYSTEM\CurrentControlSet\Control\DeviceGuard",
                    "EnableVirtualizationBasedSecurity",
                )?;
                Ok(TweakState {
                    id: "vbs-hvci-off".into(),
                    applied: v == Some(0),
                    detail: format!("{v:?}"),
                })
            },
            apply: || {
                let b = dword_backup(
                    RegHive::LocalMachine,
                    r"SYSTEM\CurrentControlSet\Control\DeviceGuard",
                    "EnableVirtualizationBasedSecurity",
                )?;
                reg_set_dword(
                    RegHive::LocalMachine,
                    r"SYSTEM\CurrentControlSet\Control\DeviceGuard",
                    "EnableVirtualizationBasedSecurity",
                    0,
                )?;
                let _ = reg_set_dword(
                    RegHive::LocalMachine,
                    r"SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity",
                    "Enabled",
                    0,
                );
                let _ = run_hidden("bcdedit", &["/set", "hypervisorlaunchtype", "off"]);
                Ok(b)
            },
            revert: |b| {
                restore_dword(b)?;
                let _ = run_hidden("bcdedit", &["/set", "hypervisorlaunchtype", "auto"]);
                Ok(())
            },
        }),
        Box::new(FnTweak {
            info: info(
                "nagle-off",
                "Disable Nagle algorithm (TcpNoDelay)",
                "Отключить алгоритм Нейгла",
                "Sets TcpAckFrequency / TCPNoDelay on network interfaces.",
                "TcpAckFrequency / TCPNoDelay для интерфейсов.",
                TweakCategory::Network,
                Risk::Risky,
                false,
            ),
            detect: || {
                Ok(TweakState {
                    id: "nagle-off".into(),
                    applied: false,
                    detail: String::new(),
                })
            },
            apply: || {
                // Apply under Interfaces key for all GUID subkeys
                let root = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
                let ifaces = root.open_subkey(
                    r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces",
                )?;
                let mut backup = serde_json::Map::new();
                for name in ifaces.enum_keys().filter_map(|k| k.ok()) {
                    let path = format!(
                        r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{name}"
                    );
                    let prev = reg_get_dword(RegHive::LocalMachine, &path, "TcpAckFrequency")?;
                    backup.insert(name.clone(), serde_json::json!(prev));
                    let _ = reg_set_dword(RegHive::LocalMachine, &path, "TcpAckFrequency", 1);
                    let _ = reg_set_dword(RegHive::LocalMachine, &path, "TCPNoDelay", 1);
                }
                Ok(serde_json::Value::Object(backup))
            },
            revert: |b| {
                if let Some(obj) = b.as_object() {
                    for (name, prev) in obj {
                        let path = format!(
                            r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{name}"
                        );
                        if prev.is_null() {
                            let _ = recleaner_win::reg_delete_value(
                                RegHive::LocalMachine,
                                &path,
                                "TcpAckFrequency",
                            );
                        } else if let Some(v) = prev.as_u64() {
                            let _ = reg_set_dword(
                                RegHive::LocalMachine,
                                &path,
                                "TcpAckFrequency",
                                v as u32,
                            );
                        }
                    }
                }
                Ok(())
            },
        }),
        Box::new(FnTweak {
            info: info(
                "recall-off",
                "Disable Windows Recall (where present)",
                "Отключить Windows Recall",
                "Sets policy to disable Recall snapshots.",
                "Отключает снимки Recall политикой.",
                TweakCategory::Privacy,
                Risk::Risky,
                false,
            ),
            detect: || {
                let v = reg_get_dword(
                    RegHive::LocalMachine,
                    r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI",
                    "DisableAIDataAnalysis",
                )?;
                Ok(TweakState {
                    id: "recall-off".into(),
                    applied: v == Some(1),
                    detail: format!("{v:?}"),
                })
            },
            apply: || {
                let b = dword_backup(
                    RegHive::LocalMachine,
                    r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI",
                    "DisableAIDataAnalysis",
                )?;
                reg_set_dword(
                    RegHive::LocalMachine,
                    r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI",
                    "DisableAIDataAnalysis",
                    1,
                )?;
                Ok(b)
            },
            revert: restore_dword,
        }),
    ]
}
