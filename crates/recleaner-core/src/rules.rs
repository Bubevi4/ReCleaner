//! TOML rule schema and embedded rule loading (`include_str!`).

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;

/// Top-level cleanup category shown in the UI tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Windows,
    Browsers,
    Apps,
    Games,
    Gpu,
    Dev,
    Privacy,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Windows => "windows",
            Category::Browsers => "browsers",
            Category::Apps => "apps",
            Category::Games => "games",
            Category::Gpu => "gpu",
            Category::Dev => "dev",
            Category::Privacy => "privacy",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Safe,
    Moderate,
    Risky,
}

impl RiskLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            RiskLevel::Safe => "safe",
            RiskLevel::Moderate => "moderate",
            RiskLevel::Risky => "risky",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    DeleteFiles,
    RecycleBin,
    StopServiceThenDelete,
    Command,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LocalizedString {
    pub en: String,
    pub ru: String,
}

impl LocalizedString {
    pub fn get(&self, lang: &str) -> &str {
        if lang.eq_ignore_ascii_case("ru") {
            &self.ru
        } else {
            &self.en
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub id: String,
    pub category: Category,
    pub name: LocalizedString,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default = "default_include")]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub min_age_hours: u64,
    #[serde(default)]
    pub requires_admin: bool,
    #[serde(default = "default_true")]
    pub default_enabled: bool,
    #[serde(default = "default_safe")]
    pub risk: RiskLevel,
    #[serde(default)]
    pub close_processes: Vec<String>,
    #[serde(default = "default_delete")]
    pub action: ActionKind,
    /// For `command` / `stop_service_then_delete`
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub command_args: Vec<String>,
}

fn default_include() -> Vec<String> {
    vec!["**/*".into()]
}
fn default_true() -> bool {
    true
}
fn default_safe() -> RiskLevel {
    RiskLevel::Safe
}
fn default_delete() -> ActionKind {
    ActionKind::DeleteFiles
}

#[derive(Debug, Clone, Deserialize)]
struct RulesFile {
    #[serde(default)]
    rule: Vec<Rule>,
}

#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    pub rules: Vec<Rule>,
}

impl RuleSet {
    pub fn by_id(&self, id: &str) -> Option<&Rule> {
        self.rules.iter().find(|r| r.id == id)
    }

    pub fn by_category(&self) -> HashMap<Category, Vec<&Rule>> {
        let mut map: HashMap<Category, Vec<&Rule>> = HashMap::new();
        for r in &self.rules {
            map.entry(r.category).or_default().push(r);
        }
        map
    }
}

const EMBEDDED: &[(&str, &str)] = &[
    ("windows", include_str!("../rules/windows.toml")),
    ("browsers", include_str!("../rules/browsers.toml")),
    ("apps", include_str!("../rules/apps.toml")),
    ("games", include_str!("../rules/games.toml")),
    ("gpu", include_str!("../rules/gpu.toml")),
    ("dev", include_str!("../rules/dev.toml")),
    ("privacy", include_str!("../rules/privacy.toml")),
];

pub fn load_embedded_rules() -> Result<RuleSet> {
    let mut rules = Vec::new();
    for (name, raw) in EMBEDDED {
        let file: RulesFile = toml::from_str(raw).with_context(|| format!("parse rules/{name}.toml"))?;
        rules.extend(file.rule);
    }
    Ok(RuleSet { rules })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_rules_parse() {
        let set = load_embedded_rules().expect("rules");
        assert!(!set.rules.is_empty());
        assert!(set.by_id("win-temp").is_some());
    }
}
