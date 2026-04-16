pub mod engine;
pub mod schedule;
pub mod switching;

use std::borrow::Cow;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use self::schedule::{ScheduleWeekday, default_schedule_weekdays, summarize_local_schedule};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EventKind {
    Kill,
    Death,
    Headshot,
    Revive,
    VehicleDestroy,
    KillStreakBonus,
    DominationKill,
    RevengeKill,
    MultipleKill,
    MaxKill,
    NemesisKill,
    PriorityKill,
    HighPriorityKill,
    SaviorKill,
    BountyKill,
    BountyCashedIn,
    BountyKillStreak,
    VehicleRoadkill,
    VehicleRamBonus,
    DropPodKill,
    ExplosiveDestruction,
    ControlPointDefend,
    ControlPointAttack,
    ObjectiveDestroyed,
    FacilityBombPlanted,
    FacilityBombDefused,
    FacilityTerminalHack,
    FacilityTurretHack,
    CapturePointConverted,
    ObjectivePulseDefend,
    ObjectivePulseCapture,
    RouterKill,
    MotionDetect,
    ScoutRadarDetect,
    MotionSensorSpotterKill,
    Saved,
    CtfFlagCaptured,
    CtfFlagReturned,
    FacilityCapture,
    FacilityDefend,
}

impl EventKind {
    pub const ALL: [EventKind; 40] = [
        EventKind::Kill,
        EventKind::Death,
        EventKind::Headshot,
        EventKind::Revive,
        EventKind::VehicleDestroy,
        EventKind::KillStreakBonus,
        EventKind::DominationKill,
        EventKind::RevengeKill,
        EventKind::MultipleKill,
        EventKind::MaxKill,
        EventKind::NemesisKill,
        EventKind::PriorityKill,
        EventKind::HighPriorityKill,
        EventKind::SaviorKill,
        EventKind::BountyKill,
        EventKind::BountyCashedIn,
        EventKind::BountyKillStreak,
        EventKind::VehicleRoadkill,
        EventKind::VehicleRamBonus,
        EventKind::DropPodKill,
        EventKind::ExplosiveDestruction,
        EventKind::ControlPointDefend,
        EventKind::ControlPointAttack,
        EventKind::ObjectiveDestroyed,
        EventKind::FacilityBombPlanted,
        EventKind::FacilityBombDefused,
        EventKind::FacilityTerminalHack,
        EventKind::FacilityTurretHack,
        EventKind::CapturePointConverted,
        EventKind::ObjectivePulseDefend,
        EventKind::ObjectivePulseCapture,
        EventKind::RouterKill,
        EventKind::MotionDetect,
        EventKind::ScoutRadarDetect,
        EventKind::MotionSensorSpotterKill,
        EventKind::Saved,
        EventKind::CtfFlagCaptured,
        EventKind::CtfFlagReturned,
        EventKind::FacilityCapture,
        EventKind::FacilityDefend,
    ];
}

