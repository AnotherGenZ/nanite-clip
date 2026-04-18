use std::collections::BTreeMap;
use std::path::Path;

use auraxis::Faction;
use chrono::Utc;
use iced::{Element, Length, mouse};

use crate::app::PendingProfileImport;
use crate::census;
use crate::db::{LookupKind, WeaponReferenceCacheEntry};
use crate::profile_transfer::ProfileTransferBundle;
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
use crate::ui::overlay::modal::modal;
use crate::ui::primitives::badge::{Tone as BadgeTone, badge};
use crate::ui::primitives::switch::switch as toggle_switch;

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
            app.rules_sub_view = sub_view;
        }
        Message::ToggleEventExpanded(rule_id, event_index) => {
            let key = (rule_id, event_index);
            if !app.rules_expanded_events.remove(&key) {
                app.rules_expanded_events.insert(key);
            }
        }
        Message::ToggleFilterExpanded(rule_id, event_index) => {
            let key = (rule_id, event_index);
            if !app.rules_expanded_filters.remove(&key) {
                app.rules_expanded_filters.insert(key);
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
                &[profile.clone()],
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
                    let Some(path) = super::settings::pick_file(
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
                app.pending_profile_import = Some(pending);
            }
            Ok(None) => {}
            Err(error) => app.set_rules_feedback(error, true),
        },
        Message::ConfirmProfileImportOverwrite => {
            let Some(pending) = app.pending_profile_import.take() else {
                return iced::Task::none();
            };
            apply_profile_import(app, pending, true);
        }
        Message::CancelProfileImportOverwrite => {
            app.pending_profile_import = None;
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
            app.selected_rule_id = Some(rule_id);
            ensure_selection(app);
        }
        Message::CreateRule => {
            let new_id = next_rule_id(app);
            app.config.rule_definitions.push(blank_rule_definition(
                &new_id,
                &format!("Rule {}", app.config.rule_definitions.len() + 1),
            ));
            app.selected_rule_id = Some(new_id);
            persist(app);
        }
        Message::DuplicateRule => {
            if let Some(index) = selected_rule_index(app) {
                let mut rule = app.config.rule_definitions[index].clone();
                rule.id = next_rule_id(app);
                rule.name = format!("{} Copy", rule.name);
                app.config.rule_definitions.push(rule.clone());
                app.selected_rule_id = Some(rule.id);
                persist(app);
            }
        }
        Message::DeleteRule => {
            if app.config.rule_definitions.len() > 1
                && let Some(rule_id) = app.selected_rule_id.clone()
            {
                app.config
                    .rule_definitions
                    .retain(|rule| rule.id != rule_id);
                for profile in &mut app.config.rule_profiles {
                    profile.enabled_rule_ids.retain(|id| id != &rule_id);
                }
                clear_rule_filter_ui_state(app, &rule_id);
                app.selected_rule_id = None;
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
                app.rule_drag_state = Some(RuleDragState::ScoredEvent(ScoredEventDragState {
                    rule_id: rule.id.clone(),
                    source_index,
                    target_index: source_index,
                    list_len: rule.scored_events.len(),
                }));
            }
        }
        Message::HoverScoredEventDrop(target_index) => {
            if let Some(RuleDragState::ScoredEvent(drag)) = &mut app.rule_drag_state {
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
            app.rule_drag_state = Some(RuleDragState::FilterClause(FilterClauseDragState {
                source_path: path,
                list_path,
                target_index: source_index,
                list_len,
            }));
        }
        Message::HoverFilterClauseDrop(list_path, target_index) => {
            if let Some(RuleDragState::FilterClause(drag)) = &mut app.rule_drag_state
                && drag.list_path == list_path
            {
                drag.target_index = target_index.min(drag.list_len);
            }
        }
        Message::RuleFilterDraftChanged(key, value) => {
            if value.trim().is_empty() {
                app.rule_filter_text_drafts.remove(&key);
            } else {
                app.rule_filter_text_drafts.insert(key, value);
            }
        }
        Message::ResolveRuleFilterDraft(key) => {
            let Some(input) = app
                .rule_filter_text_drafts
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
                    app.rule_filter_text_drafts
                        .insert(key.clone(), resolved.display_value().to_string());
                    persist(app);
                    app.set_rules_feedback(resolved.success_message(), false);
                }
            }
            Err(error) => app.set_rules_feedback(error, true),
        },
        Message::OpenHonuReference(url) => {
            return iced::Task::perform(async move { crate::launcher::open_url(&url) }, |result| {
                AppMessage::Rules(Message::HonuReferenceOpened(result))
            });
        }
        Message::HonuReferenceOpened(result) => {
            if let Err(error) = result {
                app.set_rules_feedback(format!("Failed to open Honu: {error}"), true);
            }
        }
        Message::VehicleOptionsLoaded(result) => match result {
            Ok(options) => app.rule_vehicle_options = options,
            Err(error) => {
                tracing::warn!("Failed to load vehicle options: {error}");
                if app.rule_vehicle_options.is_empty() {
                    app.set_rules_feedback(
                        "Failed to load vehicle filter options from Census.",
                        true,
                    );
                }
            }
        },
        Message::WeaponOptionsLoaded(result) => match result {
            Ok(options) => app.rule_weapon_options = options,
            Err(error) => {
                tracing::warn!("Failed to load weapon options: {error}");
                if app.rule_weapon_options.is_empty() {
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
            app.rule_weapon_browse_categories.remove(&key);
            app.rule_weapon_browse_factions.remove(&key);
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
            app.rule_weapon_browse_factions.remove(&key);
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
            if let Some(drag) = app.rule_drag_state.take() {
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

    let sub_view_tabs = tabs(app.rules_sub_view, Message::SetSubView)
        .push(Tab::new(RulesSubView::Rules, "Rules"))
        .push(
            Tab::new(RulesSubView::Profiles, "Profiles & Scheduling").badge(format!(
                "{}",
                app.config.rule_profiles.len() + app.config.auto_switch_rules.len()
            )),
        )
        .build();

    let header = page_header("Rules")
        .subtitle("Scoring rules, profiles, and auto-switching.")
        .action(sub_view_tabs)
        .build();

    let body: Element<'_, Message> = match app.rules_sub_view {
        RulesSubView::Rules => rules_split_view(app),
        RulesSubView::Profiles => profiles_view(app, &profile_options),
    };

    let content = column![header, body].spacing(12);
    let base: Element<'_, Message> = mouse_area(content)
        .on_release(Message::RuleDragReleased)
        .into();

    if let Some(pending) = &app.pending_profile_import {
        modal(
            base,
            profile_import_overwrite_dialog(pending),
            Some(Message::CancelProfileImportOverwrite),
        )
    } else {
        base
    }
}

// ---------------------------------------------------------------------------
// Rules sub-view: sidebar + editor split
// ---------------------------------------------------------------------------

fn rules_split_view(app: &App) -> Element<'_, Message> {
    let selected_rule_id = app.selected_rule_id.clone().unwrap_or_default();

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

    let actions_row = row![
        with_tooltip(
            styled_button_row(icon_label("plus", "New"), ButtonTone::Success)
                .on_press(Message::CreateRule)
                .into(),
            "New scoring rule with default weights.",
        ),
        with_tooltip(
            styled_button_row(icon_label("copy", "Dupe"), ButtonTone::Secondary)
                .on_press(Message::DuplicateRule)
                .into(),
            "Duplicate the selected rule.",
        ),
        with_tooltip(
            styled_button_row(icon_label("trash", "Del"), ButtonTone::Danger)
                .on_press(Message::DeleteRule)
                .into(),
            "Delete the selected rule from all profiles.",
        ),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);

    let mut rule_sidebar = sidebar(selected_rule_id, Message::SelectRule)
        .width(260.0)
        .header(column![profile_picker, actions_row].spacing(8));

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

    let mut profile_section = section("Active Profile")
        .description("Each profile controls which rules are enabled.")
        .push(
            row![
                field_label("Profile", "Switch the active profile.", 200.0),
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
            "Rename the active profile.",
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
                "Copy the active profile's enabled rules into a new profile.",
            ),
            with_tooltip(
                styled_button("Duplicate", ButtonTone::Secondary)
                    .on_press(Message::DuplicateActiveProfile)
                    .into(),
                "Duplicate the current profile.",
            ),
            with_tooltip(
                styled_button("Delete", ButtonTone::Danger)
                    .on_press(Message::DeleteActiveProfile)
                    .into(),
                "Delete the current profile (at least one must remain).",
            ),
            with_tooltip(
                styled_button("Export Active", ButtonTone::Secondary)
                    .on_press(Message::ExportActiveProfile)
                    .into(),
                "Export the active profile and the rules it enables.",
            ),
            with_tooltip(
                styled_button("Import", ButtonTone::Primary)
                    .on_press(Message::ImportProfiles)
                    .into(),
                "Import one or more profiles and their associated rules from a TOML bundle.",
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
    let mut content = panel("Automatic Profile Switching")
        .description("Switch the active profile by time or character.");

    if let Some(override_name) = app.manual_profile_override_name() {
        content = content.push(
            banner(format!(
                "Manual override: auto-switching paused while \"{override_name}\" is pinned."
            ))
            .warning()
            .description("Auto-switch is paused until the manual pin is cleared.")
            .build(),
        );
        content = content.push(with_tooltip(
            styled_button("Resume Auto-Switching", ButtonTone::Primary)
                .on_press(Message::ResumeAutoSwitching)
                .into(),
            "Clear the manual pin so auto-switch rules can run.",
        ));
    }

    if app.config.auto_switch_rules.is_empty() {
        content = content.push(
            empty_state("No auto-switch rules")
                .description("Switch profiles by schedule or active character.")
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
            "Delete this auto-switch rule.",
        ),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    let mut body = column![
        header_row,
        settings_text_field(
            "Rule Name",
            "Rename this auto-switch rule.",
            &auto_rule.name,
            {
                let rule_id = rule_id.clone();
                move |value| Message::RenameAutoSwitchRule(rule_id.clone(), value)
            }
        ),
        row![
            field_label(
                "Target Profile",
                "Profile that becomes active when this rule matches.",
                200.0,
            ),
            pick_list(profile_options.to_vec(), selected_profile, {
                let rule_id = rule_id.clone();
                move |profile| Message::AutoSwitchTargetProfileChanged(rule_id.clone(), profile.id)
            })
            .width(280),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
        row![
            field_label(
                "Condition",
                "Trigger by weekday/time schedule or monitored character.",
                200.0,
            ),
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
                    field_label(
                        "Active Characters",
                        "Switch when any selected character is being monitored.",
                        200.0,
                    ),
                    if character_options.is_empty() {
                        Element::<Message>::from(
                            text("Resolve at least one tracked character in the Characters tab.")
                                .size(13)
                                .width(Length::Fill),
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
                    field_label("Schedule Days", "Weekdays this schedule applies to.", 200.0,),
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
                    field_label(
                        "Time Window",
                        "Local time range that activates this rule on selected days.",
                        200.0,
                    ),
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
            .description("Pick a rule from the sidebar to configure its scoring behavior.")
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
                "Rename this rule (ID stays the same).",
                &rule.name,
                Message::RenameRule,
            ))
            .push(settings_pick_list_field(
                "Activation Class",
                "Only score activity from this character class.",
                &ClassFilterChoice::ALL[..],
                Some(ClassFilterChoice::from_option(rule.activation_class)),
                Message::ActivationClassChanged,
            ))
            .push(settings_stepper_field(
                "Lookback Window",
                "Seconds of recent activity this rule scores at once.",
                rule.lookback_secs,
                "sec",
                Message::LookbackStepped,
            ))
            .push(settings_stepper_field(
                "Trigger Threshold",
                "Score required inside the window before firing.",
                rule.trigger_threshold,
                "pts",
                Message::TriggerThresholdStepped,
            ))
            .push(settings_stepper_field(
                "Reset Threshold",
                "Score the window must fall below before firing again.",
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
    } else if let Some(feedback) = &app.rules_feedback {
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
        field_label(
            "Cooldown",
            "Minimum wait after a trigger before firing again.",
            200.0,
        ),
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
        "Minimum clip length before score scaling.",
        rule.base_duration_secs,
        "sec",
        Message::BaseDurationStepped,
    ));
    sec = sec.push(settings_stepper_field(
        "Seconds Per Point",
        "Extra clip time added per score point at trigger time.",
        rule.secs_per_point,
        "sec",
        Message::SecondsPerPointStepped,
    ));
    sec = sec.push(settings_stepper_field(
        "Max Duration",
        "Hard cap for the final computed clip duration.",
        rule.max_duration_secs,
        "sec",
        Message::MaxDurationStepped,
    ));

    // Duration preview badge
    let preview_text = if rule.use_full_buffer {
        "Always saves the full replay buffer".into()
    } else if rule.capture_entire_base_cap {
        "Facility captures use full buffer; other triggers use the score formula".into()
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
            field_label(
                "Full Buffer",
                "Always save the full replay buffer instead of the score formula.",
                200.0,
            ),
            toggle_switch(rule.use_full_buffer)
                .label(if rule.use_full_buffer { "On" } else { "Off" })
                .on_toggle(Message::ToggleFullBuffer),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    );

    sec = sec.push(
        row![
            field_label(
                "Entire Base Cap",
                "On facility capture, save the full buffer.",
                200.0,
            ),
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
            field_label(
                "Auto Extend",
                "Keep one clip open while matching events continue.",
                200.0,
            ),
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
            "Quiet time before the pending clip finalizes.",
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
    let mut sec = section("Scored Events")
        .description("Event types that contribute points within the lookback window.");

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
            .rules_expanded_events
            .contains(&(rule.id.clone(), event_index));
        let is_drag_source = active_scored_event_drag(app, &rule.id)
            .is_some_and(|drag| drag.source_index == event_index);

        let filter_summary = scored_event_filter_summary(
            &scored_event.filters,
            &app.rule_vehicle_options,
            &app.rule_weapon_options,
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
                if is_expanded {
                    "Click to collapse this event's editor."
                } else {
                    "Click to expand this event's editor."
                },
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
                "Remove this scored event.",
            ),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center);

        let event_card = if is_expanded {
            // Expanded: full editor
            let filters_enabled = scored_event.filters.is_enabled();
            let filter_groups = scored_event.filters.groups().into_owned();
            let filters_expanded = app
                .rules_expanded_filters
                .contains(&(rule.id.clone(), event_index));

            let filter_controls: Element<'_, Message> = if filters_enabled && filters_expanded {
                let mut groups_col = column![].spacing(8);
                if filter_groups.is_empty() {
                    groups_col = groups_col.push(
                        text("No filter groups yet. Add one to start building AND / OR logic.")
                            .size(12),
                    );
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
                    "Add another OR branch (any group matching satisfies the filter).",
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
                    "Click to expand the filter editor.",
                )
            } else {
                text("No extra filters: any matching event kind contributes.")
                    .size(12)
                    .into()
            };

            card()
                .body(
                    column![
                        summary_row,
                        row![
                            field_label("Event Type", "Event type this weight row scores.", 120.0,),
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
                            field_label(
                                "Points",
                                "Points awarded each time this event fires.",
                                120.0,
                            ),
                            styled_button("-", ButtonTone::Secondary)
                                .on_press(Message::ScoredEventPointsStepped(event_index, -1,)),
                            text(format!("{} pts", scored_event.points)).width(90),
                            styled_button("+", ButtonTone::Secondary)
                                .on_press(Message::ScoredEventPointsStepped(event_index, 1)),
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center),
                        row![
                            field_label(
                                "Filters",
                                "Optionally match a target, vehicle, or weapon.",
                                120.0,
                            ),
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

    let mut sec = section("Live Runtime").description("Real-time scoring state for this rule.");

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
                "Pending clip stays open until {} unless another matching event refreshes it.",
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
            app.pending_profile_import = None;
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
            app.pending_profile_import = None;
            app.set_rules_feedback(error, true);
        }
    }
}

fn default_profile_export_path(app: &App, profile_name: &str) -> String {
    let file_name = format!(
        "nanite-clip-profile-{}.toml",
        slugify_profile_name(profile_name)
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

fn slugify_profile_name(profile_name: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;

    for ch in profile_name.chars() {
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

fn profile_import_overwrite_dialog<'a>(pending: &'a PendingProfileImport) -> Element<'a, Message> {
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
        banner(format!("{conflict_count} existing item(s) already use the same id."))
            .warning()
            .description(
                "Continuing will replace the matching profiles and rules. Overwritten rules also affect any existing profiles that already enable them.",
            )
            .build(),
    ]
    .spacing(12)
    .width(Length::Fill);

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
        app.selected_rule_id = app
            .config
            .rule_definitions
            .first()
            .map(|rule| rule.id.clone());
    }
}

fn selected_rule_index(app: &App) -> Option<usize> {
    app.selected_rule_id.as_deref().and_then(|rule_id| {
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
    for (mut key, value) in std::mem::take(&mut app.rule_vehicle_browse_categories) {
        if key.rule_id == rule_id {
            key.event_index = reorder_index(key.event_index, source_index, target_index, list_len);
        }
        categories.insert(key, value);
    }
    app.rule_vehicle_browse_categories = categories;

    let mut weapon_categories = BTreeMap::new();
    for (mut key, value) in std::mem::take(&mut app.rule_weapon_browse_categories) {
        if key.rule_id == rule_id {
            key.event_index = reorder_index(key.event_index, source_index, target_index, list_len);
        }
        weapon_categories.insert(key, value);
    }
    app.rule_weapon_browse_categories = weapon_categories;

    let mut weapon_groups = BTreeMap::new();
    for (mut key, value) in std::mem::take(&mut app.rule_weapon_browse_groups) {
        if key.rule_id == rule_id {
            key.event_index = reorder_index(key.event_index, source_index, target_index, list_len);
        }
        weapon_groups.insert(key, value);
    }
    app.rule_weapon_browse_groups = weapon_groups;

    let mut weapon_factions = BTreeMap::new();
    for (mut key, value) in std::mem::take(&mut app.rule_weapon_browse_factions) {
        if key.rule_id == rule_id {
            key.event_index = reorder_index(key.event_index, source_index, target_index, list_len);
        }
        weapon_factions.insert(key, value);
    }
    app.rule_weapon_browse_factions = weapon_factions;

    let mut drafts = BTreeMap::new();
    for (mut key, value) in std::mem::take(&mut app.rule_filter_text_drafts) {
        if key.path.rule_id == rule_id {
            key.path.event_index =
                reorder_index(key.path.event_index, source_index, target_index, list_len);
        }
        drafts.insert(key, value);
    }
    app.rule_filter_text_drafts = drafts;
}

fn reindex_rule_ui_state_after_clause_reorder(
    app: &mut App,
    list_path: &FilterClauseListPath,
    source_index: usize,
    target_index: usize,
    list_len: usize,
) {
    let mut categories = BTreeMap::new();
    for (mut key, value) in std::mem::take(&mut app.rule_vehicle_browse_categories) {
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
    app.rule_vehicle_browse_categories = categories;

    let mut weapon_categories = BTreeMap::new();
    for (mut key, value) in std::mem::take(&mut app.rule_weapon_browse_categories) {
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
    app.rule_weapon_browse_categories = weapon_categories;

    let mut weapon_groups = BTreeMap::new();
    for (mut key, value) in std::mem::take(&mut app.rule_weapon_browse_groups) {
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
    app.rule_weapon_browse_groups = weapon_groups;

    let mut weapon_factions = BTreeMap::new();
    for (mut key, value) in std::mem::take(&mut app.rule_weapon_browse_factions) {
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
    app.rule_weapon_browse_factions = weapon_factions;

    let mut drafts = BTreeMap::new();
    for (mut key, value) in std::mem::take(&mut app.rule_filter_text_drafts) {
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
    app.rule_filter_text_drafts = drafts;
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
    app.rule_vehicle_browse_categories
        .retain(|key, _| key.rule_id != rule_id);
    app.rule_weapon_browse_groups
        .retain(|key, _| key.rule_id != rule_id);
    app.rule_weapon_browse_categories
        .retain(|key, _| key.rule_id != rule_id);
    app.rule_weapon_browse_factions
        .retain(|key, _| key.rule_id != rule_id);
    app.rule_filter_text_drafts
        .retain(|key, _| key.path.rule_id != rule_id);
    if app
        .rule_drag_state
        .as_ref()
        .is_some_and(|drag| drag.rule_id() == rule_id)
    {
        app.rule_drag_state = None;
    }
}

fn clear_filter_clause_resolution(
    app: &mut App,
    key: &FilterTextDraftKey,
) -> iced::Task<AppMessage> {
    app.rule_filter_text_drafts.remove(key);
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
    match app.rule_drag_state.as_ref() {
        Some(RuleDragState::ScoredEvent(drag)) if drag.rule_id == rule_id => Some(drag),
        _ => None,
    }
}

fn active_filter_clause_drag<'a>(
    app: &'a App,
    list_path: &FilterClauseListPath,
) -> Option<&'a FilterClauseDragState> {
    match app.rule_drag_state.as_ref() {
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
                "Add another AND clause to this group.",
            ),
            with_tooltip(
                styled_button_row(icon_label("trash", "Delete Group"), ButtonTone::Danger)
                    .on_press(Message::DeleteScoredEventFilterGroup(
                        event_index,
                        group_index
                    ))
                    .into(),
                "Delete this OR group.",
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
        app.rule_drag_state.as_ref(),
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
            "Character name; press Resolve to look up via Census.",
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
            "Outfit tag; press Resolve to look up via Census.",
        ),
        ScoredEventFilterClause::AttackerVehicle { vehicle } => {
            render_vehicle_filter_clause_editor(
                app,
                path.clone(),
                vehicle,
                "Match the attacker's vehicle type.",
            )
        }
        ScoredEventFilterClause::AttackerWeapon { weapon } => render_weapon_filter_clause_editor(
            app,
            path.clone(),
            weapon,
            "Match the attacker's weapon.",
        ),
        ScoredEventFilterClause::DestroyedVehicle { vehicle } => {
            render_vehicle_filter_clause_editor(
                app,
                path.clone(),
                vehicle,
                "Match the destroyed vehicle type.",
            )
        }
        ScoredEventFilterClause::Any { clauses } => {
            render_nested_any_clause_editor(app, path.clone(), clauses)
        }
    };

    row![
        render_drag_handle(
            is_drag_source,
            Message::StartFilterClauseDrag(path.clone()),
            "Drag to reorder AND conditions.",
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
            "Delete this clause from the group.",
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
                "Add another OR branch inside this nested clause.",
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
        app.rule_drag_state.as_ref(),
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
            "Character name; press Resolve to look up via Census.",
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
            "Outfit tag; press Resolve to look up via Census.",
        ),
        ScoredEventFilterClause::AttackerVehicle { vehicle } => {
            render_vehicle_filter_clause_editor(
                app,
                path.clone(),
                vehicle,
                "Match the attacker's vehicle type.",
            )
        }
        ScoredEventFilterClause::AttackerWeapon { weapon } => render_weapon_filter_clause_editor(
            app,
            path.clone(),
            weapon,
            "Match the attacker's weapon.",
        ),
        ScoredEventFilterClause::DestroyedVehicle { vehicle } => {
            render_vehicle_filter_clause_editor(
                app,
                path.clone(),
                vehicle,
                "Match the destroyed vehicle type.",
            )
        }
        ScoredEventFilterClause::Any { clauses } => {
            render_nested_any_clause_editor(app, path.clone(), clauses)
        }
    };

    row![
        render_drag_handle(
            is_drag_source,
            Message::StartFilterClauseDrag(path.clone()),
            "Drag to reorder OR branches.",
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
            "Delete this OR branch.",
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
        .rule_filter_text_drafts
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
            "Look up via Census and store the stable ID.",
        ),
        if let Some(url) = honu_url {
            with_tooltip(
                styled_button("Honu", ButtonTone::Secondary)
                    .on_press(Message::OpenHonuReference(url))
                    .into(),
                "Open this resolved reference on Honu",
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
    let current_option = current_vehicle_option(&app.rule_vehicle_options, &vehicle);
    let category = selected_vehicle_browse_category(app, &path, current_option.as_ref());
    let category_choices =
        vehicle_browse_category_choices(&app.rule_vehicle_options, current_option.as_ref());
    let vehicle_choices = vehicle_filter_choices(&app.rule_vehicle_options, &vehicle, category);

    row![
        with_tooltip(
            pick_list(category_choices, Some(category), {
                let path = path.clone();
                move |category| Message::ScoredEventVehicleCategoryChanged(path.clone(), category)
            })
            .width(170)
            .into(),
            "Choose a vehicle group first to narrow the list.",
        ),
        with_tooltip(
            pick_list(
                vehicle_choices,
                selected_vehicle_choice(&app.rule_vehicle_options, &vehicle),
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
    let current_option = current_weapon_option(&app.rule_weapon_options, &weapon);
    let group = selected_weapon_browse_group(app, &path, current_option.as_ref());
    let group_choices =
        weapon_browse_group_choices(&app.rule_weapon_options, current_option.as_ref());
    let category = selected_weapon_browse_category(app, &path, current_option.as_ref(), &group);
    let category_choices =
        weapon_browse_category_choices(&app.rule_weapon_options, &group, current_option.as_ref());
    let faction =
        selected_weapon_browse_faction(app, &path, current_option.as_ref(), &group, &category);
    let faction_choices = weapon_browse_faction_choices(
        &app.rule_weapon_options,
        &group,
        &category,
        current_option.as_ref(),
    );
    let weapon_choices = weapon_filter_choices(
        &app.rule_weapon_options,
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
            "Choose a broad weapon group first to narrow the list.",
        ),
        with_tooltip(
            pick_list(category_choices, Some(category), {
                let path = path.clone();
                move |category| Message::ScoredEventWeaponCategoryChanged(path.clone(), category)
            })
            .width(190)
            .into(),
            "Choose a weapon subcategory within that group.",
        ),
        with_tooltip(
            pick_list(faction_choices, Some(faction), {
                let path = path.clone();
                move |faction| Message::ScoredEventWeaponFactionChanged(path.clone(), faction)
            })
            .width(130)
            .into(),
            "Filter that category by faction grouping.",
        ),
        with_tooltip(
            pick_list(
                weapon_choices,
                selected_weapon_choice(&app.rule_weapon_options, &weapon),
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

fn weapon_lookup_options(entries: Vec<WeaponReferenceCacheEntry>) -> Vec<WeaponLookupOption> {
    let mut grouped = BTreeMap::<u32, Vec<WeaponReferenceCacheEntry>>::new();
    for entry in entries {
        let family_id = if entry.weapon_id != 0 {
            entry.weapon_id
        } else {
            entry.item_id
        };
        grouped.entry(family_id).or_default().push(entry);
    }

    let mut options = grouped
        .into_values()
        .map(|family| {
            let mut ids = family.iter().map(|entry| entry.item_id).collect::<Vec<_>>();
            ids.sort_unstable();
            ids.dedup();

            WeaponLookupOption {
                ids,
                label: weapon_family_display_name(&family),
                category_label: weapon_family_category_label(&family),
                faction: weapon_family_faction(&family),
            }
        })
        .collect::<Vec<_>>();

    options.sort_by(|left, right| {
        left.category_label
            .to_lowercase()
            .cmp(&right.category_label.to_lowercase())
            .then(left.label.to_lowercase().cmp(&right.label.to_lowercase()))
            .then(left.ids.cmp(&right.ids))
    });

    options
}

fn weapon_family_display_name(entries: &[WeaponReferenceCacheEntry]) -> String {
    entries
        .iter()
        .min_by(|left, right| {
            left.display_name
                .trim()
                .to_lowercase()
                .cmp(&right.display_name.trim().to_lowercase())
                .then(left.display_name.len().cmp(&right.display_name.len()))
                .then(left.item_id.cmp(&right.item_id))
        })
        .map(|entry| entry.display_name.trim().to_string())
        .unwrap_or_else(|| "Unknown weapon".into())
}

fn weapon_family_category_label(entries: &[WeaponReferenceCacheEntry]) -> String {
    let mut categories = entries
        .iter()
        .map(|entry| entry.category_label.trim().to_string())
        .filter(|label| !label.is_empty())
        .collect::<Vec<_>>();
    categories.sort_by(|left, right| {
        left.to_lowercase()
            .cmp(&right.to_lowercase())
            .then(left.cmp(right))
    });
    categories.dedup_by(|left, right| left.eq_ignore_ascii_case(right));

    if categories.len() <= 1 {
        return categories
            .into_iter()
            .next()
            .unwrap_or_else(|| "Other".into());
    }

    let mut platforms = categories
        .iter()
        .filter_map(|label| weapon_slot_platform_name(label))
        .collect::<Vec<_>>();
    platforms.sort();
    platforms.dedup();

    if platforms.len() == 1 {
        format!("{} weapon family", platforms[0])
    } else {
        categories
            .into_iter()
            .next()
            .unwrap_or_else(|| "Other".into())
    }
}

fn weapon_family_faction(entries: &[WeaponReferenceCacheEntry]) -> WeaponBrowseFaction {
    let mut factions = entries
        .iter()
        .filter_map(|entry| entry.faction)
        .map(WeaponBrowseFaction::from_faction)
        .filter(|faction| *faction != WeaponBrowseFaction::Other)
        .collect::<Vec<_>>();
    factions.sort();
    factions.dedup();

    match factions.as_slice() {
        [faction] => *faction,
        [] => WeaponBrowseFaction::Other,
        _ => WeaponBrowseFaction::Other,
    }
}

fn weapon_slot_platform_name(label: &str) -> Option<String> {
    let normalized = label.trim();
    let (platform, suffix) = normalized.split_once(' ')?;
    let suffix = suffix.to_ascii_lowercase();
    let slot_markers = [
        "left",
        "right",
        "rear",
        "front",
        "top",
        "tail",
        "nose",
        "belly",
        "wing",
        "primary",
        "secondary",
        "gunner",
        "turret",
        "bombard",
        "defense",
        "mount",
        "cannon",
        "weapon",
        "weapons",
    ];

    slot_markers
        .iter()
        .any(|marker| suffix.contains(marker))
        .then(|| platform.to_string())
}

fn weapon_browse_group_choices(
    options: &[WeaponLookupOption],
    current: Option<&WeaponLookupOption>,
) -> Vec<WeaponBrowseGroup> {
    let mut groups = vec![WeaponBrowseGroup::All];
    let mut labels = options
        .iter()
        .map(weapon_browse_group_for_option)
        .collect::<Vec<_>>();
    labels.sort();
    labels.dedup();
    groups.extend(labels);

    if let Some(current_option) = current {
        let current_group = weapon_browse_group_for_option(current_option);
        if !groups.contains(&current_group) {
            groups.push(current_group);
        }
    }

    groups
}

fn weapon_browse_category_choices(
    options: &[WeaponLookupOption],
    group: &WeaponBrowseGroup,
    current: Option<&WeaponLookupOption>,
) -> Vec<WeaponBrowseCategory> {
    let mut categories = vec![WeaponBrowseCategory::All];
    let mut labels = options
        .iter()
        .filter(|option| {
            *group == WeaponBrowseGroup::All || weapon_browse_group_for_option(option) == *group
        })
        .map(|option| WeaponBrowseCategory::Category(option.category_label.clone()))
        .collect::<Vec<_>>();
    labels.sort_by(|left, right| {
        left.label()
            .to_lowercase()
            .cmp(&right.label().to_lowercase())
            .then(left.label().cmp(right.label()))
    });
    labels.dedup();
    categories.extend(labels);

    if let Some(current_option) = current {
        let current_category =
            WeaponBrowseCategory::Category(current_option.category_label.clone());
        if !categories.contains(&current_category) {
            categories.push(current_category);
        }
    }

    categories
}

fn weapon_browse_faction_choices(
    options: &[WeaponLookupOption],
    group: &WeaponBrowseGroup,
    category: &WeaponBrowseCategory,
    current: Option<&WeaponLookupOption>,
) -> Vec<WeaponBrowseFaction> {
    let mut factions = vec![WeaponBrowseFaction::All];
    let mut labels = options
        .iter()
        .filter(|option| {
            (*group == WeaponBrowseGroup::All || weapon_browse_group_for_option(option) == *group)
                && (*category == WeaponBrowseCategory::All
                    || option.category_label.eq_ignore_ascii_case(category.label()))
        })
        .map(|option| option.faction)
        .collect::<Vec<_>>();
    labels.sort();
    labels.dedup();
    factions.extend(labels);

    if let Some(current_option) = current
        && !factions.contains(&current_option.faction)
    {
        factions.push(current_option.faction);
    }

    factions
}

fn weapon_filter_choices(
    options: &[WeaponLookupOption],
    current: &WeaponMatchFilter,
    group: &WeaponBrowseGroup,
    category: &WeaponBrowseCategory,
    faction: &WeaponBrowseFaction,
) -> Vec<WeaponFilterChoice> {
    let filtered_options = options
        .iter()
        .filter(|option| {
            (*group == WeaponBrowseGroup::All || weapon_browse_group_for_option(option) == *group)
                && (*category == WeaponBrowseCategory::All
                    || option.category_label.eq_ignore_ascii_case(category.label()))
                && (*faction == WeaponBrowseFaction::All || option.faction == *faction)
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut choices = Vec::with_capacity(filtered_options.len() + 1);
    choices.push(WeaponFilterChoice::Any);
    choices.extend(
        filtered_options
            .into_iter()
            .map(WeaponFilterChoice::Specific),
    );

    if let Some(current_option) = current_weapon_option(options, current)
        && !choices
            .iter()
            .any(|choice| matches!(choice, WeaponFilterChoice::Specific(option) if option == &current_option))
        {
            choices.push(WeaponFilterChoice::Specific(current_option));
        }

    choices
}

fn selected_weapon_choice(
    options: &[WeaponLookupOption],
    current: &WeaponMatchFilter,
) -> Option<WeaponFilterChoice> {
    Some(match current_weapon_option(options, current) {
        Some(option) => WeaponFilterChoice::Specific(option),
        None => WeaponFilterChoice::Any,
    })
}

fn current_weapon_option(
    options: &[WeaponLookupOption],
    current: &WeaponMatchFilter,
) -> Option<WeaponLookupOption> {
    if !current.weapon.ids.is_empty() {
        if let Some(option) = options
            .iter()
            .find(|option| option.ids == current.weapon.ids)
        {
            return Some(option.clone());
        }

        return Some(WeaponLookupOption {
            ids: current.weapon.ids.clone(),
            label: current
                .weapon
                .label
                .clone()
                .unwrap_or_else(|| format!("Weapon group ({})", current.weapon.ids.len())),
            category_label: "Other".into(),
            faction: WeaponBrowseFaction::Other,
        });
    }

    current.legacy_weapon_id.and_then(|weapon_id| {
        options
            .iter()
            .find(|option| option.ids.contains(&weapon_id))
            .cloned()
            .or_else(|| {
                Some(WeaponLookupOption {
                    ids: vec![weapon_id],
                    label: format!("Weapon #{weapon_id}"),
                    category_label: "Other".into(),
                    faction: WeaponBrowseFaction::Other,
                })
            })
    })
}

fn apply_weapon_filter_choice(filter: &mut WeaponMatchFilter, choice: WeaponFilterChoice) {
    match choice {
        WeaponFilterChoice::Any => *filter = WeaponMatchFilter::default(),
        WeaponFilterChoice::Specific(option) => {
            filter.weapon.label = Some(option.label);
            filter.weapon.ids = option.ids;
            filter.weapon.normalize();
            filter.legacy_weapon_id = None;
        }
    }
}

fn weapon_match_filter_mut(clause: &mut ScoredEventFilterClause) -> Option<&mut WeaponMatchFilter> {
    match clause {
        ScoredEventFilterClause::AttackerWeapon { weapon } => Some(weapon),
        _ => None,
    }
}

fn selected_weapon_browse_category(
    app: &App,
    path: &FilterClausePath,
    current_option: Option<&WeaponLookupOption>,
    group: &WeaponBrowseGroup,
) -> WeaponBrowseCategory {
    let key = WeaponBrowseKey::new(
        &path.rule_id,
        path.event_index,
        path.group_index,
        &path.clause_path,
    );
    let explicit_group = app.rule_weapon_browse_groups.get(&key).copied();

    app.rule_weapon_browse_categories
        .get(&key)
        .cloned()
        .filter(|category| {
            *category == WeaponBrowseCategory::All
                || options_have_weapon_category_in_group(
                    &app.rule_weapon_options,
                    group,
                    category.label(),
                )
        })
        .or_else(|| {
            current_option.and_then(|option| {
                let current_category =
                    WeaponBrowseCategory::Category(option.category_label.clone());
                ((*group == WeaponBrowseGroup::All
                    || weapon_browse_group_for_option(option) == *group)
                    && explicit_group.is_none_or(|selected_group| selected_group == *group))
                .then_some(current_category)
            })
        })
        .unwrap_or(WeaponBrowseCategory::All)
}

fn selected_weapon_browse_group(
    app: &App,
    path: &FilterClausePath,
    current_option: Option<&WeaponLookupOption>,
) -> WeaponBrowseGroup {
    app.rule_weapon_browse_groups
        .get(&WeaponBrowseKey::new(
            &path.rule_id,
            path.event_index,
            path.group_index,
            &path.clause_path,
        ))
        .copied()
        .or_else(|| current_option.map(weapon_browse_group_for_option))
        .unwrap_or(WeaponBrowseGroup::All)
}

fn selected_weapon_browse_faction(
    app: &App,
    path: &FilterClausePath,
    current_option: Option<&WeaponLookupOption>,
    group: &WeaponBrowseGroup,
    category: &WeaponBrowseCategory,
) -> WeaponBrowseFaction {
    let key = WeaponBrowseKey::new(
        &path.rule_id,
        path.event_index,
        path.group_index,
        &path.clause_path,
    );

    app.rule_weapon_browse_factions
        .get(&key)
        .copied()
        .filter(|faction| {
            *faction == WeaponBrowseFaction::All
                || options_have_weapon_faction_in_scope(
                    &app.rule_weapon_options,
                    group,
                    category,
                    faction,
                )
        })
        .or_else(|| {
            current_option.and_then(|option| {
                ((*group == WeaponBrowseGroup::All
                    || weapon_browse_group_for_option(option) == *group)
                    && (*category == WeaponBrowseCategory::All
                        || option.category_label.eq_ignore_ascii_case(category.label())))
                .then_some(option.faction)
            })
        })
        .unwrap_or(WeaponBrowseFaction::All)
}

fn set_weapon_browse_group(app: &mut App, key: WeaponBrowseKey, group: WeaponBrowseGroup) {
    if group == WeaponBrowseGroup::All {
        app.rule_weapon_browse_groups.remove(&key);
    } else {
        app.rule_weapon_browse_groups.insert(key, group);
    }
}

fn set_weapon_browse_category(app: &mut App, key: WeaponBrowseKey, category: WeaponBrowseCategory) {
    if category == WeaponBrowseCategory::All {
        app.rule_weapon_browse_categories.remove(&key);
    } else {
        app.rule_weapon_browse_categories.insert(key, category);
    }
}

fn set_weapon_browse_faction(app: &mut App, key: WeaponBrowseKey, faction: WeaponBrowseFaction) {
    if faction == WeaponBrowseFaction::All {
        app.rule_weapon_browse_factions.remove(&key);
    } else {
        app.rule_weapon_browse_factions.insert(key, faction);
    }
}

fn options_have_weapon_category_in_group(
    options: &[WeaponLookupOption],
    group: &WeaponBrowseGroup,
    category_label: &str,
) -> bool {
    options.iter().any(|option| {
        (*group == WeaponBrowseGroup::All || weapon_browse_group_for_option(option) == *group)
            && option.category_label.eq_ignore_ascii_case(category_label)
    })
}

fn options_have_weapon_faction_in_scope(
    options: &[WeaponLookupOption],
    group: &WeaponBrowseGroup,
    category: &WeaponBrowseCategory,
    faction: &WeaponBrowseFaction,
) -> bool {
    options.iter().any(|option| {
        (*group == WeaponBrowseGroup::All || weapon_browse_group_for_option(option) == *group)
            && (*category == WeaponBrowseCategory::All
                || option.category_label.eq_ignore_ascii_case(category.label()))
            && option.faction == *faction
    })
}

fn weapon_browse_group_for_option(option: &WeaponLookupOption) -> WeaponBrowseGroup {
    weapon_browse_group_for_category_label(&option.category_label)
}

fn weapon_browse_group_for_category_label(label: &str) -> WeaponBrowseGroup {
    let normalized = label.trim().to_ascii_lowercase();

    if matches_any_weapon_category(
        &normalized,
        &[
            "knife",
            "pistol",
            "shotgun",
            "smg",
            "lmg",
            "assault rifle",
            "carbine",
            "sniper rifle",
            "scout rifle",
            "rocket launcher",
            "heavy weapon",
            "battle rifle",
            "crossbow",
            "heavy crossbow",
            "amphibious rifle",
            "amphibious sidearm",
            "anti-materiel rifle",
            "grenade",
            "explosive",
        ],
    ) {
        return WeaponBrowseGroup::Infantry;
    }

    if normalized.contains("max") {
        return WeaponBrowseGroup::Max;
    }

    if contains_any_weapon_category(
        &normalized,
        &[
            "galaxy",
            "liberator",
            "mosquito",
            "reaver",
            "scythe",
            "valkyrie",
            "dervish",
            "aerial combat",
        ],
    ) {
        return WeaponBrowseGroup::AirVehicles;
    }

    if contains_any_weapon_category(&normalized, &["corsair"]) {
        return WeaponBrowseGroup::NavalVehicles;
    }

    if contains_any_weapon_category(&normalized, &["bastion", "colossus"]) {
        return WeaponBrowseGroup::HeavyPlatforms;
    }

    if contains_any_weapon_category(
        &normalized,
        &[
            "flash",
            "harasser",
            "lightning",
            "magrider",
            "prowler",
            "sunderer",
            "vanguard",
            "ant",
            "javelin",
            "chimera",
            "deliverer",
        ],
    ) {
        return WeaponBrowseGroup::GroundVehicles;
    }

    WeaponBrowseGroup::Other
}

fn matches_any_weapon_category(normalized: &str, categories: &[&str]) -> bool {
    categories.contains(&normalized)
}

fn contains_any_weapon_category(normalized: &str, categories: &[&str]) -> bool {
    categories
        .iter()
        .any(|category| normalized.contains(category))
}

fn vehicle_lookup_options(entries: Vec<(i64, String)>) -> Vec<LookupOption> {
    let mut grouped = BTreeMap::<String, LookupOption>::new();
    for (id, label) in entries {
        let normalized = label.trim().to_lowercase();
        let entry = grouped.entry(normalized).or_insert_with(|| LookupOption {
            ids: Vec::new(),
            label: label.clone(),
        });
        entry.ids.push(id as u16);
        entry.ids.sort_unstable();
        entry.ids.dedup();
    }

    grouped.into_values().collect()
}

fn vehicle_browse_category_choices(
    options: &[LookupOption],
    current: Option<&LookupOption>,
) -> Vec<VehicleBrowseCategory> {
    let mut choices = vec![VehicleBrowseCategory::All];
    for category in VehicleBrowseCategory::browse_categories() {
        if options
            .iter()
            .any(|option| vehicle_browse_category_for_option(option) == category)
        {
            choices.push(category);
        }
    }

    if let Some(current_option) = current {
        let current_category = vehicle_browse_category_for_option(current_option);
        if !choices.contains(&current_category) {
            choices.push(current_category);
        }
    }

    choices
}

fn vehicle_filter_choices(
    options: &[LookupOption],
    current: &VehicleMatchFilter,
    category: VehicleBrowseCategory,
) -> Vec<VehicleFilterChoice> {
    let filtered_options = options
        .iter()
        .filter(|option| {
            category == VehicleBrowseCategory::All
                || vehicle_browse_category_for_option(option) == category
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut choices = Vec::with_capacity(filtered_options.len() + 1);
    choices.push(VehicleFilterChoice::Any);
    choices.extend(
        filtered_options
            .into_iter()
            .map(VehicleFilterChoice::Specific),
    );

    if let Some(current_option) = current_vehicle_option(options, current)
        && !choices
            .iter()
            .any(|choice| matches!(choice, VehicleFilterChoice::Specific(option) if option == &current_option))
        {
            choices.push(VehicleFilterChoice::Specific(current_option));
        }

    choices
}

fn selected_vehicle_choice(
    options: &[LookupOption],
    current: &VehicleMatchFilter,
) -> Option<VehicleFilterChoice> {
    Some(match current_vehicle_option(options, current) {
        Some(option) => VehicleFilterChoice::Specific(option),
        None => VehicleFilterChoice::Any,
    })
}

fn current_vehicle_option(
    options: &[LookupOption],
    current: &VehicleMatchFilter,
) -> Option<LookupOption> {
    if !current.vehicle.ids.is_empty() {
        if let Some(option) = options
            .iter()
            .find(|option| option.ids == current.vehicle.ids)
        {
            return Some(option.clone());
        }

        return Some(LookupOption {
            ids: current.vehicle.ids.clone(),
            label: current
                .vehicle
                .label
                .clone()
                .unwrap_or_else(|| format!("Vehicle group ({})", current.vehicle.ids.len())),
        });
    }

    current.legacy_vehicle_id.and_then(|vehicle_id| {
        options
            .iter()
            .find(|option| option.ids.contains(&vehicle_id))
            .cloned()
            .or_else(|| {
                Some(LookupOption {
                    ids: vec![vehicle_id],
                    label: format!("Vehicle #{vehicle_id}"),
                })
            })
    })
}

fn apply_vehicle_filter_choice(filter: &mut VehicleMatchFilter, choice: VehicleFilterChoice) {
    match choice {
        VehicleFilterChoice::Any => *filter = VehicleMatchFilter::default(),
        VehicleFilterChoice::Specific(option) => {
            filter.vehicle.label = Some(option.label);
            filter.vehicle.ids = option.ids;
            filter.vehicle.normalize();
            filter.legacy_vehicle_id = None;
        }
    }
}

fn vehicle_match_filter_mut(
    clause: &mut ScoredEventFilterClause,
) -> Option<&mut VehicleMatchFilter> {
    match clause {
        ScoredEventFilterClause::AttackerVehicle { vehicle }
        | ScoredEventFilterClause::DestroyedVehicle { vehicle } => Some(vehicle),
        _ => None,
    }
}

fn selected_vehicle_browse_category(
    app: &App,
    path: &FilterClausePath,
    current_option: Option<&LookupOption>,
) -> VehicleBrowseCategory {
    app.rule_vehicle_browse_categories
        .get(&VehicleBrowseKey::new(
            &path.rule_id,
            path.event_index,
            path.group_index,
            &path.clause_path,
        ))
        .copied()
        .or_else(|| current_option.map(vehicle_browse_category_for_option))
        .unwrap_or(VehicleBrowseCategory::All)
}

fn set_vehicle_browse_category(
    app: &mut App,
    key: VehicleBrowseKey,
    category: VehicleBrowseCategory,
) {
    if category == VehicleBrowseCategory::All {
        app.rule_vehicle_browse_categories.remove(&key);
    } else {
        app.rule_vehicle_browse_categories.insert(key, category);
    }
}

fn vehicle_browse_category_for_option(option: &LookupOption) -> VehicleBrowseCategory {
    let normalized = option.label.trim().to_ascii_lowercase();

    if matches_any_vehicle_name(
        &normalized,
        &[
            "galaxy",
            "liberator",
            "mosquito",
            "reaver",
            "scythe",
            "valkyrie",
            "dervish",
            "bastion",
            "lodestar",
        ],
    ) {
        return VehicleBrowseCategory::Air;
    }

    if matches_any_vehicle_name(
        &normalized,
        &[
            "ant",
            "chimera",
            "colossus",
            "deliverer",
            "flash",
            "harasser",
            "javelin",
            "lightning",
            "magrider",
            "prowler",
            "sunderer",
            "vanguard",
        ],
    ) {
        return VehicleBrowseCategory::Ground;
    }

    if matches_any_vehicle_name(&normalized, &["recon drone"]) {
        return VehicleBrowseCategory::Infantry;
    }

    if normalized.contains("mana") {
        return VehicleBrowseCategory::Infantry;
    }

    if matches_any_vehicle_name(
        &normalized,
        &[
            "spear phalanx turret",
            "aspis phalanx turret",
            "xiphos phalanx turret",
        ],
    ) || normalized.contains("spitfire")
        || normalized.contains("hardlight")
        || normalized.contains("turret")
    {
        return VehicleBrowseCategory::Construction;
    }

    VehicleBrowseCategory::Other
}

fn matches_any_vehicle_name(normalized: &str, names: &[&str]) -> bool {
    names.contains(&normalized)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LookupOption {
    pub ids: Vec<u16>,
    pub label: String,
}

impl std::fmt::Display for LookupOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WeaponLookupOption {
    pub ids: Vec<u32>,
    pub label: String,
    pub category_label: String,
    pub faction: WeaponBrowseFaction,
}

impl std::fmt::Display for WeaponLookupOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum VehicleBrowseCategory {
    All,
    Ground,
    Air,
    Infantry,
    Construction,
    Other,
}

impl VehicleBrowseCategory {
    const fn browse_categories() -> [Self; 5] {
        [
            Self::Ground,
            Self::Air,
            Self::Infantry,
            Self::Construction,
            Self::Other,
        ]
    }
}

impl std::fmt::Display for VehicleBrowseCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All => f.write_str("All vehicle types"),
            Self::Ground => f.write_str("Ground vehicles"),
            Self::Air => f.write_str("Air vehicles"),
            Self::Infantry => f.write_str("Infantry"),
            Self::Construction => f.write_str("Construction"),
            Self::Other => f.write_str("Other"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct VehicleBrowseKey {
    pub rule_id: String,
    pub event_index: usize,
    pub group_index: usize,
    pub clause_path: Vec<usize>,
}

impl VehicleBrowseKey {
    fn new(rule_id: &str, event_index: usize, group_index: usize, clause_path: &[usize]) -> Self {
        Self {
            rule_id: rule_id.to_string(),
            event_index,
            group_index,
            clause_path: clause_path.to_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum WeaponBrowseCategory {
    All,
    Category(String),
}

impl WeaponBrowseCategory {
    fn label(&self) -> &str {
        match self {
            Self::All => "All weapon categories",
            Self::Category(label) => label,
        }
    }
}

impl std::fmt::Display for WeaponBrowseCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum WeaponBrowseGroup {
    All,
    Infantry,
    Max,
    GroundVehicles,
    AirVehicles,
    NavalVehicles,
    HeavyPlatforms,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum WeaponBrowseFaction {
    All,
    VS,
    NC,
    TR,
    NS,
    Other,
}

impl std::fmt::Display for WeaponBrowseFaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All => f.write_str("All factions"),
            Self::VS => f.write_str("VS"),
            Self::NC => f.write_str("NC"),
            Self::TR => f.write_str("TR"),
            Self::NS => f.write_str("NS"),
            Self::Other => f.write_str("Other"),
        }
    }
}

impl WeaponBrowseFaction {
    fn from_faction(faction: Faction) -> Self {
        match faction {
            Faction::VS => Self::VS,
            Faction::NC => Self::NC,
            Faction::TR => Self::TR,
            Faction::NS => Self::NS,
            Faction::Unknown => Self::Other,
        }
    }
}

impl std::fmt::Display for WeaponBrowseGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All => f.write_str("All weapon groups"),
            Self::Infantry => f.write_str("Infantry"),
            Self::Max => f.write_str("MAX"),
            Self::GroundVehicles => f.write_str("Ground vehicles"),
            Self::AirVehicles => f.write_str("Air vehicles"),
            Self::NavalVehicles => f.write_str("Naval vehicles"),
            Self::HeavyPlatforms => f.write_str("Heavy platforms"),
            Self::Other => f.write_str("Other"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct WeaponBrowseKey {
    pub rule_id: String,
    pub event_index: usize,
    pub group_index: usize,
    pub clause_path: Vec<usize>,
}

impl WeaponBrowseKey {
    fn new(rule_id: &str, event_index: usize, group_index: usize, clause_path: &[usize]) -> Self {
        Self {
            rule_id: rule_id.to_string(),
            event_index,
            group_index,
            clause_path: clause_path.to_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuleDragState {
    ScoredEvent(ScoredEventDragState),
    FilterClause(FilterClauseDragState),
}

impl RuleDragState {
    fn rule_id(&self) -> &str {
        match self {
            Self::ScoredEvent(drag) => &drag.rule_id,
            Self::FilterClause(drag) => &drag.list_path.rule_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScoredEventDragState {
    pub rule_id: String,
    pub source_index: usize,
    pub target_index: usize,
    pub list_len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FilterClausePath {
    pub rule_id: String,
    pub event_index: usize,
    pub group_index: usize,
    pub clause_path: Vec<usize>,
}

impl FilterClausePath {
    fn new(rule_id: &str, event_index: usize, group_index: usize, clause_path: Vec<usize>) -> Self {
        Self {
            rule_id: rule_id.to_string(),
            event_index,
            group_index,
            clause_path,
        }
    }

    fn is_first_in_group(&self) -> bool {
        self.clause_path.first() == Some(&0) && self.clause_path.len() == 1
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FilterClauseListPath {
    pub rule_id: String,
    pub event_index: usize,
    pub group_index: usize,
    pub parent_clause_path: Vec<usize>,
}

impl FilterClauseListPath {
    fn root(rule_id: &str, event_index: usize, group_index: usize) -> Self {
        Self {
            rule_id: rule_id.to_string(),
            event_index,
            group_index,
            parent_clause_path: Vec::new(),
        }
    }

    fn from_clause_path(path: &FilterClausePath) -> Option<Self> {
        let mut parent_clause_path = path.clause_path.clone();
        parent_clause_path.pop()?;
        Some(Self {
            rule_id: path.rule_id.clone(),
            event_index: path.event_index,
            group_index: path.group_index,
            parent_clause_path,
        })
    }

    fn child_clause_path(&self, index: usize) -> FilterClausePath {
        let mut clause_path = self.parent_clause_path.clone();
        clause_path.push(index);
        FilterClausePath::new(
            &self.rule_id,
            self.event_index,
            self.group_index,
            clause_path,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FilterClauseDragState {
    pub source_path: FilterClausePath,
    pub list_path: FilterClauseListPath,
    pub target_index: usize,
    pub list_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FilterTextField {
    TargetCharacter,
    TargetOutfit,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FilterTextDraftKey {
    pub path: FilterClausePath,
    pub field: FilterTextField,
}

impl FilterTextDraftKey {
    fn new(path: FilterClausePath, field: FilterTextField) -> Self {
        Self { path, field }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ResolvedFilterReference {
    Character(CharacterReferenceFilter),
    Outfit(OutfitReferenceFilter),
}

impl ResolvedFilterReference {
    fn display_value(&self) -> &str {
        match self {
            Self::Character(reference) => reference.name.as_deref().unwrap_or_default(),
            Self::Outfit(reference) => reference.tag.as_deref().unwrap_or_default(),
        }
    }

    fn success_message(&self) -> String {
        match self {
            Self::Character(reference) => format!(
                "Resolved {} to character id {}.",
                reference.name.as_deref().unwrap_or("character"),
                reference.character_id.unwrap_or_default()
            ),
            Self::Outfit(reference) => format!(
                "Resolved [{}] to outfit id {}.",
                reference.tag.as_deref().unwrap_or("outfit"),
                reference.outfit_id.unwrap_or_default()
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FilterClauseKindChoice {
    TargetCharacter,
    TargetOutfit,
    AttackerVehicle,
    AttackerWeapon,
    DestroyedVehicle,
    AnyOf,
}

impl FilterClauseKindChoice {
    const ALL: [Self; 6] = [
        Self::TargetCharacter,
        Self::TargetOutfit,
        Self::AttackerVehicle,
        Self::AttackerWeapon,
        Self::DestroyedVehicle,
        Self::AnyOf,
    ];

    const NESTED_ALL: [Self; 6] = Self::ALL;

    fn from_clause(clause: &ScoredEventFilterClause) -> Self {
        match clause {
            ScoredEventFilterClause::TargetCharacter { .. } => Self::TargetCharacter,
            ScoredEventFilterClause::TargetOutfit { .. } => Self::TargetOutfit,
            ScoredEventFilterClause::AttackerVehicle { .. } => Self::AttackerVehicle,
            ScoredEventFilterClause::AttackerWeapon { .. } => Self::AttackerWeapon,
            ScoredEventFilterClause::DestroyedVehicle { .. } => Self::DestroyedVehicle,
            ScoredEventFilterClause::Any { .. } => Self::AnyOf,
        }
    }

    fn default_clause(self) -> ScoredEventFilterClause {
        match self {
            Self::TargetCharacter => ScoredEventFilterClause::TargetCharacter {
                target: CharacterReferenceFilter::default(),
            },
            Self::TargetOutfit => ScoredEventFilterClause::TargetOutfit {
                outfit: OutfitReferenceFilter::default(),
            },
            Self::AttackerVehicle => ScoredEventFilterClause::AttackerVehicle {
                vehicle: VehicleMatchFilter::default(),
            },
            Self::AttackerWeapon => ScoredEventFilterClause::AttackerWeapon {
                weapon: WeaponMatchFilter::default(),
            },
            Self::DestroyedVehicle => ScoredEventFilterClause::DestroyedVehicle {
                vehicle: VehicleMatchFilter::default(),
            },
            Self::AnyOf => ScoredEventFilterClause::Any {
                clauses: vec![Self::TargetCharacter.default_clause()],
            },
        }
    }
}

impl std::fmt::Display for FilterClauseKindChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TargetCharacter => f.write_str("Target character"),
            Self::TargetOutfit => f.write_str("Target outfit"),
            Self::AttackerVehicle => f.write_str("Attacker vehicle"),
            Self::AttackerWeapon => f.write_str("Attacker weapon"),
            Self::DestroyedVehicle => f.write_str("Destroyed vehicle"),
            Self::AnyOf => f.write_str("Any of (OR)"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VehicleFilterChoice {
    Any,
    Specific(LookupOption),
}

impl VehicleFilterChoice {}

impl std::fmt::Display for VehicleFilterChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Any => f.write_str("Any vehicle"),
            Self::Specific(option) => option.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WeaponFilterChoice {
    Any,
    Specific(WeaponLookupOption),
}

impl std::fmt::Display for WeaponFilterChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Any => f.write_str("Any weapon"),
            Self::Specific(option) => option.fmt(f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vehicle_lookup_options_collapse_same_name_variants() {
        let options = vehicle_lookup_options(vec![
            (5, "Sunderer".into()),
            (7, "Lightning".into()),
            (6, "sunderer".into()),
        ]);

        assert_eq!(
            options,
            vec![
                LookupOption {
                    ids: vec![7],
                    label: "Lightning".into(),
                },
                LookupOption {
                    ids: vec![5, 6],
                    label: "Sunderer".into(),
                },
            ]
        );
    }

    #[test]
    fn vehicle_browse_categories_group_known_vehicle_families() {
        assert_eq!(
            vehicle_browse_category_for_option(&LookupOption {
                ids: vec![1],
                label: "Sunderer".into(),
            }),
            VehicleBrowseCategory::Ground
        );
        assert_eq!(
            vehicle_browse_category_for_option(&LookupOption {
                ids: vec![2],
                label: "Scythe".into(),
            }),
            VehicleBrowseCategory::Air
        );
        assert_eq!(
            vehicle_browse_category_for_option(&LookupOption {
                ids: vec![3],
                label: "MANA Anti-Vehicle Turret".into(),
            }),
            VehicleBrowseCategory::Infantry
        );
        assert_eq!(
            vehicle_browse_category_for_option(&LookupOption {
                ids: vec![4],
                label: "Spear Phalanx Turret".into(),
            }),
            VehicleBrowseCategory::Construction
        );
        assert_eq!(
            vehicle_browse_category_for_option(&LookupOption {
                ids: vec![5],
                label: "Recon Drone".into(),
            }),
            VehicleBrowseCategory::Infantry
        );
    }

    #[test]
    fn vehicle_filter_choices_respect_browse_category() {
        let options = vec![
            LookupOption {
                ids: vec![1],
                label: "Sunderer".into(),
            },
            LookupOption {
                ids: vec![2],
                label: "Scythe".into(),
            },
        ];

        let choices = vehicle_filter_choices(
            &options,
            &VehicleMatchFilter::default(),
            VehicleBrowseCategory::Air,
        );

        assert_eq!(
            choices,
            vec![
                VehicleFilterChoice::Any,
                VehicleFilterChoice::Specific(LookupOption {
                    ids: vec![2],
                    label: "Scythe".into(),
                }),
            ]
        );
    }

    #[test]
    fn vehicle_filter_choices_keep_current_selection_visible_outside_category() {
        let options = vec![
            LookupOption {
                ids: vec![1],
                label: "Sunderer".into(),
            },
            LookupOption {
                ids: vec![2],
                label: "Scythe".into(),
            },
        ];
        let current = VehicleMatchFilter {
            vehicle: crate::rules::VehicleVariantFilter {
                label: Some("Sunderer".into()),
                ids: vec![1],
            },
            legacy_vehicle_id: None,
        };

        let choices = vehicle_filter_choices(&options, &current, VehicleBrowseCategory::Air);

        assert_eq!(
            choices,
            vec![
                VehicleFilterChoice::Any,
                VehicleFilterChoice::Specific(LookupOption {
                    ids: vec![2],
                    label: "Scythe".into(),
                }),
                VehicleFilterChoice::Specific(LookupOption {
                    ids: vec![1],
                    label: "Sunderer".into(),
                }),
            ]
        );
    }

    #[test]
    fn weapon_lookup_options_do_not_merge_same_name_different_weapon_families() {
        let options = weapon_lookup_options(vec![
            WeaponReferenceCacheEntry {
                item_id: 180,
                weapon_id: 80,
                display_name: "Gauss Rifle".into(),
                category_label: "Assault Rifle".into(),
                faction: Some(Faction::NC),
                weapon_group_id: None,
            },
            WeaponReferenceCacheEntry {
                item_id: 1180,
                weapon_id: 1080,
                display_name: "Gauss Rifle".into(),
                category_label: "Assault Rifle".into(),
                faction: Some(Faction::TR),
                weapon_group_id: None,
            },
            WeaponReferenceCacheEntry {
                item_id: 281,
                weapon_id: 81,
                display_name: "Bishop".into(),
                category_label: "Battle Rifle".into(),
                faction: Some(Faction::NS),
                weapon_group_id: None,
            },
        ]);

        assert_eq!(
            options,
            vec![
                WeaponLookupOption {
                    ids: vec![180],
                    label: "Gauss Rifle".into(),
                    category_label: "Assault Rifle".into(),
                    faction: WeaponBrowseFaction::NC,
                },
                WeaponLookupOption {
                    ids: vec![1180],
                    label: "Gauss Rifle".into(),
                    category_label: "Assault Rifle".into(),
                    faction: WeaponBrowseFaction::TR,
                },
                WeaponLookupOption {
                    ids: vec![281],
                    label: "Bishop".into(),
                    category_label: "Battle Rifle".into(),
                    faction: WeaponBrowseFaction::NS,
                },
            ]
        );
    }

    #[test]
    fn weapon_lookup_options_collapse_cross_slot_variants_by_weapon_id() {
        let options = weapon_lookup_options(vec![
            WeaponReferenceCacheEntry {
                item_id: 401,
                weapon_id: 90,
                display_name: "Drake".into(),
                category_label: "Galaxy Left Weapon".into(),
                faction: Some(Faction::TR),
                weapon_group_id: None,
            },
            WeaponReferenceCacheEntry {
                item_id: 402,
                weapon_id: 90,
                display_name: "Drake".into(),
                category_label: "Galaxy Right Weapon".into(),
                faction: Some(Faction::TR),
                weapon_group_id: None,
            },
            WeaponReferenceCacheEntry {
                item_id: 403,
                weapon_id: 90,
                display_name: "Drake".into(),
                category_label: "Galaxy Tail Weapon".into(),
                faction: Some(Faction::TR),
                weapon_group_id: None,
            },
        ]);

        assert_eq!(
            options,
            vec![WeaponLookupOption {
                ids: vec![401, 402, 403],
                label: "Drake".into(),
                category_label: "Galaxy weapon family".into(),
                faction: WeaponBrowseFaction::TR,
            }]
        );
    }

    #[test]
    fn weapon_browse_group_mapping_classifies_major_platforms() {
        assert_eq!(
            weapon_browse_group_for_category_label("Assault Rifle"),
            WeaponBrowseGroup::Infantry
        );
        assert_eq!(
            weapon_browse_group_for_category_label("AV MAX (Left)"),
            WeaponBrowseGroup::Max
        );
        assert_eq!(
            weapon_browse_group_for_category_label("Galaxy Left Weapon"),
            WeaponBrowseGroup::AirVehicles
        );
        assert_eq!(
            weapon_browse_group_for_category_label("Sunderer Rear Gunner"),
            WeaponBrowseGroup::GroundVehicles
        );
        assert_eq!(
            weapon_browse_group_for_category_label("Corsair Front Turret"),
            WeaponBrowseGroup::NavalVehicles
        );
        assert_eq!(
            weapon_browse_group_for_category_label("Bastion Bombard"),
            WeaponBrowseGroup::HeavyPlatforms
        );
    }

    #[test]
    fn weapon_filter_choices_respect_browse_category() {
        let options = vec![
            WeaponLookupOption {
                ids: vec![180],
                label: "Gauss Rifle".into(),
                category_label: "Assault Rifle".into(),
                faction: WeaponBrowseFaction::NC,
            },
            WeaponLookupOption {
                ids: vec![281],
                label: "Bishop".into(),
                category_label: "Battle Rifle".into(),
                faction: WeaponBrowseFaction::NS,
            },
        ];

        let choices = weapon_filter_choices(
            &options,
            &WeaponMatchFilter::default(),
            &WeaponBrowseGroup::Infantry,
            &WeaponBrowseCategory::Category("Battle Rifle".into()),
            &WeaponBrowseFaction::NS,
        );

        assert_eq!(
            choices,
            vec![
                WeaponFilterChoice::Any,
                WeaponFilterChoice::Specific(WeaponLookupOption {
                    ids: vec![281],
                    label: "Bishop".into(),
                    category_label: "Battle Rifle".into(),
                    faction: WeaponBrowseFaction::NS,
                }),
            ]
        );
    }

    #[test]
    fn weapon_filter_choices_include_family_under_synthesized_slot_category() {
        let options = vec![WeaponLookupOption {
            ids: vec![401, 402, 403],
            label: "Drake".into(),
            category_label: "Galaxy weapon family".into(),
            faction: WeaponBrowseFaction::TR,
        }];

        let choices = weapon_filter_choices(
            &options,
            &WeaponMatchFilter::default(),
            &WeaponBrowseGroup::AirVehicles,
            &WeaponBrowseCategory::Category("Galaxy weapon family".into()),
            &WeaponBrowseFaction::TR,
        );

        assert_eq!(
            choices,
            vec![
                WeaponFilterChoice::Any,
                WeaponFilterChoice::Specific(WeaponLookupOption {
                    ids: vec![401, 402, 403],
                    label: "Drake".into(),
                    category_label: "Galaxy weapon family".into(),
                    faction: WeaponBrowseFaction::TR,
                }),
            ]
        );
    }

    #[test]
    fn weapon_filter_choices_keep_current_selection_visible_outside_category() {
        let options = vec![
            WeaponLookupOption {
                ids: vec![180],
                label: "Gauss Rifle".into(),
                category_label: "Assault Rifle".into(),
                faction: WeaponBrowseFaction::NC,
            },
            WeaponLookupOption {
                ids: vec![281],
                label: "Bishop".into(),
                category_label: "Battle Rifle".into(),
                faction: WeaponBrowseFaction::NS,
            },
        ];
        let current = WeaponMatchFilter {
            weapon: crate::rules::WeaponVariantFilter {
                label: Some("Gauss Rifle".into()),
                ids: vec![180],
            },
            legacy_weapon_id: None,
        };

        let choices = weapon_filter_choices(
            &options,
            &current,
            &WeaponBrowseGroup::Infantry,
            &WeaponBrowseCategory::Category("Battle Rifle".into()),
            &WeaponBrowseFaction::All,
        );

        assert_eq!(
            choices,
            vec![
                WeaponFilterChoice::Any,
                WeaponFilterChoice::Specific(WeaponLookupOption {
                    ids: vec![281],
                    label: "Bishop".into(),
                    category_label: "Battle Rifle".into(),
                    faction: WeaponBrowseFaction::NS,
                }),
                WeaponFilterChoice::Specific(WeaponLookupOption {
                    ids: vec![180],
                    label: "Gauss Rifle".into(),
                    category_label: "Assault Rifle".into(),
                    faction: WeaponBrowseFaction::NC,
                }),
            ]
        );
    }

    #[test]
    fn auto_switch_choice_maps_schedule_variants_to_local_schedule() {
        assert_eq!(
            AutoSwitchConditionChoice::from_condition(&AutoSwitchCondition::LocalSchedule {
                weekdays: default_schedule_weekdays(),
                start_hour: 18,
                start_minute: 0,
                end_hour: 23,
                end_minute: 0,
            }),
            AutoSwitchConditionChoice::LocalSchedule
        );
        assert_eq!(
            AutoSwitchConditionChoice::from_condition(&AutoSwitchCondition::LocalTimeRange {
                start_hour: 18,
                end_hour: 23,
            }),
            AutoSwitchConditionChoice::LocalSchedule
        );
    }

    #[test]
    fn auto_switch_choice_maps_character_variants_to_active_character() {
        assert_eq!(
            AutoSwitchConditionChoice::from_condition(&AutoSwitchCondition::ActiveCharacter {
                character_ids: vec![42],
                character_id: None,
            }),
            AutoSwitchConditionChoice::ActiveCharacter
        );
        assert_eq!(
            AutoSwitchConditionChoice::from_condition(&AutoSwitchCondition::OnEvent {
                event: EventKind::FacilityCapture,
            }),
            AutoSwitchConditionChoice::ActiveCharacter
        );
    }

    #[test]
    fn wrap_schedule_slot_supports_schedule_end_time_24_00() {
        assert_eq!(wrap_schedule_slot(48, 1, 48), 0);
        assert_eq!(wrap_schedule_slot(0, -1, 48), 48);
    }

    #[test]
    fn slot_time_round_trips_half_hour_schedule_values() {
        assert_eq!(slot_time(schedule_slot(19, 30)), (19, 30));
        assert_eq!(slot_time(schedule_slot(24, 0)), (24, 0));
    }

    #[test]
    fn reorder_destination_index_matches_drop_slots() {
        assert_eq!(reorder_destination_index(1, 0, 4), Some(0));
        assert_eq!(reorder_destination_index(1, 3, 4), Some(2));
        assert_eq!(reorder_destination_index(1, 4, 4), Some(3));
        assert_eq!(reorder_destination_index(1, 1, 4), None);
        assert_eq!(reorder_destination_index(1, 2, 4), None);
    }

    #[test]
    fn reorder_index_updates_peer_positions() {
        let reordered = (0..4)
            .map(|index| reorder_index(index, 1, 4, 4))
            .collect::<Vec<_>>();
        assert_eq!(reordered, vec![0, 3, 1, 2]);
    }

    #[test]
    fn remap_clause_key_updates_only_matching_clause_lists() {
        let list_path = FilterClauseListPath {
            rule_id: "rule_1".into(),
            event_index: 2,
            group_index: 1,
            parent_clause_path: vec![3],
        };
        let mut rule_id = "rule_1".to_string();
        let mut event_index = 2;
        let mut group_index = 1;
        let mut clause_path = vec![3, 0, 4];

        remap_clause_key(
            &mut rule_id,
            &mut event_index,
            &mut group_index,
            &mut clause_path,
            &list_path,
            0,
            2,
            3,
        );

        assert_eq!(clause_path, vec![3, 1, 4]);

        let mut other_clause_path = vec![4, 0];
        remap_clause_key(
            &mut rule_id,
            &mut event_index,
            &mut group_index,
            &mut other_clause_path,
            &list_path,
            0,
            2,
            3,
        );
        assert_eq!(other_clause_path, vec![4, 0]);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassFilterChoice {
    Any,
    Infiltrator,
    LightAssault,
    Medic,
    Engineer,
    HeavyAssault,
    Max,
}

impl ClassFilterChoice {
    const ALL: [Self; 7] = [
        Self::Any,
        Self::Infiltrator,
        Self::LightAssault,
        Self::Medic,
        Self::Engineer,
        Self::HeavyAssault,
        Self::Max,
    ];

    fn from_option(value: Option<CharacterClass>) -> Self {
        match value {
            None => Self::Any,
            Some(CharacterClass::Infiltrator) => Self::Infiltrator,
            Some(CharacterClass::LightAssault) => Self::LightAssault,
            Some(CharacterClass::Medic) => Self::Medic,
            Some(CharacterClass::Engineer) => Self::Engineer,
            Some(CharacterClass::HeavyAssault) => Self::HeavyAssault,
            Some(CharacterClass::Max) => Self::Max,
        }
    }

    fn into_option(self) -> Option<CharacterClass> {
        match self {
            Self::Any => None,
            Self::Infiltrator => Some(CharacterClass::Infiltrator),
            Self::LightAssault => Some(CharacterClass::LightAssault),
            Self::Medic => Some(CharacterClass::Medic),
            Self::Engineer => Some(CharacterClass::Engineer),
            Self::HeavyAssault => Some(CharacterClass::HeavyAssault),
            Self::Max => Some(CharacterClass::Max),
        }
    }
}

impl std::fmt::Display for ClassFilterChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Any => "Any",
            Self::Infiltrator => "Infiltrator",
            Self::LightAssault => "Light Assault",
            Self::Medic => "Combat Medic",
            Self::Engineer => "Engineer",
            Self::HeavyAssault => "Heavy Assault",
            Self::Max => "MAX",
        };
        f.write_str(label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutoSwitchConditionChoice {
    LocalSchedule,
    ActiveCharacter,
}

impl AutoSwitchConditionChoice {
    const ALL: [Self; 2] = [Self::LocalSchedule, Self::ActiveCharacter];

    fn from_condition(condition: &AutoSwitchCondition) -> Self {
        match condition {
            AutoSwitchCondition::LocalSchedule { .. }
            | AutoSwitchCondition::LocalTimeRange { .. }
            | AutoSwitchCondition::LocalCron { .. } => Self::LocalSchedule,
            AutoSwitchCondition::ActiveCharacter { .. } | AutoSwitchCondition::OnEvent { .. } => {
                Self::ActiveCharacter
            }
        }
    }
}

impl std::fmt::Display for AutoSwitchConditionChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::LocalSchedule => "Local schedule",
            Self::ActiveCharacter => "Active character",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProfileOption {
    id: String,
    name: String,
}

impl ProfileOption {
    fn from_profile(profile: &RuleProfile) -> Self {
        Self {
            id: profile.id.clone(),
            name: profile.name.clone(),
        }
    }
}

impl std::fmt::Display for ProfileOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CharacterOption {
    id: u64,
    name: String,
}

impl std::fmt::Display for CharacterOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}
