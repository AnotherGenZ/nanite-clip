use chrono::{DateTime, Local, Timelike, Utc};

use super::schedule::{CronSchedule, local_schedule_matches};
use super::{AutoSwitchCondition, AutoSwitchRule, EventKind, normalized_active_character_ids};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoSwitchSource {
    Schedule,
    ActiveCharacter(u64),
    #[allow(dead_code)]
    Event(EventKind),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoSwitchDecision {
    pub rule_id: String,
    pub target_profile_id: String,
    pub source: AutoSwitchSource,
}

pub fn choose_runtime_rule(
    rules: &[AutoSwitchRule],
    now: DateTime<Utc>,
    active_character_id: Option<u64>,
) -> Option<AutoSwitchDecision> {
    let local = now.with_timezone(&Local);
    rules.iter().find_map(|rule| {
        if !rule.enabled {
            return None;
        }
        match &rule.condition {
            AutoSwitchCondition::LocalSchedule {
                weekdays,
                start_hour,
                start_minute,
                end_hour,
                end_minute,
            } => local_schedule_matches(
                local,
                weekdays,
                *start_hour,
                *start_minute,
                *end_hour,
                *end_minute,
            )
            .then(|| AutoSwitchDecision {
                rule_id: rule.id.clone(),
                target_profile_id: rule.target_profile_id.clone(),
                source: AutoSwitchSource::Schedule,
            }),
            AutoSwitchCondition::ActiveCharacter {
                character_ids,
                character_id,
            } if active_character_id.is_some_and(|active| {
                normalized_active_character_ids(character_ids, *character_id).contains(&active)
            }) =>
            {
                Some(AutoSwitchDecision {
                    rule_id: rule.id.clone(),
                    target_profile_id: rule.target_profile_id.clone(),
                    source: AutoSwitchSource::ActiveCharacter(active_character_id.unwrap()),
                })
            }
            AutoSwitchCondition::ActiveCharacter { .. } => None,
            AutoSwitchCondition::LocalTimeRange {
                start_hour,
                end_hour,
            } => local_time_matches(local, *start_hour, *end_hour).then(|| AutoSwitchDecision {
                rule_id: rule.id.clone(),
                target_profile_id: rule.target_profile_id.clone(),
                source: AutoSwitchSource::Schedule,
            }),
            AutoSwitchCondition::LocalCron { expression } => {
                let schedule = match CronSchedule::parse(expression) {
                    Ok(schedule) => schedule,
                    Err(error) => {
                        tracing::warn!(
                            "Skipping invalid auto-switch local cron rule `{}`: {error}",
                            rule.id
                        );
                        return None;
                    }
                };
                schedule.matches(local).then(|| AutoSwitchDecision {
                    rule_id: rule.id.clone(),
                    target_profile_id: rule.target_profile_id.clone(),
                    source: AutoSwitchSource::Schedule,
                })
            }
            AutoSwitchCondition::OnEvent { .. } => None,
        }
    })
}

#[allow(dead_code)]
pub fn choose_event_based_rule(
    rules: &[AutoSwitchRule],
    event_kind: EventKind,
) -> Option<AutoSwitchDecision> {
    rules.iter().find_map(|rule| {
        if !rule.enabled {
            return None;
        }
        match rule.condition {
            AutoSwitchCondition::OnEvent { event } if event == event_kind => {
                Some(AutoSwitchDecision {
                    rule_id: rule.id.clone(),
                    target_profile_id: rule.target_profile_id.clone(),
                    source: AutoSwitchSource::Event(event_kind),
                })
            }
            _ => None,
        }
    })
}

fn local_time_matches(now: DateTime<Local>, start_hour: u8, end_hour: u8) -> bool {
    let hour = now.hour() as u8;
    if start_hour == 0 && end_hour == 24 {
        return true;
    }
    if start_hour == end_hour {
        return hour == start_hour;
    }
    if start_hour < end_hour {
        hour >= start_hour && hour < end_hour
    } else {
        hour >= start_hour || hour < end_hour
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{AutoSwitchCondition, AutoSwitchRule, EventKind};
    use chrono::{Datelike, TimeZone};

    fn rule(id: &str, target_profile_id: &str, condition: AutoSwitchCondition) -> AutoSwitchRule {
        AutoSwitchRule {
            id: id.into(),
            name: id.into(),
            enabled: true,
            target_profile_id: target_profile_id.into(),
            condition,
        }
    }

    #[test]
    fn chooses_first_matching_runtime_schedule_rule() {
        let now = Utc.timestamp_opt(1_710_000_000, 0).unwrap();
        let hour = now.with_timezone(&Local).hour() as u8;
        let decision = choose_runtime_rule(
            &[
                rule(
                    "time_primary",
                    "profile_a",
                    AutoSwitchCondition::LocalSchedule {
                        weekdays: vec![super::super::schedule::ScheduleWeekday::from_chrono(
                            now.with_timezone(&Local).weekday(),
                        )],
                        start_hour: hour,
                        start_minute: 0,
                        end_hour: (hour + 1) % 24,
                        end_minute: 0,
                    },
                ),
                rule(
                    "time_secondary",
                    "profile_b",
                    AutoSwitchCondition::LocalSchedule {
                        weekdays: vec![super::super::schedule::ScheduleWeekday::from_chrono(
                            now.with_timezone(&Local).weekday(),
                        )],
                        start_hour: hour,
                        start_minute: 0,
                        end_hour: (hour + 1) % 24,
                        end_minute: 0,
                    },
                ),
            ],
            now,
            None,
        )
        .unwrap();

        assert_eq!(decision.rule_id, "time_primary");
        assert_eq!(decision.target_profile_id, "profile_a");
        assert_eq!(decision.source, AutoSwitchSource::Schedule);
    }

    #[test]
    fn overnight_time_range_matches_after_midnight() {
        let now = Local::now();
        let current_hour = now.hour() as u8;
        let start_hour = (current_hour + 23) % 24;
        let end_hour = (current_hour + 1) % 24;

        assert!(local_time_matches(now, start_hour, end_hour));
    }

    #[test]
    fn chooses_first_matching_event_rule() {
        let decision = choose_event_based_rule(
            &[
                rule(
                    "event_primary",
                    "profile_a",
                    AutoSwitchCondition::OnEvent {
                        event: EventKind::FacilityCapture,
                    },
                ),
                rule(
                    "event_secondary",
                    "profile_b",
                    AutoSwitchCondition::OnEvent {
                        event: EventKind::FacilityCapture,
                    },
                ),
            ],
            EventKind::FacilityCapture,
        )
        .unwrap();

        assert_eq!(decision.rule_id, "event_primary");
        assert_eq!(decision.target_profile_id, "profile_a");
        assert_eq!(
            decision.source,
            AutoSwitchSource::Event(EventKind::FacilityCapture)
        );
    }

    #[test]
    fn chooses_matching_active_character_rule() {
        let decision = choose_runtime_rule(
            &[rule(
                "character_primary",
                "profile_a",
                AutoSwitchCondition::ActiveCharacter {
                    character_ids: vec![42, 99],
                    character_id: None,
                },
            )],
            Utc::now(),
            Some(42),
        )
        .unwrap();

        assert_eq!(decision.rule_id, "character_primary");
        assert_eq!(decision.target_profile_id, "profile_a");
        assert_eq!(decision.source, AutoSwitchSource::ActiveCharacter(42));
    }

    #[test]
    fn invalid_cron_rule_is_ignored() {
        let decision = choose_runtime_rule(
            &[rule(
                "broken_cron",
                "profile_a",
                AutoSwitchCondition::LocalCron {
                    expression: "not a cron".into(),
                },
            )],
            Utc::now(),
            None,
        );

        assert!(decision.is_none());
    }
}
