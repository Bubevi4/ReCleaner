//! Analyze (size scan) and Clean execution with optional progress channels.

use crate::rules::{ActionKind, Rule, RuleSet};
use crate::safety::{is_safe_to_delete, SafetyReason};
use anyhow::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use rayon::prelude::*;
use recleaner_win::{empty_recycle_bin, expand_env, is_process_running, run_hidden, service_stop};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct RuleHit {
    pub rule_id: String,
    pub bytes: u64,
    pub files: u64,
    pub skipped: u64,
}

#[derive(Debug, Clone, Default)]
pub struct AnalyzeResult {
    pub hits: Vec<RuleHit>,
    pub total_bytes: u64,
    pub total_files: u64,
}

#[derive(Debug, Clone)]
pub struct AnalyzeProgress {
    pub rule_id: String,
    pub files_seen: u64,
    pub bytes_seen: u64,
}

#[derive(Debug, Clone, Default)]
pub struct CleanResult {
    pub deleted_files: u64,
    pub deleted_bytes: u64,
    pub errors: Vec<String>,
    pub skipped: u64,
}

#[derive(Debug, Clone)]
pub struct CleanProgress {
    pub rule_id: String,
    pub deleted_files: u64,
    pub deleted_bytes: u64,
    pub message: String,
    /// 0.0..=1.0 overall clean progress.
    pub fraction: f32,
}

/// A process that currently blocks one or more selected cleanup rules.
#[derive(Debug, Clone)]
pub struct BlockingProcess {
    pub process: String,
    pub rule_ids: Vec<String>,
}

fn normalize_proc(name: &str) -> String {
    let n = name.trim().to_lowercase();
    n.strip_suffix(".exe").unwrap_or(&n).to_string()
}

fn selected_rules<'a>(ruleset: &'a RuleSet, selected: &[String]) -> Vec<&'a Rule> {
    ruleset
        .rules
        .iter()
        .filter(|r| selected.iter().any(|id| id == &r.id))
        .collect()
}

/// Unique running processes that would block the selected rules.
pub fn find_blocking_processes(ruleset: &RuleSet, selected: &[String]) -> Vec<BlockingProcess> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for rule in selected_rules(ruleset, selected) {
        for proc in &rule.close_processes {
            if is_process_running(proc) {
                let key = normalize_proc(proc);
                map.entry(key)
                    .or_default()
                    .push(rule.id.clone());
            }
        }
    }
    let mut out: Vec<BlockingProcess> = map
        .into_iter()
        .map(|(process, mut rule_ids)| {
            rule_ids.sort();
            rule_ids.dedup();
            BlockingProcess { process, rule_ids }
        })
        .collect();
    out.sort_by(|a, b| a.process.cmp(&b.process));
    out
}

fn build_globset(patterns: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p)?);
    }
    Ok(b.build()?)
}

fn expand_paths(paths: &[String]) -> Vec<PathBuf> {
    paths
        .iter()
        .map(|p| PathBuf::from(expand_env(p)))
        .filter(|p| p.exists())
        .collect()
}

fn collect_files(rule: &Rule, user_exclusions: &[String]) -> (u64, u64, u64, Vec<PathBuf>) {
    if rule.action == ActionKind::RecycleBin {
        return (0, 0, 0, Vec::new());
    }
    if rule.action == ActionKind::Command {
        return (0, 0, 0, Vec::new());
    }

    let include = match build_globset(&rule.include) {
        Ok(g) => g,
        Err(_) => return (0, 0, 0, Vec::new()),
    };
    let exclude = build_globset(&rule.exclude).unwrap_or_else(|_| GlobSet::empty());

    let roots = expand_paths(&rule.paths);
    let mut files = Vec::new();
    let mut bytes = 0u64;
    let mut skipped = 0u64;

    for root in roots {
        if recleaner_win::is_system_root_protected(&root) {
            skipped += 1;
            continue;
        }
        if recleaner_win::is_reparse_point(&root) {
            skipped += 1;
            continue;
        }
        let walker = WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let p = e.path();
                !recleaner_win::is_reparse_point(p)
            });
        for entry in walker.flatten() {
            let path = entry.path();
            if !entry.file_type().is_file() {
                continue;
            }
            let rel = path.strip_prefix(&root).unwrap_or(path);
            let rel_str = rel.to_string_lossy();
            if !include.is_match(rel_str.as_ref()) && !include.is_match(path) {
                continue;
            }
            if exclude.is_match(rel_str.as_ref()) || exclude.is_match(path) {
                skipped += 1;
                continue;
            }
            match is_safe_to_delete(path, rule.min_age_hours, user_exclusions) {
                SafetyReason::Ok => {
                    let sz = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    bytes += sz;
                    files.push(path.to_path_buf());
                }
                _ => skipped += 1,
            }
        }
    }
    (bytes, files.len() as u64, skipped, files)
}

