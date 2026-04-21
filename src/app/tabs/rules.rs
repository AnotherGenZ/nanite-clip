mod editor;

use std::collections::BTreeMap;
use std::path::Path;

use auraxis::Faction;
use chrono::Utc;
use iced::{Element, Length, mouse};

use crate::app::{PendingProfileImport, PendingRuleImport};
use crate::census;
use crate::db::{LookupKind, WeaponReferenceCacheEntry};
use crate::profile_transfer::{ProfileTransferBundle, RuleTransferBundle};
use crate::rules::schedule::{
    ScheduleWeekday, default_schedule_weekdays, format_schedule_time, normalize_schedule_weekdays,
};
use crate::rules::{
    AutoSwitchCondition, AutoSwitchRule, CharacterClass, CharacterReferenceFilter,
    ClipExtensionMode, ClipExtensionPolicy, EventKind, OutfitReferenceFilter, RuleDefinition,
    RuleProfile, ScoredEvent, ScoredEventFilterClause, ScoredEventFilterGroup, ScoredEventFilters,
    VehicleMatchFilter, WeaponMatchFilter, normalized_active_character_ids,
    validate_auto_switch_rule, validate_rule,
};

use super::super::shared::{
    ButtonTone, field_label, icon_label, settings_pick_list_field, settings_stepper_field,
    settings_text_field, solid_icon, styled_button, styled_button_row, with_tooltip,
};
use super::super::{App, Message as AppMessage};
use crate::ui::app::{
    column, container, mouse_area, pick_list, rounded_box, row, scrollable, text, text_input,
    transparent_box,
};
// nanite-ui layout components
use crate::ui::layout::card::card;
use crate::ui::layout::empty_state::empty_state;
use crate::ui::layout::page_header::page_header;
use crate::ui::layout::panel::panel;
use crate::ui::layout::section::section;
use crate::ui::layout::sidebar::{SidebarItem, sidebar};
use crate::ui::layout::tabs::{Tab, tabs};
use crate::ui::overlay::banner::banner;
use crate::ui::overlay::modal::modal_with_backdrop_action;
use crate::ui::primitives::badge::{Tone as BadgeTone, badge};
use crate::ui::primitives::switch::switch as toggle_switch;
pub(crate) use editor::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RulesSubView {
    #[default]
    Rules,
    Profiles,
}

#[derive(Debug, Clone)]
pub enum Message {
    SetSubView(RulesSubView),
    ToggleEventExpanded(String, usize),
    ToggleFilterExpanded(String, usize),
    SetActiveProfile(String),
    CreateProfile,
    DuplicateActiveProfile,
    DeleteActiveProfile,
    ExportActiveProfile,
    ActiveProfileExported(Result<Option<String>, String>),
    ImportProfiles,
    ProfilesImportPrepared(Result<Option<PendingProfileImport>, String>),
    ConfirmProfileImportOverwrite,
    CancelProfileImportOverwrite,
    RequireProfileImportDecision,
    ExportSelectedRule,
    SelectedRuleExported(Result<Option<String>, String>),
    ImportRules,
    RulesImportPrepared(Result<Option<PendingRuleImport>, String>),
    ConfirmRuleImportOverwrite,
    CancelRuleImportOverwrite,
    RequireRuleImportDecision,
    RenameActiveProfile(String),
    ToggleRule(String, bool),
    SelectRule(String),
    CreateRule,
    DuplicateRule,
    DeleteRule,
    RenameRule(String),
    ToggleFullBuffer(bool),
    ToggleCaptureEntireBaseCap(bool),
    ToggleAutoExtend(bool),
    ExtensionWindowStepped(i32),
    BaseDurationStepped(i32),
    LookbackStepped(i32),
    TriggerThresholdStepped(i32),
    ResetThresholdStepped(i32),
    ToggleCooldown(bool),
    CooldownStepped(i32),
    ActivationClassChanged(ClassFilterChoice),
    SecondsPerPointStepped(i32),
    MaxDurationStepped(i32),
    AddScoredEvent,
    DeleteScoredEvent(usize),
    ScoredEventKindChanged(usize, EventKind),
    ScoredEventPointsStepped(usize, i32),
    StartScoredEventDrag(usize),
    HoverScoredEventDrop(usize),
    ToggleScoredEventFilters(usize, bool),
    AddScoredEventFilterGroup(usize),
    DeleteScoredEventFilterGroup(usize, usize),
    AddScoredEventFilterClause(usize, usize),
    AddNestedOrClause(FilterClausePath),
    DeleteScoredEventFilterClause(FilterClausePath),
    ScoredEventFilterClauseKindChanged(FilterClausePath, FilterClauseKindChoice),
    StartFilterClauseDrag(FilterClausePath),
    HoverFilterClauseDrop(FilterClauseListPath, usize),
    RuleFilterDraftChanged(FilterTextDraftKey, String),
    ResolveRuleFilterDraft(FilterTextDraftKey),
    RuleFilterDraftResolved {
        key: FilterTextDraftKey,
        result: Result<ResolvedFilterReference, String>,
    },
    OpenHonuReference(String),
    HonuReferenceOpened(Result<(), String>),
    VehicleOptionsLoaded(Result<Vec<LookupOption>, String>),
    WeaponOptionsLoaded(Result<Vec<WeaponLookupOption>, String>),
    ScoredEventWeaponGroupChanged(FilterClausePath, WeaponBrowseGroup),
    ScoredEventVehicleCategoryChanged(FilterClausePath, VehicleBrowseCategory),
    ScoredEventVehicleChanged(FilterClausePath, VehicleFilterChoice),
    ScoredEventWeaponCategoryChanged(FilterClausePath, WeaponBrowseCategory),
    ScoredEventWeaponFactionChanged(FilterClausePath, WeaponBrowseFaction),
    ScoredEventWeaponChanged(FilterClausePath, WeaponFilterChoice),
    CreateAutoSwitchRule,
    DeleteAutoSwitchRule(String),
    ToggleAutoSwitchRule(String, bool),
    RenameAutoSwitchRule(String, String),
    AutoSwitchTargetProfileChanged(String, String),
    AutoSwitchConditionChanged(String, AutoSwitchConditionChoice),
    ToggleAutoSwitchCharacter(String, u64),
    SetAutoSwitchAllCharacters(String),
    ToggleAutoSwitchWeekday(String, ScheduleWeekday),
    SetAutoSwitchAllDays(String),
    AutoSwitchStartTimeStepped(String, i32),
    AutoSwitchEndTimeStepped(String, i32),
    RuleDragReleased,
    ResumeAutoSwitching,
}

