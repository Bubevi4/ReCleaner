//! Recleaner desktop application (Slint UI + cleaner/optimizer wiring).
//!
//! Release builds use `windows_subsystem = "windows"` (no console). The embedded
//! manifest requests elevation and PerMonitorV2 DPI.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;

use anyhow::Result;
use config::{app_data_dir, load_config, save_config, AppConfig};
use recleaner_core::{
    analyze, clean, find_blocking_processes, load_embedded_rules, BlockingProcess, CleanProgress,
};
use recleaner_optimizer::{
    all_tweaks, apply_selected, create_restore_point_only, list_startup_entries, revert_all,
    set_startup_enabled, Risk,
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use tracing_subscriber::{fmt, EnvFilter};

slint::include_modules!();

#[derive(Default)]
struct CleanSession {
    selected: Vec<String>,
    blockers: VecDeque<BlockingProcess>,
    skip_processes: HashSet<String>,
}

fn format_bytes(n: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let f = n as f64;
    if f >= GB {
        format!("{:.2} GB", f / GB)
    } else if f >= MB {
        format!("{:.1} MB", f / MB)
    } else if f >= KB {
        format!("{:.0} KB", f / KB)
    } else {
        format!("{n} B")
    }
}

fn init_logging() -> tracing_appender::non_blocking::WorkerGuard {
    let dir = app_data_dir().join("logs");
    let file_appender = tracing_appender::rolling::daily(&dir, "recleaner.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt()
        .with_env_filter(filter)
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();
    guard
}

fn rule_rows(lang: &str, sizes: &[(String, u64, u64)]) -> Vec<RuleRow> {
    let rules = load_embedded_rules().unwrap_or_default();
    rules
        .rules
        .iter()
        .map(|r| {
            let (bytes, files) = sizes
                .iter()
                .find(|(id, _, _)| id == &r.id)
                .map(|(_, b, f)| (*b, *f))
                .unwrap_or((0, 0));
            RuleRow {
                id: SharedString::from(r.id.as_str()),
                name: SharedString::from(r.name.get(lang)),
                category: SharedString::from(r.category.as_str()),
                risk: SharedString::from(r.risk.as_str()),
                enabled: r.default_enabled,
                size_label: SharedString::from(if bytes > 0 {
                    format_bytes(bytes)
                } else {
                    "-".into()
                }),
                files_label: SharedString::from(if files > 0 {
                    format!("{files}")
                } else {
                    "".into()
                }),
            }
        })
        .collect()
}

fn tweak_rows(lang: &str) -> Vec<TweakRow> {
    all_tweaks()
        .into_iter()
        .map(|t| {
            let info = t.info();
            let state = t.detect().ok();
            let applied = state.as_ref().map(|s| s.applied).unwrap_or(false);
            TweakRow {
                id: SharedString::from(info.id.as_str()),
                name: SharedString::from(if lang == "ru" {
                    info.name_ru.as_str()
                } else {
                    info.name_en.as_str()
                }),
                description: SharedString::from(if lang == "ru" {
                    info.description_ru.as_str()
                } else {
                    info.description_en.as_str()
                }),
                risk: SharedString::from(info.risk.as_str()),
                selected: info.risk == Risk::Safe,
                applied,
                requires_reboot: info.requires_reboot,
            }
        })
        .collect()
}

fn startup_rows() -> Vec<StartupRow> {
    list_startup_entries()
        .unwrap_or_default()
        .into_iter()
        .map(|s| StartupRow {
            id: SharedString::from(s.id.as_str()),
            name: SharedString::from(s.name.as_str()),
            command: SharedString::from(s.command.as_str()),
            location: SharedString::from(s.location.as_str()),
            enabled: s.enabled,
        })
        .collect()
}

fn set_lang(lang: &str) {
    match slint::select_bundled_translation(lang) {
        Ok(()) => tracing::info!("UI language set to {lang}"),
        Err(e) => tracing::warn!("UI language '{lang}' not applied: {e}"),
    }
}

fn collect_enabled_rules(ui: &AppWindow) -> Vec<(String, bool)> {
    let model = ui.get_rules();
    (0..model.row_count())
        .filter_map(|i| model.row_data(i))
        .map(|r| (r.id.to_string(), r.enabled))
        .collect()
}

fn collect_selected_tweaks(ui: &AppWindow) -> Vec<(String, String)> {
    let model = ui.get_tweaks();
    (0..model.row_count())
        .filter_map(|i| model.row_data(i))
        .filter(|t| t.selected)
        .map(|t| (t.id.to_string(), t.risk.to_string()))
        .collect()
}

fn confirm_labels(ui: &AppWindow, accept: &str, cancel: &str) {
    ui.set_confirm_accept_label(SharedString::from(accept));
    ui.set_confirm_cancel_label(SharedString::from(cancel));
}

fn process_prompt(lang: &str, blocker: &BlockingProcess) -> (String, String, String) {
    let rules = blocker.rule_ids.join(", ");
    if lang == "ru" {
        (
            format!(
                "Процесс «{}» блокирует очистку ({rules}). Закрыть процесс или пропустить эти правила?",
                blocker.process
            ),
            "Закрыть".into(),
            "Пропустить".into(),
        )
    } else {
        (
            format!(
                "Process «{}» is blocking cleanup ({rules}). Close the process or skip these rules?",
                blocker.process
            ),
            "Close".into(),
            "Skip".into(),
        )
    }
}

fn show_next_blocker(
    ui: &AppWindow,
    pending: &Mutex<String>,
    session: &Mutex<CleanSession>,
) -> bool {
    let sess = session.lock().unwrap();
    let Some(blocker) = sess.blockers.front().cloned() else {
        return false;
    };
    let lang = ui.get_language().to_string();
    let (msg, accept, cancel) = process_prompt(&lang, &blocker);
    *pending.lock().unwrap() = "close_process".into();
    confirm_labels(ui, &accept, &cancel);
    ui.set_confirm_message(SharedString::from(msg));
    ui.set_show_confirm(true);
    true
}

fn begin_clean_after_confirms(
    ui_weak: slint::Weak<AppWindow>,
    cfg: Arc<Mutex<AppConfig>>,
    pending: Arc<Mutex<String>>,
    session: Arc<Mutex<CleanSession>>,
    selected: Vec<String>,
) {
    let Some(u) = ui_weak.upgrade() else { return };
    let ruleset = load_embedded_rules().unwrap_or_default();
    let blockers = find_blocking_processes(&ruleset, &selected);
    {
        let mut sess = session.lock().unwrap();
        sess.selected = selected;
        sess.blockers = blockers.into();
        sess.skip_processes.clear();
    }
    if show_next_blocker(&u, &pending, &session) {
        return;
    }
    run_clean(ui_weak, cfg, session);
}

fn main() -> Result<()> {
    let _guard = init_logging();
    tracing::info!("Recleaner starting (admin={})", recleaner_win::is_admin());

    let cfg = Arc::new(Mutex::new(load_config()));
    let ui = AppWindow::new()?;

    // Bundled translations are only available after the first component exists.
    let lang = cfg.lock().unwrap().language.clone();
    set_lang(&lang);
    ui.set_language(SharedString::from(lang.as_str()));
    ui.set_status_text(SharedString::from(if recleaner_win::is_admin() {
        "Administrator"
    } else {
        "Not elevated"
    }));

    ui.set_rules(ModelRc::new(VecModel::from(rule_rows(&lang, &[]))));
    ui.set_tweaks(ModelRc::new(VecModel::from(tweak_rows(&lang))));
    ui.set_startups(ModelRc::new(VecModel::from(startup_rows())));

    let pending = Arc::new(Mutex::new(String::new()));
    let clean_session = Arc::new(Mutex::new(CleanSession::default()));

    {
        let ui_weak = ui.as_weak();
        ui.on_toggle_rule(move |idx, enabled| {
            if let Some(u) = ui_weak.upgrade() {
                let model = u.get_rules();
                if let Some(mut row) = model.row_data(idx as usize) {
                    row.enabled = enabled;
                    model.set_row_data(idx as usize, row);
                }
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.on_toggle_tweak(move |idx, selected| {
            if let Some(u) = ui_weak.upgrade() {
                let model = u.get_tweaks();
                if let Some(mut row) = model.row_data(idx as usize) {
                    row.selected = selected;
                    model.set_row_data(idx as usize, row);
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let cfg = cfg.clone();
        ui.on_language_changed(move |lang| {
            let lang = lang.to_string();
            set_lang(&lang);
            if let Ok(mut c) = cfg.lock() {
                c.language = lang.clone();
                let _ = save_config(&c);
            }
            if let Some(u) = ui_weak.upgrade() {
                u.set_language(SharedString::from(lang.as_str()));
                u.set_rules(ModelRc::new(VecModel::from(rule_rows(&lang, &[]))));
                u.set_tweaks(ModelRc::new(VecModel::from(tweak_rows(&lang))));
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let cfg = cfg.clone();
        ui.on_analyze_clicked(move || {
            let Some(u) = ui_weak.upgrade() else { return };
            let exclusions = cfg.lock().unwrap().exclusions.clone();
            let enabled = collect_enabled_rules(&u);
            let selected: Vec<String> = enabled
                .iter()
                .filter(|(_, e)| *e)
                .map(|(id, _)| id.clone())
                .collect();
            u.set_busy(true);
            u.set_progress(0.0);
            u.set_error_text(SharedString::from(""));
            u.set_status_text(SharedString::from("Analyzing…"));
            let ui = ui_weak.clone();
            std::thread::spawn(move || {
                let ruleset = load_embedded_rules().unwrap_or_default();
                let result = analyze(&ruleset, &selected, &exclusions, None);
                let sizes: Vec<(String, u64, u64)> = result
                    .hits
                    .iter()
                    .map(|h| (h.rule_id.clone(), h.bytes, h.files))
                    .collect();
                let total_bytes = result.total_bytes;
                let total_files = result.total_files;
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(u) = ui.upgrade() {
                        let lang = u.get_language().to_string();
                        let mut rows = rule_rows(&lang, &sizes);
                        for row in &mut rows {
                            if let Some((_, e)) =
                                enabled.iter().find(|(id, _)| id == row.id.as_str())
                            {
                                row.enabled = *e;
                            }
                        }
                        u.set_rules(ModelRc::new(VecModel::from(rows)));
                        u.set_results_text(SharedString::from(format!(
                            "{} · {}",
                            format_bytes(total_bytes),
                            total_files
                        )));
                        u.set_busy(false);
                        u.set_progress(1.0);
                        u.set_error_text(SharedString::from(""));
                        u.set_status_text(SharedString::from("Analyze done"));
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        let pending = pending.clone();
        let cfg = cfg.clone();
        let clean_session = clean_session.clone();
        ui.on_clean_clicked(move || {
            let Some(u) = ui_weak.upgrade() else { return };
            let ruleset = load_embedded_rules().unwrap_or_default();
            let selected: Vec<String> = collect_enabled_rules(&u)
                .into_iter()
                .filter(|(_, e)| *e)
                .map(|(id, _)| id)
                .collect();
            let risky = selected.iter().any(|id| {
                ruleset
                    .by_id(id)
                    .map(|r| matches!(r.risk, recleaner_core::RiskLevel::Risky))
                    .unwrap_or(false)
            });
            if risky {
                *pending.lock().unwrap() = "clean_risky".into();
                let lang = u.get_language().to_string();
                if lang == "ru" {
                    confirm_labels(&u, "Продолжить", "Отмена");
                    u.set_confirm_message(SharedString::from(
                        "Выбраны правила с меткой Risky. Продолжить очистку?",
                    ));
                } else {
                    confirm_labels(&u, "Continue", "Cancel");
                    u.set_confirm_message(SharedString::from(
                        "Selected rules include Risky items. Continue with cleanup?",
                    ));
                }
                u.set_show_confirm(true);
            } else {
                begin_clean_after_confirms(
                    ui_weak.clone(),
                    cfg.clone(),
                    pending.clone(),
                    clean_session.clone(),
                    selected,
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let pending = pending.clone();
        ui.on_apply_tweaks_clicked(move || {
            let Some(u) = ui_weak.upgrade() else { return };
            let selected = collect_selected_tweaks(&u);
            let risky = selected.iter().any(|(_, r)| r == "risky");
            if risky {
                *pending.lock().unwrap() = "apply_tweaks".into();
                let lang = u.get_language().to_string();
                if lang == "ru" {
                    confirm_labels(&u, "Продолжить", "Отмена");
                    u.set_confirm_message(SharedString::from(
                        "Risky-твики могут снизить безопасность (VBS/HVCI, службы). Продолжить?",
                    ));
                } else {
                    confirm_labels(&u, "Continue", "Cancel");
                    u.set_confirm_message(SharedString::from(
                        "Risky tweaks can reduce security (VBS/HVCI, services). Continue?",
                    ));
                }
                u.set_show_confirm(true);
            } else {
                run_apply_tweaks(ui_weak.clone());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_revert_all_clicked(move || {
            let ui = ui_weak.clone();
            std::thread::spawn(move || {
                let (ok, msg) = match revert_all() {
                    Ok(()) => (true, "Reverted from latest backup".to_string()),
                    Err(e) => (false, format!("Revert failed: {e}")),
                };
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(u) = ui.upgrade() {
                        u.set_status_text(SharedString::from(msg.clone()));
                        u.set_error_text(SharedString::from(if ok { String::new() } else { msg }));
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_restore_point_clicked(move || {
            let ui = ui_weak.clone();
            std::thread::spawn(move || {
                let (ok, msg) = match create_restore_point_only() {
                    Ok(()) => (true, "Restore point created".to_string()),
                    Err(e) => (false, format!("Restore point failed: {e}")),
                };
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(u) = ui.upgrade() {
                        u.set_status_text(SharedString::from(msg.clone()));
                        u.set_error_text(SharedString::from(if ok { String::new() } else { msg }));
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        let pending = pending.clone();
        let cfg = cfg.clone();
        let clean_session = clean_session.clone();
        ui.on_confirm_accepted(move || {
            let action = pending.lock().unwrap().clone();
            if let Some(u) = ui_weak.upgrade() {
                u.set_show_confirm(false);
            }
            match action.as_str() {
                "clean_risky" => {
                    let selected = {
                        let sess = clean_session.lock().unwrap();
                        if !sess.selected.is_empty() {
                            sess.selected.clone()
                        } else if let Some(u) = ui_weak.upgrade() {
                            collect_enabled_rules(&u)
                                .into_iter()
                                .filter(|(_, e)| *e)
                                .map(|(id, _)| id)
                                .collect()
                        } else {
                            Vec::new()
                        }
                    };
                    // Prefer fresh selection from UI at accept time.
                    let selected = if let Some(u) = ui_weak.upgrade() {
                        collect_enabled_rules(&u)
                            .into_iter()
                            .filter(|(_, e)| *e)
                            .map(|(id, _)| id)
                            .collect()
                    } else {
                        selected
                    };
                    begin_clean_after_confirms(
                        ui_weak.clone(),
                        cfg.clone(),
                        pending.clone(),
                        clean_session.clone(),
                        selected,
                    );
                }
                "close_process" => {
                    let proc = {
                        let mut sess = clean_session.lock().unwrap();
                        sess.blockers.pop_front().map(|b| b.process)
                    };
                    if let Some(proc) = proc {
                        match recleaner_win::kill_processes_by_name(&proc) {
                            Ok(n) => tracing::info!("closed {n} process(es) named {proc}"),
                            Err(e) => {
                                tracing::warn!("close {proc}: {e}");
                                if let Some(u) = ui_weak.upgrade() {
                                    u.set_error_text(SharedString::from(format!(
                                        "Could not close {proc}: {e}"
                                    )));
                                    // Still skip rules for this process if kill failed.
                                    clean_session
                                        .lock()
                                        .unwrap()
                                        .skip_processes
                                        .insert(proc.clone());
                                }
                            }
                        }
                    }
                    if let Some(u) = ui_weak.upgrade() {
                        if show_next_blocker(&u, &pending, &clean_session) {
                            return;
                        }
                    }
                    run_clean(ui_weak.clone(), cfg.clone(), clean_session.clone());
                }
                "apply_tweaks" => run_apply_tweaks(ui_weak.clone()),
                _ => {}
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let pending = pending.clone();
        let cfg = cfg.clone();
        let clean_session = clean_session.clone();
        ui.on_confirm_cancelled(move || {
            let action = pending.lock().unwrap().clone();
            if let Some(u) = ui_weak.upgrade() {
                u.set_show_confirm(false);
            }
            if action == "close_process" {
                // Skip = leave process running, skip its rules.
                if let Some(blocker) = clean_session.lock().unwrap().blockers.pop_front() {
                    clean_session
                        .lock()
                        .unwrap()
                        .skip_processes
                        .insert(blocker.process);
                }
                if let Some(u) = ui_weak.upgrade() {
                    if show_next_blocker(&u, &pending, &clean_session) {
                        return;
                    }
                }
                run_clean(ui_weak.clone(), cfg.clone(), clean_session.clone());
            } else {
                *pending.lock().unwrap() = String::new();
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_refresh_startup(move || {
            if let Some(u) = ui_weak.upgrade() {
                u.set_startups(ModelRc::new(VecModel::from(startup_rows())));
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.on_toggle_startup(move |idx, enabled| {
            if let Some(u) = ui_weak.upgrade() {
                let model = u.get_startups();
                if let Some(mut row) = model.row_data(idx as usize) {
                    if let Ok(list) = list_startup_entries() {
                        if let Some(entry) = list.into_iter().find(|e| e.id == row.id.as_str()) {
                            if let Err(e) = set_startup_enabled(&entry, enabled) {
                                tracing::warn!("startup toggle: {e}");
                                u.set_error_text(SharedString::from(format!(
                                    "Startup toggle failed: {e}"
                                )));
                            } else {
                                row.enabled = enabled;
                                model.set_row_data(idx as usize, row);
                                u.set_error_text(SharedString::from(""));
                            }
                        }
                    }
                }
            }
        });
    }

    ui.run()?;
    Ok(())
}

fn run_clean(
    ui: slint::Weak<AppWindow>,
    cfg: Arc<Mutex<AppConfig>>,
    session: Arc<Mutex<CleanSession>>,
) {
    let Some(u) = ui.upgrade() else { return };
    let exclusions = cfg.lock().unwrap().exclusions.clone();
    let (selected, skip_processes) = {
        let sess = session.lock().unwrap();
        (sess.selected.clone(), sess.skip_processes.clone())
    };
    let selected = if selected.is_empty() {
        collect_enabled_rules(&u)
            .into_iter()
            .filter(|(_, e)| *e)
            .map(|(id, _)| id)
            .collect()
    } else {
        selected
    };

    u.set_busy(true);
    u.set_progress(0.02);
    u.set_error_text(SharedString::from(""));
    u.set_status_text(SharedString::from("Cleaning…"));
    let ui_progress = ui.clone();
    std::thread::spawn(move || {
        let ruleset = load_embedded_rules().unwrap_or_default();
        let (tx, rx) = std::sync::mpsc::channel::<CleanProgress>();
        let progress_ui = ui_progress.clone();
        std::thread::spawn(move || {
            while let Ok(p) = rx.recv() {
                let progress_ui = progress_ui.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(u) = progress_ui.upgrade() {
                        u.set_progress(p.fraction);
                        u.set_status_text(SharedString::from(p.message));
                    }
                });
            }
        });

        let result = clean(&ruleset, &selected, &exclusions, Some(tx), &skip_processes);
        let msg = format!(
            "Deleted {} files ({}); skipped {}; errors {}",
            result.deleted_files,
            format_bytes(result.deleted_bytes),
            result.skipped,
            result.errors.len()
        );
        tracing::info!("{msg}");
        for e in &result.errors {
            tracing::warn!("{e}");
        }
        let err_banner = if result.errors.is_empty() {
            String::new()
        } else {
            result.errors.join("; ")
        };
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(u) = ui.upgrade() {
                u.set_results_text(SharedString::from(msg.clone()));
                u.set_status_text(SharedString::from(msg));
                u.set_error_text(SharedString::from(err_banner));
                u.set_busy(false);
                u.set_progress(1.0);
            }
        });
    });
}

fn run_apply_tweaks(ui: slint::Weak<AppWindow>) {
    let Some(u) = ui.upgrade() else { return };
    let selected: Vec<String> = collect_selected_tweaks(&u)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    u.set_busy(true);
    u.set_error_text(SharedString::from(""));
    u.set_status_text(SharedString::from("Applying tweaks…"));
    std::thread::spawn(move || {
        let (ok, msg) = match apply_selected(&selected, true) {
            Ok(path) => (true, format!("Applied. Backup: {}", path.display())),
            Err(e) => (false, format!("Apply failed: {e}")),
        };
        tracing::info!("{msg}");
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(u) = ui.upgrade() {
                u.set_status_text(SharedString::from(msg.clone()));
                u.set_error_text(SharedString::from(if ok { String::new() } else { msg }));
                u.set_busy(false);
                let lang = u.get_language().to_string();
                u.set_tweaks(ModelRc::new(VecModel::from(tweak_rows(&lang))));
            }
        });
    });
}
