use std::collections::{HashMap, HashSet, VecDeque};

use chrono::{DateTime, Duration, Utc};

use super::{
    CharacterClass, ClassifiedEvent, ClipAction, ClipActionLifecycle, RuleDefinition, RuleProfile,
    ScoreBreakdown, ScoredEvent, ScoredEventFilterClause, validate_rule,
};

pub struct RuleEngine {
    rules: Vec<CompiledRule>,
    last_observed_class: Option<CharacterClass>,
}

#[derive(Debug, Clone)]
pub struct RuleRuntimeStatus {
    pub current_score: u32,
    pub armed: bool,
    pub extending_until: Option<DateTime<Utc>>,
    pub cooldown_remaining_secs: Option<u32>,
    pub contributions: Vec<ScoreBreakdown>,
}

impl RuleEngine {
    pub fn new(
        definitions: Vec<RuleDefinition>,
        profiles: Vec<RuleProfile>,
        active_profile_id: String,
    ) -> Self {
        let enabled_rule_ids = profiles
            .iter()
            .find(|profile| profile.id == active_profile_id)
            .map(|profile| profile.enabled_rule_ids.clone())
            .unwrap_or_default();

        let enabled_rule_ids: HashSet<_> = enabled_rule_ids.into_iter().collect();
        let rules = definitions
            .into_iter()
            .filter(|rule| enabled_rule_ids.contains(&rule.id))
            .filter_map(CompiledRule::compile)
            .collect();

        Self {
            rules,
            last_observed_class: None,
        }
    }

    pub fn update_rules(
        &mut self,
        definitions: Vec<RuleDefinition>,
        profiles: Vec<RuleProfile>,
        active_profile_id: String,
    ) {
        *self = Self::new(definitions, profiles, active_profile_id);
    }

    pub fn ingest(&mut self, event: &ClassifiedEvent) -> Vec<ClipAction> {
        let effective_class = event.actor_class.or(self.last_observed_class);
        if event.actor_class.is_some() {
            self.last_observed_class = event.actor_class;
        }

        let mut actions = Vec::new();
        for rule in &mut self.rules {
            actions.extend(rule.ingest(event, effective_class));
        }
        actions
    }

    pub fn poll_due(&mut self, now: DateTime<Utc>) -> Vec<ClipAction> {
        let mut actions = Vec::new();
        for rule in &mut self.rules {
            actions.extend(rule.expire_capture_if_due(now));
            rule.prune(now);
        }
        actions
    }

    pub fn reset(&mut self) {
        self.last_observed_class = None;
        for rule in &mut self.rules {
            rule.reset();
        }
    }

    pub fn runtime_status(&self, rule_id: &str, now: DateTime<Utc>) -> Option<RuleRuntimeStatus> {
        self.rules
            .iter()
            .find(|rule| rule.definition.id == rule_id)
            .map(|rule| rule.runtime_status(now))
    }
}

struct CompiledRule {
    definition: RuleDefinition,
    window: VecDeque<WindowContribution>,
    current_score: u32,
    armed: bool,
    last_emitted_at: Option<DateTime<Utc>>,
    active_capture: Option<ActiveCaptureState>,
}

#[derive(Clone)]
struct WindowContribution {
    event_at: DateTime<Utc>,
    event_kind: super::EventKind,
    points: u32,
}

#[derive(Debug, Clone, Copy)]
struct ActiveCaptureState {
    expires_at: DateTime<Utc>,
}

impl CompiledRule {
    fn compile(definition: RuleDefinition) -> Option<Self> {
        if let Err(error) = validate_rule(&definition) {
            tracing::warn!("Skipping invalid rule `{}`: {error}", definition.name);
            return None;
        }

        Some(Self {
            definition,
            window: VecDeque::new(),
            current_score: 0,
            armed: true,
            last_emitted_at: None,
            active_capture: None,
        })
    }