pub(in crate::app) fn update(app: &mut App, message: Message) -> iced::Task<AppMessage> {
    match message {
        Message::SetSubView(sub_view) => {
            app.rules.sub_view = sub_view;
        }
        Message::ToggleEventExpanded(rule_id, event_index) => {
            let key = (rule_id, event_index);
            if !app.rules.expanded_events.remove(&key) {
                app.rules.expanded_events.insert(key);
            }
        }
        Message::ToggleFilterExpanded(rule_id, event_index) => {
            let key = (rule_id, event_index);
            if !app.rules.expanded_filters.remove(&key) {
                app.rules.expanded_filters.insert(key);
            }
        }
        Message::SetActiveProfile(profile_id) => {
            app.apply_manual_profile_selection(profile_id);
        }
        Message::CreateProfile => {
            let new_id = next_profile_id(app);
            let enabled_rule_ids = app
                .active_profile()
                .map(|profile| profile.enabled_rule_ids.clone())
                .unwrap_or_default();
            app.config.rule_profiles.push(RuleProfile {
                id: new_id.clone(),
                name: format!("Profile {}", app.config.rule_profiles.len() + 1),
                enabled_rule_ids,
            });
            app.config.active_profile_id = new_id;
            persist(app);
            app.notify_active_profile_activated();
        }
        Message::DuplicateActiveProfile => {
            if let Some(profile) = app.active_profile().cloned() {
                let new_id = next_profile_id(app);
                app.config.rule_profiles.push(RuleProfile {
                    id: new_id.clone(),
                    name: format!("{} Copy", profile.name),
                    enabled_rule_ids: profile.enabled_rule_ids,
                });
                app.config.active_profile_id = new_id;
                persist(app);
                app.notify_active_profile_activated();
            }
        }
        Message::DeleteActiveProfile => {
            if app.config.rule_profiles.len() > 1 {
                app.config
                    .rule_profiles
                    .retain(|profile| profile.id != app.config.active_profile_id);
                app.config.active_profile_id = app
                    .config
                    .rule_profiles
                    .first()
                    .map(|profile| profile.id.clone())
                    .unwrap_or_default();
                persist(app);
                app.notify_active_profile_activated();
            }
        }
        Message::ExportActiveProfile => {
            let Some(profile) = app.active_profile().cloned() else {
                app.set_rules_feedback("No active profile is available to export.", true);
                return iced::Task::none();
            };
            let bundle = ProfileTransferBundle::from_profiles(
                std::slice::from_ref(&profile),
                &app.config.rule_definitions,
            );
            let suggested_path = default_profile_export_path(app, &profile.name);

            return iced::Task::perform(
                async move {
                    let Some(path) = super::settings::save_file(
                        suggested_path,
                        "Export NaniteClip profile".into(),
                    )
                    .await?
                    else {
                        return Ok(None);
                    };

                    let contents = bundle.to_toml_string()?;
                    tokio::fs::write(&path, contents)
                        .await
                        .map_err(|error| format!("Failed to write {}: {error}", path))?;
                    Ok(Some(path))
                },
                |result| AppMessage::Rules(Message::ActiveProfileExported(result)),
            );
        }
        Message::ActiveProfileExported(result) => match result {
            Ok(Some(path)) => {
                app.set_rules_feedback(format!("Exported active profile to {path}"), false)
            }
            Ok(None) => {}
            Err(error) => app.set_rules_feedback(error, true),
        },
        Message::ImportProfiles => {
            let initial_path = default_profile_import_path(app);
            let existing_profiles = app.config.rule_profiles.clone();
            let existing_rules = app.config.rule_definitions.clone();

            return iced::Task::perform(
                async move {
                    let Some(path) = super::settings::pick_toml_file(
                        initial_path,
                        "Import NaniteClip profiles".into(),
                    )
                    .await?
                    else {
                        return Ok(None);
                    };

                    let contents = tokio::fs::read_to_string(&path)
                        .await
                        .map_err(|error| format!("Failed to read {}: {error}", path))?;
                    let bundle = ProfileTransferBundle::from_toml(&contents)?;
                    let conflicts = bundle.detect_conflicts(&existing_profiles, &existing_rules);
                    Ok(Some(PendingProfileImport {
                        source_path: path,
                        bundle,
                        conflicts,
                    }))
                },
                |result| AppMessage::Rules(Message::ProfilesImportPrepared(result)),
            );
        }
        Message::ProfilesImportPrepared(result) => match result {
            Ok(Some(pending)) if pending.conflicts.is_empty() => {
                apply_profile_import(app, pending, false);
            }
            Ok(Some(pending)) => {
                app.rules.pending_profile_import_shake_started_at = None;
                app.rules.pending_profile_import = Some(pending);
            }
            Ok(None) => {}
            Err(error) => app.set_rules_feedback(error, true),
        },
        Message::ConfirmProfileImportOverwrite => {
            let Some(pending) = app.rules.pending_profile_import.take() else {
                return iced::Task::none();
            };
            app.rules.pending_profile_import_shake_started_at = None;
            apply_profile_import(app, pending, true);
        }
        Message::CancelProfileImportOverwrite => {
            app.rules.pending_profile_import = None;
            app.rules.pending_profile_import_shake_started_at = None;
        }
        Message::RequireProfileImportDecision => {
            app.rules.pending_profile_import_shake_started_at = Some(std::time::Instant::now());
        }
        Message::ExportSelectedRule => {
            let Some(rule) = selected_rule(app).cloned() else {
                app.set_rules_feedback("No selected rule is available to export.", true);
                return iced::Task::none();
            };
            let bundle = RuleTransferBundle::from_rules(std::slice::from_ref(&rule));
            let suggested_path = default_rule_export_path(app, &rule.name);

            return iced::Task::perform(
                async move {
                    let Some(path) =
                        super::settings::save_file(suggested_path, "Export NaniteClip rule".into())
                            .await?
                    else {
                        return Ok(None);
                    };

                    let contents = bundle.to_toml_string()?;
                    tokio::fs::write(&path, contents)
                        .await
                        .map_err(|error| format!("Failed to write {}: {error}", path))?;
                    Ok(Some(path))
                },
                |result| AppMessage::Rules(Message::SelectedRuleExported(result)),
            );
        }
        Message::SelectedRuleExported(result) => match result {
            Ok(Some(path)) => {
                app.push_success_toast("Rules", format!("Exported selected rule to {path}"), true)
            }
            Ok(None) => {}
            Err(error) => app.set_rules_feedback(error, true),
        },
        Message::ImportRules => {
            let initial_path = default_rule_import_path(app);
            let existing_rules = app.config.rule_definitions.clone();

            return iced::Task::perform(
                async move {
                    let Some(path) = super::settings::pick_toml_file(
                        initial_path,
                        "Import NaniteClip rules".into(),
                    )
                    .await?
                    else {
                        return Ok(None);
                    };

                    let contents = tokio::fs::read_to_string(&path)
                        .await
                        .map_err(|error| format!("Failed to read {}: {error}", path))?;
                    let bundle = RuleTransferBundle::from_toml(&contents)?;
                    let conflicts = bundle.detect_conflicts(&existing_rules);
                    Ok(Some(PendingRuleImport {
                        source_path: path,
                        bundle,
                        conflicts,
                    }))
                },
                |result| AppMessage::Rules(Message::RulesImportPrepared(result)),
            );
        }
        Message::RulesImportPrepared(result) => match result {
            Ok(Some(pending)) if pending.conflicts.is_empty() => {
                apply_rule_import(app, pending, false);
            }
            Ok(Some(pending)) => {
                app.rules.pending_rule_import_shake_started_at = None;
                app.rules.pending_rule_import = Some(pending);
            }
            Ok(None) => {}
            Err(error) => app.set_rules_feedback(error, true),
        },
        Message::ConfirmRuleImportOverwrite => {
            let Some(pending) = app.rules.pending_rule_import.take() else {
                return iced::Task::none();
            };
            app.rules.pending_rule_import_shake_started_at = None;
            apply_rule_import(app, pending, true);
        }
        Message::CancelRuleImportOverwrite => {
            app.rules.pending_rule_import = None;
            app.rules.pending_rule_import_shake_started_at = None;
        }
        Message::RequireRuleImportDecision => {
            app.rules.pending_rule_import_shake_started_at = Some(std::time::Instant::now());
        }
        Message::RenameActiveProfile(name) => {
            if let Some(index) = app.active_profile_index() {
                app.config.rule_profiles[index].name =
                    name_or_default_preserving_whitespace(&name, "Profile");
                persist(app);
            }
        }
        Message::ToggleRule(rule_id, enabled) => {
            if let Some(index) = app.active_profile_index() {
                let enabled_ids = &mut app.config.rule_profiles[index].enabled_rule_ids;
                if enabled {
                    if !enabled_ids.iter().any(|id| id == &rule_id) {
                        enabled_ids.push(rule_id);
                    }
                } else {
                    enabled_ids.retain(|id| id != &rule_id);
                }
                persist(app);
            }
        }
        Message::SelectRule(rule_id) => {
            app.rules.selected_rule_id = Some(rule_id);
            ensure_selection(app);
        }
        Message::CreateRule => {
            let new_id = next_rule_id(app);
            app.config.rule_definitions.push(blank_rule_definition(
                &new_id,
                &format!("Rule {}", app.config.rule_definitions.len() + 1),
            ));
            app.rules.selected_rule_id = Some(new_id);
            persist(app);
        }
        Message::DuplicateRule => {
            if let Some(index) = selected_rule_index(app) {
                let mut rule = app.config.rule_definitions[index].clone();
                rule.id = next_rule_id(app);
                rule.name = format!("{} Copy", rule.name);
                app.config.rule_definitions.push(rule.clone());
                app.rules.selected_rule_id = Some(rule.id);
                persist(app);
            }
        }
        Message::DeleteRule => {
            if app.config.rule_definitions.len() > 1
                && let Some(rule_id) = app.rules.selected_rule_id.clone()
            {
                app.config
                    .rule_definitions
                    .retain(|rule| rule.id != rule_id);
                for profile in &mut app.config.rule_profiles {
                    profile.enabled_rule_ids.retain(|id| id != &rule_id);
                }
                clear_rule_filter_ui_state(app, &rule_id);
                app.rules.selected_rule_id = None;
                persist(app);
            }
        }
        Message::RenameRule(name) => {
            if let Some(index) = selected_rule_index(app) {
                app.config.rule_definitions[index].name =
                    name_or_default_preserving_whitespace(&name, "Rule");
                persist(app);
            }
        }
        Message::ToggleFullBuffer(value) => {
            if let Some(index) = selected_rule_index(app) {
                app.config.rule_definitions[index].use_full_buffer = value;
                persist(app);
            }
        }
        Message::ToggleCaptureEntireBaseCap(value) => {
            if let Some(index) = selected_rule_index(app) {
                app.config.rule_definitions[index].capture_entire_base_cap = value;
                persist(app);
            }
        }
        Message::ToggleAutoExtend(value) => {
            if let Some(index) = selected_rule_index(app) {
                let extension = &mut app.config.rule_definitions[index].extension;
                extension.mode = if value {
                    ClipExtensionMode::HoldUntilQuiet
                } else {
                    ClipExtensionMode::Disabled
                };
                persist(app);
            }
        }
        Message::ExtensionWindowStepped(delta) => {
            if let Some(index) = selected_rule_index(app) {
                let extension = &mut app.config.rule_definitions[index].extension;
                extension.window_secs = step_value(extension.window_secs, delta, 1, 1, 120);
                persist(app);
            }
        }
        Message::BaseDurationStepped(delta) => {
            if let Some(index) = selected_rule_index(app) {
                let rule = &mut app.config.rule_definitions[index];
                rule.base_duration_secs =
                    step_value(rule.base_duration_secs, delta, 5, 5, 3600).max(5);
                if rule.max_duration_secs < rule.base_duration_secs {
                    rule.max_duration_secs = rule.base_duration_secs;
                }
                persist(app);
            }
        }
        Message::LookbackStepped(delta) => {
            if let Some(index) = selected_rule_index(app) {
                let rule = &mut app.config.rule_definitions[index];
                rule.lookback_secs = step_value(rule.lookback_secs, delta, 1, 1, 600);
                persist(app);
            }
        }
        Message::TriggerThresholdStepped(delta) => {
            if let Some(index) = selected_rule_index(app) {
                let rule = &mut app.config.rule_definitions[index];
                rule.trigger_threshold = step_value(rule.trigger_threshold, delta, 1, 1, 10_000);
                if rule.reset_threshold >= rule.trigger_threshold {
                    rule.reset_threshold = rule.trigger_threshold.saturating_sub(1);
                }
                persist(app);
            }
        }
        Message::ResetThresholdStepped(delta) => {
            if let Some(index) = selected_rule_index(app) {
                let rule = &mut app.config.rule_definitions[index];
                let max_reset = rule.trigger_threshold.saturating_sub(1);
                rule.reset_threshold = step_value(rule.reset_threshold, delta, 1, 0, max_reset);
                persist(app);
            }
        }
        Message::ToggleCooldown(enabled) => {
            if let Some(index) = selected_rule_index(app) {
                app.config.rule_definitions[index].cooldown_secs = enabled.then_some(
                    app.config.rule_definitions[index]
                        .cooldown_secs
                        .unwrap_or(15),
                );
                persist(app);
            }
        }
        Message::CooldownStepped(delta) => {
            if let Some(index) = selected_rule_index(app) {
                let current = app.config.rule_definitions[index]
                    .cooldown_secs
                    .unwrap_or(15);
                app.config.rule_definitions[index].cooldown_secs =
                    Some(step_value(current, delta, 5, 5, 600));
                persist(app);
            }
        }
        Message::ActivationClassChanged(choice) => {
            if let Some(index) = selected_rule_index(app) {
                app.config.rule_definitions[index].activation_class = choice.into_option();
                persist(app);
            }
        }
        Message::SecondsPerPointStepped(delta) => {
            if let Some(index) = selected_rule_index(app) {
                let rule = &mut app.config.rule_definitions[index];
                rule.secs_per_point = step_value(rule.secs_per_point, delta, 1, 0, 120);
                persist(app);
            }
        }
        Message::MaxDurationStepped(delta) => {
            if let Some(index) = selected_rule_index(app) {
                let rule = &mut app.config.rule_definitions[index];
                rule.max_duration_secs = step_value(
                    rule.max_duration_secs,
                    delta,
                    5,
                    rule.base_duration_secs,
                    3600,
                );
                persist(app);
            }
        }
        Message::AddScoredEvent => {
            if let Some(index) = selected_rule_index(app) {
                let rule = &mut app.config.rule_definitions[index];
                rule.scored_events.push(ScoredEvent {
                    event: EventKind::Kill,
                    points: 1,
                    filters: ScoredEventFilters::default(),
                });
                persist(app);
            }
        }
        Message::DeleteScoredEvent(event_index) => {
            if let Some(index) = selected_rule_index(app) {
                let rule_id = app.config.rule_definitions[index].id.clone();
                let removed = {
                    let rule = &mut app.config.rule_definitions[index];
                    if rule.scored_events.len() > 1 && event_index < rule.scored_events.len() {
                        rule.scored_events.remove(event_index);
                        true
                    } else {
                        false
                    }
                };
                if removed {
                    clear_rule_filter_ui_state(app, &rule_id);
                    persist(app);
                }
            }
        }
        Message::ScoredEventKindChanged(event_index, event_kind) => {
            if let Some(index) = selected_rule_index(app) {
                let rule = &mut app.config.rule_definitions[index];
                if event_index < rule.scored_events.len() {
                    rule.scored_events[event_index].event = event_kind;
                    persist(app);
                }
            }
        }
        Message::ScoredEventPointsStepped(event_index, delta) => {
            if let Some(index) = selected_rule_index(app) {
                let rule = &mut app.config.rule_definitions[index];
                if let Some(scored_event) = rule.scored_events.get_mut(event_index) {
                    scored_event.points = step_value(scored_event.points, delta, 1, 1, 1_000);
                    persist(app);
                }
            }
        }
        Message::StartScoredEventDrag(source_index) => {
            let Some(rule) = selected_rule(app) else {
                return iced::Task::none();
            };
            if source_index < rule.scored_events.len() {
                app.rules.drag_state = Some(RuleDragState::ScoredEvent(ScoredEventDragState {
                    rule_id: rule.id.clone(),
                    source_index,
                    target_index: source_index,
                    list_len: rule.scored_events.len(),
                }));
            }
        }
        Message::HoverScoredEventDrop(target_index) => {
            if let Some(RuleDragState::ScoredEvent(drag)) = &mut app.rules.drag_state {
                drag.target_index = target_index.min(drag.list_len);
            }
        }
        Message::ToggleScoredEventFilters(event_index, enabled) => {
            if let Some(index) = selected_rule_index(app) {
                let rule = &mut app.config.rule_definitions[index];
                if let Some(scored_event) = rule.scored_events.get_mut(event_index) {
                    scored_event.filters.set_enabled(enabled);
                    if enabled {
                        ensure_filter_group_exists(&mut scored_event.filters);
                    }
                    persist(app);
                }
            }
        }
        Message::AddScoredEventFilterGroup(event_index) => {
            if let Some(filters) = scored_event_filters_mut(app, event_index) {
                filters.groups.push(default_filter_group());
                persist(app);
            }
        }
        Message::DeleteScoredEventFilterGroup(event_index, group_index) => {
            if let Some(rule_id) = selected_rule(app).map(|rule| rule.id.clone())
                && let Some(filters) = scored_event_filters_mut(app, event_index)
                && group_index < filters.groups.len()
            {
                filters.groups.remove(group_index);
                clear_rule_filter_ui_state(app, &rule_id);
                persist(app);
            }
        }
        Message::AddScoredEventFilterClause(event_index, group_index) => {
            if let Some(filters) = scored_event_filters_mut(app, event_index)
                && let Some(group) = filters.groups.get_mut(group_index)
            {
                group.clauses.push(default_filter_clause());
                persist(app);
            }
        }
        Message::AddNestedOrClause(path) => {
            if let Some(ScoredEventFilterClause::Any { clauses }) = filter_clause_mut(app, &path) {
                clauses.push(default_filter_clause());
                persist(app);
            }
        }
        Message::DeleteScoredEventFilterClause(path) => {
            if let Some(rule_id) = selected_rule(app).map(|rule| rule.id.clone())
                && delete_filter_clause(app, &path)
            {
                let remove_group = scored_event_filters_mut(app, path.event_index)
                    .and_then(|filters| filters.groups.get(path.group_index))
                    .is_some_and(|group| group.clauses.is_empty());
                if remove_group
                    && let Some(filters) = scored_event_filters_mut(app, path.event_index)
                    && path.group_index < filters.groups.len()
                {
                    filters.groups.remove(path.group_index);
                }
                clear_rule_filter_ui_state(app, &rule_id);
                persist(app);
            }
        }
        Message::ScoredEventFilterClauseKindChanged(path, choice) => {
            if let Some(rule_id) = selected_rule(app).map(|rule| rule.id.clone())
                && let Some(clause) = filter_clause_mut(app, &path)
            {
                *clause = choice.default_clause();
                clear_rule_filter_ui_state(app, &rule_id);
                persist(app);
            }
        }
        Message::StartFilterClauseDrag(path) => {
            let Some(list_path) = FilterClauseListPath::from_clause_path(&path) else {
                return iced::Task::none();
            };
            let Some(source_index) = path.clause_path.last().copied() else {
                return iced::Task::none();
            };
            let Some(list_len) = filter_clause_list_len(app, &list_path) else {
                return iced::Task::none();
            };
            app.rules.drag_state = Some(RuleDragState::FilterClause(FilterClauseDragState {
                source_path: path,
                list_path,
                target_index: source_index,
                list_len,
            }));
        }
        Message::HoverFilterClauseDrop(list_path, target_index) => {
            if let Some(RuleDragState::FilterClause(drag)) = &mut app.rules.drag_state
                && drag.list_path == list_path
            {
                drag.target_index = target_index.min(drag.list_len);
            }
        }
        Message::RuleFilterDraftChanged(key, value) => {
            if value.trim().is_empty() {
                app.rules.filter_text_drafts.remove(&key);
            } else {
                app.rules.filter_text_drafts.insert(key, value);
            }
        }
        Message::ResolveRuleFilterDraft(key) => {
            let Some(input) = app
                .rules
                .filter_text_drafts
                .get(&key)
                .map(|value| value.trim().to_string())
            else {
                return clear_filter_clause_resolution(app, &key);
            };

            if input.is_empty() {
                return clear_filter_clause_resolution(app, &key);
            }

            let store = app.clip_store.clone();
            let service_id = app.config.service_id.clone();
            return iced::Task::perform(
                async move {
                    match key.field {
                        FilterTextField::TargetCharacter => {
                            if let Some(store) = &store
                                && let Some((lookup_id, display_name)) = store
                                    .find_lookup_by_name(LookupKind::Character, &input)
                                    .await
                                    .map_err(|error| error.to_string())?
                            {
                                return Ok(ResolvedFilterReference::Character(
                                    CharacterReferenceFilter {
                                        name: Some(display_name),
                                        character_id: Some(lookup_id as u64),
                                    },
                                ));
                            }

                            if service_id.trim().is_empty() {
                                return Err(
                                    "Set a Census service id before resolving character filters."
                                        .into(),
                                );
                            }

                            let resolved = census::resolve_character_reference(&service_id, &input)
                                .await
                                .map_err(|error| error.to_string())?
                                .ok_or_else(|| format!("Character `{input}` was not found."))?;

                            if let Some(store) = &store {
                                store
                                    .store_lookup(
                                        LookupKind::Character,
                                        resolved.id,
                                        &resolved.display_name,
                                    )
                                    .await
                                    .map_err(|error| error.to_string())?;
                            }

                            Ok(ResolvedFilterReference::Character(
                                CharacterReferenceFilter {
                                    name: Some(resolved.display_name),
                                    character_id: Some(resolved.id as u64),
                                },
                            ))
                        }
                        FilterTextField::TargetOutfit => {
                            if let Some(store) = &store
                                && let Some((lookup_id, display_name)) = store
                                    .find_lookup_by_name(LookupKind::Outfit, &input)
                                    .await
                                    .map_err(|error| error.to_string())?
                            {
                                return Ok(ResolvedFilterReference::Outfit(
                                    OutfitReferenceFilter {
                                        tag: Some(display_name),
                                        outfit_id: Some(lookup_id as u64),
                                    },
                                ));
                            }

                            if service_id.trim().is_empty() {
                                return Err(
                                    "Set a Census service id before resolving outfit filters."
                                        .into(),
                                );
                            }

                            let resolved = census::resolve_outfit_reference(&service_id, &input)
                                .await
                                .map_err(|error| error.to_string())?
                                .ok_or_else(|| format!("Outfit tag `{input}` was not found."))?;

                            if let Some(store) = &store {
                                store
                                    .store_lookup(LookupKind::Outfit, resolved.id, &resolved.tag)
                                    .await
                                    .map_err(|error| error.to_string())?;
                            }

                            Ok(ResolvedFilterReference::Outfit(OutfitReferenceFilter {
                                tag: Some(resolved.tag),
                                outfit_id: Some(resolved.id as u64),
                            }))
                        }
                    }
                },
                move |result| AppMessage::Rules(Message::RuleFilterDraftResolved { key, result }),
            );
        }
        Message::RuleFilterDraftResolved { key, result } => match result {
            Ok(resolved) => {
                if let Some(clause) = filter_clause_mut(app, &key.path) {
                    apply_resolved_filter_reference(clause, &resolved);
                    app.rules
                        .filter_text_drafts
                        .insert(key.clone(), resolved.display_value().to_string());
                    persist(app);
                    app.set_rules_feedback(resolved.success_message(), false);
                }
            }
            Err(error) => app.set_rules_feedback(error, true),
        },
        Message::OpenHonuReference(url) => {
            let platform = app.platform.clone();
            return iced::Task::perform(async move { platform.open_url(&url) }, |result| {
                AppMessage::Rules(Message::HonuReferenceOpened(result))
            });
        }
        Message::HonuReferenceOpened(result) => {
            if let Err(error) = result {
                app.set_rules_feedback(format!("Failed to open Honu: {error}"), true);
            }
        }
        Message::VehicleOptionsLoaded(result) => match result {
            Ok(options) => app.rules.vehicle_options = options,
            Err(error) => {
                tracing::warn!("Failed to load vehicle options: {error}");
                if app.rules.vehicle_options.is_empty() {
                    app.set_rules_feedback(
                        "Failed to load vehicle filter options from Census.",
                        true,
                    );
                }
            }
        },
        Message::WeaponOptionsLoaded(result) => match result {
            Ok(options) => app.rules.weapon_options = options,
            Err(error) => {
                tracing::warn!("Failed to load weapon options: {error}");
                if app.rules.weapon_options.is_empty() {
                    app.set_rules_feedback(
                        "Failed to load weapon filter options from Census.",
                        true,
                    );
                }
            }
        },
        Message::ScoredEventWeaponGroupChanged(path, group) => {
            let key = WeaponBrowseKey::new(
                &path.rule_id,
                path.event_index,
                path.group_index,
                &path.clause_path,
            );
            set_weapon_browse_group(app, key.clone(), group);
            app.rules.weapon_browse_categories.remove(&key);
            app.rules.weapon_browse_factions.remove(&key);
        }
        Message::ScoredEventVehicleCategoryChanged(path, category) => {
            set_vehicle_browse_category(
                app,
                VehicleBrowseKey::new(
                    &path.rule_id,
                    path.event_index,
                    path.group_index,
                    &path.clause_path,
                ),
                category,
            );
        }
        Message::ScoredEventWeaponCategoryChanged(path, category) => {
            let key = WeaponBrowseKey::new(
                &path.rule_id,
                path.event_index,
                path.group_index,
                &path.clause_path,
            );
            set_weapon_browse_category(app, key.clone(), category);
            app.rules.weapon_browse_factions.remove(&key);
        }
        Message::ScoredEventWeaponFactionChanged(path, faction) => {
            set_weapon_browse_faction(
                app,
                WeaponBrowseKey::new(
                    &path.rule_id,
                    path.event_index,
                    path.group_index,
                    &path.clause_path,
                ),
                faction,
            );
        }
        Message::ScoredEventVehicleChanged(path, choice) => {
            if let Some(clause) = filter_clause_mut(app, &path)
                && let Some(vehicle) = vehicle_match_filter_mut(clause)
            {
                apply_vehicle_filter_choice(vehicle, choice);
                persist(app);
            }
        }
        Message::ScoredEventWeaponChanged(path, choice) => {
            if let Some(clause) = filter_clause_mut(app, &path)
                && let Some(weapon) = weapon_match_filter_mut(clause)
            {
                apply_weapon_filter_choice(weapon, choice);
                persist(app);
            }
        }
        Message::CreateAutoSwitchRule => {
            let target_profile_id = app
                .config
                .rule_profiles
                .first()
                .map(|profile| profile.id.clone())
                .unwrap_or_default();
            let next_id = next_auto_switch_rule_id(app);
            app.config.auto_switch_rules.push(AutoSwitchRule {
                id: next_id.clone(),
                name: format!("Auto Switch {}", app.config.auto_switch_rules.len() + 1),
                enabled: true,
                target_profile_id,
                condition: AutoSwitchCondition::LocalSchedule {
                    weekdays: default_schedule_weekdays(),
                    start_hour: 18,
                    start_minute: 0,
                    end_hour: 23,
                    end_minute: 0,
                },
            });
            persist(app);
        }
        Message::DeleteAutoSwitchRule(rule_id) => {
            app.config
                .auto_switch_rules
                .retain(|rule| rule.id != rule_id);
            persist(app);
        }
        Message::ToggleAutoSwitchRule(rule_id, enabled) => {
            if let Some(rule) = auto_switch_rule_mut(app, &rule_id) {
                rule.enabled = enabled;
                persist(app);
            }
        }
        Message::RenameAutoSwitchRule(rule_id, name) => {
            if let Some(rule) = auto_switch_rule_mut(app, &rule_id) {
                rule.name = name_or_default_preserving_whitespace(&name, "Auto switch");
                persist(app);
            }
        }
        Message::AutoSwitchTargetProfileChanged(rule_id, profile_id) => {
            if let Some(rule) = auto_switch_rule_mut(app, &rule_id) {
                rule.target_profile_id = profile_id;
                persist(app);
            }
        }
        Message::AutoSwitchConditionChanged(rule_id, choice) => {
            let next_condition = app
                .config
                .auto_switch_rules
                .iter()
                .find(|rule| rule.id == rule_id)
                .map(|rule| auto_switch_condition_for_choice(app, choice, rule.condition.clone()));
            if let Some(next_condition) = next_condition
                && let Some(rule) = auto_switch_rule_mut(app, &rule_id)
            {
                rule.condition = next_condition;
                persist(app);
            }
        }
        Message::ToggleAutoSwitchCharacter(rule_id, character_id) => {
            if let Some(rule) = auto_switch_rule_mut(app, &rule_id)
                && let AutoSwitchCondition::ActiveCharacter {
                    character_ids,
                    character_id: legacy_character_id,
                } = &mut rule.condition
            {
                let mut ids = normalized_active_character_ids(character_ids, *legacy_character_id);
                if let Some(index) = ids.iter().position(|id| *id == character_id) {
                    ids.remove(index);
                } else {
                    ids.push(character_id);
                    ids.sort_unstable();
                    ids.dedup();
                }
                *character_ids = ids;
                *legacy_character_id = None;
                persist(app);
            }
        }
        Message::SetAutoSwitchAllCharacters(rule_id) => {
            let all_character_ids = tracked_character_options(app)
                .into_iter()
                .map(|character| character.id)
                .collect::<Vec<_>>();
            if let Some(rule) = auto_switch_rule_mut(app, &rule_id)
                && let AutoSwitchCondition::ActiveCharacter {
                    character_ids,
                    character_id: legacy_character_id,
                } = &mut rule.condition
            {
                *character_ids = all_character_ids;
                *legacy_character_id = None;
                persist(app);
            }
        }
        Message::ToggleAutoSwitchWeekday(rule_id, weekday) => {
            if let Some(rule) = auto_switch_rule_mut(app, &rule_id)
                && let AutoSwitchCondition::LocalSchedule { weekdays, .. } = &mut rule.condition
            {
                if let Some(index) = weekdays.iter().position(|day| *day == weekday) {
                    weekdays.remove(index);
                } else {
                    weekdays.push(weekday);
                    normalize_schedule_weekdays(weekdays);
                }
                persist(app);
            }
        }
        Message::SetAutoSwitchAllDays(rule_id) => {
            if let Some(rule) = auto_switch_rule_mut(app, &rule_id)
                && let AutoSwitchCondition::LocalSchedule { weekdays, .. } = &mut rule.condition
            {
                *weekdays = default_schedule_weekdays();
                persist(app);
            }
        }
        Message::AutoSwitchStartTimeStepped(rule_id, delta) => {
            if let Some(rule) = auto_switch_rule_mut(app, &rule_id)
                && let AutoSwitchCondition::LocalSchedule {
                    ref mut start_hour,
                    ref mut start_minute,
                    ..
                } = rule.condition
            {
                let next = wrap_schedule_slot(schedule_slot(*start_hour, *start_minute), delta, 47);
                (*start_hour, *start_minute) = slot_time(next);
                persist(app);
            }
        }
        Message::AutoSwitchEndTimeStepped(rule_id, delta) => {
            if let Some(rule) = auto_switch_rule_mut(app, &rule_id)
                && let AutoSwitchCondition::LocalSchedule {
                    ref mut end_hour,
                    ref mut end_minute,
                    ..
                } = rule.condition
            {
                let next = wrap_schedule_slot(schedule_slot(*end_hour, *end_minute), delta, 48);
                (*end_hour, *end_minute) = slot_time(next);
                persist(app);
            }
        }
        Message::RuleDragReleased => {
            if let Some(drag) = app.rules.drag_state.take() {
                match drag {
                    RuleDragState::ScoredEvent(drag) => {
                        if move_scored_event(app, &drag) {
                            persist(app);
                        }
                    }
                    RuleDragState::FilterClause(drag) => {
                        if move_filter_clause(app, &drag) {
                            persist(app);
                        }
                    }
                }
            }
        }
        Message::ResumeAutoSwitching => {
            app.resume_auto_switching();
            app.clear_rules_feedback();
        }
    }
    iced::Task::none()
}

