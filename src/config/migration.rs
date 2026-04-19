use super::{
    Config, default_clip_naming_template, default_rule_definitions, default_rule_profiles,
    normalize_audio_sources,
};
use crate::rules::schedule::default_schedule_weekdays;
use crate::rules::{
    AutoSwitchCondition, default_auto_switch_rules, normalized_active_character_ids,
    validate_auto_switch_rule,
};

pub(super) fn normalize_config(config: &mut Config) {
    config.schema_version = 9;

    if config
        .recorder
        .backends
        .gsr
        .capture_source
        .trim()
        .is_empty()
        || config.recorder.backends.gsr.capture_source == "screen"
    {
        config.recorder.backends.gsr.capture_source = "planetside2".into();
    }
    config.recorder.audio_sources =
        normalize_audio_sources(std::mem::take(&mut config.recorder.audio_sources));

    if config.clip_naming_template.trim().is_empty() {
        config.clip_naming_template = default_clip_naming_template();
    }
    config.manual_clip.normalize();
    config.storage_tiering.normalize();
    config.uploads.normalize();
    config.discord_webhook.normalize();
    config.launch_at_login.normalize();
    config.capture.normalize();
    config.updates.normalize();
    config.recorder.backends.normalize();
    config.recorder.post_processing.normalize();

    if config.rule_definitions.is_empty() {
        config.rule_definitions = default_rule_definitions();
    }
    if config.rule_profiles.is_empty() {
        config.rule_profiles = default_rule_profiles();
    }
    if config.auto_switch_rules.is_empty() {
        config.auto_switch_rules = default_auto_switch_rules();
    }

    let first_resolved_character_id = config
        .characters
        .iter()
        .find_map(|character| character.character_id);
    let mut auto_switch_migration_notes = Vec::new();
    for rule in &mut config.auto_switch_rules {
        rule.condition = match &rule.condition {
            AutoSwitchCondition::LocalSchedule {
                weekdays,
                start_hour,
                start_minute,
                end_hour,
                end_minute,
            } => {
                let mut weekdays = weekdays.clone();
                crate::rules::schedule::normalize_schedule_weekdays(&mut weekdays);
                AutoSwitchCondition::LocalSchedule {
                    weekdays,
                    start_hour: *start_hour,
                    start_minute: *start_minute,
                    end_hour: *end_hour,
                    end_minute: *end_minute,
                }
            }
            AutoSwitchCondition::ActiveCharacter {
                character_ids,
                character_id,
            } => AutoSwitchCondition::ActiveCharacter {
                character_ids: normalized_active_character_ids(character_ids, *character_id),
                character_id: None,
            },
            AutoSwitchCondition::LocalTimeRange {
                start_hour,
                end_hour,
            } => {
                auto_switch_migration_notes.push(format!(
                    "Converted legacy local time rule `{}` into a weekday schedule.",
                    rule.name
                ));
                AutoSwitchCondition::LocalSchedule {
                    weekdays: default_schedule_weekdays(),
                    start_hour: *start_hour,
                    start_minute: 0,
                    end_hour: *end_hour,
                    end_minute: 0,
                }
            }
            AutoSwitchCondition::LocalCron { expression } => {
                match crate::rules::schedule::legacy_cron_to_local_schedule(expression) {
                    Ok(schedule) => {
                        auto_switch_migration_notes.push(format!(
                            "Converted legacy cron rule `{}` into the new weekday/time schedule editor.",
                            rule.name
                        ));
                        AutoSwitchCondition::LocalSchedule {
                            weekdays: schedule.weekdays,
                            start_hour: schedule.start_hour,
                            start_minute: schedule.start_minute,
                            end_hour: schedule.end_hour,
                            end_minute: schedule.end_minute,
                        }
                    }
                    Err(_) => {
                        auto_switch_migration_notes.push(format!(
                            "Legacy cron rule `{}` could not be mapped exactly, so it was reset to an every-day 18:00-23:00 schedule.",
                            rule.name
                        ));
                        AutoSwitchCondition::LocalSchedule {
                            weekdays: default_schedule_weekdays(),
                            start_hour: 18,
                            start_minute: 0,
                            end_hour: 23,
                            end_minute: 0,
                        }
                    }
                }
            }
            AutoSwitchCondition::OnEvent { .. } => {
                auto_switch_migration_notes.push(format!(
                    "Converted legacy event rule `{}` into an active-character rule that needs a character selection.",
                    rule.name
                ));
                AutoSwitchCondition::ActiveCharacter {
                    character_ids: first_resolved_character_id.into_iter().collect(),
                    character_id: None,
                }
            }
        };
    }
    if !auto_switch_migration_notes.is_empty() && config.migration_notice.is_none() {
        config.migration_notice = Some(auto_switch_migration_notes.join(" "));
    }

    let profile_exists = config
        .rule_profiles
        .iter()
        .any(|profile| profile.id == config.active_profile_id);
    if !profile_exists {
        config.active_profile_id = config
            .rule_profiles
            .first()
            .map(|profile| profile.id.clone())
            .unwrap_or_default();
    }

    let rule_ids: std::collections::HashSet<_> = config
        .rule_definitions
        .iter()
        .map(|rule| rule.id.clone())
        .collect();
    for rule in &mut config.rule_definitions {
        if rule.extension.window_secs == 0 {
            rule.extension.window_secs = 1;
        }
        for scored_event in &mut rule.scored_events {
            scored_event.filters.normalize();
        }
    }
    config.auto_switch_rules.retain(|rule| {
        validate_auto_switch_rule(rule).is_ok()
            && config
                .rule_profiles
                .iter()
                .any(|profile| profile.id == rule.target_profile_id)
    });
    for profile in &mut config.rule_profiles {
        profile
            .enabled_rule_ids
            .retain(|rule_id| rule_ids.contains(rule_id));
    }
}
