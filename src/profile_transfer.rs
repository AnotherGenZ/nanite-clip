use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::rules::{RuleDefinition, RuleProfile, validate_rule};

const PROFILE_TRANSFER_FORMAT_VERSION: u32 = 1;
const RULE_TRANSFER_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileTransferBundle {
    #[serde(default = "default_profile_transfer_format_version")]
    pub format_version: u32,
    #[serde(default)]
    pub profiles: Vec<RuleProfile>,
    #[serde(default)]
    pub rules: Vec<RuleDefinition>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileTransferConflicts {
    pub profile_ids: Vec<String>,
    pub rule_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileTransferOutcome {
    pub imported_profiles: usize,
    pub overwritten_profiles: usize,
    pub imported_rules: usize,
    pub overwritten_rules: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleTransferBundle {
    #[serde(default = "default_rule_transfer_format_version")]
    pub format_version: u32,
    #[serde(default)]
    pub rules: Vec<RuleDefinition>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleTransferConflicts {
    pub rule_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleTransferOutcome {
    pub imported_rules: usize,
    pub overwritten_rules: usize,
}

impl ProfileTransferConflicts {
    pub fn is_empty(&self) -> bool {
        self.profile_ids.is_empty() && self.rule_ids.is_empty()
    }
}

impl ProfileTransferOutcome {
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.imported_profiles > 0 {
            parts.push(format!(
                "added {}",
                pluralized_count(self.imported_profiles, "profile", "profiles")
            ));
        }
        if self.overwritten_profiles > 0 {
            parts.push(format!(
                "overwrote {}",
                pluralized_count(self.overwritten_profiles, "profile", "profiles")
            ));
        }
        if self.imported_rules > 0 {
            parts.push(format!(
                "added {}",
                pluralized_count(self.imported_rules, "rule", "rules")
            ));
        }
        if self.overwritten_rules > 0 {
            parts.push(format!(
                "overwrote {}",
                pluralized_count(self.overwritten_rules, "rule", "rules")
            ));
        }

        match parts.as_slice() {
            [] => "Imported profile bundle without changes.".into(),
            [single] => format!("Imported profile bundle: {single}."),
            _ => format!("Imported profile bundle: {}.", parts.join(", ")),
        }
    }
}

impl RuleTransferConflicts {
    pub fn is_empty(&self) -> bool {
        self.rule_ids.is_empty()
    }
}

impl RuleTransferOutcome {
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.imported_rules > 0 {
            parts.push(format!(
                "added {}",
                pluralized_count(self.imported_rules, "rule", "rules")
            ));
        }
        if self.overwritten_rules > 0 {
            parts.push(format!(
                "overwrote {}",
                pluralized_count(self.overwritten_rules, "rule", "rules")
            ));
        }

        match parts.as_slice() {
            [] => "Imported rule bundle without changes.".into(),
            [single] => format!("Imported rule bundle: {single}."),
            _ => format!("Imported rule bundle: {}.", parts.join(", ")),
        }
    }
}

impl ProfileTransferBundle {
    pub fn from_profiles(profiles: &[RuleProfile], all_rules: &[RuleDefinition]) -> Self {
        let referenced_rule_ids: BTreeSet<&str> = profiles
            .iter()
            .flat_map(|profile| profile.enabled_rule_ids.iter().map(String::as_str))
            .collect();

        let rules = all_rules
            .iter()
            .filter(|rule| referenced_rule_ids.contains(rule.id.as_str()))
            .cloned()
            .collect();

        Self {
            format_version: PROFILE_TRANSFER_FORMAT_VERSION,
            profiles: profiles.to_vec(),
            rules,
        }
    }

    pub fn from_toml(contents: &str) -> Result<Self, String> {
        let bundle: Self = toml::from_str(contents)
            .map_err(|error| format!("Failed to parse import file: {error}"))?;
        bundle.validate()?;
        Ok(bundle)
    }

    pub fn to_toml_string(&self) -> Result<String, String> {
        self.validate()?;
        toml::to_string_pretty(self)
            .map_err(|error| format!("Failed to serialize profile bundle: {error}"))
    }

    pub fn detect_conflicts(
        &self,
        existing_profiles: &[RuleProfile],
        existing_rules: &[RuleDefinition],
    ) -> ProfileTransferConflicts {
        let existing_profile_ids: BTreeSet<&str> = existing_profiles
            .iter()
            .map(|profile| profile.id.as_str())
            .collect();
        let existing_rule_ids: BTreeSet<&str> =
            existing_rules.iter().map(|rule| rule.id.as_str()).collect();

        ProfileTransferConflicts {
            profile_ids: self
                .profiles
                .iter()
                .filter(|profile| existing_profile_ids.contains(profile.id.as_str()))
                .map(|profile| profile.id.clone())
                .collect(),
            rule_ids: self
                .rules
                .iter()
                .filter(|rule| existing_rule_ids.contains(rule.id.as_str()))
                .map(|rule| rule.id.clone())
                .collect(),
        }
    }

    pub fn apply(
        self,
        existing_profiles: &mut Vec<RuleProfile>,
        existing_rules: &mut Vec<RuleDefinition>,
        overwrite_existing: bool,
    ) -> Result<ProfileTransferOutcome, String> {
        self.validate()?;

        let conflicts = self.detect_conflicts(existing_profiles, existing_rules);
        if !overwrite_existing && !conflicts.is_empty() {
            return Err("Import would overwrite existing profiles or rules.".into());
        }

        let mut outcome = ProfileTransferOutcome::default();

        for imported_rule in self.rules {
            match existing_rules
                .iter_mut()
                .find(|existing_rule| existing_rule.id == imported_rule.id)
            {
                Some(existing_rule) if overwrite_existing => {
                    *existing_rule = imported_rule;
                    outcome.overwritten_rules += 1;
                }
                Some(_) => {}
                None => {
                    existing_rules.push(imported_rule);
                    outcome.imported_rules += 1;
                }
            }
        }

        for imported_profile in self.profiles {
            match existing_profiles
                .iter_mut()
                .find(|existing_profile| existing_profile.id == imported_profile.id)
            {
                Some(existing_profile) if overwrite_existing => {
                    *existing_profile = imported_profile;
                    outcome.overwritten_profiles += 1;
                }
                Some(_) => {}
                None => {
                    existing_profiles.push(imported_profile);
                    outcome.imported_profiles += 1;
                }
            }
        }

        Ok(outcome)
    }

    fn validate(&self) -> Result<(), String> {
        if self.format_version != PROFILE_TRANSFER_FORMAT_VERSION {
            return Err(format!(
                "Unsupported profile bundle format version {}.",
                self.format_version
            ));
        }

        if self.profiles.is_empty() {
            return Err("Import file does not contain any profiles.".into());
        }

        let duplicate_profile_ids =
            duplicate_ids(self.profiles.iter().map(|profile| profile.id.as_str()));
        if !duplicate_profile_ids.is_empty() {
            return Err(format!(
                "Import file contains duplicate profile ids: {}.",
                duplicate_profile_ids.join(", ")
            ));
        }

        let duplicate_rule_ids = duplicate_ids(self.rules.iter().map(|rule| rule.id.as_str()));
        if !duplicate_rule_ids.is_empty() {
            return Err(format!(
                "Import file contains duplicate rule ids: {}.",
                duplicate_rule_ids.join(", ")
            ));
        }

        let rule_ids: BTreeSet<&str> = self.rules.iter().map(|rule| rule.id.as_str()).collect();

        validate_transfer_rules(&self.rules)?;

        for profile in &self.profiles {
            if profile.id.trim().is_empty() {
                return Err("Imported profile id cannot be empty.".into());
            }
            if profile.name.trim().is_empty() {
                return Err(format!(
                    "Imported profile `{}` must have a name.",
                    profile.id
                ));
            }

            let duplicate_enabled_rule_ids =
                duplicate_ids(profile.enabled_rule_ids.iter().map(String::as_str));
            if !duplicate_enabled_rule_ids.is_empty() {
                return Err(format!(
                    "Imported profile `{}` contains duplicate enabled rule ids: {}.",
                    profile.id,
                    duplicate_enabled_rule_ids.join(", ")
                ));
            }

            let missing_rule_ids: Vec<String> = profile
                .enabled_rule_ids
                .iter()
                .filter(|rule_id| !rule_ids.contains(rule_id.as_str()))
                .cloned()
                .collect();
            if !missing_rule_ids.is_empty() {
                return Err(format!(
                    "Imported profile `{}` references missing rules: {}.",
                    profile.id,
                    missing_rule_ids.join(", ")
                ));
            }
        }

        Ok(())
    }
}

impl RuleTransferBundle {
    pub fn from_rules(rules: &[RuleDefinition]) -> Self {
        Self {
            format_version: RULE_TRANSFER_FORMAT_VERSION,
            rules: rules.to_vec(),
        }
    }

    pub fn from_toml(contents: &str) -> Result<Self, String> {
        let bundle: Self = toml::from_str(contents)
            .map_err(|error| format!("Failed to parse import file: {error}"))?;
        bundle.validate()?;
        Ok(bundle)
    }

    pub fn to_toml_string(&self) -> Result<String, String> {
        self.validate()?;
        toml::to_string_pretty(self)
            .map_err(|error| format!("Failed to serialize rule bundle: {error}"))
    }

    pub fn detect_conflicts(&self, existing_rules: &[RuleDefinition]) -> RuleTransferConflicts {
        let existing_rule_ids: BTreeSet<&str> =
            existing_rules.iter().map(|rule| rule.id.as_str()).collect();

        RuleTransferConflicts {
            rule_ids: self
                .rules
                .iter()
                .filter(|rule| existing_rule_ids.contains(rule.id.as_str()))
                .map(|rule| rule.id.clone())
                .collect(),
        }
    }

    pub fn apply(
        self,
        existing_rules: &mut Vec<RuleDefinition>,
        overwrite_existing: bool,
    ) -> Result<RuleTransferOutcome, String> {
        self.validate()?;

        let conflicts = self.detect_conflicts(existing_rules);
        if !overwrite_existing && !conflicts.is_empty() {
            return Err("Import would overwrite existing rules.".into());
        }

        let mut outcome = RuleTransferOutcome::default();

        for imported_rule in self.rules {
            match existing_rules
                .iter_mut()
                .find(|existing_rule| existing_rule.id == imported_rule.id)
            {
                Some(existing_rule) if overwrite_existing => {
                    *existing_rule = imported_rule;
                    outcome.overwritten_rules += 1;
                }
                Some(_) => {}
                None => {
                    existing_rules.push(imported_rule);
                    outcome.imported_rules += 1;
                }
            }
        }

        Ok(outcome)
    }

    fn validate(&self) -> Result<(), String> {
        if self.format_version != RULE_TRANSFER_FORMAT_VERSION {
            return Err(format!(
                "Unsupported rule bundle format version {}.",
                self.format_version
            ));
        }

        if self.rules.is_empty() {
            return Err("Import file does not contain any rules.".into());
        }

        validate_transfer_rules(&self.rules)
    }
}

fn default_profile_transfer_format_version() -> u32 {
    PROFILE_TRANSFER_FORMAT_VERSION
}

fn default_rule_transfer_format_version() -> u32 {
    RULE_TRANSFER_FORMAT_VERSION
}

fn validate_transfer_rules(rules: &[RuleDefinition]) -> Result<(), String> {
    let duplicate_rule_ids = duplicate_ids(rules.iter().map(|rule| rule.id.as_str()));
    if !duplicate_rule_ids.is_empty() {
        return Err(format!(
            "Import file contains duplicate rule ids: {}.",
            duplicate_rule_ids.join(", ")
        ));
    }

    for rule in rules {
        if rule.id.trim().is_empty() {
            return Err("Imported rule id cannot be empty.".into());
        }
        if rule.name.trim().is_empty() {
            return Err(format!("Imported rule `{}` must have a name.", rule.id));
        }
        validate_rule(rule)?;
    }

    Ok(())
}

fn duplicate_ids<'a>(ids: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();

    for id in ids {
        if !seen.insert(id) {
            duplicates.insert(id.to_string());
        }
    }

    duplicates.into_iter().collect()
}

fn pluralized_count(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::default_rule_definitions;

    fn sample_rule(id: &str, name: &str) -> RuleDefinition {
        let mut rule = default_rule_definitions().remove(0);
        rule.id = id.into();
        rule.name = name.into();
        rule
    }

    fn sample_profile(id: &str, name: &str, enabled_rule_ids: &[&str]) -> RuleProfile {
        RuleProfile {
            id: id.into(),
            name: name.into(),
            enabled_rule_ids: enabled_rule_ids
                .iter()
                .map(|rule_id| (*rule_id).into())
                .collect(),
        }
    }

    #[test]
    fn export_bundle_only_includes_rules_enabled_by_profiles() {
        let bundle = ProfileTransferBundle::from_profiles(
            &[sample_profile("profile_live", "Live", &["rule_live"])],
            &[
                sample_rule("rule_live", "Live Rule"),
                sample_rule("rule_unused", "Unused Rule"),
            ],
        );

        assert_eq!(bundle.profiles.len(), 1);
        assert_eq!(bundle.rules.len(), 1);
        assert_eq!(bundle.rules[0].id, "rule_live");
    }

    #[test]
    fn import_file_with_duplicate_profile_ids_is_rejected() {
        let result = ProfileTransferBundle::from_toml(
            r#"
format_version = 1

[[profiles]]
id = "profile_live"
name = "Live"
enabled_rule_ids = ["rule_live"]

[[profiles]]
id = "profile_live"
name = "Live Copy"
enabled_rule_ids = ["rule_live"]

[[rules]]
id = "rule_live"
name = "Live Rule"
lookback_secs = 15
trigger_threshold = 8
reset_threshold = 3
cooldown_secs = 20
use_full_buffer = false
capture_entire_base_cap = false
base_duration_secs = 30
secs_per_point = 0
max_duration_secs = 30
scored_events = [{ event = "Kill", points = 1 }]
"#,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("duplicate profile ids"));
    }

    #[test]
    fn apply_overwrites_conflicting_profiles_and_rules_when_allowed() {
        let bundle = ProfileTransferBundle {
            format_version: 1,
            profiles: vec![
                sample_profile("profile_live", "Imported Live", &["rule_live"]),
                sample_profile("profile_alt", "Imported Alt", &["rule_alt"]),
            ],
            rules: vec![
                sample_rule("rule_live", "Imported Live Rule"),
                sample_rule("rule_alt", "Imported Alt Rule"),
            ],
        };

        let mut existing_profiles = vec![sample_profile(
            "profile_live",
            "Existing Live",
            &["rule_live"],
        )];
        let mut existing_rules = vec![sample_rule("rule_live", "Existing Live Rule")];

        let conflicts = bundle.detect_conflicts(&existing_profiles, &existing_rules);
        assert_eq!(conflicts.profile_ids, vec!["profile_live"]);
        assert_eq!(conflicts.rule_ids, vec!["rule_live"]);

        let outcome = bundle
            .apply(&mut existing_profiles, &mut existing_rules, true)
            .unwrap();

        assert_eq!(
            outcome,
            ProfileTransferOutcome {
                imported_profiles: 1,
                overwritten_profiles: 1,
                imported_rules: 1,
                overwritten_rules: 1,
            }
        );
        assert_eq!(existing_profiles.len(), 2);
        assert_eq!(existing_rules.len(), 2);
        assert_eq!(existing_profiles[0].name, "Imported Live");
        assert_eq!(existing_rules[0].name, "Imported Live Rule");
    }

    #[test]
    fn rule_bundle_detects_conflicting_rule_ids() {
        let bundle = RuleTransferBundle::from_rules(&[
            sample_rule("rule_live", "Imported Live Rule"),
            sample_rule("rule_alt", "Imported Alt Rule"),
        ]);

        let conflicts = bundle.detect_conflicts(&[sample_rule("rule_live", "Existing Live Rule")]);

        assert_eq!(conflicts.rule_ids, vec!["rule_live"]);
    }

    #[test]
    fn rule_bundle_apply_overwrites_existing_rules_when_allowed() {
        let bundle = RuleTransferBundle::from_rules(&[
            sample_rule("rule_live", "Imported Live Rule"),
            sample_rule("rule_alt", "Imported Alt Rule"),
        ]);

        let mut existing_rules = vec![sample_rule("rule_live", "Existing Live Rule")];
        let outcome = bundle.apply(&mut existing_rules, true).unwrap();

        assert_eq!(
            outcome,
            RuleTransferOutcome {
                imported_rules: 1,
                overwritten_rules: 1,
            }
        );
        assert_eq!(existing_rules.len(), 2);
        assert_eq!(existing_rules[0].name, "Imported Live Rule");
    }

    #[test]
    fn rule_bundle_rejects_duplicate_rule_ids() {
        let result = RuleTransferBundle::from_toml(
            r#"
format_version = 1

[[rules]]
id = "rule_live"
name = "Live Rule"
lookback_secs = 15
trigger_threshold = 8
reset_threshold = 3
cooldown_secs = 20
use_full_buffer = false
capture_entire_base_cap = false
base_duration_secs = 30
secs_per_point = 0
max_duration_secs = 30
scored_events = [{ event = "Kill", points = 1 }]

[[rules]]
id = "rule_live"
name = "Live Rule Copy"
lookback_secs = 15
trigger_threshold = 8
reset_threshold = 3
cooldown_secs = 20
use_full_buffer = false
capture_entire_base_cap = false
base_duration_secs = 30
secs_per_point = 0
max_duration_secs = 30
scored_events = [{ event = "Kill", points = 1 }]
"#,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("duplicate rule ids"));
    }
}