pub(in crate::app) fn view(app: &App) -> Element<'_, Message> {
    let profile_options: Vec<ProfileOption> = app
        .config
        .rule_profiles
        .iter()
        .map(ProfileOption::from_profile)
        .collect();

    let sub_view_tabs = tabs(app.rules.sub_view, Message::SetSubView)
        .push(Tab::new(RulesSubView::Rules, "Rules"))
        .push(
            Tab::new(RulesSubView::Profiles, "Profiles & Scheduling").badge(format!(
                "{}",
                app.config.rule_profiles.len() + app.config.auto_switch_rules.len()
            )),
        )
        .build();

    let header = page_header("Rules")
        .subtitle("")
        .action(sub_view_tabs)
        .build();

    let body: Element<'_, Message> = match app.rules.sub_view {
        RulesSubView::Rules => rules_split_view(app),
        RulesSubView::Profiles => profiles_view(app, &profile_options),
    };

    let content = column![header, body].spacing(12);
    let base: Element<'_, Message> = mouse_area(content)
        .on_release(Message::RuleDragReleased)
        .into();

    if let Some(pending) = &app.rules.pending_rule_import {
        modal_with_backdrop_action(
            base,
            rule_import_overwrite_dialog(app, pending),
            Some(Message::RequireRuleImportDecision),
            import_decision_shake_offset(app.rules.pending_rule_import_shake_started_at),
        )
    } else if let Some(pending) = &app.rules.pending_profile_import {
        modal_with_backdrop_action(
            base,
            profile_import_overwrite_dialog(app, pending),
            Some(Message::RequireProfileImportDecision),
            import_decision_shake_offset(app.rules.pending_profile_import_shake_started_at),
        )
    } else {
        base
    }
}

// ---------------------------------------------------------------------------
// Rules sub-view: sidebar + editor split
// ---------------------------------------------------------------------------

fn rules_split_view(app: &App) -> Element<'_, Message> {
    let selected_rule_id = app.rules.selected_rule_id.clone().unwrap_or_default();

    // -- Sidebar: compact profile picker + rule list with toggles --
    let profile_options: Vec<ProfileOption> = app
        .config
        .rule_profiles
        .iter()
        .map(ProfileOption::from_profile)
        .collect();
    let selected_profile = app.active_profile().map(ProfileOption::from_profile);

    let profile_picker = column![
        text("Active Profile").size(11),
        pick_list(profile_options, selected_profile, |option| {
            Message::SetActiveProfile(option.id)
        })
        .width(Length::Fill),
    ]
    .spacing(4);

    let primary_actions_row = row![
        with_tooltip(
            styled_button_row(icon_label("plus", "New"), ButtonTone::Success)
                .on_press(Message::CreateRule)
                .into(),
            "Create rule.",
        ),
        with_tooltip(
            styled_button_row(icon_label("copy", "Dupe"), ButtonTone::Secondary)
                .on_press(Message::DuplicateRule)
                .into(),
            "Duplicate rule.",
        ),
        with_tooltip(
            styled_button_row(icon_label("trash", "Del"), ButtonTone::Danger)
                .on_press(Message::DeleteRule)
                .into(),
            "Delete rule.",
        ),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);

    let transfer_actions_row = row![
        with_tooltip(
            styled_button_row(icon_label("upload", "Export"), ButtonTone::Secondary)
                .on_press(Message::ExportSelectedRule)
                .into(),
            "Export as TOML.",
        ),
        with_tooltip(
            styled_button_row(icon_label("download", "Import"), ButtonTone::Primary)
                .on_press(Message::ImportRules)
                .into(),
            "Import from TOML.",
        ),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);

    let mut rule_sidebar = sidebar(selected_rule_id, Message::SelectRule)
        .width(260.0)
        .header(column![profile_picker, primary_actions_row, transfer_actions_row].spacing(8));

    for rule_def in &app.config.rule_definitions {
        let enabled = app
            .active_profile()
            .is_some_and(|profile| profile.enables(&rule_def.id));
        let badge_text = format!(
            "{} ev \u{00B7} {}",
            rule_def.scored_events.len(),
            if enabled { "on" } else { "off" }
        );
        rule_sidebar = rule_sidebar
            .push(SidebarItem::new(rule_def.id.clone(), &rule_def.name).badge(badge_text));
    }

    let sidebar_element: Element<'_, Message> =
        scrollable(rule_sidebar.build()).height(Length::Fill).into();

    let detail_pane = scrollable(
        container(rule_editor_panel(app))
            .width(Length::Fill)
            .padding([8, 16]),
    )
    .height(Length::Fill);

    row![sidebar_element, detail_pane]
        .spacing(0)
        .height(Length::Fill)
        .into()
}

// ---------------------------------------------------------------------------
// Profiles & Scheduling sub-view
// ---------------------------------------------------------------------------

fn profiles_view<'a>(app: &'a App, profile_options: &[ProfileOption]) -> Element<'a, Message> {
    let body = column![
        profiles_panel(app, profile_options),
        auto_switch_panel(app, profile_options),
    ]
    .spacing(16);

    scrollable(container(body).width(Length::Fill).padding([8, 16]))
        .height(Length::Fill)
        .into()
}