/// Analyze selected rules (by id). If `selected` is empty, analyze all default_enabled.
pub fn analyze(
    ruleset: &RuleSet,
    selected: &[String],
    user_exclusions: &[String],
    progress: Option<Sender<AnalyzeProgress>>,
) -> AnalyzeResult {
    let rules: Vec<&Rule> = if selected.is_empty() {
        ruleset.rules.iter().filter(|r| r.default_enabled).collect()
    } else {
        selected_rules(ruleset, selected)
    };

    let hits: Vec<RuleHit> = rules
        .par_iter()
        .map(|rule| {
            for proc in &rule.close_processes {
                if is_process_running(proc) {
                    return RuleHit {
                        rule_id: rule.id.clone(),
                        bytes: 0,
                        files: 0,
                        skipped: 1,
                    };
                }
            }
            let (bytes, files, skipped, _) = collect_files(rule, user_exclusions);
            if let Some(tx) = &progress {
                let _ = tx.send(AnalyzeProgress {
                    rule_id: rule.id.clone(),
                    files_seen: files,
                    bytes_seen: bytes,
                });
            }
            RuleHit {
                rule_id: rule.id.clone(),
                bytes,
                files,
                skipped,
            }
        })
        .collect();

    let total_bytes = hits.iter().map(|h| h.bytes).sum();
    let total_files = hits.iter().map(|h| h.files).sum();
    AnalyzeResult {
        hits,
        total_bytes,
        total_files,
    }
}