    fn ingest(
        &mut self,
        event: &ClassifiedEvent,
        effective_class: Option<CharacterClass>,
    ) -> Vec<ClipAction> {
        let mut actions = self.expire_capture_if_due(event.timestamp);
        self.prune(event.timestamp);

        if !self.activation_class_matches(effective_class) {
            return actions;
        }

        let mut matched = false;
        for scored_event in &self.definition.scored_events {
            if !event_matches(scored_event, event) {
                continue;
            }
            matched = true;
            self.window.push_back(WindowContribution {
                event_at: event.timestamp,
                event_kind: scored_event.event,
                points: scored_event.points,
            });
            self.current_score = self.current_score.saturating_add(scored_event.points);
        }

        if matched {
            if self.active_capture.is_some() {
                let expires_at = self.extension_deadline_for(event.timestamp);
                if let Some(active_capture) = self.active_capture.as_mut() {
                    active_capture.expires_at = expires_at;
                }
                actions.push(
                    self.build_clip_action(
                        event.clone(),
                        ClipActionLifecycle::Extend { expires_at },
                    ),
                );
                return actions;
            }

            if self.armed
                && self.current_score >= self.definition.trigger_threshold
                && self.cooldown_allows(event.timestamp)
            {
                self.armed = false;
                self.last_emitted_at = Some(event.timestamp);

                let lifecycle = if self.definition.extension.is_enabled() {
                    let expires_at = self.extension_deadline_for(event.timestamp);
                    self.active_capture = Some(ActiveCaptureState { expires_at });
                    ClipActionLifecycle::StartExtending { expires_at }
                } else {
                    ClipActionLifecycle::Trigger
                };

                actions.push(self.build_clip_action(event.clone(), lifecycle));
            }
        }

        actions
    }

    fn prune(&mut self, now: DateTime<Utc>) {
        let cutoff = now - Duration::seconds(i64::from(self.definition.lookback_secs));
        while self
            .window
            .front()
            .is_some_and(|entry| entry.event_at < cutoff)
        {
            if let Some(expired) = self.window.pop_front() {
                self.current_score = self.current_score.saturating_sub(expired.points);
            }
        }

        if self.active_capture.is_none()
            && !self.armed
            && score_is_below_reset_threshold(self.current_score, self.definition.reset_threshold)
        {
            self.armed = true;
        }
    }

    fn reset(&mut self) {
        self.window.clear();
        self.current_score = 0;
        self.armed = true;
        self.last_emitted_at = None;
        self.active_capture = None;
    }

    fn cooldown_allows(&self, now: DateTime<Utc>) -> bool {
        let Some(cooldown_secs) = self.definition.cooldown_secs else {
            return true;
        };
        let Some(last_emitted_at) = self.last_emitted_at else {
            return true;
        };
        now.signed_duration_since(last_emitted_at).num_seconds() >= i64::from(cooldown_secs)
    }

    fn activation_class_matches(&self, effective_class: Option<CharacterClass>) -> bool {
        match self.definition.activation_class {
            Some(required) => effective_class == Some(required),
            None => true,
        }
    }

    fn runtime_status(&self, now: DateTime<Utc>) -> RuleRuntimeStatus {
        let cutoff = now - Duration::seconds(i64::from(self.definition.lookback_secs));
        let active_entries: Vec<_> = self
            .window
            .iter()
            .filter(|entry| entry.event_at >= cutoff)
            .cloned()
            .collect();
        let current_score = active_entries.iter().map(|entry| entry.points).sum();
        let armed = if self.active_capture.is_some() {
            false
        } else if self.armed {
            true
        } else {
            score_is_below_reset_threshold(current_score, self.definition.reset_threshold)
        };
        let cooldown_remaining_secs = self.last_emitted_at.and_then(|last_emitted_at| {
            let cooldown = self.definition.cooldown_secs?;
            let elapsed = now
                .signed_duration_since(last_emitted_at)
                .num_seconds()
                .max(0) as u32;
            if elapsed < cooldown {
                Some(cooldown - elapsed)
            } else {
                None
            }
        });

        RuleRuntimeStatus {
            current_score,
            armed,
            extending_until: self.active_capture.map(|capture| capture.expires_at),
            cooldown_remaining_secs,
            contributions: aggregate_breakdown(active_entries.iter()),
        }
    }

    fn expire_capture_if_due(&mut self, now: DateTime<Utc>) -> Vec<ClipAction> {
        let Some(active_capture) = self.active_capture else {
            return Vec::new();
        };
        if now < active_capture.expires_at {
            return Vec::new();
        }

        self.active_capture = None;
        self.prune(active_capture.expires_at);
        if score_is_below_reset_threshold(self.current_score, self.definition.reset_threshold) {
            self.armed = true;
        }

        vec![ClipAction {
            rule_id: self.definition.id.clone(),
            rule_name: self.definition.name.clone(),
            lifecycle: ClipActionLifecycle::Finalize {
                finalized_at: active_capture.expires_at,
            },
            clip_length: self
                .definition
                .clip_length_for_trigger(0, super::EventKind::Kill),
            trigger_score: self.current_score,
            score_breakdown: aggregate_breakdown(self.window.iter()),
            event: ClassifiedEvent {
                kind: super::EventKind::Saved,
                timestamp: active_capture.expires_at,
                world_id: 0,
                zone_id: None,
                facility_id: None,
                actor_character_id: None,
                other_character_id: None,
                other_character_outfit_id: None,
                characters_killed: 0,
                attacker_weapon_id: None,
                attacker_vehicle_id: None,
                vehicle_killed_id: None,
                is_headshot: false,
                actor_class: None,
                experience_id: None,
            },
        }]
    }