fn rules_badge<'a>(label: impl Into<String>, tone: BadgeTone) -> Element<'a, Message> {
    badge(label).tone(tone).build().into()
}

// ---------------------------------------------------------------------------
// Profiles panel
// ---------------------------------------------------------------------------

fn profiles_panel<'a>(app: &'a App, profile_options: &[ProfileOption]) -> Element<'a, Message> {
    let selected_profile = app.active_profile().map(ProfileOption::from_profile);

    let mut profile_section = section("Active Profile").description("").push(
        row![
            field_label("Profile", "", 200.0),
            pick_list(profile_options.to_vec(), selected_profile, |option| {
                Message::SetActiveProfile(option.id)
            })
            .width(280),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    );

    if let Some(profile) = app.active_profile() {
        profile_section = profile_section.push(settings_text_field(
            "Profile Name",
            "",
            &profile.name,
            Message::RenameActiveProfile,
        ));
    }

    profile_section = profile_section.push(
        row![
            with_tooltip(
                styled_button("New Profile", ButtonTone::Success)
                    .on_press(Message::CreateProfile)
                    .into(),
                "Create profile.",
            ),
            with_tooltip(
                styled_button("Duplicate", ButtonTone::Secondary)
                    .on_press(Message::DuplicateActiveProfile)
                    .into(),
                "Duplicate profile.",
            ),
            with_tooltip(
                styled_button("Delete", ButtonTone::Danger)
                    .on_press(Message::DeleteActiveProfile)
                    .into(),
                "Delete profile.",
            ),
            with_tooltip(
                styled_button("Export Active", ButtonTone::Secondary)
                    .on_press(Message::ExportActiveProfile)
                    .into(),
                "Export as TOML.",
            ),
            with_tooltip(
                styled_button("Import", ButtonTone::Primary)
                    .on_press(Message::ImportProfiles)
                    .into(),
                "Import from TOML.",
            ),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    );

    panel("Profiles")
        .push(profile_section.build())
        .build()
        .into()
}

// ---------------------------------------------------------------------------
// Auto-switch panel
// ---------------------------------------------------------------------------

fn auto_switch_panel<'a>(app: &'a App, profile_options: &[ProfileOption]) -> Element<'a, Message> {
    let mut content = panel("Automatic Profile Switching").description("");

    if let Some(override_name) = app.manual_profile_override_name() {
        content = content.push(
            banner(format!(
                "Auto-switching paused — \"{override_name}\" is pinned."
            ))
            .warning()
            .description("")
            .build(),
        );
        content = content.push(with_tooltip(
            styled_button("Resume Auto-Switching", ButtonTone::Primary)
                .on_press(Message::ResumeAutoSwitching)
                .into(),
            "Unpins the active profile.",
        ));
    }

    if app.config.auto_switch_rules.is_empty() {
        content = content.push(
            empty_state("No auto-switch rules")
                .description("")
                .action(
                    styled_button("Add Auto-Switch Rule", ButtonTone::Success)
                        .on_press(Message::CreateAutoSwitchRule),
                )
                .build(),
        );
    } else {
        content = content.push(
            styled_button("Add Auto-Switch Rule", ButtonTone::Success)
                .on_press(Message::CreateAutoSwitchRule),
        );
        for auto_rule in &app.config.auto_switch_rules {
            content = content.push(auto_switch_rule_card(app, auto_rule, profile_options));
        }
    }

    content.build().into()
}