fn delete_file(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn rule_blocked_by_skip(rule: &Rule, skip_processes: &HashSet<String>) -> bool {
    rule.close_processes
        .iter()
        .any(|p| skip_processes.contains(&normalize_proc(p)))
}

fn rule_still_blocked(rule: &Rule) -> Option<String> {
    for proc in &rule.close_processes {
        if is_process_running(proc) {
            return Some(proc.clone());
        }
    }
    None
}

/// Clean selected rules.
///
/// `skip_processes`: normalized process names the user chose to skip (rules needing them are skipped).
/// Processes the user chose to close should already be terminated before calling this.
pub fn clean(
    ruleset: &RuleSet,
    selected: &[String],
    user_exclusions: &[String],
    progress: Option<Sender<CleanProgress>>,
    skip_processes: &HashSet<String>,
) -> CleanResult {
    let rules = selected_rules(ruleset, selected);

    let deleted_files = AtomicU64::new(0);
    let deleted_bytes = AtomicU64::new(0);
    let skipped = AtomicU64::new(0);
    let mut errors = Vec::new();

    let rule_count = rules.len().max(1) as f32;
    let mut rule_index = 0usize;

    let send_progress = |tx: &Option<Sender<CleanProgress>>,
                         rule_id: &str,
                         local_files: u64,
                         local_bytes: u64,
                         message: String,
                         fraction: f32| {
        if let Some(tx) = tx {
            let _ = tx.send(CleanProgress {
                rule_id: rule_id.to_string(),
                deleted_files: local_files,
                deleted_bytes: local_bytes,
                message,
                fraction: fraction.clamp(0.0, 1.0),
            });
        }
    };

    for rule in rules {
        let base = rule_index as f32 / rule_count;
        let span = 1.0 / rule_count;
        rule_index += 1;

        if rule_blocked_by_skip(rule, skip_processes) {
            skipped.fetch_add(1, Ordering::Relaxed);
            send_progress(
                &progress,
                &rule.id,
                0,
                0,
                format!("Skipped {} (process left running)", rule.id),
                base + span,
            );
            continue;
        }

        if let Some(proc) = rule_still_blocked(rule) {
            errors.push(format!(
                "rule {}: process {proc} is still running, skipped",
                rule.id
            ));
            skipped.fetch_add(1, Ordering::Relaxed);
            send_progress(
                &progress,
                &rule.id,
                0,
                0,
                format!("Blocked by {proc}"),
                base + span,
            );
            continue;
        }

        match rule.action {
            ActionKind::RecycleBin => match empty_recycle_bin() {
                Ok(()) => {
                    deleted_files.fetch_add(1, Ordering::Relaxed);
                    send_progress(
                        &progress,
                        &rule.id,
                        1,
                        0,
                        "Recycle Bin emptied".into(),
                        base + span,
                    );
                }
                Err(e) => errors.push(format!("recycle_bin: {e}")),
            },
            ActionKind::Command => {
                if let Some(cmd) = &rule.command {
                    let args: Vec<&str> = rule.command_args.iter().map(|s| s.as_str()).collect();
                    match run_hidden(cmd, &args) {
                        Ok(r) if r.success() => {
                            deleted_files.fetch_add(1, Ordering::Relaxed);
                            send_progress(
                                &progress,
                                &rule.id,
                                1,
                                0,
                                format!("Ran command for {}", rule.id),
                                base + span,
                            );
                        }
                        Ok(r) => errors.push(format!("command {}: {}", rule.id, r.stderr)),
                        Err(e) => errors.push(format!("command {}: {e}", rule.id)),
                    }
                }
            }
            ActionKind::StopServiceThenDelete | ActionKind::DeleteFiles => {
                if rule.action == ActionKind::StopServiceThenDelete {
                    if let Some(svc) = &rule.service {
                        let _ = service_stop(svc);
                    }
                }
                let (_bytes, _count, skip, files) = collect_files(rule, user_exclusions);
                skipped.fetch_add(skip, Ordering::Relaxed);
                let total = files.len().max(1) as f32;
                let mut local_bytes = 0u64;
                let mut local_files = 0u64;
                for (i, f) in files.into_iter().enumerate() {
                    let sz = std::fs::metadata(&f).map(|m| m.len()).unwrap_or(0);
                    match delete_file(&f) {
                        Ok(()) => {
                            local_files += 1;
                            local_bytes += sz;
                        }
                        Err(_) => {
                            skipped.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    // Update often enough to feel live, without flooding the UI thread.
                    if i == 0 || (i + 1) % 8 == 0 || i + 1 == total as usize {
                        let frac = base + span * ((i as f32 + 1.0) / total);
                        send_progress(
                            &progress,
                            &rule.id,
                            local_files,
                            local_bytes,
                            format!("Cleaning {} ({}/{})", rule.id, i + 1, total as usize),
                            frac,
                        );
                    }
                }
                for root in expand_paths(&rule.paths) {
                    remove_empty_dirs(&root);
                }
                deleted_files.fetch_add(local_files, Ordering::Relaxed);
                deleted_bytes.fetch_add(local_bytes, Ordering::Relaxed);
                send_progress(
                    &progress,
                    &rule.id,
                    local_files,
                    local_bytes,
                    format!("Cleaned {}", rule.id),
                    base + span,
                );
            }
        }
    }

    CleanResult {
        deleted_files: deleted_files.load(Ordering::Relaxed),
        deleted_bytes: deleted_bytes.load(Ordering::Relaxed),
        errors,
        skipped: skipped.load(Ordering::Relaxed),
    }
}

fn remove_empty_dirs(root: &Path) {
    let entries: Vec<_> = WalkDir::new(root)
        .follow_links(false)
        .contents_first(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .collect();
    for e in entries {
        if e.file_type().is_dir() && e.path() != root {
            let _ = std::fs::remove_dir(e.path());
        }
    }
}