    fn extension_deadline_for(&self, event_at: DateTime<Utc>) -> DateTime<Utc> {
        event_at + Duration::seconds(i64::from(self.definition.extension.window_secs))
    }

    fn build_clip_action(
        &self,
        event: ClassifiedEvent,
        lifecycle: ClipActionLifecycle,
    ) -> ClipAction {
        let trigger_score = self.current_score;
        ClipAction {
            rule_id: self.definition.id.clone(),
            rule_name: self.definition.name.clone(),
            lifecycle,
            clip_length: self
                .definition
                .clip_length_for_trigger(trigger_score, event.kind),
            trigger_score,
            score_breakdown: aggregate_breakdown(self.window.iter()),
            event,
        }
    }
}

fn event_matches(scored_event: &ScoredEvent, event: &ClassifiedEvent) -> bool {
    let kind_matches = scored_event.event == event.kind
        || (scored_event.event == super::EventKind::Kill
            && event.kind == super::EventKind::Headshot);
    kind_matches && filters_match(scored_event, event)
}

fn filters_match(scored_event: &ScoredEvent, event: &ClassifiedEvent) -> bool {
    let filters = &scored_event.filters;
    if !filters.is_enabled() {
        return true;
    }

    let groups = filters.groups();
    groups.is_empty()
        || groups.iter().any(|group| {
            group
                .clauses
                .iter()
                .all(|clause| filter_clause_matches(clause, event))
        })
}

fn filter_clause_matches(clause: &ScoredEventFilterClause, event: &ClassifiedEvent) -> bool {
    match clause {
        ScoredEventFilterClause::TargetCharacter { target } => target
            .character_id
            .is_none_or(|target_id| event.other_character_id == Some(target_id)),
        ScoredEventFilterClause::TargetOutfit { outfit } => outfit
            .outfit_id
            .is_none_or(|outfit_id| event.other_character_outfit_id == Some(outfit_id)),
        ScoredEventFilterClause::AttackerWeapon { weapon } => {
            weapon_filter_matches(weapon, event.attacker_weapon_id)
        }
        ScoredEventFilterClause::AttackerVehicle { vehicle }
        | ScoredEventFilterClause::DestroyedVehicle { vehicle } => vehicle_filter_matches(
            vehicle,
            match clause {
                ScoredEventFilterClause::AttackerVehicle { .. } => event.attacker_vehicle_id,
                ScoredEventFilterClause::DestroyedVehicle { .. } => event.vehicle_killed_id,
                _ => unreachable!(),
            },
        ),
        ScoredEventFilterClause::Any { clauses } => clauses
            .iter()
            .any(|clause| filter_clause_matches(clause, event)),
    }
}

fn vehicle_filter_matches(filter: &super::VehicleMatchFilter, event_id: Option<u16>) -> bool {
    if !filter.vehicle.ids.is_empty() {
        event_id.is_some_and(|id| filter.vehicle.ids.contains(&id))
    } else {
        filter
            .legacy_vehicle_id
            .is_none_or(|vehicle_id| event_id == Some(vehicle_id))
    }
}

fn weapon_filter_matches(filter: &super::WeaponMatchFilter, event_id: Option<u32>) -> bool {
    if !filter.weapon.ids.is_empty() {
        event_id.is_some_and(|id| filter.weapon.ids.contains(&id))
    } else {
        filter
            .legacy_weapon_id
            .is_none_or(|weapon_id| event_id == Some(weapon_id))
    }
}

fn score_is_below_reset_threshold(current_score: u32, reset_threshold: u32) -> bool {
    if reset_threshold == 0 {
        current_score == 0
    } else {
        current_score < reset_threshold
    }
}