fn auto_switch_rule_card<'a>(
    app: &'a App,
    auto_rule: &'a AutoSwitchRule,
    profile_options: &[ProfileOption],
) -> Element<'a, Message> {
    let rule_id = auto_rule.id.clone();
    let selected_profile = app
        .config
        .rule_profiles
        .iter()
        .find(|profile| profile.id == auto_rule.target_profile_id)
        .map(ProfileOption::from_profile);
    let character_options = tracked_character_options(app);
    let selected_character_ids = match &auto_rule.condition {
        AutoSwitchCondition::ActiveCharacter {
            character_ids,
            character_id,
        } => normalized_active_character_ids(character_ids, *character_id),
        _ => Vec::new(),
    };
    let selected_condition = AutoSwitchConditionChoice::from_condition(&auto_rule.condition);

    let header_row = row![
        toggle_switch(auto_rule.enabled)
            .label(&auto_rule.name)
            .on_toggle({
                let rule_id = rule_id.clone();
                move |enabled| Message::ToggleAutoSwitchRule(rule_id.clone(), enabled)
            }),
        rules_badge(
            selected_condition.to_string(),
            if auto_rule.enabled {
                BadgeTone::Primary
            } else {
                BadgeTone::Neutral
            },
        ),
        iced::widget::Space::new().width(Length::Fill),
        with_tooltip(
            styled_button("Delete", ButtonTone::Danger)
                .on_press(Message::DeleteAutoSwitchRule(rule_id.clone()))
                .into(),
            "Delete rule.",
        ),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    let mut body = column![
        header_row,
        settings_text_field("Rule Name", "", &auto_rule.name, {
            let rule_id = rule_id.clone();
            move |value| Message::RenameAutoSwitchRule(rule_id.clone(), value)
        }),
        row![
            field_label("Target Profile", "", 200.0,),
            pick_list(profile_options.to_vec(), selected_profile, {
                let rule_id = rule_id.clone();
                move |profile| Message::AutoSwitchTargetProfileChanged(rule_id.clone(), profile.id)
            })
            .width(280),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
        row![
            field_label("Condition", "", 200.0,),
            pick_list(
                &AutoSwitchConditionChoice::ALL[..],
                Some(selected_condition),
                {
                    let rule_id = rule_id.clone();
                    move |choice| Message::AutoSwitchConditionChanged(rule_id.clone(), choice)
                },
            )
            .width(220),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(8);

    match &auto_rule.condition {
        AutoSwitchCondition::ActiveCharacter { .. } => {
            let all_characters_selected = !character_options.is_empty()
                && selected_character_ids.len() == character_options.len();
            body = body.push(
                row![
                    field_label("Active Characters", "", 200.0,),
                    if character_options.is_empty() {
                        Element::<Message>::from(
                            text("Add a character first.").size(13).width(Length::Fill),
                        )
                    } else {
                        row![
                            styled_button(
                                if all_characters_selected {
                                    "All Characters"
                                } else {
                                    "Select All"
                                },
                                if all_characters_selected {
                                    ButtonTone::Primary
                                } else {
                                    ButtonTone::Secondary
                                },
                            )
                            .on_press(Message::SetAutoSwitchAllCharacters(rule_id.clone())),
                            row(character_options.iter().map(|character| {
                                let selected = selected_character_ids.contains(&character.id);
                                styled_button(
                                    &character.name,
                                    if selected {
                                        ButtonTone::Primary
                                    } else {
                                        ButtonTone::Secondary
                                    },
                                )
                                .on_press(Message::ToggleAutoSwitchCharacter(
                                    rule_id.clone(),
                                    character.id,
                                ))
                                .into()
                            }))
                            .spacing(4),
                        ]
                        .spacing(8)
                        .into()
                    },
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            );
        }
        AutoSwitchCondition::LocalSchedule {
            weekdays,
            start_hour,
            start_minute,
            end_hour,
            end_minute,
        } => {
            let all_days_selected = weekdays.len() == ScheduleWeekday::ALL.len();
            body = body.push(
                row![
                    field_label("Schedule Days", "", 200.0,),
                    styled_button(
                        if all_days_selected {
                            "All Days"
                        } else {
                            "Select All"
                        },
                        if all_days_selected {
                            ButtonTone::Primary
                        } else {
                            ButtonTone::Secondary
                        },
                    )
                    .on_press(Message::SetAutoSwitchAllDays(rule_id.clone())),
                    row(ScheduleWeekday::ALL.into_iter().map(|weekday| {
                        let selected = weekdays.contains(&weekday);
                        styled_button(
                            weekday.short_label(),
                            if selected {
                                ButtonTone::Primary
                            } else {
                                ButtonTone::Secondary
                            },
                        )
                        .on_press(Message::ToggleAutoSwitchWeekday(rule_id.clone(), weekday))
                        .into()
                    }))
                    .spacing(4),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            );
            body = body.push(
                row![
                    field_label("Time Window", "", 200.0,),
                    styled_button("-", ButtonTone::Secondary)
                        .on_press(Message::AutoSwitchStartTimeStepped(rule_id.clone(), -1,)),
                    text(format!(
                        "Start {}",
                        format_schedule_time(*start_hour, *start_minute)
                    ))
                    .width(110),
                    styled_button("+", ButtonTone::Secondary)
                        .on_press(Message::AutoSwitchStartTimeStepped(rule_id.clone(), 1,)),
                    styled_button("-", ButtonTone::Secondary)
                        .on_press(Message::AutoSwitchEndTimeStepped(rule_id.clone(), -1,)),
                    text(format!(
                        "End {}",
                        format_schedule_time(*end_hour, *end_minute)
                    ))
                    .width(110),
                    styled_button("+", ButtonTone::Secondary)
                        .on_press(Message::AutoSwitchEndTimeStepped(rule_id.clone(), 1)),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            );
        }
        AutoSwitchCondition::LocalTimeRange { .. }
        | AutoSwitchCondition::LocalCron { .. }
        | AutoSwitchCondition::OnEvent { .. } => {}
    }

    body = body.push(text(auto_rule.condition.summary()).size(12));

    card().body(body).width(Length::Fill).into()
}

// ---------------------------------------------------------------------------
// Rule editor panel
// ---------------------------------------------------------------------------

fn rule_editor_panel(app: &App) -> Element<'_, Message> {
    let Some(rule) = selected_rule(app) else {
        return empty_state("No rule selected")
            .description("Select a rule from the sidebar.")
            .build()
            .into();
    };

    let enabled = app
        .active_profile()
        .is_some_and(|profile| profile.enables(&rule.id));

    // Header: enable toggle + name field
    let header_row = row![
        toggle_switch(enabled)
            .label(if enabled {
                "Enabled in active profile"
            } else {
                "Disabled in active profile"
            })
            .on_toggle({
                let rule_id = rule.id.clone();
                move |value| Message::ToggleRule(rule_id.clone(), value)
            }),
        iced::widget::Space::new().width(Length::Fill),
        rules_badge(
            format!(
                "{} events \u{00B7} {}s window \u{00B7} trigger {}",
                rule.scored_events.len(),
                rule.lookback_secs,
                rule.trigger_threshold
            ),
            if enabled {
                BadgeTone::Success
            } else {
                BadgeTone::Outline
            },
        ),
    ]
    .spacing(12)
    .align_y(iced::Alignment::Center);

    let mut editor = panel(&rule.name);

    editor = editor.push(header_row);

    // Scoring section
    editor = editor.push(
        section("Scoring")
            .push(settings_text_field(
                "Name",
                "",
                &rule.name,
                Message::RenameRule,
            ))
            .push(settings_pick_list_field(
                "Activation Class",
                "",
                &ClassFilterChoice::ALL[..],
                Some(ClassFilterChoice::from_option(rule.activation_class)),
                Message::ActivationClassChanged,
            ))
            .push(settings_stepper_field(
                "Lookback Window",
                "",
                rule.lookback_secs,
                "sec",
                Message::LookbackStepped,
            ))
            .push(settings_stepper_field(
                "Trigger Threshold",
                "",
                rule.trigger_threshold,
                "pts",
                Message::TriggerThresholdStepped,
            ))
            .push(settings_stepper_field(
                "Reset Threshold",
                "",
                rule.reset_threshold,
                "pts",
                Message::ResetThresholdStepped,
            ))
            .push(cooldown_row(rule))
            .build(),
    );

    // Clip duration section
    editor = editor.push(clip_formula_section(rule));

    // Validation feedback
    if let Some(error) = validate_rule(rule).err() {
        editor = editor.push(
            banner("Rule validation failed")
                .error()
                .description(error)
                .build(),
        );
    } else if let Some(feedback) = &app.rules.feedback {
        editor = editor.push(
            banner("Rule status")
                .info()
                .description(feedback.clone())
                .build(),
        );
    }

    // Scored events section
    editor = editor.push(scored_events_section(app, rule));

    // Live runtime section
    if let Some(status) = app.rule_engine.runtime_status(&rule.id, Utc::now()) {
        editor = editor.push(live_runtime_section(rule, status));
    }

    editor.build().into()
}

fn cooldown_row<'a>(rule: &RuleDefinition) -> Element<'a, Message> {
    row![
        field_label("Cooldown", "", 200.0,),
        toggle_switch(rule.cooldown_secs.is_some())
            .label(if rule.cooldown_secs.is_some() {
                "On"
            } else {
                "Off"
            })
            .on_toggle(Message::ToggleCooldown),
        styled_button("-", ButtonTone::Secondary).on_press(Message::CooldownStepped(-1)),
        text(
            rule.cooldown_secs
                .map(|secs| format!("{secs}s"))
                .unwrap_or_else(|| "Off".into())
        )
        .width(80),
        styled_button("+", ButtonTone::Secondary).on_press(Message::CooldownStepped(1)),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into()
}

fn clip_formula_section<'a>(rule: &RuleDefinition) -> Element<'a, Message> {
    let mut sec = section("Clip Duration");

    sec = sec.push(settings_stepper_field(
        "Base Duration",
        "",
        rule.base_duration_secs,
        "sec",
        Message::BaseDurationStepped,
    ));
    sec = sec.push(settings_stepper_field(
        "Seconds Per Point",
        "",
        rule.secs_per_point,
        "sec",
        Message::SecondsPerPointStepped,
    ));
    sec = sec.push(settings_stepper_field(
        "Max Duration",
        "",
        rule.max_duration_secs,
        "sec",
        Message::MaxDurationStepped,
    ));

    // Duration preview badge
    let preview_text = if rule.use_full_buffer {
        "Full buffer on every trigger".into()
    } else if rule.capture_entire_base_cap {
        "Full buffer on facility capture; score formula otherwise".into()
    } else {
        format!(
            "{}s base + {}s per point, capped at {}s",
            rule.base_duration_secs, rule.secs_per_point, rule.max_duration_secs
        )
    };
    sec = sec.push(rules_badge(preview_text, BadgeTone::Outline));

    // Overrides — toggles that bypass the score formula
    sec = sec.push(
        row![
            field_label("Full Buffer", "", 200.0,),
            toggle_switch(rule.use_full_buffer)
                .label(if rule.use_full_buffer { "On" } else { "Off" })
                .on_toggle(Message::ToggleFullBuffer),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    );

    sec = sec.push(
        row![
            field_label("Entire Base Cap", "", 200.0,),
            toggle_switch(rule.capture_entire_base_cap)
                .label(if rule.capture_entire_base_cap {
                    "On"
                } else {
                    "Off"
                })
                .on_toggle(Message::ToggleCaptureEntireBaseCap),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    );

    // Auto-extend — flattened inline
    sec = sec.push(
        row![
            field_label("Auto Extend", "", 200.0,),
            toggle_switch(rule.extension.is_enabled())
                .label(if rule.extension.is_enabled() {
                    "On"
                } else {
                    "Off"
                })
                .on_toggle(Message::ToggleAutoExtend),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    );

    if rule.extension.is_enabled() {
        sec = sec.push(settings_stepper_field(
            "Quiet Window",
            "",
            rule.extension.window_secs,
            "sec",
            Message::ExtensionWindowStepped,
        ));
        sec = sec.push(rules_badge(
            format!(
                "Finalizes {}s after last matching event",
                rule.extension.window_secs
            ),
            BadgeTone::Info,
        ));
    }

    sec.build().into()
}

fn scored_events_section<'a>(app: &'a App, rule: &'a RuleDefinition) -> Element<'a, Message> {
    let mut sec = section("Scored Events").description("");

    sec = sec.push(
        styled_button_row(icon_label("plus", "Add Event"), ButtonTone::Success)
            .on_press(Message::AddScoredEvent),
    );

    let mut events_col = column![].spacing(4);

    if let Some(drag) = active_scored_event_drag(app, &rule.id) {
        events_col = events_col.push(render_scored_event_drop_zone(drag, 0));
    }

    for (event_index, scored_event) in rule.scored_events.iter().enumerate() {
        let is_expanded = app
            .rules
            .expanded_events
            .contains(&(rule.id.clone(), event_index));
        let is_drag_source = active_scored_event_drag(app, &rule.id)
            .is_some_and(|drag| drag.source_index == event_index);

        let filter_summary = scored_event_filter_summary(
            &scored_event.filters,
            &app.rules.vehicle_options,
            &app.rules.weapon_options,
        );

        // Compact summary row (always shown)
        let summary_row = row![
            render_drag_handle(
                is_drag_source,
                Message::StartScoredEventDrag(event_index),
                "Drag to reorder.",
            ),
            with_tooltip(
                mouse_area(
                    row![
                        rules_badge(format!("#{}", event_index + 1), BadgeTone::Outline),
                        text(format!("{}", scored_event.event)).size(13),
                        rules_badge(format!("{} pts", scored_event.points), BadgeTone::Info),
                        if scored_event.filters.is_enabled() {
                            rules_badge(filter_summary.clone(), BadgeTone::Primary)
                        } else {
                            rules_badge("no filters", BadgeTone::Neutral)
                        },
                    ]
                    .spacing(6)
                    .align_y(iced::Alignment::Center),
                )
                .on_press(Message::ToggleEventExpanded(rule.id.clone(), event_index))
                .interaction(mouse::Interaction::Pointer)
                .into(),
                if is_expanded { "Collapse" } else { "Expand" },
            ),
            iced::widget::Space::new().width(Length::Fill),
            with_tooltip(
                styled_button(
                    if is_expanded { "\u{f077}" } else { "\u{f078}" },
                    ButtonTone::Secondary,
                )
                .on_press(Message::ToggleEventExpanded(rule.id.clone(), event_index))
                .into(),
                if is_expanded { "Collapse" } else { "Expand" },
            ),
            with_tooltip(
                styled_button_row(icon_label("trash", ""), ButtonTone::Danger)
                    .on_press(Message::DeleteScoredEvent(event_index))
                    .into(),
                "Delete event.",
            ),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center);

        let event_card = if is_expanded {
            // Expanded: full editor
            let filters_enabled = scored_event.filters.is_enabled();
            let filter_groups = scored_event.filters.groups().into_owned();
            let filters_expanded = app
                .rules
                .expanded_filters
                .contains(&(rule.id.clone(), event_index));

            let filter_controls: Element<'_, Message> = if filters_enabled && filters_expanded {
                let mut groups_col = column![].spacing(8);
                if filter_groups.is_empty() {
                    groups_col = groups_col.push(text("No filter groups. Add one below.").size(12));
                } else {
                    for (group_index, group) in filter_groups.iter().enumerate() {
                        groups_col = groups_col.push(render_filter_group_editor(
                            app,
                            &rule.id,
                            event_index,
                            group_index,
                            group.clone(),
                        ));
                    }
                }
                groups_col = groups_col.push(with_tooltip(
                    styled_button_row(icon_label("plus", "Add OR Group"), ButtonTone::Success)
                        .on_press(Message::AddScoredEventFilterGroup(event_index))
                        .into(),
                    "Add OR group.",
                ));
                groups_col.into()
            } else if filters_enabled {
                // Show summary with expand button
                with_tooltip(
                    mouse_area(
                        row![
                            text(filter_summary).size(12),
                            styled_button("Edit Filters", ButtonTone::Secondary).on_press(
                                Message::ToggleFilterExpanded(rule.id.clone(), event_index,)
                            ),
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center),
                    )
                    .on_press(Message::ToggleFilterExpanded(rule.id.clone(), event_index))
                    .interaction(mouse::Interaction::Pointer)
                    .into(),
                    "Edit filters.",
                )
            } else {
                text("No filters active.").size(12).into()
            };

            card()
                .body(
                    column![
                        summary_row,
                        row![
                            field_label("Event Type", "", 120.0,),
                            pick_list(
                                &EventKind::ALL[..],
                                Some(scored_event.event),
                                move |value| {
                                    Message::ScoredEventKindChanged(event_index, value)
                                },
                            )
                            .width(220),
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center),
                        row![
                            field_label("Points", "", 120.0,),
                            styled_button("-", ButtonTone::Secondary)
                                .on_press(Message::ScoredEventPointsStepped(event_index, -1,)),
                            text(format!("{} pts", scored_event.points)).width(90),
                            styled_button("+", ButtonTone::Secondary)
                                .on_press(Message::ScoredEventPointsStepped(event_index, 1)),
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center),
                        row![
                            field_label("Filters", "", 120.0,),
                            toggle_switch(filters_enabled)
                                .label(if filters_enabled { "Active" } else { "Off" })
                                .on_toggle(move |enabled| {
                                    Message::ToggleScoredEventFilters(event_index, enabled)
                                }),
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center),
                        filter_controls,
                    ]
                    .spacing(8),
                )
                .width(Length::Fill)
        } else {
            // Collapsed: just the summary row
            card().body(summary_row).width(Length::Fill)
        };

        events_col = events_col.push(event_card);
        if let Some(drag) = active_scored_event_drag(app, &rule.id) {
            events_col = events_col.push(render_scored_event_drop_zone(drag, event_index + 1));
        }
    }

    sec = sec.push(events_col);

    sec.build().into()
}

fn live_runtime_section<'a>(
    rule: &RuleDefinition,
    status: crate::rules::engine::RuleRuntimeStatus,
) -> Element<'a, Message> {
    let cooldown_text = status
        .cooldown_remaining_secs
        .map(|secs| format!("{secs}s remaining"))
        .unwrap_or_else(|| "ready".into());

    let state_label = if status.extending_until.is_some() {
        "Extending"
    } else if status.armed {
        "Armed"
    } else {
        "Disarmed"
    };

    let mut sec = section("Live Runtime").description("");

    sec = sec.push(
        row![
            rules_badge(
                format!(
                    "Score: {} / {}",
                    status.current_score, rule.trigger_threshold
                ),
                if status.current_score >= rule.trigger_threshold {
                    BadgeTone::Success
                } else if status.current_score > 0 {
                    BadgeTone::Warning
                } else {
                    BadgeTone::Neutral
                },
            ),
            rules_badge(
                state_label,
                if status.armed {
                    BadgeTone::Success
                } else {
                    BadgeTone::Neutral
                },
            ),
            rules_badge(format!("Cooldown: {cooldown_text}"), BadgeTone::Outline,),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    );

    if let Some(extending_until) = status.extending_until {
        sec = sec.push(
            text(format!(
                "Extending until {}",
                extending_until.format("%H:%M:%S UTC")
            ))
            .size(12),
        );
    }

    if status.contributions.is_empty() {
        sec = sec.push(text("No active window contributions.").size(12));
    } else {
        let mut contrib_col = column![].spacing(2);
        for contribution in status.contributions {
            contrib_col = contrib_col.push(text(contribution.summary_line()).size(12));
        }
        sec = sec.push(contrib_col);
    }

    sec.build().into()
}

pub(in crate::app) fn load_reference_data(app: &App) -> iced::Task<AppMessage> {
    let Some(store) = app.clip_store.clone() else {
        return iced::Task::none();
    };

    let cached_vehicle_store = store.clone();
    let cached_vehicle_task = iced::Task::perform(
        async move {
            cached_vehicle_store
                .list_lookups(LookupKind::Vehicle)
                .await
                .map(vehicle_lookup_options)
                .map_err(|error| error.to_string())
        },
        |result| AppMessage::Rules(Message::VehicleOptionsLoaded(result)),
    );

    let cached_weapon_store = store.clone();
    let cached_weapon_task = iced::Task::perform(
        async move {
            cached_weapon_store
                .list_weapon_references()
                .await
                .map(weapon_lookup_options)
                .map_err(|error| error.to_string())
        },
        |result| AppMessage::Rules(Message::WeaponOptionsLoaded(result)),
    );

    let mut tasks = vec![cached_vehicle_task, cached_weapon_task];

    if app.config.service_id.trim().is_empty() {
        return iced::Task::batch(tasks);
    }

    let refresh_vehicle_store = store.clone();
    let service_id = app.config.service_id.clone();
    let refresh_vehicle_task = iced::Task::perform(
        async move {
            let lookups = census::fetch_vehicle_references(&service_id)
                .await
                .map_err(|error| error.to_string())?;
            let persisted = lookups
                .iter()
                .map(|lookup| (lookup.id, lookup.display_name.clone()))
                .collect::<Vec<_>>();
            refresh_vehicle_store
                .store_lookups(LookupKind::Vehicle, &persisted)
                .await
                .map_err(|error| error.to_string())?;
            Ok(vehicle_lookup_options(persisted))
        },
        |result| AppMessage::Rules(Message::VehicleOptionsLoaded(result)),
    );

    let refresh_weapon_store = store;
    let weapon_service_id = app.config.service_id.clone();
    let refresh_weapon_task = iced::Task::perform(
        async move {
            let lookups = census::fetch_weapon_references(&weapon_service_id)
                .await
                .map_err(|error| error.to_string())?;
            let persisted = lookups
                .into_iter()
                .map(|lookup| WeaponReferenceCacheEntry {
                    item_id: lookup.item_id as u32,
                    weapon_id: lookup.weapon_id as u32,
                    display_name: lookup.display_name,
                    category_label: lookup.category_label,
                    faction: lookup.faction,
                    weapon_group_id: lookup.weapon_group_id.map(|value| value as u32),
                })
                .collect::<Vec<_>>();
            refresh_weapon_store
                .store_weapon_references(&persisted)
                .await
                .map_err(|error| error.to_string())?;
            Ok(weapon_lookup_options(persisted))
        },
        |result| AppMessage::Rules(Message::WeaponOptionsLoaded(result)),
    );

    tasks.push(refresh_vehicle_task);
    tasks.push(refresh_weapon_task);

    iced::Task::batch(tasks)
}

fn apply_profile_import(app: &mut App, pending: PendingProfileImport, overwrite_existing: bool) {
    let active_profile_touched = pending
        .bundle
        .profiles
        .iter()
        .any(|profile| profile.id == app.config.active_profile_id);

    match pending.bundle.apply(
        &mut app.config.rule_profiles,
        &mut app.config.rule_definitions,
        overwrite_existing,
    ) {
        Ok(outcome) => {
            app.rules.pending_profile_import = None;
            app.rules.pending_profile_import_shake_started_at = None;
            persist(app);
            let _ = app.sync_tray_snapshot();
            if active_profile_touched {
                app.notify_active_profile_activated();
            }

            let source_name = Path::new(&pending.source_path)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(pending.source_path.as_str());
            let summary_text = outcome.summary();
            let summary = summary_text.trim_end_matches('.');
            app.set_rules_feedback(format!("{summary}. Source: {source_name}."), false);
        }
        Err(error) => {
            app.rules.pending_profile_import = None;
            app.rules.pending_profile_import_shake_started_at = None;
            app.set_rules_feedback(error, true);
        }
    }
}

fn default_profile_export_path(app: &App, profile_name: &str) -> String {
    let file_name = format!(
        "nanite-clip-profile-{}.toml",
        slugify_transfer_name(profile_name)
    );
    app.config
        .recorder
        .save_directory
        .join(file_name)
        .display()
        .to_string()
}

fn default_profile_import_path(app: &App) -> String {
    app.config.recorder.save_directory.display().to_string()
}

fn apply_rule_import(app: &mut App, pending: PendingRuleImport, overwrite_existing: bool) {
    let selected_rule_touched = selected_rule(app)
        .map(|rule| {
            pending
                .bundle
                .rules
                .iter()
                .any(|imported_rule| imported_rule.id == rule.id)
        })
        .unwrap_or(false);

    match pending
        .bundle
        .apply(&mut app.config.rule_definitions, overwrite_existing)
    {
        Ok(outcome) => {
            app.rules.pending_rule_import = None;
            app.rules.pending_rule_import_shake_started_at = None;
            persist(app);

            if selected_rule_touched {
                ensure_selection(app);
            }

            let source_name = Path::new(&pending.source_path)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(pending.source_path.as_str());
            let summary_text = outcome.summary();
            let summary = summary_text.trim_end_matches('.');
            app.push_success_toast("Rules", format!("{summary}. Source: {source_name}."), true);
        }
        Err(error) => {
            app.rules.pending_rule_import = None;
            app.rules.pending_rule_import_shake_started_at = None;
            app.set_rules_feedback(error, true);
        }
    }
}

fn default_rule_export_path(app: &App, rule_name: &str) -> String {
    let file_name = format!("nanite-clip-rule-{}.toml", slugify_transfer_name(rule_name));
    app.config
        .recorder
        .save_directory
        .join(file_name)
        .display()
        .to_string()
}

fn default_rule_import_path(app: &App) -> String {
    app.config.recorder.save_directory.display().to_string()
}

fn slugify_transfer_name(name: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;

    for ch in name.chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            Some(ch.to_ascii_lowercase())
        } else if matches!(ch, ' ' | '-' | '_') {
            Some('-')
        } else {
            None
        };

        match normalized {
            Some('-') if !slug.is_empty() && !last_was_separator => {
                slug.push('-');
                last_was_separator = true;
            }
            Some(value) => {
                slug.push(value);
                last_was_separator = false;
            }
            None => {}
        }
    }

    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "profile".into()
    } else {
        slug.into()
    }
}

fn profile_import_overwrite_dialog<'a>(
    app: &'a App,
    pending: &'a PendingProfileImport,
) -> Element<'a, Message> {
    let conflict_count = pending.conflicts.profile_ids.len() + pending.conflicts.rule_ids.len();
    let source_name = Path::new(&pending.source_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(pending.source_path.as_str());

    let mut body = column![
        text("Overwrite existing profiles or rules?").size(24),
        text(format!(
            "Importing {} profile(s) and {} rule(s) from {source_name}.",
            pending.bundle.profiles.len(),
            pending.bundle.rules.len()
        ))
        .size(14),
        banner(format!(
            "{conflict_count} existing item(s) already use the same id."
        ))
        .warning()
        .description("Matching profiles and rules will be replaced.",)
        .build(),
    ]
    .spacing(12)
    .width(Length::Fill);

    if import_decision_shake_offset(app.rules.pending_profile_import_shake_started_at) != 0.0 {
        body = body.push(
            banner("Choose an action below.")
                .error()
                .description("")
                .build(),
        );
    }

    if !pending.conflicts.profile_ids.is_empty() {
        body = body.push(conflict_section(
            "Conflicting Profiles",
            &pending.conflicts.profile_ids,
        ));
    }

    if !pending.conflicts.rule_ids.is_empty() {
        body = body.push(conflict_section(
            "Conflicting Rules",
            &pending.conflicts.rule_ids,
        ));
    }

    body = body.push(
        row![
            iced::widget::Space::new().width(Length::Fill),
            styled_button("Cancel", ButtonTone::Secondary)
                .on_press(Message::CancelProfileImportOverwrite),
            styled_button("Overwrite & Import", ButtonTone::Danger)
                .on_press(Message::ConfirmProfileImportOverwrite),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    );

    container(body).width(Length::Fill).into()
}

fn rule_import_overwrite_dialog<'a>(
    app: &'a App,
    pending: &'a PendingRuleImport,
) -> Element<'a, Message> {
    let conflict_count = pending.conflicts.rule_ids.len();
    let source_name = Path::new(&pending.source_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(pending.source_path.as_str());

    let mut body = column![
        text("Overwrite existing rules?").size(24),
        text(format!(
            "Importing {} rule(s) from {source_name}.",
            pending.bundle.rules.len()
        ))
        .size(14),
        banner(format!(
            "{conflict_count} existing rule(s) already use the same id."
        ))
        .warning()
        .description("Matching rules will be replaced.",)
        .build(),
    ]
    .spacing(12)
    .width(Length::Fill);

    if import_decision_shake_offset(app.rules.pending_rule_import_shake_started_at) != 0.0 {
        body = body.push(
            banner("Choose an action below.")
                .error()
                .description("")
                .build(),
        );
    }

    if !pending.conflicts.rule_ids.is_empty() {
        body = body.push(conflict_section(
            "Conflicting Rules",
            &pending.conflicts.rule_ids,
        ));
    }

    body = body.push(
        row![
            iced::widget::Space::new().width(Length::Fill),
            styled_button("Cancel", ButtonTone::Secondary)
                .on_press(Message::CancelRuleImportOverwrite),
            styled_button("Overwrite & Import", ButtonTone::Danger)
                .on_press(Message::ConfirmRuleImportOverwrite),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    );

    container(body).width(Length::Fill).into()
}

fn conflict_section<'a>(title: &'a str, ids: &'a [String]) -> Element<'a, Message> {
    column![
        text(title).size(16),
        container(text(conflict_preview(ids)).size(14))
            .width(Length::Fill)
            .padding(12)
            .style(rounded_box),
    ]
    .spacing(6)
    .into()
}

fn conflict_preview(ids: &[String]) -> String {
    const MAX_PREVIEW_IDS: usize = 8;

    let mut preview: Vec<String> = ids.iter().take(MAX_PREVIEW_IDS).cloned().collect();
    if ids.len() > MAX_PREVIEW_IDS {
        preview.push(format!("and {} more", ids.len() - MAX_PREVIEW_IDS));
    }
    preview.join(", ")
}

fn import_decision_shake_offset(shake_started_at: Option<std::time::Instant>) -> f32 {
    const SHAKE_DURATION_SECS: f32 = 0.75;
    const SHAKE_STEP_SECS: f32 = 0.09;
    const SHAKE_SEQUENCE: [f32; 8] = [18.0, -16.0, 12.0, -10.0, 7.0, -5.0, 3.0, 0.0];

    let Some(started_at) = shake_started_at else {
        return 0.0;
    };

    let elapsed = started_at.elapsed().as_secs_f32();
    if elapsed >= SHAKE_DURATION_SECS {
        return 0.0;
    }

    let index = (elapsed / SHAKE_STEP_SECS) as usize;
    SHAKE_SEQUENCE
        .get(index)
        .copied()
        .unwrap_or_else(|| *SHAKE_SEQUENCE.last().unwrap_or(&0.0))
}

pub(in crate::app) fn persist(app: &mut App) {
    app.config.normalize();
    ensure_selection(app);
    app.rule_engine.update_rules(
        app.config.rule_definitions.clone(),
        app.config.rule_profiles.clone(),
        app.config.active_profile_id.clone(),
    );
    if let Some(error) = selected_rule(app).and_then(|rule| validate_rule(rule).err()) {
        app.set_rules_feedback(error, true);
    } else if let Some(error) = app
        .config
        .auto_switch_rules
        .iter()
        .find_map(|rule| validate_auto_switch_rule(rule).err())
    {
        app.set_rules_feedback(error, true);
    } else {
        app.clear_rules_feedback();
    }
    if let Err(error) = app.config.save() {
        tracing::error!("Failed to save config: {error}");
    }
}

pub(in crate::app) fn ensure_selection(app: &mut App) {
    if !app
        .config
        .rule_profiles
        .iter()
        .any(|profile| profile.id == app.config.active_profile_id)
    {
        app.config.active_profile_id = app
            .config
            .rule_profiles
            .first()
            .map(|profile| profile.id.clone())
            .unwrap_or_default();
    }

    if selected_rule_index(app).is_none() {
        app.rules.selected_rule_id = app
            .config
            .rule_definitions
            .first()
            .map(|rule| rule.id.clone());
    }
}

fn selected_rule_index(app: &App) -> Option<usize> {
    app.rules.selected_rule_id.as_deref().and_then(|rule_id| {
        app.config
            .rule_definitions
            .iter()
            .position(|rule| rule.id == rule_id)
    })
}

fn selected_rule(app: &App) -> Option<&RuleDefinition> {
    selected_rule_index(app).and_then(|index| app.config.rule_definitions.get(index))
}

fn next_profile_id(app: &App) -> String {
    next_identifier(
        app.config
            .rule_profiles
            .iter()
            .map(|profile| profile.id.as_str())
            .collect(),
        "profile",
    )
}

fn next_rule_id(app: &App) -> String {
    next_identifier(
        app.config
            .rule_definitions
            .iter()
            .map(|rule| rule.id.as_str())
            .collect(),
        "rule",
    )
}

fn next_auto_switch_rule_id(app: &App) -> String {
    next_identifier(
        app.config
            .auto_switch_rules
            .iter()
            .map(|rule| rule.id.as_str())
            .collect(),
        "auto_switch",
    )
}

fn auto_switch_condition_for_choice(
    app: &App,
    choice: AutoSwitchConditionChoice,
    current: AutoSwitchCondition,
) -> AutoSwitchCondition {
    match (choice, current) {
        (
            AutoSwitchConditionChoice::LocalSchedule,
            AutoSwitchCondition::LocalSchedule {
                mut weekdays,
                start_hour,
                start_minute,
                end_hour,
                end_minute,
            },
        ) => {
            normalize_schedule_weekdays(&mut weekdays);
            AutoSwitchCondition::LocalSchedule {
                weekdays,
                start_hour,
                start_minute,
                end_hour,
                end_minute,
            }
        }
        (
            AutoSwitchConditionChoice::LocalSchedule,
            AutoSwitchCondition::LocalTimeRange {
                start_hour,
                end_hour,
            },
        ) => AutoSwitchCondition::LocalSchedule {
            weekdays: default_schedule_weekdays(),
            start_hour,
            start_minute: 0,
            end_hour,
            end_minute: 0,
        },
        (AutoSwitchConditionChoice::LocalSchedule, _) => AutoSwitchCondition::LocalSchedule {
            weekdays: default_schedule_weekdays(),
            start_hour: 18,
            start_minute: 0,
            end_hour: 23,
            end_minute: 0,
        },
        (
            AutoSwitchConditionChoice::ActiveCharacter,
            AutoSwitchCondition::ActiveCharacter {
                character_ids,
                character_id,
            },
        ) => AutoSwitchCondition::ActiveCharacter {
            character_ids: normalized_active_character_ids(&character_ids, character_id),
            character_id: None,
        },
        (AutoSwitchConditionChoice::ActiveCharacter, _) => AutoSwitchCondition::ActiveCharacter {
            character_ids: tracked_character_options(app)
                .first()
                .map(|character| vec![character.id])
                .unwrap_or_default(),
            character_id: None,
        },
    }
}

fn tracked_character_options(app: &App) -> Vec<CharacterOption> {
    let mut options = app
        .config
        .characters
        .iter()
        .filter_map(|character| {
            character.character_id.map(|id| CharacterOption {
                id,
                name: character.name.clone(),
            })
        })
        .collect::<Vec<_>>();
    options.sort_by(|left, right| left.name.cmp(&right.name));
    options
}

fn auto_switch_rule_mut<'a>(app: &'a mut App, rule_id: &str) -> Option<&'a mut AutoSwitchRule> {
    app.config
        .auto_switch_rules
        .iter_mut()
        .find(|rule| rule.id == rule_id)
}

fn next_identifier(existing: Vec<&str>, prefix: &str) -> String {
    let existing: std::collections::HashSet<_> = existing.into_iter().collect();
    for index in 1.. {
        let candidate = format!("{prefix}_{index}");
        if !existing.contains(candidate.as_str()) {
            return candidate;
        }
    }
    unreachable!()
}

fn blank_rule_definition(id: &str, name: &str) -> RuleDefinition {
    RuleDefinition {
        id: id.into(),
        name: name.into(),
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
        ],
    }
}

fn step_value(current: u32, delta: i32, step: u32, min: u32, max: u32) -> u32 {
    let signed_step = i64::from(step) * i64::from(delta);
    (i64::from(current) + signed_step).clamp(i64::from(min), i64::from(max)) as u32
}

fn schedule_slot(hour: u8, minute: u8) -> u8 {
    hour.saturating_mul(2) + u8::from(minute >= 30)
}

fn slot_time(slot: u8) -> (u8, u8) {
    (slot / 2, if slot.is_multiple_of(2) { 0 } else { 30 })
}

fn wrap_schedule_slot(current: u8, delta: i32, max: u8) -> u8 {
    ((i32::from(current) + delta).rem_euclid(i32::from(max) + 1)) as u8
}

fn name_or_default_preserving_whitespace(value: &str, default: &str) -> String {
    if value.trim().is_empty() {
        default.to_string()
    } else {
        value.to_string()
    }
}

fn ensure_filter_group_exists(filters: &mut ScoredEventFilters) {
    if filters.groups().is_empty() {
        filters.groups.push(default_filter_group());
    }
}

fn default_filter_group() -> ScoredEventFilterGroup {
    ScoredEventFilterGroup {
        clauses: vec![default_filter_clause()],
    }
}

fn default_filter_clause() -> ScoredEventFilterClause {
    FilterClauseKindChoice::TargetCharacter.default_clause()
}

fn scored_event_filters_mut(app: &mut App, event_index: usize) -> Option<&mut ScoredEventFilters> {
    let index = selected_rule_index(app)?;
    app.config.rule_definitions[index]
        .scored_events
        .get_mut(event_index)
        .map(|event| &mut event.filters)
}

fn filter_clause_mut<'a>(
    app: &'a mut App,
    path: &FilterClausePath,
) -> Option<&'a mut ScoredEventFilterClause> {
    let group = scored_event_filters_mut(app, path.event_index)?
        .groups
        .get_mut(path.group_index)?;
    filter_clause_vec_mut(&mut group.clauses, &path.clause_path)
}