impl std::fmt::Display for EventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Kill => write!(f, "Kill"),
            Self::Death => write!(f, "Death"),
            Self::Headshot => write!(f, "Headshot"),
            Self::Revive => write!(f, "Revive"),
            Self::VehicleDestroy => write!(f, "Vehicle Destroy"),
            Self::KillStreakBonus => write!(f, "Kill Streak Bonus"),
            Self::DominationKill => write!(f, "Domination Kill"),
            Self::RevengeKill => write!(f, "Revenge Kill"),
            Self::MultipleKill => write!(f, "Multiple Kill"),
            Self::MaxKill => write!(f, "MAX Kill"),
            Self::NemesisKill => write!(f, "Nemesis Kill"),
            Self::PriorityKill => write!(f, "Priority Kill"),
            Self::HighPriorityKill => write!(f, "High Priority Kill"),
            Self::SaviorKill => write!(f, "Savior Kill"),
            Self::BountyKill => write!(f, "Bounty Kill"),
            Self::BountyCashedIn => write!(f, "Bounty Cashed In"),
            Self::BountyKillStreak => write!(f, "Bounty Kill Streak"),
            Self::VehicleRoadkill => write!(f, "Vehicle Roadkill"),
            Self::VehicleRamBonus => write!(f, "Vehicle Ram Bonus"),
            Self::DropPodKill => write!(f, "Drop Pod Kill"),
            Self::ExplosiveDestruction => write!(f, "Explosive Destruction"),
            Self::ControlPointDefend => write!(f, "Control Point Defend"),
            Self::ControlPointAttack => write!(f, "Control Point Attack"),
            Self::ObjectiveDestroyed => write!(f, "Objective Destroyed"),
            Self::FacilityBombPlanted => write!(f, "Facility Bomb Planted"),
            Self::FacilityBombDefused => write!(f, "Facility Bomb Defused"),
            Self::FacilityTerminalHack => write!(f, "Facility Terminal Hack"),
            Self::FacilityTurretHack => write!(f, "Facility Turret Hack"),
            Self::CapturePointConverted => write!(f, "Capture Point Converted"),
            Self::ObjectivePulseDefend => write!(f, "Objective Pulse Defend"),
            Self::ObjectivePulseCapture => write!(f, "Objective Pulse Capture"),
            Self::RouterKill => write!(f, "Router Kill"),
            Self::MotionDetect => write!(f, "Motion Detect"),
            Self::ScoutRadarDetect => write!(f, "Scout Radar Detect"),
            Self::MotionSensorSpotterKill => write!(f, "Motion Sensor Spotter Kill"),
            Self::Saved => write!(f, "Saved"),
            Self::CtfFlagCaptured => write!(f, "CTF Flag Captured"),
            Self::CtfFlagReturned => write!(f, "CTF Flag Returned"),
            Self::FacilityCapture => write!(f, "Facility Capture"),
            Self::FacilityDefend => write!(f, "Facility Defend"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CharacterClass {
    Infiltrator,
    LightAssault,
    Medic,
    Engineer,
    HeavyAssault,
    Max,
}

impl CharacterClass {
    #[allow(dead_code)]
    pub const ALL: [CharacterClass; 6] = [
        CharacterClass::Infiltrator,
        CharacterClass::LightAssault,
        CharacterClass::Medic,
        CharacterClass::Engineer,
        CharacterClass::HeavyAssault,
        CharacterClass::Max,
    ];
}

impl std::fmt::Display for CharacterClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Infiltrator => write!(f, "Infiltrator"),
            Self::LightAssault => write!(f, "Light Assault"),
            Self::Medic => write!(f, "Combat Medic"),
            Self::Engineer => write!(f, "Engineer"),
            Self::HeavyAssault => write!(f, "Heavy Assault"),
            Self::Max => write!(f, "MAX"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClipLength {
    Seconds(u32),
    FullBuffer,
}

impl Default for ClipLength {
    fn default() -> Self {
        Self::Seconds(30)
    }
}

impl std::fmt::Display for ClipLength {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Seconds(value) => write!(f, "{value} seconds"),
            Self::FullBuffer => write!(f, "Full buffer"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClipExtensionPolicy {
    #[serde(default)]
    pub mode: ClipExtensionMode,
    #[serde(default = "default_clip_extension_window_secs")]
    pub window_secs: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClipExtensionMode {
    #[default]
    Disabled,
    HoldUntilQuiet,
}

impl Default for ClipExtensionPolicy {
    fn default() -> Self {
        Self {
            mode: ClipExtensionMode::Disabled,
            window_secs: default_clip_extension_window_secs(),
        }
    }
}

impl ClipExtensionPolicy {
    pub fn is_enabled(&self) -> bool {
        self.mode != ClipExtensionMode::Disabled
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleDefinition {
    pub id: String,
    pub name: String,
    pub activation_class: Option<CharacterClass>,
    pub lookback_secs: u32,
    pub trigger_threshold: u32,
    pub reset_threshold: u32,
    pub cooldown_secs: Option<u32>,
    #[serde(default)]
    pub use_full_buffer: bool,
    #[serde(default)]
    pub capture_entire_base_cap: bool,
    pub base_duration_secs: u32,
    pub secs_per_point: u32,
    pub max_duration_secs: u32,
    #[serde(default)]
    pub extension: ClipExtensionPolicy,
    pub scored_events: Vec<ScoredEvent>,
}

impl RuleDefinition {
    pub fn clip_length_for_trigger(
        &self,
        trigger_score: u32,
        trigger_event: EventKind,
    ) -> ClipLength {
        if self.use_full_buffer
            || (self.capture_entire_base_cap && trigger_event == EventKind::FacilityCapture)
        {
            return ClipLength::FullBuffer;
        }

        let scaled = trigger_score.saturating_mul(self.secs_per_point);
        let duration = self
            .base_duration_secs
            .saturating_add(scaled)
            .clamp(self.base_duration_secs, self.max_duration_secs);
        ClipLength::Seconds(duration)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredEvent {
    pub event: EventKind,
    pub points: u32,
    #[serde(default)]
    pub filters: ScoredEventFilters,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VehicleVariantFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<u16>,
}

impl VehicleVariantFilter {
    pub fn is_configured(&self) -> bool {
        self.label
            .as_deref()
            .is_some_and(|label| !label.trim().is_empty())
            || !self.ids.is_empty()
    }

    pub fn normalize(&mut self) {
        self.label = self
            .label
            .as_deref()
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(str::to_string);
        self.ids.sort_unstable();
        self.ids.dedup();
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WeaponVariantFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<u32>,
}

impl WeaponVariantFilter {
    pub fn is_configured(&self) -> bool {
        self.label
            .as_deref()
            .is_some_and(|label| !label.trim().is_empty())
            || !self.ids.is_empty()
    }

    pub fn normalize(&mut self) {
        self.label = self
            .label
            .as_deref()
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(str::to_string);
        self.ids.sort_unstable();
        self.ids.dedup();
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CharacterReferenceFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub character_id: Option<u64>,
}

impl CharacterReferenceFilter {
    pub fn is_configured(&self) -> bool {
        self.name
            .as_deref()
            .is_some_and(|name| !name.trim().is_empty())
            || self.character_id.is_some()
    }

    pub fn normalize(&mut self) {
        self.name = self
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string);
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutfitReferenceFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default)]
    pub outfit_id: Option<u64>,
}

impl OutfitReferenceFilter {
    pub fn is_configured(&self) -> bool {
        self.tag
            .as_deref()
            .is_some_and(|tag| !tag.trim().is_empty())
            || self.outfit_id.is_some()
    }

    pub fn normalize(&mut self) {
        self.tag = self
            .tag
            .as_deref()
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_string);
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VehicleMatchFilter {
    #[serde(default)]
    pub vehicle: VehicleVariantFilter,
    #[serde(default)]
    pub legacy_vehicle_id: Option<u16>,
}

impl VehicleMatchFilter {
    pub fn is_configured(&self) -> bool {
        self.vehicle.is_configured() || self.legacy_vehicle_id.is_some()
    }

    pub fn normalize(&mut self) {
        self.vehicle.normalize();
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WeaponMatchFilter {
    #[serde(default)]
    pub weapon: WeaponVariantFilter,
    #[serde(default)]
    pub legacy_weapon_id: Option<u32>,
}

impl WeaponMatchFilter {
    pub fn is_configured(&self) -> bool {
        self.weapon.is_configured() || self.legacy_weapon_id.is_some()
    }

    pub fn normalize(&mut self) {
        self.weapon.normalize();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScoredEventFilterClause {
    TargetCharacter {
        #[serde(flatten)]
        target: CharacterReferenceFilter,
    },
    TargetOutfit {
        #[serde(flatten)]
        outfit: OutfitReferenceFilter,
    },
    AttackerVehicle {
        #[serde(flatten)]
        vehicle: VehicleMatchFilter,
    },
    AttackerWeapon {
        #[serde(flatten)]
        weapon: WeaponMatchFilter,
    },
    DestroyedVehicle {
        #[serde(flatten)]
        vehicle: VehicleMatchFilter,
    },
    Any {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        clauses: Vec<ScoredEventFilterClause>,
    },
}

impl ScoredEventFilterClause {
    pub fn is_configured(&self) -> bool {
        match self {
            Self::TargetCharacter { target } => target.is_configured(),
            Self::TargetOutfit { outfit } => outfit.is_configured(),
            Self::AttackerVehicle { vehicle } | Self::DestroyedVehicle { vehicle } => {
                vehicle.is_configured()
            }
            Self::AttackerWeapon { weapon } => weapon.is_configured(),
            Self::Any { clauses } => clauses.iter().any(Self::is_configured),
        }
    }

    pub fn normalize(&mut self) {
        match self {
            Self::TargetCharacter { target } => target.normalize(),
            Self::TargetOutfit { outfit } => outfit.normalize(),
            Self::AttackerVehicle { vehicle } | Self::DestroyedVehicle { vehicle } => {
                vehicle.normalize()
            }
            Self::AttackerWeapon { weapon } => weapon.normalize(),
            Self::Any { clauses } => {
                for clause in clauses {
                    clause.normalize();
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScoredEventFilterGroup {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clauses: Vec<ScoredEventFilterClause>,
}

impl ScoredEventFilterGroup {
    pub fn is_configured(&self) -> bool {
        self.clauses
            .iter()
            .any(ScoredEventFilterClause::is_configured)
    }

    pub fn normalize(&mut self) {
        for clause in &mut self.clauses {
            clause.normalize();
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScoredEventFilters {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<ScoredEventFilterGroup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_character_name: Option<String>,
    #[serde(default)]
    pub target_character_id: Option<u64>,
    #[serde(default)]
    pub attacker_vehicle: VehicleVariantFilter,
    #[serde(default)]
    pub attacker_vehicle_id: Option<u16>,
    #[serde(default)]
    pub vehicle_killed: VehicleVariantFilter,
    #[serde(default)]
    pub vehicle_killed_id: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_outfit_tag: Option<String>,
    #[serde(default)]
    pub target_outfit_id: Option<u64>,
}

impl ScoredEventFilters {
    pub fn has_criteria(&self) -> bool {
        self.groups
            .iter()
            .any(ScoredEventFilterGroup::is_configured)
            || self.has_legacy_criteria()
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or_else(|| self.has_criteria())
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty() && !self.has_legacy_criteria()
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = Some(enabled);
    }

    pub fn normalize(&mut self) {
        if self.groups.is_empty()
            && let Some(group) = self.legacy_group()
        {
            self.groups.push(group);
        }

        for group in &mut self.groups {
            group.normalize();
        }

        self.target_character_name = None;
        self.target_character_id = None;
        self.attacker_vehicle = VehicleVariantFilter::default();
        self.attacker_vehicle_id = None;
        self.vehicle_killed = VehicleVariantFilter::default();
        self.vehicle_killed_id = None;
        self.target_outfit_tag = None;
        self.target_outfit_id = None;
    }

    pub fn groups(&self) -> Cow<'_, [ScoredEventFilterGroup]> {
        if self.groups.is_empty() {
            match self.legacy_group() {
                Some(group) => Cow::Owned(vec![group]),
                None => Cow::Owned(Vec::new()),
            }
        } else {
            Cow::Borrowed(&self.groups)
        }
    }

    fn has_legacy_criteria(&self) -> bool {
        self.target_character_name
            .as_deref()
            .is_some_and(|name| !name.trim().is_empty())
            || self.target_character_id.is_some()
            || self.attacker_vehicle.is_configured()
            || self.attacker_vehicle_id.is_some()
            || self.vehicle_killed.is_configured()
            || self.vehicle_killed_id.is_some()
            || self
                .target_outfit_tag
                .as_deref()
                .is_some_and(|tag| !tag.trim().is_empty())
            || self.target_outfit_id.is_some()
    }

    fn legacy_group(&self) -> Option<ScoredEventFilterGroup> {
        if !self.has_legacy_criteria() {
            return None;
        }

        let mut clauses = Vec::new();
        let mut target = CharacterReferenceFilter {
            name: self.target_character_name.clone(),
            character_id: self.target_character_id,
        };
        target.normalize();
        if target.is_configured() {
            clauses.push(ScoredEventFilterClause::TargetCharacter { target });
        }

        let mut outfit = OutfitReferenceFilter {
            tag: self.target_outfit_tag.clone(),
            outfit_id: self.target_outfit_id,
        };
        outfit.normalize();
        if outfit.is_configured() {
            clauses.push(ScoredEventFilterClause::TargetOutfit { outfit });
        }

        let mut attacker = VehicleMatchFilter {
            vehicle: self.attacker_vehicle.clone(),
            legacy_vehicle_id: self.attacker_vehicle_id,
        };
        attacker.normalize();
        if attacker.is_configured() {
            clauses.push(ScoredEventFilterClause::AttackerVehicle { vehicle: attacker });
        }

        let mut destroyed = VehicleMatchFilter {
            vehicle: self.vehicle_killed.clone(),
            legacy_vehicle_id: self.vehicle_killed_id,
        };
        destroyed.normalize();
        if destroyed.is_configured() {
            clauses.push(ScoredEventFilterClause::DestroyedVehicle { vehicle: destroyed });
        }

        Some(ScoredEventFilterGroup { clauses })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CharacterReferenceFilter, EventKind, OutfitReferenceFilter, ScoredEvent,
        ScoredEventFilterClause, ScoredEventFilterGroup, ScoredEventFilters, VehicleMatchFilter,
        VehicleVariantFilter, WeaponMatchFilter, WeaponVariantFilter, default_rule_definitions,
        validate_rule,
    };

    #[test]
    fn legacy_filters_with_criteria_are_treated_as_enabled() {
        let filters = ScoredEventFilters {
            enabled: None,
            groups: Vec::new(),
            target_character_name: None,
            target_character_id: Some(42),
            attacker_vehicle: VehicleVariantFilter::default(),
            attacker_vehicle_id: None,
            vehicle_killed: VehicleVariantFilter::default(),
            vehicle_killed_id: None,
            target_outfit_tag: None,
            target_outfit_id: None,
        };

        assert!(filters.is_enabled());
    }

    #[test]
    fn explicit_disable_overrides_saved_criteria() {
        let filters = ScoredEventFilters {
            enabled: Some(false),
            groups: Vec::new(),
            target_character_name: Some("Example".into()),
            target_character_id: Some(42),
            attacker_vehicle: VehicleVariantFilter::default(),
            attacker_vehicle_id: Some(7),
            vehicle_killed: VehicleVariantFilter::default(),
            vehicle_killed_id: None,
            target_outfit_tag: None,
            target_outfit_id: None,
        };

        assert!(!filters.is_enabled());
        assert!(filters.has_criteria());
    }

    #[test]
    fn unresolved_target_name_is_invalid_when_filters_are_enabled() {
        let mut rule = default_rule_definitions().remove(0);
        rule.scored_events[0].filters.enabled = Some(true);
        rule.scored_events[0].filters.groups = vec![ScoredEventFilterGroup {
            clauses: vec![ScoredEventFilterClause::TargetCharacter {
                target: CharacterReferenceFilter {
                    name: Some("Example".into()),
                    character_id: None,
                },
            }],
        }];

        let error = validate_rule(&rule).unwrap_err();
        assert!(error.contains("must be resolved"));
    }

    #[test]
    fn duplicate_scored_events_are_valid() {
        let mut rule = default_rule_definitions().remove(0);
        rule.scored_events = vec![
            ScoredEvent {
                event: EventKind::Kill,
                points: 2,
                filters: ScoredEventFilters {
                    enabled: Some(true),
                    groups: vec![ScoredEventFilterGroup {
                        clauses: vec![ScoredEventFilterClause::TargetCharacter {
                            target: CharacterReferenceFilter {
                                name: Some("EnemyOne".into()),
                                character_id: Some(42),
                            },
                        }],
                    }],
                    ..ScoredEventFilters::default()
                },
            },
            ScoredEvent {
                event: EventKind::Kill,
                points: 3,
                filters: ScoredEventFilters {
                    enabled: Some(true),
                    groups: vec![ScoredEventFilterGroup {
                        clauses: vec![ScoredEventFilterClause::TargetCharacter {
                            target: CharacterReferenceFilter {
                                name: Some("EnemyTwo".into()),
                                character_id: Some(99),
                            },
                        }],
                    }],
                    ..ScoredEventFilters::default()
                },
            },
        ];

        assert!(validate_rule(&rule).is_ok());
    }

    #[test]
    fn weapon_filters_report_criteria() {
        let filters = ScoredEventFilters {
            enabled: Some(true),
            groups: vec![ScoredEventFilterGroup {
                clauses: vec![ScoredEventFilterClause::AttackerWeapon {
                    weapon: WeaponMatchFilter {
                        weapon: WeaponVariantFilter {
                            label: Some("Gauss Rifle".into()),
                            ids: vec![80],
                        },
                        legacy_weapon_id: None,
                    },
                }],
            }],
            ..ScoredEventFilters::default()
        };

        assert!(filters.has_criteria());
    }

    #[test]
    fn normalize_migrates_legacy_fields_into_single_group() {
        let mut filters = ScoredEventFilters {
            enabled: Some(true),
            groups: Vec::new(),
            target_character_name: Some("Enemy".into()),
            target_character_id: Some(42),
            attacker_vehicle: VehicleVariantFilter {
                label: Some("Scythe".into()),
                ids: vec![1],
            },
            attacker_vehicle_id: None,
            vehicle_killed: VehicleVariantFilter::default(),
            vehicle_killed_id: None,
            target_outfit_tag: None,
            target_outfit_id: None,
        };

        filters.normalize();

        assert_eq!(filters.groups.len(), 1);
        assert_eq!(filters.target_character_name, None);
        assert_eq!(filters.target_character_id, None);
    }

    #[test]
    fn groups_report_has_criteria_without_legacy_fields() {
        let filters = ScoredEventFilters {
            enabled: Some(true),
            groups: vec![ScoredEventFilterGroup {
                clauses: vec![ScoredEventFilterClause::AttackerVehicle {
                    vehicle: VehicleMatchFilter {
                        vehicle: VehicleVariantFilter {
                            label: Some("Scythe".into()),
                            ids: vec![1],
                        },
                        legacy_vehicle_id: None,
                    },
                }],
            }],
            ..ScoredEventFilters::default()
        };

        assert!(filters.has_criteria());
        assert!(!filters.groups().is_empty());
    }

    #[test]
    fn empty_groups_do_not_count_as_criteria() {
        let filters = ScoredEventFilters {
            enabled: None,
            groups: vec![ScoredEventFilterGroup {
                clauses: vec![ScoredEventFilterClause::TargetCharacter {
                    target: CharacterReferenceFilter::default(),
                }],
            }],
            ..ScoredEventFilters::default()
        };

        assert!(!filters.has_criteria());
        assert!(!filters.is_enabled());
        assert_eq!(filters.groups().len(), 1);
    }

    #[test]
    fn normalize_preserves_empty_groups_for_editing() {
        let mut filters = ScoredEventFilters {
            enabled: Some(true),
            groups: vec![ScoredEventFilterGroup {
                clauses: vec![ScoredEventFilterClause::TargetOutfit {
                    outfit: OutfitReferenceFilter::default(),
                }],
            }],
            ..ScoredEventFilters::default()
        };

        filters.normalize();

        assert_eq!(filters.groups.len(), 1);
        assert_eq!(filters.groups[0].clauses.len(), 1);
        assert!(!filters.has_criteria());
    }

    #[test]
    fn nested_any_clause_counts_as_criteria_when_child_is_configured() {
        let filters = ScoredEventFilters {
            enabled: Some(true),
            groups: vec![ScoredEventFilterGroup {
                clauses: vec![ScoredEventFilterClause::Any {
                    clauses: vec![ScoredEventFilterClause::TargetCharacter {
                        target: CharacterReferenceFilter {
                            name: Some("Enemy".into()),
                            character_id: Some(42),
                        },
                    }],
                }],
            }],
            ..ScoredEventFilters::default()
        };

        assert!(filters.has_criteria());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutoSwitchRule {
    pub id: String,
    pub name: String,
    #[serde(default = "default_auto_switch_rule_enabled")]
    pub enabled: bool,
    pub target_profile_id: String,
    pub condition: AutoSwitchCondition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AutoSwitchCondition {
    LocalSchedule {
        #[serde(default = "default_schedule_weekdays")]
        weekdays: Vec<ScheduleWeekday>,
        start_hour: u8,
        #[serde(default)]
        start_minute: u8,
        end_hour: u8,
        #[serde(default)]
        end_minute: u8,
    },
    ActiveCharacter {
        #[serde(default)]
        character_ids: Vec<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        character_id: Option<u64>,
    },
    #[serde(alias = "local_time_range")]
    LocalTimeRange { start_hour: u8, end_hour: u8 },
    #[serde(alias = "local_cron")]
    LocalCron { expression: String },
    #[serde(alias = "on_event")]
    OnEvent { event: EventKind },
}

impl AutoSwitchCondition {
    pub fn summary(&self) -> String {
        match self {
            Self::LocalSchedule {
                weekdays,
                start_hour,
                start_minute,
                end_hour,
                end_minute,
            } => summarize_local_schedule(
                weekdays,
                *start_hour,
                *start_minute,
                *end_hour,
                *end_minute,
            ),
            Self::ActiveCharacter {
                character_ids,
                character_id,
            } => {
                let ids = normalized_active_character_ids(character_ids, *character_id);
                match ids.as_slice() {
                    [] => "No active characters selected".into(),
                    [id] => format!("When monitoring character #{id}"),
                    _ => format!("When monitoring any of {} characters", ids.len()),
                }
            }
            Self::LocalTimeRange {
                start_hour,
                end_hour,
            } => format!(
                "Local time {:02}:00-{:02}:00",
                start_hour % 24,
                end_hour % 24
            ),
            Self::LocalCron { expression } => {
                format!("Local schedule `{}`", expression.trim())
            }
            Self::OnEvent { event } => format!("On event: {event}"),
        }
    }
}

pub fn normalized_active_character_ids(
    character_ids: &[u64],
    legacy_character_id: Option<u64>,
) -> Vec<u64> {
    let mut ids = character_ids.to_vec();
    if let Some(character_id) = legacy_character_id {
        ids.push(character_id);
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleProfile {
    pub id: String,
    pub name: String,
    pub enabled_rule_ids: Vec<String>,
}

impl RuleProfile {
    pub fn enables(&self, rule_id: &str) -> bool {
        self.enabled_rule_ids.iter().any(|id| id == rule_id)
    }
}

#[derive(Debug, Clone)]
pub struct ClassifiedEvent {
    pub kind: EventKind,
    pub timestamp: DateTime<Utc>,
    pub world_id: u32,
    pub zone_id: Option<u32>,
    pub facility_id: Option<u32>,
    pub actor_character_id: Option<u64>,
    pub other_character_id: Option<u64>,
    pub other_character_outfit_id: Option<u64>,
    pub characters_killed: u32,
    pub attacker_weapon_id: Option<u32>,
    pub attacker_vehicle_id: Option<u16>,
    pub vehicle_killed_id: Option<u16>,
    pub is_headshot: bool,
    pub actor_class: Option<CharacterClass>,
    pub experience_id: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct ScoreBreakdown {
    pub event: EventKind,
    pub occurrences: u32,
    pub points: u32,
}

impl ScoreBreakdown {
    pub fn summary_line(&self) -> String {
        format!("{} x{} = {}", self.event, self.occurrences, self.points)
    }
}

#[derive(Debug, Clone)]
pub struct ClipAction {
    pub rule_id: String,
    #[allow(dead_code)]
    pub rule_name: String,
    pub lifecycle: ClipActionLifecycle,
    pub clip_length: ClipLength,
    pub trigger_score: u32,
    pub score_breakdown: Vec<ScoreBreakdown>,
    pub event: ClassifiedEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipActionLifecycle {
    Trigger,
    StartExtending { expires_at: DateTime<Utc> },
    Extend { expires_at: DateTime<Utc> },
    Finalize { finalized_at: DateTime<Utc> },
}

pub fn default_rule_definitions() -> Vec<RuleDefinition> {
    vec![
        RuleDefinition {
            id: "rule_infantry_momentum".into(),
            name: "Infantry Momentum".into(),
            activation_class: None,
            lookback_secs: 15,
            trigger_threshold: 8,
            reset_threshold: 3,
            cooldown_secs: Some(20),
            use_full_buffer: false,
            capture_entire_base_cap: false,
            base_duration_secs: 20,
            secs_per_point: 3,
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
                ScoredEvent {
                    event: EventKind::Revive,
                    points: 2,
                    filters: ScoredEventFilters::default(),
                },
                ScoredEvent {
                    event: EventKind::MultipleKill,
                    points: 3,
                    filters: ScoredEventFilters::default(),
                },
                ScoredEvent {
                    event: EventKind::KillStreakBonus,
                    points: 3,
                    filters: ScoredEventFilters::default(),
                },
            ],
        },
        RuleDefinition {
            id: "rule_support_heroics".into(),
            name: "Support Heroics".into(),
            activation_class: Some(CharacterClass::Medic),
            lookback_secs: 20,
            trigger_threshold: 9,
            reset_threshold: 4,
            cooldown_secs: Some(25),
            use_full_buffer: false,
            capture_entire_base_cap: false,
            base_duration_secs: 25,
            secs_per_point: 3,
            max_duration_secs: 75,
            extension: ClipExtensionPolicy::default(),
            scored_events: vec![
                ScoredEvent {
                    event: EventKind::Revive,
                    points: 3,
                    filters: ScoredEventFilters::default(),
                },
                ScoredEvent {
                    event: EventKind::Kill,
                    points: 1,
                    filters: ScoredEventFilters::default(),
                },
                ScoredEvent {
                    event: EventKind::Headshot,
                    points: 2,
                    filters: ScoredEventFilters::default(),
                },
                ScoredEvent {
                    event: EventKind::SaviorKill,
                    points: 3,
                    filters: ScoredEventFilters::default(),
                },
            ],
        },
        RuleDefinition {
            id: "rule_objective_swing".into(),
            name: "Objective Swing".into(),
            activation_class: None,
            lookback_secs: 30,
            trigger_threshold: 8,
            reset_threshold: 3,
            cooldown_secs: Some(30),
            use_full_buffer: false,
            capture_entire_base_cap: false,
            base_duration_secs: 30,
            secs_per_point: 4,
            max_duration_secs: 90,
            extension: ClipExtensionPolicy::default(),
            scored_events: vec![
                ScoredEvent {
                    event: EventKind::VehicleDestroy,
                    points: 4,
                    filters: ScoredEventFilters::default(),
                },
                ScoredEvent {
                    event: EventKind::FacilityCapture,
                    points: 6,
                    filters: ScoredEventFilters::default(),
                },
                ScoredEvent {
                    event: EventKind::FacilityDefend,
                    points: 4,
                    filters: ScoredEventFilters::default(),
                },
                ScoredEvent {
                    event: EventKind::ControlPointAttack,
                    points: 2,
                    filters: ScoredEventFilters::default(),
                },
                ScoredEvent {
                    event: EventKind::CapturePointConverted,
                    points: 3,
                    filters: ScoredEventFilters::default(),
                },
                ScoredEvent {
                    event: EventKind::ObjectivePulseCapture,
                    points: 4,
                    filters: ScoredEventFilters::default(),
                },
                ScoredEvent {
                    event: EventKind::Kill,
                    points: 1,
                    filters: ScoredEventFilters::default(),
                },
            ],
        },
    ]
}

pub fn default_rule_profiles() -> Vec<RuleProfile> {
    vec![RuleProfile {
        id: "profile_default".into(),
        name: "Default".into(),
        enabled_rule_ids: vec![
            "rule_infantry_momentum".into(),
            "rule_support_heroics".into(),
            "rule_objective_swing".into(),
        ],
    }]
}

pub fn default_auto_switch_rules() -> Vec<AutoSwitchRule> {
    Vec::new()
}

pub fn validate_rule(rule: &RuleDefinition) -> Result<(), String> {
    if rule.id.trim().is_empty() {
        return Err("rule id cannot be empty".into());
    }
    if rule.name.trim().is_empty() {
        return Err("rule name cannot be empty".into());
    }
    if rule.lookback_secs == 0 {
        return Err("lookback window must be at least 1 second".into());
    }
    if rule.trigger_threshold == 0 {
        return Err("trigger threshold must be greater than zero".into());
    }
    if rule.reset_threshold >= rule.trigger_threshold {
        return Err("reset threshold must be below the trigger threshold".into());
    }
    if !rule.use_full_buffer {
        if rule.base_duration_secs == 0 {
            return Err("base duration must be greater than zero".into());
        }
        if rule.max_duration_secs < rule.base_duration_secs {
            return Err("max duration must be at least the base duration".into());
        }
    }
    if rule.extension.is_enabled() && rule.extension.window_secs == 0 {
        return Err(
            "extension window must be at least 1 second when auto-extend is enabled".into(),
        );
    }
    if rule.scored_events.is_empty() {
        return Err("rule must contain at least one scored event".into());
    }

    for scored_event in &rule.scored_events {
        if scored_event.points == 0 {
            return Err(format!(
                "scored event `{}` must award at least 1 point",
                scored_event.event
            ));
        }
        let filters = &scored_event.filters;
        if filters.is_enabled() {
            for group in filters.groups().iter() {
                if let Some(error) = validate_filter_clauses(&group.clauses, scored_event.event) {
                    return Err(error);
                }
            }
        }
    }

    Ok(())
}

fn validate_filter_clauses(
    clauses: &[ScoredEventFilterClause],
    event: EventKind,
) -> Option<String> {
    for clause in clauses {
        match clause {
            ScoredEventFilterClause::TargetCharacter { target }
                if target
                    .name
                    .as_deref()
                    .is_some_and(|name| !name.trim().is_empty())
                    && target.character_id.is_none() =>
            {
                return Some(format!(
                    "target character filter for `{event}` must be resolved before the rule can match"
                ));
            }
            ScoredEventFilterClause::TargetOutfit { outfit }
                if outfit
                    .tag
                    .as_deref()
                    .is_some_and(|tag| !tag.trim().is_empty())
                    && outfit.outfit_id.is_none() =>
            {
                return Some(format!(
                    "target outfit filter for `{event}` must be resolved before the rule can match"
                ));
            }
            ScoredEventFilterClause::Any { clauses } => {
                if let Some(error) = validate_filter_clauses(clauses, event) {
                    return Some(error);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn validate_auto_switch_rule(rule: &AutoSwitchRule) -> Result<(), String> {
    if rule.id.trim().is_empty() {
        return Err("auto-switch rule id cannot be empty".into());
    }
    if rule.name.trim().is_empty() {
        return Err("auto-switch rule name cannot be empty".into());
    }
    if rule.target_profile_id.trim().is_empty() {
        return Err("auto-switch rule must target a profile".into());
    }
    match rule.condition {
        AutoSwitchCondition::LocalSchedule {
            start_hour,
            start_minute,
            end_hour,
            end_minute,
            ..
        } => {
            if start_hour > 23 {
                return Err("auto-switch schedule start hour must be between 0 and 23".into());
            }
            if start_minute != 0 && start_minute != 30 {
                return Err(
                    "auto-switch schedule start minute must use 30-minute increments".into(),
                );
            }
            if end_hour > 24 {
                return Err("auto-switch schedule end hour must be between 0 and 24".into());
            }
            if end_minute != 0 && end_minute != 30 {
                return Err("auto-switch schedule end minute must use 30-minute increments".into());
            }
            if end_hour == 24 && end_minute != 0 {
                return Err("auto-switch schedule end time cannot be later than 24:00".into());
            }
        }
        AutoSwitchCondition::ActiveCharacter { .. } => {}
        AutoSwitchCondition::LocalTimeRange {
            start_hour,
            end_hour,
        } => {
            if start_hour > 23 || end_hour > 23 {
                return Err("auto-switch time range must use hours between 0 and 23".into());
            }
        }
        AutoSwitchCondition::LocalCron { ref expression } => {
            schedule::validate_local_cron_expression(expression)?;
        }
        AutoSwitchCondition::OnEvent { .. } => {}
    }

    Ok(())
}

fn default_clip_extension_window_secs() -> u32 {
    6
}

fn default_auto_switch_rule_enabled() -> bool {
    true
}

#[cfg(test)]
mod auto_switch_tests {
    use super::{AutoSwitchCondition, AutoSwitchRule, validate_auto_switch_rule};
    use crate::rules::schedule::ScheduleWeekday;

    fn sample_rule(condition: AutoSwitchCondition) -> AutoSwitchRule {
        AutoSwitchRule {
            id: "rule_1".into(),
            name: "Rule 1".into(),
            enabled: true,
            target_profile_id: "profile_default".into(),
            condition,
        }
    }

    #[test]
    fn validate_auto_switch_rule_accepts_local_schedule() {
        let rule = sample_rule(AutoSwitchCondition::LocalSchedule {
            weekdays: vec![ScheduleWeekday::Monday, ScheduleWeekday::Friday],
            start_hour: 19,
            start_minute: 30,
            end_hour: 23,
            end_minute: 0,
        });

        assert!(validate_auto_switch_rule(&rule).is_ok());
    }

    #[test]
    fn validate_auto_switch_rule_rejects_invalid_schedule_hour() {
        let rule = sample_rule(AutoSwitchCondition::LocalSchedule {
            weekdays: vec![ScheduleWeekday::Monday],
            start_hour: 25,
            start_minute: 0,
            end_hour: 23,
            end_minute: 0,
        });

        assert!(validate_auto_switch_rule(&rule).is_err());
    }

    #[test]
    fn validate_auto_switch_rule_rejects_invalid_schedule_minutes() {
        let rule = sample_rule(AutoSwitchCondition::LocalSchedule {
            weekdays: vec![ScheduleWeekday::Monday],
            start_hour: 19,
            start_minute: 15,
            end_hour: 23,
            end_minute: 0,
        });

        assert!(validate_auto_switch_rule(&rule).is_err());
    }

    #[test]
    fn auto_switch_condition_summary_formats_schedule_days_and_hours() {
        let condition = AutoSwitchCondition::LocalSchedule {
            weekdays: vec![ScheduleWeekday::Monday, ScheduleWeekday::Friday],
            start_hour: 19,
            start_minute: 30,
            end_hour: 23,
            end_minute: 0,
        };

        assert_eq!(condition.summary(), "Mon, Fri 19:30-23:00".to_string());
    }

    #[test]
    fn validate_auto_switch_rule_accepts_active_character_rules() {
        let rule = sample_rule(AutoSwitchCondition::ActiveCharacter {
            character_ids: vec![42],
            character_id: None,
        });

        assert!(validate_auto_switch_rule(&rule).is_ok());
    }

    #[test]
    fn active_character_summary_handles_multiple_character_ids() {
        let condition = AutoSwitchCondition::ActiveCharacter {
            character_ids: vec![42, 99],
            character_id: None,
        };

        assert_eq!(condition.summary(), "When monitoring any of 2 characters");
    }
}
