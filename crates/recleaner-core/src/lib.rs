//! Cleanup engine for Recleaner.
//!
//! Loads embedded TOML rules (`rules/*.toml`), **analyzes** reclaimable size in
//! parallel (rayon), then **cleans** with progress callbacks. Safety gates refuse
//! reparse points, system roots, too-young files, user exclusions, and skip locked
//! files / busy processes listed in each rule.

mod engine;
mod rules;
mod safety;

pub use engine::{
    analyze, clean, find_blocking_processes, AnalyzeProgress, AnalyzeResult, BlockingProcess,
    CleanProgress, CleanResult, RuleHit,
};
pub use rules::{
    load_embedded_rules, ActionKind, Category, LocalizedString, RiskLevel, Rule, RuleSet,
};
pub use safety::{is_safe_to_delete, SafetyReason};