fn filter_clause_vec_mut<'a>(
    clauses: &'a mut [ScoredEventFilterClause],
    path: &[usize],
) -> Option<&'a mut ScoredEventFilterClause> {
    let (&index, rest) = path.split_first()?;
    let clause = clauses.get_mut(index)?;
    if rest.is_empty() {
        return Some(clause);
    }
    match clause {
        ScoredEventFilterClause::Any { clauses } => filter_clause_vec_mut(clauses, rest),
        _ => None,
    }
}

fn delete_filter_clause(app: &mut App, path: &FilterClausePath) -> bool {
    let Some(group) = scored_event_filters_mut(app, path.event_index)
        .and_then(|filters| filters.groups.get_mut(path.group_index))
    else {
        return false;
    };
    delete_clause_from_vec(&mut group.clauses, &path.clause_path)
}

fn delete_clause_from_vec(clauses: &mut Vec<ScoredEventFilterClause>, path: &[usize]) -> bool {
    let Some((&index, rest)) = path.split_first() else {
        return false;
    };
    if rest.is_empty() {
        if index < clauses.len() {
            clauses.remove(index);
            return true;
        }
        return false;
    }
    match clauses.get_mut(index) {
        Some(ScoredEventFilterClause::Any { clauses: nested }) => {
            delete_clause_from_vec(nested, rest)
        }
        _ => false,
    }
}

fn filter_clause_list_len(app: &App, path: &FilterClauseListPath) -> Option<usize> {
    let rule_index = selected_rule_index(app)?;
    let filters = app.config.rule_definitions[rule_index]
        .scored_events
        .get(path.event_index)
        .map(|event| &event.filters)?;
    let groups = filters.groups();
    let group = groups.get(path.group_index)?;
    filter_clause_list_len_in_vec(&group.clauses, &path.parent_clause_path)
}

fn filter_clause_list_len_in_vec(
    clauses: &[ScoredEventFilterClause],
    path: &[usize],
) -> Option<usize> {
    let Some((&index, rest)) = path.split_first() else {
        return Some(clauses.len());
    };
    match clauses.get(index) {
        Some(ScoredEventFilterClause::Any { clauses: nested }) => {
            filter_clause_list_len_in_vec(nested, rest)
        }
        _ => None,
    }
}

fn filter_clause_list_mut<'a>(
    app: &'a mut App,
    path: &FilterClauseListPath,
) -> Option<&'a mut Vec<ScoredEventFilterClause>> {
    let group = scored_event_filters_mut(app, path.event_index)?
        .groups
        .get_mut(path.group_index)?;
    filter_clause_list_vec_mut(&mut group.clauses, &path.parent_clause_path)
}