fn aggregate_breakdown<'a, I>(entries: I) -> Vec<ScoreBreakdown>
where
    I: Iterator<Item = &'a WindowContribution>,
{
    let mut totals = HashMap::new();
    for entry in entries {
        let total = totals.entry(entry.event_kind).or_insert((0_u32, 0_u32));
        total.0 = total.0.saturating_add(1);
        total.1 = total.1.saturating_add(entry.points);
    }

    let mut breakdown: Vec<_> = totals
        .into_iter()
        .map(|(event, (occurrences, points))| ScoreBreakdown {
            event,
            occurrences,
            points,
        })
        .collect();
    breakdown.sort_by_key(|item| item.event.to_string());
    breakdown
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{
        CharacterReferenceFilter, ClipExtensionMode, ClipExtensionPolicy, ClipLength, EventKind,
        OutfitReferenceFilter, RuleDefinition, RuleProfile, ScoredEvent, ScoredEventFilterClause,
        ScoredEventFilterGroup, ScoredEventFilters, VehicleMatchFilter, VehicleVariantFilter,
        WeaponMatchFilter, WeaponVariantFilter,
    };
    use chrono::TimeZone;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_700_000_000 + secs, 0).unwrap()
    }

    fn event(kind: EventKind, secs: i64, actor_class: Option<CharacterClass>) -> ClassifiedEvent {
        ClassifiedEvent {
            kind,
            timestamp: ts(secs),
            world_id: 17,
            zone_id: Some(2),
            facility_id: None,
            actor_character_id: Some(10),
            other_character_id: Some(20),
            other_character_outfit_id: None,
            characters_killed: 1,
            attacker_weapon_id: Some(80),
            attacker_vehicle_id: Some(4),
            vehicle_killed_id: None,
            is_headshot: matches!(kind, EventKind::Headshot),
            actor_class,
            experience_id: None,
        }
    }

    fn profile(rule_id: &str) -> RuleProfile {
        RuleProfile {
            id: "default".into(),
            name: "Default".into(),
            enabled_rule_ids: vec![rule_id.into()],
        }
    }

    fn sample_rule() -> RuleDefinition {
        RuleDefinition {
            id: "infantry".into(),
            name: "Infantry".into(),
            activation_class: None,
            lookback_secs: 10,
            trigger_threshold: 6,
            reset_threshold: 2,
            cooldown_secs: Some(10),
            use_full_buffer: false,
            capture_entire_base_cap: false,
            base_duration_secs: 20,
            secs_per_point: 2,
            max_duration_secs: 60,
            extension: ClipExtensionPolicy::default(),
            scored_events: vec![
                ScoredEvent {
                    event: EventKind::Kill,
                    points: 2,
                    filters: ScoredEventFilters::default(),
                },
                ScoredEvent {
                    event: EventKind::Headshot,
                    points: 3,
                    filters: ScoredEventFilters::default(),
                },
            ],
        }
    }

    #[test]
    fn threshold_crossing_fires_once() {
        let mut engine = RuleEngine::new(
            vec![sample_rule()],
            vec![profile("infantry")],
            "default".into(),
        );

        assert!(engine.ingest(&event(EventKind::Kill, 0, None)).is_empty());
        let actions = engine.ingest(&event(EventKind::Headshot, 1, None));

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].lifecycle, ClipActionLifecycle::Trigger);
        assert_eq!(actions[0].trigger_score, 7);
        assert_eq!(actions[0].clip_length, ClipLength::Seconds(34));

        assert!(engine.ingest(&event(EventKind::Kill, 2, None)).is_empty());
    }

    #[test]
    fn reset_threshold_rearms_after_score_drops() {
        let mut engine = RuleEngine::new(
            vec![sample_rule()],
            vec![profile("infantry")],
            "default".into(),
        );

        let _ = engine.ingest(&event(EventKind::Kill, 0, None));
        let actions = engine.ingest(&event(EventKind::Headshot, 1, None));
        assert_eq!(actions.len(), 1);

        let _ = engine.poll_due(ts(20));
        let actions = engine.ingest(&event(EventKind::Kill, 21, None));
        assert!(actions.is_empty());
        let actions = engine.ingest(&event(EventKind::Headshot, 22, None));
        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn zero_reset_threshold_rearms_when_window_clears() {
        let mut rule = sample_rule();
        rule.reset_threshold = 0;

        let mut engine = RuleEngine::new(vec![rule], vec![profile("infantry")], "default".into());

        let _ = engine.ingest(&event(EventKind::Kill, 0, None));
        let actions = engine.ingest(&event(EventKind::Headshot, 1, None));
        assert_eq!(actions.len(), 1);

        let _ = engine.poll_due(ts(20));
        let status = engine.runtime_status("infantry", ts(20)).unwrap();
        assert!(status.armed);
        assert_eq!(status.current_score, 0);
    }

    #[test]
    fn cooldown_suppresses_retrigger() {
        let mut engine = RuleEngine::new(
            vec![sample_rule()],
            vec![profile("infantry")],
            "default".into(),
        );

        let _ = engine.ingest(&event(EventKind::Kill, 0, None));
        let actions = engine.ingest(&event(EventKind::Headshot, 1, None));
        assert_eq!(actions.len(), 1);

        let _ = engine.poll_due(ts(5));
        let _ = engine.ingest(&event(EventKind::Kill, 5, None));
        let actions = engine.ingest(&event(EventKind::Headshot, 6, None));
        assert!(actions.is_empty());

        let _ = engine.poll_due(ts(20));
        let _ = engine.ingest(&event(EventKind::Kill, 20, None));
        let actions = engine.ingest(&event(EventKind::Headshot, 21, None));
        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn runtime_status_does_not_underflow_after_cooldown_expires() {
        let mut engine = RuleEngine::new(
            vec![sample_rule()],
            vec![profile("infantry")],
            "default".into(),
        );

        let _ = engine.ingest(&event(EventKind::Kill, 0, None));
        let actions = engine.ingest(&event(EventKind::Headshot, 1, None));
        assert_eq!(actions.len(), 1);

        let status = engine.runtime_status("infantry", ts(20)).unwrap();
        assert_eq!(status.cooldown_remaining_secs, None);
    }

    #[test]
    fn class_gating_uses_last_observed_class() {
        let mut rule = sample_rule();
        rule.id = "medic".into();
        rule.activation_class = Some(CharacterClass::Medic);
        let mut engine = RuleEngine::new(vec![rule], vec![profile("medic")], "default".into());

        assert!(
            engine
                .ingest(&event(EventKind::Kill, 0, Some(CharacterClass::Engineer)))
                .is_empty()
        );
        assert!(engine.ingest(&event(EventKind::Kill, 1, None)).is_empty());
        assert!(
            engine
                .ingest(&event(EventKind::Kill, 2, Some(CharacterClass::Medic)))
                .is_empty()
        );
        let actions = engine.ingest(&event(EventKind::Headshot, 3, None));
        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn headshots_can_stack_kill_and_headshot_points() {
        let mut engine = RuleEngine::new(
            vec![sample_rule()],
            vec![profile("infantry")],
            "default".into(),
        );

        let actions = engine.ingest(&event(EventKind::Headshot, 0, None));
        assert!(actions.is_empty());

        let status = engine.runtime_status("infantry", ts(0)).unwrap();
        assert_eq!(status.current_score, 5);
        assert_eq!(status.contributions.len(), 2);
    }

    #[test]
    fn filtered_events_require_all_configured_fields_to_match() {
        let mut rule = sample_rule();
        rule.scored_events = vec![ScoredEvent {
            event: EventKind::Kill,
            points: 6,
            filters: ScoredEventFilters {
                enabled: Some(true),
                groups: vec![ScoredEventFilterGroup {
                    clauses: vec![
                        ScoredEventFilterClause::TargetCharacter {
                            target: CharacterReferenceFilter {
                                name: None,
                                character_id: Some(99),
                            },
                        },
                        ScoredEventFilterClause::AttackerVehicle {
                            vehicle: VehicleMatchFilter {
                                vehicle: VehicleVariantFilter::default(),
                                legacy_vehicle_id: Some(4),
                            },
                        },
                        ScoredEventFilterClause::DestroyedVehicle {
                            vehicle: VehicleMatchFilter {
                                vehicle: VehicleVariantFilter::default(),
                                legacy_vehicle_id: Some(7),
                            },
                        },
                    ],
                }],
                ..ScoredEventFilters::default()
            },
        }];
        rule.trigger_threshold = 6;
        rule.reset_threshold = 0;

        let mut engine = RuleEngine::new(vec![rule], vec![profile("infantry")], "default".into());

        let mut non_matching = event(EventKind::Kill, 0, None);
        non_matching.other_character_id = Some(98);
        non_matching.vehicle_killed_id = Some(7);
        assert!(engine.ingest(&non_matching).is_empty());
        assert_eq!(
            engine
                .runtime_status("infantry", ts(0))
                .unwrap()
                .current_score,
            0
        );

        let mut matching = event(EventKind::Kill, 1, None);
        matching.other_character_id = Some(99);
        matching.vehicle_killed_id = Some(7);
        let actions = engine.ingest(&matching);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].trigger_score, 6);
    }

    #[test]
    fn duplicate_event_kinds_with_different_filters_score_independently() {
        let mut rule = sample_rule();
        rule.scored_events = vec![
            ScoredEvent {
                event: EventKind::Kill,
                points: 2,
                filters: ScoredEventFilters {
                    enabled: Some(true),
                    groups: vec![ScoredEventFilterGroup {
                        clauses: vec![ScoredEventFilterClause::TargetCharacter {
                            target: CharacterReferenceFilter {
                                name: None,
                                character_id: Some(99),
                            },
                        }],
                    }],
                    ..ScoredEventFilters::default()
                },
            },
            ScoredEvent {
                event: EventKind::Kill,
                points: 6,
                filters: ScoredEventFilters {
                    enabled: Some(true),
                    groups: vec![ScoredEventFilterGroup {
                        clauses: vec![ScoredEventFilterClause::TargetCharacter {
                            target: CharacterReferenceFilter {
                                name: None,
                                character_id: Some(42),
                            },
                        }],
                    }],
                    ..ScoredEventFilters::default()
                },
            },
        ];
        rule.trigger_threshold = 6;
        rule.reset_threshold = 0;

        let mut engine = RuleEngine::new(vec![rule], vec![profile("infantry")], "default".into());

        let mut matching = event(EventKind::Kill, 0, None);
        matching.other_character_id = Some(42);
        let actions = engine.ingest(&matching);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].trigger_score, 6);

        let status = engine.runtime_status("infantry", ts(0)).unwrap();
        assert_eq!(status.current_score, 6);
        assert_eq!(status.contributions.len(), 1);
    }

    #[test]
    fn unfiltered_events_keep_matching_by_kind_only() {
        let mut rule = sample_rule();
        rule.scored_events = vec![ScoredEvent {
            event: EventKind::Kill,
            points: 6,
            filters: ScoredEventFilters::default(),
        }];
        rule.trigger_threshold = 6;
        rule.reset_threshold = 0;

        let mut engine = RuleEngine::new(vec![rule], vec![profile("infantry")], "default".into());
        let mut event = event(EventKind::Kill, 0, None);
        event.other_character_id = None;
        event.attacker_vehicle_id = None;
        event.vehicle_killed_id = None;

        let actions = engine.ingest(&event);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].trigger_score, 6);
    }

    #[test]
    fn explicitly_disabled_filters_do_not_gate_matching() {
        let mut rule = sample_rule();
        rule.scored_events = vec![ScoredEvent {
            event: EventKind::Kill,
            points: 6,
            filters: ScoredEventFilters {
                enabled: Some(false),
                groups: vec![ScoredEventFilterGroup {
                    clauses: vec![
                        ScoredEventFilterClause::TargetCharacter {
                            target: CharacterReferenceFilter {
                                name: None,
                                character_id: Some(99),
                            },
                        },
                        ScoredEventFilterClause::AttackerVehicle {
                            vehicle: VehicleMatchFilter {
                                vehicle: VehicleVariantFilter::default(),
                                legacy_vehicle_id: Some(7),
                            },
                        },
                        ScoredEventFilterClause::DestroyedVehicle {
                            vehicle: VehicleMatchFilter {
                                vehicle: VehicleVariantFilter::default(),
                                legacy_vehicle_id: Some(8),
                            },
                        },
                    ],
                }],
                ..ScoredEventFilters::default()
            },
        }];
        rule.trigger_threshold = 6;
        rule.reset_threshold = 0;

        let mut engine = RuleEngine::new(vec![rule], vec![profile("infantry")], "default".into());
        let actions = engine.ingest(&event(EventKind::Kill, 0, None));

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].trigger_score, 6);
    }

    #[test]
    fn grouped_vehicle_filter_matches_any_variant_id() {
        let mut rule = sample_rule();
        rule.scored_events = vec![ScoredEvent {
            event: EventKind::Kill,
            points: 6,
            filters: ScoredEventFilters {
                enabled: Some(true),
                groups: vec![ScoredEventFilterGroup {
                    clauses: vec![ScoredEventFilterClause::AttackerVehicle {
                        vehicle: VehicleMatchFilter {
                            vehicle: VehicleVariantFilter {
                                label: Some("Sunderer".into()),
                                ids: vec![5, 105],
                            },
                            legacy_vehicle_id: None,
                        },
                    }],
                }],
                ..ScoredEventFilters::default()
            },
        }];
        rule.trigger_threshold = 6;
        rule.reset_threshold = 0;

        let mut engine = RuleEngine::new(vec![rule], vec![profile("infantry")], "default".into());
        let mut event = event(EventKind::Kill, 0, None);
        event.attacker_vehicle_id = Some(105);

        let actions = engine.ingest(&event);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].trigger_score, 6);
    }

    #[test]
    fn grouped_weapon_filter_matches_any_variant_id() {
        let mut rule = sample_rule();
        rule.scored_events = vec![ScoredEvent {
            event: EventKind::Kill,
            points: 6,
            filters: ScoredEventFilters {
                enabled: Some(true),
                groups: vec![ScoredEventFilterGroup {
                    clauses: vec![ScoredEventFilterClause::AttackerWeapon {
                        weapon: WeaponMatchFilter {
                            weapon: WeaponVariantFilter {
                                label: Some("Gauss Rifle".into()),
                                ids: vec![80, 1080],
                            },
                            legacy_weapon_id: None,
                        },
                    }],
                }],
                ..ScoredEventFilters::default()
            },
        }];
        rule.trigger_threshold = 6;
        rule.reset_threshold = 0;

        let mut engine = RuleEngine::new(vec![rule], vec![profile("infantry")], "default".into());
        let mut event = event(EventKind::Kill, 0, None);
        event.attacker_weapon_id = Some(1080);

        let actions = engine.ingest(&event);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].trigger_score, 6);
    }

    #[test]
    fn target_outfit_filter_matches_resolved_outfit_id() {
        let mut rule = sample_rule();
        rule.scored_events = vec![ScoredEvent {
            event: EventKind::Kill,
            points: 6,
            filters: ScoredEventFilters {
                enabled: Some(true),
                groups: vec![ScoredEventFilterGroup {
                    clauses: vec![ScoredEventFilterClause::TargetOutfit {
                        outfit: OutfitReferenceFilter {
                            tag: Some("TAG".into()),
                            outfit_id: Some(55),
                        },
                    }],
                }],
                ..ScoredEventFilters::default()
            },
        }];
        rule.trigger_threshold = 6;
        rule.reset_threshold = 0;

        let mut engine = RuleEngine::new(vec![rule], vec![profile("infantry")], "default".into());
        let mut event = event(EventKind::Kill, 0, None);
        event.other_character_outfit_id = Some(55);

        let actions = engine.ingest(&event);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].trigger_score, 6);
    }

    #[test]
    fn filter_groups_or_together() {
        let mut rule = sample_rule();
        rule.scored_events = vec![ScoredEvent {
            event: EventKind::Kill,
            points: 6,
            filters: ScoredEventFilters {
                enabled: Some(true),
                groups: vec![
                    ScoredEventFilterGroup {
                        clauses: vec![ScoredEventFilterClause::TargetCharacter {
                            target: CharacterReferenceFilter {
                                name: Some("Enemy".into()),
                                character_id: Some(99),
                            },
                        }],
                    },
                    ScoredEventFilterGroup {
                        clauses: vec![ScoredEventFilterClause::TargetOutfit {
                            outfit: OutfitReferenceFilter {
                                tag: Some("TAG".into()),
                                outfit_id: Some(55),
                            },
                        }],
                    },
                ],
                ..ScoredEventFilters::default()
            },
        }];
        rule.trigger_threshold = 6;
        rule.reset_threshold = 0;

        let mut engine = RuleEngine::new(vec![rule], vec![profile("infantry")], "default".into());
        let mut event = event(EventKind::Kill, 0, None);
        event.other_character_id = Some(10);
        event.other_character_outfit_id = Some(55);

        let actions = engine.ingest(&event);

        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn nested_any_clause_matches_inside_and_group() {
        let mut rule = sample_rule();
        rule.scored_events = vec![ScoredEvent {
            event: EventKind::Kill,
            points: 6,
            filters: ScoredEventFilters {
                enabled: Some(true),
                groups: vec![ScoredEventFilterGroup {
                    clauses: vec![
                        ScoredEventFilterClause::AttackerVehicle {
                            vehicle: VehicleMatchFilter {
                                vehicle: VehicleVariantFilter::default(),
                                legacy_vehicle_id: Some(4),
                            },
                        },
                        ScoredEventFilterClause::Any {
                            clauses: vec![
                                ScoredEventFilterClause::TargetCharacter {
                                    target: CharacterReferenceFilter {
                                        name: Some("Enemy".into()),
                                        character_id: Some(99),
                                    },
                                },
                                ScoredEventFilterClause::TargetOutfit {
                                    outfit: OutfitReferenceFilter {
                                        tag: Some("TAG".into()),
                                        outfit_id: Some(55),
                                    },
                                },
                            ],
                        },
                    ],
                }],
                ..ScoredEventFilters::default()
            },
        }];
        rule.trigger_threshold = 6;
        rule.reset_threshold = 0;

        let mut engine = RuleEngine::new(vec![rule], vec![profile("infantry")], "default".into());
        let mut event = event(EventKind::Kill, 0, None);
        event.attacker_vehicle_id = Some(4);
        event.other_character_id = Some(10);
        event.other_character_outfit_id = Some(55);

        let actions = engine.ingest(&event);

        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn facility_capture_can_force_full_buffer() {
        let mut rule = sample_rule();
        rule.id = "objective".into();
        rule.capture_entire_base_cap = true;
        rule.scored_events = vec![ScoredEvent {
            event: EventKind::FacilityCapture,
            points: 6,
            filters: ScoredEventFilters::default(),
        }];
        rule.trigger_threshold = 6;
        rule.reset_threshold = 2;

        let mut engine = RuleEngine::new(vec![rule], vec![profile("objective")], "default".into());
        let actions = engine.ingest(&event(EventKind::FacilityCapture, 0, None));

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].clip_length, ClipLength::FullBuffer);
    }

    #[test]
    fn extending_rule_emits_start_extend_and_finalize() {
        let mut rule = sample_rule();
        rule.extension = ClipExtensionPolicy {
            mode: ClipExtensionMode::HoldUntilQuiet,
            window_secs: 4,
        };

        let mut engine = RuleEngine::new(vec![rule], vec![profile("infantry")], "default".into());
        let _ = engine.ingest(&event(EventKind::Kill, 0, None));
        let actions = engine.ingest(&event(EventKind::Headshot, 1, None));

        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].lifecycle,
            ClipActionLifecycle::StartExtending { expires_at: ts(5) }
        );

        let status = engine.runtime_status("infantry", ts(2)).unwrap();
        assert_eq!(status.extending_until, Some(ts(5)));
        assert!(!status.armed);

        let actions = engine.poll_due(ts(5));
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].lifecycle,
            ClipActionLifecycle::Finalize {
                finalized_at: ts(5)
            }
        );
    }

    #[test]
    fn extending_rule_refreshes_deadline_when_action_continues() {
        let mut rule = sample_rule();
        rule.extension = ClipExtensionPolicy {
            mode: ClipExtensionMode::HoldUntilQuiet,
            window_secs: 4,
        };

        let mut engine = RuleEngine::new(vec![rule], vec![profile("infantry")], "default".into());
        let _ = engine.ingest(&event(EventKind::Kill, 0, None));
        let _ = engine.ingest(&event(EventKind::Headshot, 1, None));

        let actions = engine.ingest(&event(EventKind::Kill, 3, None));
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].lifecycle,
            ClipActionLifecycle::Extend { expires_at: ts(7) }
        );

        assert!(engine.poll_due(ts(6)).is_empty());
        let actions = engine.poll_due(ts(7));
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].lifecycle,
            ClipActionLifecycle::Finalize {
                finalized_at: ts(7)
            }
        );
    }

    #[test]
    fn extending_rule_expiry_before_new_event_finalizes_then_retriggers_after_reset() {
        let mut rule = sample_rule();
        rule.extension = ClipExtensionPolicy {
            mode: ClipExtensionMode::HoldUntilQuiet,
            window_secs: 3,
        };
        rule.cooldown_secs = Some(2);

        let mut engine = RuleEngine::new(vec![rule], vec![profile("infantry")], "default".into());
        let _ = engine.ingest(&event(EventKind::Kill, 0, None));
        let actions = engine.ingest(&event(EventKind::Headshot, 1, None));
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].lifecycle,
            ClipActionLifecycle::StartExtending { expires_at: ts(4) }
        );

        let actions = engine.ingest(&event(EventKind::Kill, 20, None));
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].lifecycle,
            ClipActionLifecycle::Finalize {
                finalized_at: ts(4)
            }
        );

        let actions = engine.ingest(&event(EventKind::Headshot, 21, None));
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].lifecycle,
            ClipActionLifecycle::StartExtending { expires_at: ts(24) }
        );
    }

    #[test]
    fn extending_rule_stays_disarmed_until_capture_expires() {
        let mut rule = sample_rule();
        rule.extension = ClipExtensionPolicy {
            mode: ClipExtensionMode::HoldUntilQuiet,
            window_secs: 5,
        };

        let mut engine = RuleEngine::new(vec![rule], vec![profile("infantry")], "default".into());
        let _ = engine.ingest(&event(EventKind::Kill, 0, None));
        let _ = engine.ingest(&event(EventKind::Headshot, 1, None));

        let status = engine.runtime_status("infantry", ts(4)).unwrap();
        assert_eq!(status.extending_until, Some(ts(6)));
        assert!(!status.armed);

        let _ = engine.poll_due(ts(20));
        let status = engine.runtime_status("infantry", ts(20)).unwrap();
        assert!(status.extending_until.is_none());
        assert!(status.armed);
    }
}