fn filter_clause_list_vec_mut<'a>(
    clauses: &'a mut Vec<ScoredEventFilterClause>,
    path: &[usize],
) -> Option<&'a mut Vec<ScoredEventFilterClause>> {
    let Some((&index, rest)) = path.split_first() else {
        return Some(clauses);
    };
    match clauses.get_mut(index) {
        Some(ScoredEventFilterClause::Any { clauses: nested }) => {
            filter_clause_list_vec_mut(nested, rest)
        }
        _ => None,
    }
}

fn move_scored_event(app: &mut App, drag: &ScoredEventDragState) -> bool {
    let Some(rule_index) = selected_rule_index(app) else {
        return false;
    };
    if app.config.rule_definitions[rule_index].id != drag.rule_id {
        return false;
    }

    let event_count = app.config.rule_definitions[rule_index].scored_events.len();
    let Some(destination_index) =
        reorder_destination_index(drag.source_index, drag.target_index, event_count)
    else {
        return false;
    };

    {
        let rule = &mut app.config.rule_definitions[rule_index];
        let entry = rule.scored_events.remove(drag.source_index);
        rule.scored_events.insert(destination_index, entry);
    }
    reindex_rule_ui_state_after_event_reorder(
        app,
        &drag.rule_id,
        drag.source_index,
        drag.target_index,
        event_count,
    );
    true
}

fn move_filter_clause(app: &mut App, drag: &FilterClauseDragState) -> bool {
    let Some(source_index) = drag.source_path.clause_path.last().copied() else {
        return false;
    };
    let Some(list_len) = filter_clause_list_len(app, &drag.list_path) else {
        return false;
    };
    let Some(destination_index) =
        reorder_destination_index(source_index, drag.target_index, list_len)
    else {
        return false;
    };

    {
        let Some(clauses) = filter_clause_list_mut(app, &drag.list_path) else {
            return false;
        };
        let clause = clauses.remove(source_index);
        clauses.insert(destination_index, clause);
    }
    reindex_rule_ui_state_after_clause_reorder(
        app,
        &drag.list_path,
        source_index,
        drag.target_index,
        list_len,
    );
    true
}

fn reorder_destination_index(
    source_index: usize,
    target_index: usize,
    list_len: usize,
) -> Option<usize> {
    if source_index >= list_len || target_index > list_len {
        return None;
    }

    let destination_index = if target_index > source_index {
        target_index.saturating_sub(1)
    } else {
        target_index
    };
    if destination_index == source_index || destination_index >= list_len {
        return None;
    }
    Some(destination_index)
}

fn reorder_index(index: usize, source_index: usize, target_index: usize, list_len: usize) -> usize {
    let mut order = (0..list_len).collect::<Vec<_>>();
    let item = order.remove(source_index);
    let insert_at = if target_index > source_index {
        target_index - 1
    } else {
        target_index
    };
    order.insert(insert_at, item);
    order
        .iter()
        .position(|candidate| *candidate == index)
        .unwrap_or(index)
}

fn reindex_rule_ui_state_after_event_reorder(
    app: &mut App,
    rule_id: &str,
    source_index: usize,
    target_index: usize,
    list_len: usize,
) {
    let mut categories = BTreeMap::new();
    for (mut key, value) in std::mem::take(&mut app.rules.vehicle_browse_categories) {
        if key.rule_id == rule_id {
            key.event_index = reorder_index(key.event_index, source_index, target_index, list_len);
        }
        categories.insert(key, value);
    }
    app.rules.vehicle_browse_categories = categories;

    let mut weapon_categories = BTreeMap::new();
    for (mut key, value) in std::mem::take(&mut app.rules.weapon_browse_categories) {
        if key.rule_id == rule_id {
            key.event_index = reorder_index(key.event_index, source_index, target_index, list_len);
        }
        weapon_categories.insert(key, value);
    }
    app.rules.weapon_browse_categories = weapon_categories;

    let mut weapon_groups = BTreeMap::new();
    for (mut key, value) in std::mem::take(&mut app.rules.weapon_browse_groups) {
        if key.rule_id == rule_id {
            key.event_index = reorder_index(key.event_index, source_index, target_index, list_len);
        }
        weapon_groups.insert(key, value);
    }
    app.rules.weapon_browse_groups = weapon_groups;

    let mut weapon_factions = BTreeMap::new();
    for (mut key, value) in std::mem::take(&mut app.rules.weapon_browse_factions) {
        if key.rule_id == rule_id {
            key.event_index = reorder_index(key.event_index, source_index, target_index, list_len);
        }
        weapon_factions.insert(key, value);
    }
    app.rules.weapon_browse_factions = weapon_factions;

    let mut drafts = BTreeMap::new();
    for (mut key, value) in std::mem::take(&mut app.rules.filter_text_drafts) {
        if key.path.rule_id == rule_id {
            key.path.event_index =
                reorder_index(key.path.event_index, source_index, target_index, list_len);
        }
        drafts.insert(key, value);
    }
    app.rules.filter_text_drafts = drafts;
}

fn reindex_rule_ui_state_after_clause_reorder(
    app: &mut App,
    list_path: &FilterClauseListPath,
    source_index: usize,
    target_index: usize,
    list_len: usize,
) {
    let mut categories = BTreeMap::new();
    for (mut key, value) in std::mem::take(&mut app.rules.vehicle_browse_categories) {
        remap_clause_key(
            &mut key.rule_id,
            &mut key.event_index,
            &mut key.group_index,
            &mut key.clause_path,
            list_path,
            source_index,
            target_index,
            list_len,
        );
        categories.insert(key, value);
    }
    app.rules.vehicle_browse_categories = categories;

    let mut weapon_categories = BTreeMap::new();
    for (mut key, value) in std::mem::take(&mut app.rules.weapon_browse_categories) {
        remap_clause_key(
            &mut key.rule_id,
            &mut key.event_index,
            &mut key.group_index,
            &mut key.clause_path,
            list_path,
            source_index,
            target_index,
            list_len,
        );
        weapon_categories.insert(key, value);
    }
    app.rules.weapon_browse_categories = weapon_categories;

    let mut weapon_groups = BTreeMap::new();
    for (mut key, value) in std::mem::take(&mut app.rules.weapon_browse_groups) {
        remap_clause_key(
            &mut key.rule_id,
            &mut key.event_index,
            &mut key.group_index,
            &mut key.clause_path,
            list_path,
            source_index,
            target_index,
            list_len,
        );
        weapon_groups.insert(key, value);
    }
    app.rules.weapon_browse_groups = weapon_groups;

    let mut weapon_factions = BTreeMap::new();
    for (mut key, value) in std::mem::take(&mut app.rules.weapon_browse_factions) {
        remap_clause_key(
            &mut key.rule_id,
            &mut key.event_index,
            &mut key.group_index,
            &mut key.clause_path,
            list_path,
            source_index,
            target_index,
            list_len,
        );
        weapon_factions.insert(key, value);
    }
    app.rules.weapon_browse_factions = weapon_factions;

    let mut drafts = BTreeMap::new();
    for (mut key, value) in std::mem::take(&mut app.rules.filter_text_drafts) {
        remap_clause_key(
            &mut key.path.rule_id,
            &mut key.path.event_index,
            &mut key.path.group_index,
            &mut key.path.clause_path,
            list_path,
            source_index,
            target_index,
            list_len,
        );
        drafts.insert(key, value);
    }
    app.rules.filter_text_drafts = drafts;
}

#[allow(clippy::too_many_arguments)]
fn remap_clause_key(
    rule_id: &mut String,
    event_index: &mut usize,
    group_index: &mut usize,
    clause_path: &mut [usize],
    list_path: &FilterClauseListPath,
    source_index: usize,
    target_index: usize,
    list_len: usize,
) {
    if *rule_id != list_path.rule_id
        || *event_index != list_path.event_index
        || *group_index != list_path.group_index
        || clause_path.len() <= list_path.parent_clause_path.len()
        || !clause_path.starts_with(&list_path.parent_clause_path)
    {
        return;
    }

    let depth = list_path.parent_clause_path.len();
    clause_path[depth] = reorder_index(clause_path[depth], source_index, target_index, list_len);
}

fn clear_rule_filter_ui_state(app: &mut App, rule_id: &str) {
    app.rules
        .vehicle_browse_categories
        .retain(|key, _| key.rule_id != rule_id);
    app.rules
        .weapon_browse_groups
        .retain(|key, _| key.rule_id != rule_id);
    app.rules
        .weapon_browse_categories
        .retain(|key, _| key.rule_id != rule_id);
    app.rules
        .weapon_browse_factions
        .retain(|key, _| key.rule_id != rule_id);
    app.rules
        .filter_text_drafts
        .retain(|key, _| key.path.rule_id != rule_id);
    if app
        .rules
        .drag_state
        .as_ref()
        .is_some_and(|drag| drag.rule_id() == rule_id)
    {
        app.rules.drag_state = None;
    }
}

fn clear_filter_clause_resolution(
    app: &mut App,
    key: &FilterTextDraftKey,
) -> iced::Task<AppMessage> {
    app.rules.filter_text_drafts.remove(key);
    if let Some(clause) = filter_clause_mut(app, &key.path) {
        match clause {
            ScoredEventFilterClause::TargetCharacter { target } => {
                target.name = None;
                target.character_id = None;
            }
            ScoredEventFilterClause::TargetOutfit { outfit } => {
                outfit.tag = None;
                outfit.outfit_id = None;
            }
            _ => return iced::Task::none(),
        }
        persist(app);
    }
    iced::Task::none()
}

fn apply_resolved_filter_reference(
    clause: &mut ScoredEventFilterClause,
    resolved: &ResolvedFilterReference,
) {
    match (clause, resolved) {
        (
            ScoredEventFilterClause::TargetCharacter { target },
            ResolvedFilterReference::Character(resolved),
        ) => *target = resolved.clone(),
        (
            ScoredEventFilterClause::TargetOutfit { outfit },
            ResolvedFilterReference::Outfit(resolved),
        ) => *outfit = resolved.clone(),
        _ => {}
    }
}

fn scored_event_filter_summary(
    filters: &ScoredEventFilters,
    vehicle_options: &[LookupOption],
    weapon_options: &[WeaponLookupOption],
) -> String {
    let groups = filters.groups();
    let group_summaries = groups
        .iter()
        .map(|group| {
            group
                .clauses
                .iter()
                .map(|clause| filter_clause_summary(clause, vehicle_options, weapon_options))
                .collect::<Vec<_>>()
                .join(" AND ")
        })
        .filter(|summary| !summary.is_empty())
        .collect::<Vec<_>>();

    if !filters.is_enabled() {
        if group_summaries.is_empty() {
            "Disabled".into()
        } else {
            format!("Disabled: {}", group_summaries.join(" OR "))
        }
    } else if group_summaries.is_empty() {
        "Any target / outfit / vehicle / weapon".into()
    } else {
        group_summaries.join(" OR ")
    }
}

fn filter_clause_summary(
    clause: &ScoredEventFilterClause,
    vehicle_options: &[LookupOption],
    weapon_options: &[WeaponLookupOption],
) -> String {
    match clause {
        ScoredEventFilterClause::TargetCharacter { target } => match (
            target
                .name
                .as_deref()
                .filter(|name| !name.trim().is_empty()),
            target.character_id,
        ) {
            (Some(name), Some(id)) => format!("target {name} (#{id})"),
            (Some(name), None) => format!("target {name} (unresolved)"),
            (None, Some(id)) => format!("target #{id}"),
            (None, None) => String::new(),
        },
        ScoredEventFilterClause::TargetOutfit { outfit } => match (
            outfit.tag.as_deref().filter(|tag| !tag.trim().is_empty()),
            outfit.outfit_id,
        ) {
            (Some(tag), Some(id)) => format!("target outfit [{tag}] (#{id})"),
            (Some(tag), None) => format!("target outfit [{tag}] (unresolved)"),
            (None, Some(id)) => format!("target outfit #{id}"),
            (None, None) => String::new(),
        },
        ScoredEventFilterClause::AttackerVehicle { vehicle } => format!(
            "attacker vehicle {}",
            vehicle_display_name(vehicle_options, vehicle)
        ),
        ScoredEventFilterClause::AttackerWeapon { weapon } => format!(
            "attacker weapon {}",
            weapon_display_name(weapon_options, weapon)
        ),
        ScoredEventFilterClause::DestroyedVehicle { vehicle } => format!(
            "destroyed vehicle {}",
            vehicle_display_name(vehicle_options, vehicle)
        ),
        ScoredEventFilterClause::Any { clauses } => {
            let summary = clauses
                .iter()
                .map(|clause| filter_clause_summary(clause, vehicle_options, weapon_options))
                .filter(|summary| !summary.is_empty())
                .collect::<Vec<_>>()
                .join(" OR ");
            if summary.is_empty() {
                summary
            } else {
                format!("({summary})")
            }
        }
    }
}

fn active_scored_event_drag<'a>(app: &'a App, rule_id: &str) -> Option<&'a ScoredEventDragState> {
    match app.rules.drag_state.as_ref() {
        Some(RuleDragState::ScoredEvent(drag)) if drag.rule_id == rule_id => Some(drag),
        _ => None,
    }
}

fn active_filter_clause_drag<'a>(
    app: &'a App,
    list_path: &FilterClauseListPath,
) -> Option<&'a FilterClauseDragState> {
    match app.rules.drag_state.as_ref() {
        Some(RuleDragState::FilterClause(drag)) if drag.list_path == *list_path => Some(drag),
        _ => None,
    }
}

fn render_drag_handle<'a>(
    active: bool,
    on_press: Message,
    tooltip: &'static str,
) -> Element<'a, Message> {
    with_tooltip(
        mouse_area(
            container(solid_icon("grip-lines", 14.0))
                .padding([4, 8])
                .style(if active { rounded_box } else { transparent_box }),
        )
        .on_press(on_press)
        .interaction(if active {
            mouse::Interaction::Grabbing
        } else {
            mouse::Interaction::Grab
        })
        .into(),
        tooltip,
    )
}

fn render_scored_event_drop_zone<'a>(
    drag: &ScoredEventDragState,
    target_index: usize,
) -> Element<'a, Message> {
    let active = drag.target_index == target_index;
    mouse_area(
        container(text(if active { "Drop weight here" } else { "" }).size(11))
            .padding([2, 8])
            .width(Length::Fill)
            .style(if active { rounded_box } else { transparent_box }),
    )
    .on_enter(Message::HoverScoredEventDrop(target_index))
    .on_release(Message::RuleDragReleased)
    .interaction(mouse::Interaction::Move)
    .into()
}

fn render_filter_clause_drop_zone<'a>(
    drag: &FilterClauseDragState,
    list_path: FilterClauseListPath,
    target_index: usize,
) -> Element<'a, Message> {
    let active = drag.target_index == target_index;
    mouse_area(
        container(text(if active { "Drop clause here" } else { "" }).size(11))
            .padding([2, 8])
            .width(Length::Fill)
            .style(if active { rounded_box } else { transparent_box }),
    )
    .on_enter(Message::HoverFilterClauseDrop(list_path, target_index))
    .on_release(Message::RuleDragReleased)
    .interaction(mouse::Interaction::Move)
    .into()
}

fn vehicle_display_name(options: &[LookupOption], vehicle: &VehicleMatchFilter) -> String {
    if let Some(label) = vehicle
        .vehicle
        .label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty())
    {
        return label.to_string();
    }

    if let Some(vehicle_id) = vehicle.legacy_vehicle_id {
        return options
            .iter()
            .find(|option| option.ids.contains(&vehicle_id))
            .map(|option| option.label.clone())
            .unwrap_or_else(|| format!("Vehicle #{vehicle_id}"));
    }

    "Any vehicle".into()
}

fn weapon_display_name(options: &[WeaponLookupOption], weapon: &WeaponMatchFilter) -> String {
    if let Some(label) = weapon
        .weapon
        .label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty())
    {
        return label.to_string();
    }

    if let Some(weapon_id) = weapon.legacy_weapon_id {
        return options
            .iter()
            .find(|option| option.ids.contains(&weapon_id))
            .map(|option| option.label.clone())
            .unwrap_or_else(|| format!("Weapon #{weapon_id}"));
    }

    "Any weapon".into()
}

fn render_filter_group_editor<'a>(
    app: &'a App,
    rule_id: &'a str,
    event_index: usize,
    group_index: usize,
    group: ScoredEventFilterGroup,
) -> Element<'a, Message> {
    let list_path = FilterClauseListPath::root(rule_id, event_index, group_index);
    let mut group_col = column![
        row![
            text(if group_index == 0 {
                "Match this group"
            } else {
                "OR match this group"
            })
            .size(13)
            .width(Length::Fill),
            with_tooltip(
                styled_button_row(icon_label("plus", "Add AND Clause"), ButtonTone::Secondary)
                    .on_press(Message::AddScoredEventFilterClause(
                        event_index,
                        group_index
                    ))
                    .into(),
                "Add AND clause.",
            ),
            with_tooltip(
                styled_button_row(icon_label("trash", "Delete Group"), ButtonTone::Danger)
                    .on_press(Message::DeleteScoredEventFilterGroup(
                        event_index,
                        group_index
                    ))
                    .into(),
                "Delete group.",
            ),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
    ]
    .spacing(8);

    if let Some(drag) = active_filter_clause_drag(app, &list_path) {
        group_col = group_col.push(render_filter_clause_drop_zone(drag, list_path.clone(), 0));
    }

    for (clause_index, clause) in group.clauses.into_iter().enumerate() {
        group_col = group_col.push(render_filter_clause_editor(
            app,
            list_path.child_clause_path(clause_index),
            clause,
        ));
        if let Some(drag) = active_filter_clause_drag(app, &list_path) {
            group_col = group_col.push(render_filter_clause_drop_zone(
                drag,
                list_path.clone(),
                clause_index + 1,
            ));
        }
    }

    container(group_col).padding(8).style(rounded_box).into()
}

fn render_filter_clause_editor<'a>(
    app: &'a App,
    path: FilterClausePath,
    clause: ScoredEventFilterClause,
) -> Element<'a, Message> {
    let kind = FilterClauseKindChoice::from_clause(&clause);
    let is_drag_source = matches!(
        app.rules.drag_state.as_ref(),
        Some(RuleDragState::FilterClause(drag)) if drag.source_path == path
    );
    let controls: Element<'a, Message> = match clause {
        ScoredEventFilterClause::TargetCharacter { target } => render_text_filter_clause_editor(
            app,
            FilterTextDraftKey::new(path.clone(), FilterTextField::TargetCharacter),
            target.name,
            target.character_id.map(|id| format!("#{id}")),
            target
                .character_id
                .map(|id| format!("https://wt.honu.pw/c/{id}")),
            "Target username",
            "Resolve to look up ID.",
        ),
        ScoredEventFilterClause::TargetOutfit { outfit } => render_text_filter_clause_editor(
            app,
            FilterTextDraftKey::new(path.clone(), FilterTextField::TargetOutfit),
            outfit.tag,
            outfit.outfit_id.map(|id| format!("#{id}")),
            outfit
                .outfit_id
                .map(|id| format!("https://wt.honu.pw/o/{id}")),
            "Target outfit tag",
            "Resolve to look up ID.",
        ),
        ScoredEventFilterClause::AttackerVehicle { vehicle } => {
            render_vehicle_filter_clause_editor(app, path.clone(), vehicle, "Attacker vehicle.")
        }
        ScoredEventFilterClause::AttackerWeapon { weapon } => {
            render_weapon_filter_clause_editor(app, path.clone(), weapon, "Attacker weapon.")
        }
        ScoredEventFilterClause::DestroyedVehicle { vehicle } => {
            render_vehicle_filter_clause_editor(app, path.clone(), vehicle, "Destroyed vehicle.")
        }
        ScoredEventFilterClause::Any { clauses } => {
            render_nested_any_clause_editor(app, path.clone(), clauses)
        }
    };

    row![
        render_drag_handle(
            is_drag_source,
            Message::StartFilterClauseDrag(path.clone()),
            "Drag to reorder.",
        ),
        text(if path.is_first_in_group() {
            "IF"
        } else {
            "AND"
        })
        .width(32),
        {
            let kind_path = path.clone();
            pick_list(
                &FilterClauseKindChoice::ALL[..],
                Some(kind),
                move |choice| {
                    Message::ScoredEventFilterClauseKindChanged(kind_path.clone(), choice)
                },
            )
        }
        .width(190),
        controls,
        with_tooltip(
            styled_button_row(icon_label("trash", "Delete"), ButtonTone::Danger)
                .on_press(Message::DeleteScoredEventFilterClause(path.clone()))
                .into(),
            "Delete clause.",
        ),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into()
}

fn render_nested_any_clause_editor<'a>(
    app: &'a App,
    path: FilterClausePath,
    clauses: Vec<ScoredEventFilterClause>,
) -> Element<'a, Message> {
    let list_path = FilterClauseListPath {
        rule_id: path.rule_id.clone(),
        event_index: path.event_index,
        group_index: path.group_index,
        parent_clause_path: path.clause_path.clone(),
    };
    let mut col = column![
        row![
            text("Match any of").size(12).width(Length::Fill),
            with_tooltip(
                styled_button_row(icon_label("plus", "Add OR Clause"), ButtonTone::Secondary)
                    .on_press(Message::AddNestedOrClause(path.clone()))
                    .into(),
                "Add OR clause.",
            ),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
    ]
    .spacing(6);

    if let Some(drag) = active_filter_clause_drag(app, &list_path) {
        col = col.push(render_filter_clause_drop_zone(drag, list_path.clone(), 0));
    }

    for (index, clause) in clauses.into_iter().enumerate() {
        col = col.push(render_nested_or_option_editor(
            app,
            list_path.child_clause_path(index),
            clause,
        ));
        if let Some(drag) = active_filter_clause_drag(app, &list_path) {
            col = col.push(render_filter_clause_drop_zone(
                drag,
                list_path.clone(),
                index + 1,
            ));
        }
    }

    container(col)
        .padding(8)
        .style(rounded_box)
        .width(Length::Shrink)
        .into()
}

fn render_nested_or_option_editor<'a>(
    app: &'a App,
    path: FilterClausePath,
    clause: ScoredEventFilterClause,
) -> Element<'a, Message> {
    let kind = FilterClauseKindChoice::from_clause(&clause);
    let is_drag_source = matches!(
        app.rules.drag_state.as_ref(),
        Some(RuleDragState::FilterClause(drag)) if drag.source_path == path
    );
    let controls: Element<'a, Message> = match clause {
        ScoredEventFilterClause::TargetCharacter { target } => render_text_filter_clause_editor(
            app,
            FilterTextDraftKey::new(path.clone(), FilterTextField::TargetCharacter),
            target.name,
            target.character_id.map(|id| format!("#{id}")),
            target
                .character_id
                .map(|id| format!("https://wt.honu.pw/c/{id}")),
            "Target username",
            "Resolve to look up ID.",
        ),
        ScoredEventFilterClause::TargetOutfit { outfit } => render_text_filter_clause_editor(
            app,
            FilterTextDraftKey::new(path.clone(), FilterTextField::TargetOutfit),
            outfit.tag,
            outfit.outfit_id.map(|id| format!("#{id}")),
            outfit
                .outfit_id
                .map(|id| format!("https://wt.honu.pw/o/{id}")),
            "Target outfit tag",
            "Resolve to look up ID.",
        ),
        ScoredEventFilterClause::AttackerVehicle { vehicle } => {
            render_vehicle_filter_clause_editor(app, path.clone(), vehicle, "Attacker vehicle.")
        }
        ScoredEventFilterClause::AttackerWeapon { weapon } => {
            render_weapon_filter_clause_editor(app, path.clone(), weapon, "Attacker weapon.")
        }
        ScoredEventFilterClause::DestroyedVehicle { vehicle } => {
            render_vehicle_filter_clause_editor(app, path.clone(), vehicle, "Destroyed vehicle.")
        }
        ScoredEventFilterClause::Any { clauses } => {
            render_nested_any_clause_editor(app, path.clone(), clauses)
        }
    };

    row![
        render_drag_handle(
            is_drag_source,
            Message::StartFilterClauseDrag(path.clone()),
            "Drag to reorder.",
        ),
        text("OR").width(32),
        pick_list(&FilterClauseKindChoice::NESTED_ALL[..], Some(kind), {
            let path = path.clone();
            move |choice| Message::ScoredEventFilterClauseKindChanged(path.clone(), choice)
        })
        .width(190),
        controls,
        with_tooltip(
            styled_button_row(icon_label("trash", "Delete"), ButtonTone::Danger)
                .on_press(Message::DeleteScoredEventFilterClause(path))
                .into(),
            "Delete clause.",
        ),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into()
}

fn render_text_filter_clause_editor<'a>(
    app: &'a App,
    key: FilterTextDraftKey,
    resolved_value: Option<String>,
    resolved_id: Option<String>,
    honu_url: Option<String>,
    placeholder: &'static str,
    tooltip: &'static str,
) -> Element<'a, Message> {
    let display_value = app
        .rules
        .filter_text_drafts
        .get(&key)
        .cloned()
        .or(resolved_value.clone())
        .unwrap_or_default();
    let status = resolved_id.unwrap_or_else(|| {
        if resolved_value.is_some() {
            "unresolved".into()
        } else {
            String::new()
        }
    });

    row![
        with_tooltip(
            text_input(placeholder, &display_value)
                .on_input({
                    let key = key.clone();
                    move |value| Message::RuleFilterDraftChanged(key.clone(), value)
                })
                .on_submit(Message::ResolveRuleFilterDraft(key.clone()))
                .width(190)
                .into(),
            tooltip,
        ),
        with_tooltip(
            styled_button("Resolve", ButtonTone::Secondary)
                .on_press(Message::ResolveRuleFilterDraft(key))
                .into(),
            "Census lookup.",
        ),
        if let Some(url) = honu_url {
            with_tooltip(
                styled_button("Honu", ButtonTone::Secondary)
                    .on_press(Message::OpenHonuReference(url))
                    .into(),
                "View on Honu.",
            )
        } else {
            styled_button("Honu", ButtonTone::Secondary).into()
        },
        text(status).size(12),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into()
}

fn render_vehicle_filter_clause_editor<'a>(
    app: &'a App,
    path: FilterClausePath,
    vehicle: VehicleMatchFilter,
    tooltip: &'static str,
) -> Element<'a, Message> {
    let current_option = current_vehicle_option(&app.rules.vehicle_options, &vehicle);
    let category = selected_vehicle_browse_category(app, &path, current_option.as_ref());
    let category_choices =
        vehicle_browse_category_choices(&app.rules.vehicle_options, current_option.as_ref());
    let vehicle_choices = vehicle_filter_choices(&app.rules.vehicle_options, &vehicle, category);

    row![
        with_tooltip(
            pick_list(category_choices, Some(category), {
                let path = path.clone();
                move |category| Message::ScoredEventVehicleCategoryChanged(path.clone(), category)
            })
            .width(170)
            .into(),
            "Vehicle category.",
        ),
        with_tooltip(
            pick_list(
                vehicle_choices,
                selected_vehicle_choice(&app.rules.vehicle_options, &vehicle),
                move |choice| { Message::ScoredEventVehicleChanged(path.clone(), choice) },
            )
            .width(220)
            .into(),
            tooltip,
        ),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into()
}

fn render_weapon_filter_clause_editor<'a>(
    app: &'a App,
    path: FilterClausePath,
    weapon: WeaponMatchFilter,
    tooltip: &'static str,
) -> Element<'a, Message> {
    let current_option = current_weapon_option(&app.rules.weapon_options, &weapon);
    let group = selected_weapon_browse_group(app, &path, current_option.as_ref());
    let group_choices =
        weapon_browse_group_choices(&app.rules.weapon_options, current_option.as_ref());
    let category = selected_weapon_browse_category(app, &path, current_option.as_ref(), &group);
    let category_choices =
        weapon_browse_category_choices(&app.rules.weapon_options, &group, current_option.as_ref());
    let faction =
        selected_weapon_browse_faction(app, &path, current_option.as_ref(), &group, &category);
    let faction_choices = weapon_browse_faction_choices(
        &app.rules.weapon_options,
        &group,
        &category,
        current_option.as_ref(),
    );
    let weapon_choices = weapon_filter_choices(
        &app.rules.weapon_options,
        &weapon,
        &group,
        &category,
        &faction,
    );

    row![
        with_tooltip(
            pick_list(group_choices, Some(group), {
                let path = path.clone();
                move |group| Message::ScoredEventWeaponGroupChanged(path.clone(), group)
            })
            .width(170)
            .into(),
            "Weapon group.",
        ),
        with_tooltip(
            pick_list(category_choices, Some(category), {
                let path = path.clone();
                move |category| Message::ScoredEventWeaponCategoryChanged(path.clone(), category)
            })
            .width(190)
            .into(),
            "Weapon category.",
        ),
        with_tooltip(
            pick_list(faction_choices, Some(faction), {
                let path = path.clone();
                move |faction| Message::ScoredEventWeaponFactionChanged(path.clone(), faction)
            })
            .width(130)
            .into(),
            "Faction filter.",
        ),
        with_tooltip(
            pick_list(
                weapon_choices,
                selected_weapon_choice(&app.rules.weapon_options, &weapon),
                move |choice| Message::ScoredEventWeaponChanged(path.clone(), choice),
            )
            .width(210)
            .into(),
            tooltip,
        ),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into()
}
