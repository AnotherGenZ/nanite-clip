use std::path::PathBuf;
use std::time::Duration as StdDuration;

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc};
use iced::widget::{Space, button, operation as widget_operation, scrollable as widget_scrollable};
use iced::{Alignment, Background, Color, Element, Length, Padding, Task};

use crate::census;
use crate::db::{
    ClipDetailRecord, ClipFilterOptions, ClipRecord, ClipUploadState, OverlapFilterState,
    UploadProvider,
};
use crate::storage_tiering::{self, StorageTier};
use crate::ui::app::{
    Column, ContainerStyle, TextStyle, center, checkbox, column, container, mouse_area, pick_list,
    row, scrollable, text, text_input,
};
use crate::ui::data::pagination::pagination;
use crate::ui::layout::card::card;
use crate::ui::layout::empty_state::empty_state;
use crate::ui::layout::page_header::page_header;
use crate::ui::layout::panel::panel;
use crate::ui::layout::section::section;
use crate::ui::layout::toolbar::toolbar;
use crate::ui::overlay::modal::modal;
use crate::ui::pickers::date::date_picker;
use crate::ui::primitives::badge::{Tone as BadgeTone, badge};
use crate::ui::theme;

use super::super::shared::{ButtonTone, styled_button, with_tooltip};
use super::super::{App, AppState, Message as AppMessage};

// ---------------------------------------------------------------------------
// Constants and lightweight types
// ---------------------------------------------------------------------------

pub const ALL_PROFILES_LABEL: &str = "All profiles";
pub const ALL_RULES_LABEL: &str = "All rules";
pub const ALL_CHARACTERS_LABEL: &str = "All characters";
pub const ALL_SERVERS_LABEL: &str = "All servers";
pub const ALL_CONTINENTS_LABEL: &str = "All continents";
pub const ALL_BASES_LABEL: &str = "All bases";
pub const ALL_TARGETS_LABEL: &str = "All targets";
pub const ALL_WEAPONS_LABEL: &str = "All weapons";
pub const ALL_ALERTS_LABEL: &str = "All alerts";

pub const DEFAULT_PAGE_SIZE: usize = 50;
pub const PAGE_SIZE_OPTIONS: [usize; 4] = [25, 50, 100, 200];
const SEARCH_DEBOUNCE_MS: u64 = 180;
const CLIP_HISTORY_SCROLLABLE_ID: &str = "clips-history-scrollable";
const CLIP_HISTORY_ROW_SPACING: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HistoryViewportState {
    offset_y: f32,
    viewport_height: f32,
    content_height: f32,
}

impl From<widget_scrollable::Viewport> for HistoryViewportState {
    fn from(viewport: widget_scrollable::Viewport) -> Self {
        let offset = viewport.absolute_offset();
        let bounds = viewport.bounds();
        let content_bounds = viewport.content_bounds();

        Self {
            offset_y: offset.y,
            viewport_height: bounds.height,
            content_height: content_bounds.height,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    // Data loading
    Loaded(u64, Result<Vec<ClipRecord>, String>),

    // Filters
    SearchChanged(String),
    SearchDebounceFired(u64),
    TargetFilterChanged(String),
    WeaponFilterChanged(String),
    AlertFilterChanged(String),
    OverlapFilterChanged(OverlapFilterChoice),
    ProfileFilterChanged(String),
    RuleFilterChanged(String),
    CharacterFilterChanged(String),
    ServerFilterChanged(String),
    ContinentFilterChanged(String),
    BaseFilterChanged(String),
    ClearFilters,
    ToggleAdvancedFilters,

    // Date range
    DateRangePresetChanged(DateRangePreset),
    DateRangeStartChanged(String),
    DateRangeEndChanged(String),
    ApplyDateRange,
    ToggleCalendar(CalendarField),
    DismissCalendar,
    CalendarMonthChanged(NaiveDate),
    CalendarDaySelected(NaiveDate),

    // Sorting / pagination
    SortColumnClicked(ClipSortColumn),
    PageChanged(usize),
    PageSizeChanged(usize),
    HistoryScrolled(HistoryViewportState),

    // Selection and row actions
    RowSelected(i64),
    OpenRequested(i64),
    ExportChaptersRequested(i64),
    ExportSubtitlesRequested(i64),
    SetStorageTier(i64, StorageTier),
    UploadToCopypartyRequested(i64),
    UploadToYouTubeRequested(i64),
    RetryPostProcessRequested(i64),
    UseOriginalAudioRequested(i64),
    OpenUploadUrl(String),
    OpenHonuSession(i64),

    // Montage
    ToggleMontageSelection(i64),
    MontageMoveUp(i64),
    MontageMoveDown(i64),
    MontageRemove(i64),
    ClearMontageSelection,
    CreateMontage,
    CancelMontageModal,
    ConfirmMontageCreation,

    // Delete
    DeleteRequested(i64),
    DeleteCanceled,
    DeleteConfirmed,

    // Detail workspace
    RawEventFilterChanged(String),
    ToggleDetailSection(DetailSection),

    // Keyboard navigation
    KeyNav(KeyNav),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateRangePreset {
    AllTime,
    Today,
    Yesterday,
    Last24Hours,
    Last7Days,
    Last30Days,
    Custom,
}

impl DateRangePreset {
    pub(super) const ALL: [Self; 7] = [
        Self::AllTime,
        Self::Today,
        Self::Yesterday,
        Self::Last24Hours,
        Self::Last7Days,
        Self::Last30Days,
        Self::Custom,
    ];

    fn bounds(
        self,
        now: chrono::DateTime<Local>,
    ) -> Option<(chrono::DateTime<Utc>, chrono::DateTime<Utc>)> {
        match self {
            Self::AllTime | Self::Custom => None,
            Self::Today => {
                let start = local_day_start(now.date_naive())?;
                Some((start, now.with_timezone(&Utc)))
            }
            Self::Yesterday => {
                let today = local_day_start(now.date_naive())?;
                let yesterday = local_day_start(now.date_naive().pred_opt()?)?;
                Some((yesterday, today - chrono::Duration::milliseconds(1)))
            }
            Self::Last24Hours => Some((
                now.with_timezone(&Utc) - chrono::Duration::hours(24),
                now.with_timezone(&Utc),
            )),
            Self::Last7Days => Some((
                now.with_timezone(&Utc) - chrono::Duration::days(7),
                now.with_timezone(&Utc),
            )),
            Self::Last30Days => Some((
                now.with_timezone(&Utc) - chrono::Duration::days(30),
                now.with_timezone(&Utc),
            )),
        }
    }
}

impl std::fmt::Display for DateRangePreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::AllTime => "All time",
            Self::Today => "Today",
            Self::Yesterday => "Yesterday",
            Self::Last24Hours => "Last 24 hours",
            Self::Last7Days => "Last 7 days",
            Self::Last30Days => "Last 30 days",
            Self::Custom => "Custom range",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarField {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipSortColumn {
    When,
    Rule,
    Character,
    Score,
    Duration,
}

impl Default for ClipSortColumn {
    fn default() -> Self {
        Self::When
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailSection {
    AudioTracks,
    Uploads,
    Alerts,
    Overlaps,
    RawEvents,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyNav {
    SelectNext,
    SelectPrevious,
    NextPage,
    PreviousPage,
    First,
    Last,
    OpenSelected,
    DeleteSelected,
    ToggleMontageSelected,
    Escape,
}

// ---------------------------------------------------------------------------
// Clip data refresh helpers
// ---------------------------------------------------------------------------

pub(in crate::app) fn reload_views(app: &mut App) -> Task<AppMessage> {
    Task::batch([refresh_recent(app), refresh_history(app)])
}

pub(in crate::app) fn refresh_recent(app: &App) -> Task<AppMessage> {
    let Some(store) = app.clip_store.clone() else {
        return Task::none();
    };

    Task::perform(async move { store.recent_clips(20).await }, |result| {
        AppMessage::RecentClipsLoaded(result.map_err(|e| e.to_string()))
    })
}

pub(in crate::app) fn refresh_history(app: &mut App) -> Task<AppMessage> {
    let Some(store) = app.clip_store.clone() else {
        return Task::none();
    };

    app.clip_query_revision += 1;
    let revision = app.clip_query_revision;
    let filters = app.clip_filters.clone();

    Task::perform(
        async move { store.search_clips(&filters, 1_000).await },
        move |result| {
            AppMessage::Clips(Message::Loaded(revision, result.map_err(|e| e.to_string())))
        },
    )
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

pub(in crate::app) fn update(app: &mut App, message: Message) -> Task<AppMessage> {
    match message {
        Message::Loaded(revision, result) => {
            if revision != app.clip_query_revision {
                return Task::none();
            }
            match result {
                Ok(clips) => {
                    app.clip_history_source = clips;
                    rebuild_history(app);
                    app.clear_clip_error();
                    let lookup_clips = app.clip_history_source.clone();
                    return app.schedule_clip_record_lookup_resolutions(&lookup_clips);
                }
                Err(error) => {
                    app.set_clip_error(error.clone());
                    tracing::error!("Failed to load clip history: {error}");
                }
            }
            Task::none()
        }

        Message::SearchChanged(value) => {
            app.clip_filters.search = value;
            app.clip_search_revision = app.clip_search_revision.wrapping_add(1);
            let revision = app.clip_search_revision;
            Task::perform(
                async move {
                    tokio::time::sleep(StdDuration::from_millis(SEARCH_DEBOUNCE_MS)).await;
                    revision
                },
                |revision| AppMessage::Clips(Message::SearchDebounceFired(revision)),
            )
        }
        Message::SearchDebounceFired(revision) => {
            if revision == app.clip_search_revision {
                rebuild_history(app);
            }
            Task::none()
        }

        Message::TargetFilterChanged(value) => {
            app.clip_filters.target = filter_value_from_selection(value, ALL_TARGETS_LABEL);
            refresh_history(app)
        }
        Message::WeaponFilterChanged(value) => {
            app.clip_filters.weapon = filter_value_from_selection(value, ALL_WEAPONS_LABEL);
            refresh_history(app)
        }
        Message::AlertFilterChanged(value) => {
            app.clip_filters.alert = filter_value_from_selection(value, ALL_ALERTS_LABEL);
            refresh_history(app)
        }
        Message::OverlapFilterChanged(value) => {
            app.clip_filters.overlap_state = value.into_state();
            refresh_history(app)
        }

        Message::ProfileFilterChanged(value) => {
            app.clip_filters.profile = filter_value_from_selection(value, ALL_PROFILES_LABEL);
            rebuild_history(app);
            Task::none()
        }
        Message::RuleFilterChanged(value) => {
            app.clip_filters.rule = filter_value_from_selection(value, ALL_RULES_LABEL);
            rebuild_history(app);
            Task::none()
        }
        Message::CharacterFilterChanged(value) => {
            app.clip_filters.character = filter_value_from_selection(value, ALL_CHARACTERS_LABEL);
            rebuild_history(app);
            Task::none()
        }
        Message::ServerFilterChanged(value) => {
            app.clip_filters.server = filter_value_from_selection(value, ALL_SERVERS_LABEL);
            rebuild_history(app);
            Task::none()
        }
        Message::ContinentFilterChanged(value) => {
            app.clip_filters.continent = filter_value_from_selection(value, ALL_CONTINENTS_LABEL);
            rebuild_history(app);
            Task::none()
        }
        Message::BaseFilterChanged(value) => {
            app.clip_filters.base = filter_value_from_selection(value, ALL_BASES_LABEL);
            rebuild_history(app);
            Task::none()
        }

        Message::ClearFilters => {
            app.clip_filters = crate::db::ClipFilters::default();
            app.clip_date_range_preset = DateRangePreset::AllTime;
            app.clip_date_range_start.clear();
            app.clip_date_range_end.clear();
            app.active_clip_calendar = None;
            app.clear_clip_filter_feedback();
            refresh_history(app)
        }
        Message::ToggleAdvancedFilters => {
            app.clip_advanced_filters_open = !app.clip_advanced_filters_open;
            Task::none()
        }

        Message::DateRangePresetChanged(preset) => set_date_range_preset(app, preset),
        Message::DateRangeStartChanged(value) => {
            app.clip_date_range_start = value;
            app.clear_clip_filter_feedback();
            Task::none()
        }
        Message::DateRangeEndChanged(value) => {
            app.clip_date_range_end = value;
            app.clear_clip_filter_feedback();
            Task::none()
        }
        Message::ApplyDateRange => apply_date_range(app),
        Message::ToggleCalendar(field) => {
            toggle_calendar(app, field);
            Task::none()
        }
        Message::DismissCalendar => {
            app.active_clip_calendar = None;
            Task::none()
        }
        Message::CalendarMonthChanged(month) => {
            app.clip_calendar_month = month.with_day(1).unwrap_or(month);
            Task::none()
        }
        Message::CalendarDaySelected(date) => {
            select_calendar_day(app, date);
            Task::none()
        }

        Message::SortColumnClicked(column) => {
            if app.clip_sort_column == column {
                app.clip_sort_descending = !app.clip_sort_descending;
            } else {
                app.clip_sort_column = column;
                app.clip_sort_descending = matches!(
                    column,
                    ClipSortColumn::When | ClipSortColumn::Score | ClipSortColumn::Duration
                );
            }
            rebuild_history(app);
            Task::none()
        }
        Message::PageChanged(page) => {
            let total_pages = total_pages(app);
            app.clip_history_page = page.saturating_sub(1).min(total_pages.saturating_sub(1));
            app.clip_history_viewport = None;
            scroll_history_to_top()
        }
        Message::PageSizeChanged(size) => {
            app.clip_history_page_size = size.max(1);
            app.clip_history_page = 0;
            app.clip_history_viewport = None;
            scroll_history_to_top()
        }
        Message::HistoryScrolled(viewport) => {
            app.clip_history_viewport = Some(viewport);
            Task::none()
        }

        Message::RowSelected(clip_id) => {
            app.clear_clip_error();
            if app.selected_clip_id == Some(clip_id) {
                app.load_clip_detail(None)
            } else {
                app.load_clip_detail(Some(clip_id))
            }
        }
        Message::OpenRequested(clip_id) => {
            let Some(record) = app.clip_history.iter().find(|record| record.id == clip_id) else {
                app.set_clip_error(format!("Clip #{clip_id} is no longer available."));
                return Task::none();
            };
            let Some(path) = record.path.clone() else {
                app.set_clip_error(
                    "This clip does not have a saved file path yet. New clips will populate it after they finish saving.",
                );
                return Task::none();
            };
            app.clear_clip_error();
            Task::done(AppMessage::OpenClipRequested(PathBuf::from(path)))
        }
        Message::ExportChaptersRequested(clip_id) => app.export_selected_clip_timeline_artifact(
            clip_id,
            crate::timeline_export::TimelineExportKind::Chapters,
        ),
        Message::ExportSubtitlesRequested(clip_id) => app.export_selected_clip_timeline_artifact(
            clip_id,
            crate::timeline_export::TimelineExportKind::Subtitles,
        ),
        Message::SetStorageTier(clip_id, target_tier) => Task::done(AppMessage::MoveClipToTier {
            clip_id,
            target_tier,
        }),
        Message::UploadToCopypartyRequested(clip_id) => {
            Task::done(AppMessage::UploadClipRequested {
                clip_id,
                provider: UploadProvider::Copyparty,
            })
        }
        Message::UploadToYouTubeRequested(clip_id) => Task::done(AppMessage::UploadClipRequested {
            clip_id,
            provider: UploadProvider::YouTube,
        }),
        Message::RetryPostProcessRequested(clip_id) => {
            let Some(record) = app
                .clip_history_source
                .iter()
                .find(|record| record.id == clip_id)
            else {
                app.set_clip_error(format!("Clip #{clip_id} is no longer available."));
                return Task::none();
            };
            let Some(path) = record.path.clone() else {
                app.set_clip_error(format!(
                    "Clip #{clip_id} does not have a saved path to retry audio post-processing."
                ));
                return Task::none();
            };
            app.queue_post_process_retry_for_clip(clip_id, PathBuf::from(path))
        }
        Message::UseOriginalAudioRequested(clip_id) => app.use_original_clip_audio(clip_id),
        Message::OpenUploadUrl(url) => {
            Task::perform(async move { crate::launcher::open_url(&url) }, |result| {
                if let Err(error) = result {
                    tracing::warn!("Failed to open uploaded clip URL: {error}");
                }
                AppMessage::Tick
            })
        }
        Message::OpenHonuSession(session_id) => {
            let url = format!("https://wt.honu.pw/s/{session_id}");
            Task::perform(async move { crate::launcher::open_url(&url) }, |result| {
                if let Err(error) = result {
                    tracing::warn!("Failed to open Honu session URL: {error}");
                }
                AppMessage::Tick
            })
        }

        Message::ToggleMontageSelection(clip_id) => {
            if let Some(record) = app
                .clip_history_source
                .iter()
                .find(|record| record.id == clip_id)
            {
                if let Some(reason) = super::super::clip_post_process_block_reason(record) {
                    app.set_clip_error(reason);
                    return Task::none();
                }
            }
            if let Some(index) = app.montage_selection.iter().position(|id| *id == clip_id) {
                app.montage_selection.remove(index);
                if app.selected_montage_clip_id == Some(clip_id) {
                    app.selected_montage_clip_id = app.montage_selection.first().copied();
                }
            } else {
                app.montage_selection.push(clip_id);
                app.selected_montage_clip_id = Some(clip_id);
            }
            Task::none()
        }
        Message::MontageMoveUp(clip_id) => {
            if let Some(index) = app.montage_selection.iter().position(|id| *id == clip_id) {
                if index > 0 {
                    app.montage_selection.swap(index, index - 1);
                }
            }
            Task::none()
        }
        Message::MontageMoveDown(clip_id) => {
            if let Some(index) = app.montage_selection.iter().position(|id| *id == clip_id) {
                if index + 1 < app.montage_selection.len() {
                    app.montage_selection.swap(index, index + 1);
                }
            }
            Task::none()
        }
        Message::MontageRemove(clip_id) => {
            app.montage_selection.retain(|id| *id != clip_id);
            if app.selected_montage_clip_id == Some(clip_id) {
                app.selected_montage_clip_id = app.montage_selection.first().copied();
            }
            Task::none()
        }
        Message::ClearMontageSelection => {
            app.montage_selection.clear();
            app.selected_montage_clip_id = None;
            app.clip_montage_modal_open = false;
            Task::none()
        }
        Message::CreateMontage => {
            if app.montage_selection.len() < 2 {
                app.set_clip_error("Choose at least two clips for a montage.");
                return Task::none();
            }

            app.clip_montage_modal_open = true;
            app.active_clip_calendar = None;
            app.clear_clip_error();
            Task::none()
        }
        Message::CancelMontageModal => {
            app.clip_montage_modal_open = false;
            Task::none()
        }
        Message::ConfirmMontageCreation => {
            if app.montage_selection.len() < 2 {
                app.set_clip_error("Choose at least two clips for a montage.");
                return Task::none();
            }

            app.clip_montage_modal_open = false;
            Task::done(AppMessage::CreateMontageRequested)
        }

        Message::DeleteRequested(clip_id) => {
            let Some(record) = app.clip_history.iter().find(|record| record.id == clip_id) else {
                app.set_clip_error(format!("Clip #{clip_id} is no longer available."));
                return Task::none();
            };

            app.pending_clip_delete = Some(crate::app::PendingClipDelete {
                clip_id,
                path: record.path.as_ref().map(PathBuf::from),
                file_size_bytes: record.file_size_bytes,
            });
            app.active_clip_calendar = None;
            app.clip_montage_modal_open = false;
            app.clear_clip_error();
            Task::none()
        }
        Message::DeleteCanceled => {
            app.pending_clip_delete = None;
            Task::none()
        }
        Message::DeleteConfirmed => {
            let Some(pending) = app.pending_clip_delete.take() else {
                return Task::none();
            };

            Task::done(AppMessage::DeleteClipRequested {
                clip_id: pending.clip_id,
                path: pending.path,
            })
        }

        Message::RawEventFilterChanged(value) => {
            app.clip_raw_event_filter = value;
            Task::none()
        }
        Message::ToggleDetailSection(section) => {
            if app.clip_collapsed_detail_sections.contains(&section) {
                app.clip_collapsed_detail_sections.retain(|s| *s != section);
            } else {
                app.clip_collapsed_detail_sections.push(section);
            }
            Task::none()
        }

        Message::KeyNav(action) => handle_key_nav(app, action),
    }
}

fn handle_key_nav(app: &mut App, action: KeyNav) -> Task<AppMessage> {
    match action {
        KeyNav::Escape => {
            if app.pending_clip_delete.is_some() {
                app.pending_clip_delete = None;
            } else if app.clip_montage_modal_open {
                app.clip_montage_modal_open = false;
            } else if app.active_clip_calendar.is_some() {
                app.active_clip_calendar = None;
            } else if app.selected_clip_id.is_some() {
                return app.load_clip_detail(None);
            }
            Task::none()
        }
        KeyNav::SelectNext => move_selection(app, 1),
        KeyNav::SelectPrevious => move_selection(app, -1),
        KeyNav::First => select_at_offset(app, 0, true),
        KeyNav::Last => {
            let last = app.clip_history.len().saturating_sub(1);
            select_at_offset(app, last as isize, true)
        }
        KeyNav::NextPage => {
            let total = total_pages(app);
            if app.clip_history_page + 1 < total {
                app.clip_history_page += 1;
            }
            scroll_history_to_top()
        }
        KeyNav::PreviousPage => {
            if app.clip_history_page > 0 {
                app.clip_history_page -= 1;
            }
            scroll_history_to_top()
        }
        KeyNav::OpenSelected => {
            let Some(clip_id) = app.selected_clip_id else {
                return Task::none();
            };
            update(app, Message::OpenRequested(clip_id))
        }
        KeyNav::DeleteSelected => {
            let Some(clip_id) = app.selected_clip_id else {
                return Task::none();
            };
            update(app, Message::DeleteRequested(clip_id))
        }
        KeyNav::ToggleMontageSelected => {
            let Some(clip_id) = app.selected_clip_id else {
                return Task::none();
            };
            update(app, Message::ToggleMontageSelection(clip_id))
        }
    }
}

fn move_selection(app: &mut App, delta: isize) -> Task<AppMessage> {
    if app.clip_history.is_empty() {
        return Task::none();
    }
    let current_index = app
        .selected_clip_id
        .and_then(|id| app.clip_history.iter().position(|r| r.id == id));

    let target = match current_index {
        Some(idx) => (idx as isize + delta)
            .max(0)
            .min(app.clip_history.len() as isize - 1),
        None => {
            if delta >= 0 {
                0
            } else {
                app.clip_history.len() as isize - 1
            }
        }
    };

    select_at_offset(app, target, false)
}

fn select_at_offset(app: &mut App, index: isize, force: bool) -> Task<AppMessage> {
    if app.clip_history.is_empty() || index < 0 {
        return Task::none();
    }
    let index = index as usize;
    if index >= app.clip_history.len() {
        return Task::none();
    }

    let page_size = app.clip_history_page_size.max(1);
    let target_page = index / page_size;
    let page_changed = target_page != app.clip_history_page;
    if page_changed {
        app.clip_history_page = target_page;
        app.clip_history_viewport = None;
    }

    let clip_id = app.clip_history[index].id;
    let scroll_task = if force || page_changed || !history_row_is_visible(app, index) {
        scroll_history_to_index(app, index)
    } else {
        Task::none()
    };

    if force || app.selected_clip_id != Some(clip_id) {
        Task::batch([app.load_clip_detail(Some(clip_id)), scroll_task])
    } else {
        scroll_task
    }
}

fn scroll_history_to_index(app: &App, index: usize) -> Task<AppMessage> {
    let page_size = app.clip_history_page_size.max(1);
    let page_start = app.clip_history_page * page_size;
    let page_end = (page_start + page_size).min(app.clip_history.len());
    let page_row_count = page_end.saturating_sub(page_start);
    let page_row_index = index.saturating_sub(page_start);

    scroll_history_to_row(page_row_index, page_row_count)
}

fn scroll_history_to_top() -> Task<AppMessage> {
    scroll_history_to_row(0, 1)
}

fn scroll_history_to_row(row_index: usize, row_count: usize) -> Task<AppMessage> {
    let Some(y) = history_scroll_ratio(row_index, row_count) else {
        return Task::none();
    };

    widget_operation::snap_to(
        CLIP_HISTORY_SCROLLABLE_ID,
        widget_scrollable::RelativeOffset {
            x: None,
            y: Some(y),
        },
    )
}

fn history_scroll_ratio(row_index: usize, row_count: usize) -> Option<f32> {
    if row_count == 0 {
        return None;
    }

    if row_count == 1 {
        return Some(0.0);
    }

    let last_row = row_count.saturating_sub(1);
    let row_index = row_index.min(last_row);

    Some(row_index as f32 / last_row as f32)
}

fn history_row_is_visible(app: &App, index: usize) -> bool {
    let Some(viewport) = app.clip_history_viewport else {
        return false;
    };

    let page_size = app.clip_history_page_size.max(1);
    let page_start = app.clip_history_page * page_size;
    let page_end = (page_start + page_size).min(app.clip_history.len());
    let page_row_count = page_end.saturating_sub(page_start);
    if page_row_count == 0 || index < page_start || index >= page_end {
        return false;
    }

    history_page_row_is_visible(viewport, index - page_start, page_row_count)
}

fn history_page_row_is_visible(
    viewport: HistoryViewportState,
    row_index: usize,
    row_count: usize,
) -> bool {
    if row_count == 0 {
        return false;
    }

    if viewport.content_height <= viewport.viewport_height {
        return true;
    }

    let spacing_total = CLIP_HISTORY_ROW_SPACING * row_count.saturating_sub(1) as f32;
    let row_height = (viewport.content_height - spacing_total) / row_count as f32;
    if row_height <= 0.0 {
        return false;
    }

    let row_pitch = row_height + CLIP_HISTORY_ROW_SPACING;
    let row_top = row_index.min(row_count.saturating_sub(1)) as f32 * row_pitch;
    let row_bottom = row_top + row_height;
    let visible_top = viewport.offset_y;
    let visible_bottom = viewport.offset_y + viewport.viewport_height;
    let epsilon = 1.0;

    row_bottom > visible_top - epsilon && row_top < visible_bottom + epsilon
}

fn set_date_range_preset(app: &mut App, preset: DateRangePreset) -> Task<AppMessage> {
    app.clip_date_range_preset = preset;
    app.clear_clip_filter_feedback();
    if preset != DateRangePreset::Custom {
        app.active_clip_calendar = None;
    }

    match preset {
        DateRangePreset::AllTime => {
            app.clip_filters.event_after_ts = None;
            app.clip_filters.event_before_ts = None;
            refresh_history(app)
        }
        DateRangePreset::Custom => Task::none(),
        _ => match preset.bounds(Local::now()) {
            Some((start, end)) => {
                app.clip_filters.event_after_ts = Some(start.timestamp_millis());
                app.clip_filters.event_before_ts = Some(end.timestamp_millis());
                refresh_history(app)
            }
            None => Task::none(),
        },
    }
}

fn apply_date_range(app: &mut App) -> Task<AppMessage> {
    let start = match parse_range_input(&app.clip_date_range_start, false) {
        Ok(value) => value,
        Err(error) => {
            app.set_clip_filter_feedback(error, true);
            return Task::none();
        }
    };

    let end = match parse_range_input(&app.clip_date_range_end, true) {
        Ok(value) => value,
        Err(error) => {
            app.set_clip_filter_feedback(error, true);
            return Task::none();
        }
    };

    if let (Some(start), Some(end)) = (start, end) {
        if start > end {
            app.set_clip_filter_feedback("Custom range start must be before the end.", true);
            return Task::none();
        }
    }

    app.clip_filters.event_after_ts = start.map(|value| value.timestamp_millis());
    app.clip_filters.event_before_ts = end.map(|value| value.timestamp_millis());
    app.active_clip_calendar = None;
    app.set_clip_filter_feedback(
        match (start, end) {
            (Some(start), Some(end)) => format!(
                "Showing clips from {} through {}.",
                format_timestamp(start),
                format_timestamp(end)
            ),
            (Some(start), None) => {
                format!("Showing clips from {} onward.", format_timestamp(start))
            }
            (None, Some(end)) => format!("Showing clips through {}.", format_timestamp(end)),
            (None, None) => "Showing clips from all time.".into(),
        },
        false,
    );
    refresh_history(app)
}

fn toggle_calendar(app: &mut App, field: CalendarField) {
    if app.active_clip_calendar == Some(field) {
        app.active_clip_calendar = None;
        return;
    }

    app.active_clip_calendar = Some(field);
    app.clip_calendar_month =
        calendar_seed_date(calendar_input(app, field)).unwrap_or_else(today_local_date);
}

fn select_calendar_day(app: &mut App, date: NaiveDate) {
    let Some(field) = app.active_clip_calendar else {
        return;
    };

    let next_value = merge_calendar_date(calendar_input(app, field), date);

    match field {
        CalendarField::Start => app.clip_date_range_start = next_value,
        CalendarField::End => app.clip_date_range_end = next_value,
    }

    app.clear_clip_filter_feedback();
    app.active_clip_calendar = None;
}

fn calendar_input(app: &App, field: CalendarField) -> &str {
    match field {
        CalendarField::Start => &app.clip_date_range_start,
        CalendarField::End => &app.clip_date_range_end,
    }
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

pub(in crate::app) fn view(app: &App) -> Element<'_, Message> {
    let header = {
        let mut builder =
            page_header("Clips").subtitle("Browse, inspect, and package saved clips.");
        if active_filter_count(app) > 0 {
            builder = builder.action(with_tooltip(
                styled_button("Clear filters", ButtonTone::Secondary)
                    .on_press(Message::ClearFilters)
                    .into(),
                "Reset search, metadata, and date filters.",
            ));
        }
        builder.build()
    };

    let body = column![header, filters_panel(app), clip_workspace(app),]
        .spacing(12)
        .height(Length::Fill);

    let base: Element<'_, Message> = container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

    let with_calendar = if let Some(field) = app.active_clip_calendar {
        modal(
            base,
            calendar_dropdown(app, field),
            Some(Message::DismissCalendar),
        )
    } else {
        base
    };

    let with_montage = if app.clip_montage_modal_open {
        modal(
            with_calendar,
            montage_queue_dialog(app),
            Some(Message::CancelMontageModal),
        )
    } else {
        with_calendar
    };

    if let Some(pending) = &app.pending_clip_delete {
        modal(
            with_montage,
            delete_dialog(pending),
            Some(Message::DeleteCanceled),
        )
    } else {
        with_montage
    }
}

// ---------------------------------------------------------------------------
// Filters panel
// ---------------------------------------------------------------------------

fn filters_panel(app: &App) -> Element<'static, Message> {
    let filter_options = build_filter_options(app);
    let profile_options = filter_pick_list_options(ALL_PROFILES_LABEL, &filter_options.profiles);
    let rule_options = filter_pick_list_options(ALL_RULES_LABEL, &filter_options.rules);
    let character_options =
        filter_pick_list_options(ALL_CHARACTERS_LABEL, &filter_options.characters);

    let active = active_filter_count(app);

    // Basic row: search + date preset + profile + rule + character
    let basic_row = row![
        text_input(
            "Search profiles, rules, characters, servers, bases, events...",
            &app.clip_filters.search,
        )
        .on_input(Message::SearchChanged)
        .width(Length::FillPortion(3)),
        pick_list(
            &DateRangePreset::ALL[..],
            Some(app.clip_date_range_preset),
            Message::DateRangePresetChanged,
        )
        .width(Length::FillPortion(1)),
        pick_list(
            profile_options,
            Some(selected_filter_option(
                &app.clip_filters.profile,
                ALL_PROFILES_LABEL,
            )),
            Message::ProfileFilterChanged,
        )
        .width(Length::FillPortion(1))
        .placeholder(ALL_PROFILES_LABEL),
        pick_list(
            rule_options,
            Some(selected_filter_option(
                &app.clip_filters.rule,
                ALL_RULES_LABEL
            )),
            Message::RuleFilterChanged,
        )
        .width(Length::FillPortion(1))
        .placeholder(ALL_RULES_LABEL),
        pick_list(
            character_options,
            Some(selected_filter_option(
                &app.clip_filters.character,
                ALL_CHARACTERS_LABEL,
            )),
            Message::CharacterFilterChanged,
        )
        .width(Length::FillPortion(1))
        .placeholder(ALL_CHARACTERS_LABEL),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let advanced_label = if app.clip_advanced_filters_open {
        "Hide advanced filters"
    } else {
        "Show advanced filters"
    };
    let advanced_toggle = with_tooltip(
        styled_button(advanced_label, ButtonTone::Secondary)
            .on_press(Message::ToggleAdvancedFilters)
            .into(),
        "Target, weapon, alert, overlap, server, continent, and base filters.",
    );

    let reset_button: Element<'static, Message> = if active > 0 {
        with_tooltip(
            styled_button("Reset", ButtonTone::Secondary)
                .on_press(Message::ClearFilters)
                .into(),
            "Reset every filter back to the defaults.",
        )
    } else {
        styled_button("Reset", ButtonTone::Secondary).into()
    };

    let mut content = column![
        toolbar()
            .push(clips_badge(
                format!(
                    "{active} active filter{}",
                    if active == 1 { "" } else { "s" }
                ),
                if active > 0 {
                    BadgeTone::Primary
                } else {
                    BadgeTone::Neutral
                },
            ))
            .push(clips_badge(date_range_summary(app), BadgeTone::Outline))
            .trailing(advanced_toggle)
            .trailing(reset_button)
            .build(),
        basic_row,
    ]
    .spacing(8);

    if app.clip_advanced_filters_open {
        content = content.push(advanced_filters_row(app, &filter_options));
    }

    if app.clip_date_range_preset == DateRangePreset::Custom {
        content = content.push(custom_range_row(app));
    }

    if let Some(feedback) = app.clip_filter_feedback.as_ref() {
        content = content.push(
            container(text(feedback.clone()).size(12))
                .padding([6, 10])
                .style(|theme: &iced::Theme| {
                    let c = &theme::tokens_for(theme).color;
                    ContainerStyle {
                        text_color: Some(c.muted_foreground),
                        background: Some(Background::Color(c.muted)),
                        border: theme::border(c.border, 1.0, theme::RADIUS.md),
                        ..Default::default()
                    }
                }),
        );
    }

    card()
        .title("Find clips")
        .body(content)
        .width(Length::Fill)
        .build()
        .into()
}

fn advanced_filters_row(
    app: &App,
    filter_options: &ClipFilterOptions,
) -> Element<'static, Message> {
    let target_options = filter_pick_list_options(ALL_TARGETS_LABEL, &filter_options.targets);
    let weapon_options = filter_pick_list_options(ALL_WEAPONS_LABEL, &filter_options.weapons);
    let alert_options = filter_pick_list_options(ALL_ALERTS_LABEL, &filter_options.alerts);
    let server_options = filter_pick_list_options(ALL_SERVERS_LABEL, &filter_options.servers);
    let continent_options =
        filter_pick_list_options(ALL_CONTINENTS_LABEL, &filter_options.continents);
    let base_options = filter_pick_list_options(ALL_BASES_LABEL, &filter_options.bases);

    column![
        row![
            pick_list(
                target_options,
                Some(selected_filter_option(
                    &app.clip_filters.target,
                    ALL_TARGETS_LABEL,
                )),
                Message::TargetFilterChanged,
            )
            .width(Length::FillPortion(1))
            .placeholder(ALL_TARGETS_LABEL),
            pick_list(
                weapon_options,
                Some(selected_filter_option(
                    &app.clip_filters.weapon,
                    ALL_WEAPONS_LABEL,
                )),
                Message::WeaponFilterChanged,
            )
            .width(Length::FillPortion(1))
            .placeholder(ALL_WEAPONS_LABEL),
            pick_list(
                alert_options,
                Some(selected_filter_option(
                    &app.clip_filters.alert,
                    ALL_ALERTS_LABEL
                )),
                Message::AlertFilterChanged,
            )
            .width(Length::FillPortion(1))
            .placeholder(ALL_ALERTS_LABEL),
            pick_list(
                &OverlapFilterChoice::ALL[..],
                Some(OverlapFilterChoice::from_state(
                    app.clip_filters.overlap_state
                )),
                Message::OverlapFilterChanged,
            )
            .width(Length::FillPortion(1)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        row![
            pick_list(
                server_options,
                Some(selected_filter_option(
                    &app.clip_filters.server,
                    ALL_SERVERS_LABEL
                )),
                Message::ServerFilterChanged,
            )
            .width(Length::FillPortion(1))
            .placeholder(ALL_SERVERS_LABEL),
            pick_list(
                continent_options,
                Some(selected_filter_option(
                    &app.clip_filters.continent,
                    ALL_CONTINENTS_LABEL,
                )),
                Message::ContinentFilterChanged,
            )
            .width(Length::FillPortion(1))
            .placeholder(ALL_CONTINENTS_LABEL),
            pick_list(
                base_options,
                Some(selected_filter_option(
                    &app.clip_filters.base,
                    ALL_BASES_LABEL
                )),
                Message::BaseFilterChanged,
            )
            .width(Length::FillPortion(1))
            .placeholder(ALL_BASES_LABEL),
            Space::new().width(Length::FillPortion(1)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(8)
    .into()
}

fn custom_range_row(app: &App) -> Element<'static, Message> {
    row![
        with_tooltip(
            text_input("Start: 2026-04-05 18:30", &app.clip_date_range_start)
                .on_input(Message::DateRangeStartChanged)
                .width(Length::Fill)
                .into(),
            "Local start time. Accepts YYYY-MM-DD, optional HH:MM(:SS), or RFC3339.",
        ),
        styled_button("Pick start", ButtonTone::Secondary)
            .on_press(Message::ToggleCalendar(CalendarField::Start)),
        with_tooltip(
            text_input("End: 2026-04-06 01:15", &app.clip_date_range_end)
                .on_input(Message::DateRangeEndChanged)
                .width(Length::Fill)
                .into(),
            "Local end time. Date-only values include the full day.",
        ),
        styled_button("Pick end", ButtonTone::Secondary)
            .on_press(Message::ToggleCalendar(CalendarField::End)),
        styled_button("Apply range", ButtonTone::Primary).on_press(Message::ApplyDateRange),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

// ---------------------------------------------------------------------------
// Workspace + history
// ---------------------------------------------------------------------------

fn clip_workspace(app: &App) -> Element<'static, Message> {
    row![
        container(clip_history_panel(app))
            .width(Length::FillPortion(5))
            .height(Length::Fill),
        container(clip_detail_workspace(app))
            .width(Length::FillPortion(4))
            .height(Length::Fill),
    ]
    .spacing(12)
    .height(Length::Fill)
    .into()
}

fn clip_history_panel(app: &App) -> Element<'static, Message> {
    let visible = app.clip_history.len();
    let total = app.clip_history_source.len();
    let page = app.clip_history_page;
    let page_size = app.clip_history_page_size.max(1);
    let total_pages = total_pages(app);

    let status_row = {
        let montage_count = app.montage_selection.len();
        let detail_label: &'static str = if app.clip_detail_loading {
            "Detail loading"
        } else if app.selected_clip_id.is_some() {
            "Detail ready"
        } else {
            "No clip selected"
        };
        let detail_tone = if app.clip_detail_loading {
            BadgeTone::Warning
        } else if app.selected_clip_id.is_some() {
            BadgeTone::Info
        } else {
            BadgeTone::Neutral
        };

        let mut bar = toolbar()
            .push(clips_badge(
                format!(
                    "{visible} matching clip{}",
                    if visible == 1 { "" } else { "s" }
                ),
                if visible == 0 {
                    BadgeTone::Warning
                } else {
                    BadgeTone::Info
                },
            ))
            .push(clips_badge(format!("{total} loaded"), BadgeTone::Outline))
            .push(clips_badge(detail_label, detail_tone));

        if montage_count > 0 {
            bar = bar.push(clips_badge(
                format!("{montage_count} in montage queue"),
                BadgeTone::Success,
            ));
            bar = bar.trailing(
                styled_button("Clear selection", ButtonTone::Secondary)
                    .on_press(Message::ClearMontageSelection),
            );
        }

        let montage_button: Element<'static, Message> = if montage_count >= 2 {
            styled_button(
                format!("Create montage ({montage_count})"),
                ButtonTone::Primary,
            )
            .on_press(Message::CreateMontage)
            .into()
        } else {
            with_tooltip(
                styled_button("Create montage", ButtonTone::Primary).into(),
                "Select at least two clips to build a montage.",
            )
        };
        bar.trailing(montage_button).build()
    };

    let header_row = history_header_row(app);
    let body_rows = history_body_rows(app, page, page_size);

    let footer = history_footer_row(app, page, page_size, total_pages);

    panel("Clip history")
        .push(status_row)
        .push(header_row)
        .push(
            container(body_rows)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .push(footer)
        .build()
        .into()
}

fn history_header_row(app: &App) -> Element<'static, Message> {
    let current = app.clip_sort_column;
    let desc = app.clip_sort_descending;

    container(
        row![
            header_static_cell(" ", Length::Fixed(42.0)),
            header_sort_cell(
                "When",
                Length::Fixed(150.0),
                ClipSortColumn::When,
                current,
                desc
            ),
            header_sort_cell(
                "Rule",
                Length::FillPortion(3),
                ClipSortColumn::Rule,
                current,
                desc
            ),
            header_sort_cell(
                "Character",
                Length::FillPortion(2),
                ClipSortColumn::Character,
                current,
                desc,
            ),
            header_static_cell("Location", Length::FillPortion(3)),
            header_sort_cell(
                "Score",
                Length::Fixed(70.0),
                ClipSortColumn::Score,
                current,
                desc,
            ),
            header_sort_cell(
                "Dur",
                Length::Fixed(64.0),
                ClipSortColumn::Duration,
                current,
                desc,
            ),
            header_static_cell("Flags", Length::Fixed(120.0)),
        ]
        .spacing(0),
    )
    .width(Length::Fill)
    .style(|theme: &iced::Theme| {
        let c = &theme::tokens_for(theme).color;
        ContainerStyle {
            text_color: Some(c.muted_foreground),
            background: Some(Background::Color(c.muted)),
            border: theme::border(c.border, 1.0, theme::RADIUS.md),
            ..Default::default()
        }
    })
    .into()
}

fn header_static_cell(label: &'static str, width: Length) -> Element<'static, Message> {
    container(text(label).size(11).style(|theme: &iced::Theme| TextStyle {
        color: Some(theme::tokens_for(theme).color.muted_foreground),
    }))
    .padding([8, 10])
    .width(width)
    .into()
}

fn header_sort_cell(
    label: &'static str,
    width: Length,
    column: ClipSortColumn,
    current: ClipSortColumn,
    desc: bool,
) -> Element<'static, Message> {
    let arrow = if column == current {
        if desc { " \u{2193}" } else { " \u{2191}" }
    } else {
        ""
    };
    let is_active = column == current;
    let content = container(text(format!("{label}{arrow}")).size(11).style(
        move |theme: &iced::Theme| {
            let c = &theme::tokens_for(theme).color;
            TextStyle {
                color: Some(if is_active {
                    c.foreground
                } else {
                    c.muted_foreground
                }),
            }
        },
    ))
    .padding([8, 10])
    .width(Length::Fill);

    button(content)
        .padding(0)
        .width(width)
        .style(
            |theme: &iced::Theme, status: iced::widget::button::Status| {
                let c = &theme::tokens_for(theme).color;
                let bg = match status {
                    iced::widget::button::Status::Hovered => Some(Background::Color(c.accent)),
                    iced::widget::button::Status::Pressed => Some(Background::Color(c.secondary)),
                    _ => None,
                };
                iced::widget::button::Style {
                    background: bg,
                    text_color: c.foreground,
                    border: theme::border(Color::TRANSPARENT, 0.0, 0.0),
                    shadow: Default::default(),
                    snap: false,
                }
            },
        )
        .on_press(Message::SortColumnClicked(column))
        .into()
}

fn history_body_rows(app: &App, page: usize, page_size: usize) -> Element<'static, Message> {
    if app.clip_history.is_empty() {
        return empty_state("No matching clips")
            .description("Adjust the search, widen the range, or clear filters.")
            .build()
            .into();
    }

    let start = page * page_size;
    let end = (start + page_size).min(app.clip_history.len());
    let slice = &app.clip_history[start..end];

    let rows: Vec<Element<'static, Message>> = slice
        .iter()
        .map(|record| dense_history_row(app, record, app.selected_clip_id == Some(record.id)))
        .collect();

    scrollable(Column::with_children(rows).spacing(2))
        .id(CLIP_HISTORY_SCROLLABLE_ID)
        .height(Length::Fill)
        .on_scroll(|viewport| Message::HistoryScrolled(viewport.into()))
        .into()
}

fn history_footer_row(
    app: &App,
    page: usize,
    page_size: usize,
    total_pages: usize,
) -> Element<'static, Message> {
    let visible = app.clip_history.len();
    let start = if visible == 0 {
        0
    } else {
        page * page_size + 1
    };
    let end = ((page + 1) * page_size).min(visible);
    let range_label = if visible == 0 {
        "No results".to_string()
    } else {
        format!("{start}\u{2013}{end} of {visible}")
    };

    let paginator: Element<'static, Message> = if total_pages > 1 {
        pagination(page + 1, total_pages, 1, Message::PageChanged)
    } else {
        Space::new().width(Length::Shrink).into()
    };

    let size_picker = with_tooltip(
        pick_list(
            &PAGE_SIZE_OPTIONS[..],
            Some(app.clip_history_page_size),
            Message::PageSizeChanged,
        )
        .width(Length::Fixed(88.0))
        .into(),
        "Number of clips per page.",
    );

    toolbar()
        .push(clips_badge(range_label, BadgeTone::Outline))
        .trailing(paginator)
        .trailing(size_picker)
        .build()
}

fn dense_history_row(app: &App, record: &ClipRecord, selected: bool) -> Element<'static, Message> {
    let clip_id = record.id;
    let montage_selected = app.montage_selection.contains(&clip_id);
    let post_process_block = super::super::clip_post_process_block_reason(record);

    let checkbox_cell = container(montage_selection_checkbox(
        clip_id,
        montage_selected,
        post_process_block.as_deref(),
    ))
    .padding([6, 10])
    .width(Length::Fixed(42.0))
    .align_y(Alignment::Center);

    let content = row![
        checkbox_cell,
        dense_text_cell(
            format_smart_timestamp(record.trigger_event_at),
            Length::Fixed(150.0)
        ),
        dense_text_cell(rule_label(app, record), Length::FillPortion(3)),
        dense_text_cell(character_label(app, record), Length::FillPortion(2)),
        dense_text_cell(location_summary(record), Length::FillPortion(3)),
        dense_text_cell(record.score.to_string(), Length::Fixed(70.0)),
        dense_text_cell(short_duration_label(record), Length::Fixed(64.0)),
        container(flag_badges(record))
            .padding([6, 10])
            .width(Length::Fixed(120.0))
            .align_y(Alignment::Center),
    ]
    .spacing(0)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let row_container = container(content)
        .width(Length::Fill)
        .style(move |theme: &iced::Theme| clip_list_row_style(theme, selected));

    button(row_container)
        .padding(0)
        .style(move |theme, status| clip_list_button_style(theme, status, selected))
        .on_press(Message::RowSelected(clip_id))
        .into()
}

fn dense_text_cell(value: impl Into<String>, width: Length) -> Element<'static, Message> {
    container(
        text(value.into())
            .size(12)
            .style(|theme: &iced::Theme| TextStyle {
                color: Some(theme::tokens_for(theme).color.foreground),
            }),
    )
    .padding([6, 10])
    .width(width)
    .align_y(Alignment::Center)
    .into()
}

fn clip_list_row_style(theme: &iced::Theme, selected: bool) -> ContainerStyle {
    let c = &theme::tokens_for(theme).color;
    let background = if selected {
        Some(Background::Color(c.primary))
    } else {
        None
    };
    let border_color = if selected { c.primary } else { c.border };
    let text_color = if selected {
        Some(c.primary_foreground)
    } else {
        Some(c.foreground)
    };
    ContainerStyle {
        text_color,
        background,
        border: theme::border(border_color, 1.0, theme::RADIUS.md),
        ..Default::default()
    }
}

fn clip_list_button_style(
    theme: &iced::Theme,
    status: iced::widget::button::Status,
    selected: bool,
) -> iced::widget::button::Style {
    let c = &theme::tokens_for(theme).color;
    let (bg, fg) = if selected {
        (Some(Background::Color(c.primary)), c.primary_foreground)
    } else {
        match status {
            iced::widget::button::Status::Hovered => {
                (Some(Background::Color(c.accent)), c.foreground)
            }
            iced::widget::button::Status::Pressed => {
                (Some(Background::Color(c.muted)), c.foreground)
            }
            _ => (None, c.foreground),
        }
    };
    iced::widget::button::Style {
        background: bg,
        text_color: fg,
        border: theme::border(
            if selected {
                c.primary
            } else {
                Color::TRANSPARENT
            },
            1.0,
            theme::RADIUS.md,
        ),
        shadow: Default::default(),
        snap: false,
    }
}

fn montage_selection_checkbox(
    clip_id: i64,
    selected: bool,
    block_reason: Option<&str>,
) -> Element<'static, Message> {
    let checkbox = if selected || block_reason.is_none() {
        center(checkbox(selected).on_toggle(move |_| Message::ToggleMontageSelection(clip_id)))
            .width(Length::Fixed(32.0))
            .into()
    } else {
        center(checkbox(selected)).width(Length::Fixed(32.0)).into()
    };
    with_tooltip(
        checkbox,
        block_reason.unwrap_or("Select for the montage builder."),
    )
}

fn flag_badges(record: &ClipRecord) -> Element<'static, Message> {
    let mut items: Vec<Element<'static, Message>> = Vec::new();
    if record.overlap_count > 0 {
        items.push(with_tooltip(
            badge(format!("O{}", record.overlap_count))
                .tone(BadgeTone::Warning)
                .build()
                .into(),
            format!(
                "{} overlapping clip{} recorded.",
                record.overlap_count,
                if record.overlap_count == 1 { "" } else { "s" }
            ),
        ));
    }
    if record.alert_count > 0 {
        items.push(with_tooltip(
            badge(format!("A{}", record.alert_count))
                .tone(BadgeTone::Info)
                .build()
                .into(),
            format!(
                "Linked to {} alert{}.",
                record.alert_count,
                if record.alert_count == 1 { "" } else { "s" }
            ),
        ));
    }
    match record.post_process_status {
        crate::db::PostProcessStatus::Failed => {
            items.push(with_tooltip(
                badge("PP").tone(BadgeTone::Destructive).build().into(),
                "Audio post-processing failed — see audio recovery in the detail panel.",
            ));
        }
        crate::db::PostProcessStatus::Pending => {
            items.push(with_tooltip(
                badge("PP").tone(BadgeTone::Warning).build().into(),
                "Audio post-processing pending — upload and export may be blocked.",
            ));
        }
        crate::db::PostProcessStatus::Completed => {
            items.push(with_tooltip(
                badge("PP").tone(BadgeTone::Success).build().into(),
                "Audio post-processing complete.",
            ));
        }
        _ => {}
    }

    if items.is_empty() {
        return Space::new().width(Length::Shrink).into();
    }

    let mut bar = row![].spacing(4).align_y(Alignment::Center);
    for item in items {
        bar = bar.push(item);
    }
    bar.into()
}

// ---------------------------------------------------------------------------
// Detail workspace
// ---------------------------------------------------------------------------

fn clip_detail_workspace(app: &App) -> Element<'static, Message> {
    if let Some(detail) = app.selected_clip_detail.as_ref() {
        clip_detail_panel(app, detail)
    } else if app.clip_detail_loading {
        panel("Clip detail")
            .push(
                empty_state("Loading clip detail")
                    .description("Resolving the selected clip from storage.")
                    .build(),
            )
            .build()
            .into()
    } else {
        panel("Clip detail")
            .push(
                empty_state("No clip selected")
                    .description(
                        "Arrow keys navigate, Enter opens, Space toggles montage selection, Delete removes.",
                    )
                    .build(),
            )
            .build()
            .into()
    }
}

fn clip_detail_panel(app: &App, detail: &ClipDetailRecord) -> Element<'static, Message> {
    let record = &detail.clip;
    let storage_tier = storage_tiering::clip_storage_tier(record, &app.config.storage_tiering);
    let post_process_block = super::super::clip_post_process_block_reason(record);
    let can_export_timeline = record.path.is_some() && !detail.raw_events.is_empty();

    let mut content: Column<'static, Message> = column![].spacing(12);
    content = content.push(detail_summary_card(app, record, storage_tier));

    content = content.push(detail_actions_section(
        record,
        storage_tier,
        post_process_block.as_deref(),
        can_export_timeline,
        detail,
        app.deleting_clip_id == Some(record.id),
    ));

    if matches!(
        record.post_process_status,
        crate::db::PostProcessStatus::Failed
    ) {
        content = content.push(
            section("Audio recovery")
                .description("Audio post-process failed. Retry or keep the original tracks.")
                .push(
                    row![
                        with_tooltip(
                            styled_button("Retry audio post-process", ButtonTone::Warning)
                                .on_press(Message::RetryPostProcessRequested(record.id))
                                .into(),
                            "Re-run audio post-processing.",
                        ),
                        with_tooltip(
                            styled_button("Use original audio", ButtonTone::Secondary)
                                .on_press(Message::UseOriginalAudioRequested(record.id))
                                .into(),
                            "Keep the original audio.",
                        ),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .build(),
        );
    }

    content = content.push(audio_tracks_section(app, detail));
    content = content.push(uploads_section(app, detail));
    content = content.push(alerts_section(app, detail));
    content = content.push(overlaps_section(app, detail));
    content = content.push(raw_events_section(app, detail));

    panel(format!("Clip #{} detail", record.id))
        .push(scrollable(content).height(Length::Fill))
        .build()
        .into()
}

fn detail_summary_card(
    app: &App,
    record: &ClipRecord,
    storage_tier: StorageTier,
) -> Element<'static, Message> {
    let badges = row![
        clips_badge(format!("Clip #{}", record.id), BadgeTone::Primary),
        clips_badge(storage_tier.label(), BadgeTone::Outline),
        clips_badge(format!("Score {}", record.score), BadgeTone::Info),
        clips_badge(duration_label(record), BadgeTone::Info),
        clips_badge(post_process_status_label(record), BadgeTone::Neutral),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let meta = column![
        text(format!("Rule: {}", rule_label(app, record))).size(13),
        text(format!(
            "Character: {}  \u{2022}  Server: {}  \u{2022}  Continent: {}",
            character_label(app, record),
            server_label(record),
            continent_label(record)
        ))
        .size(13),
        text(format!("Location: {}", location_summary(record))).size(13),
        text(format!(
            "Window: {}  \u{2192}  {}",
            format_timestamp(record.clip_start_at),
            format_timestamp(record.clip_end_at)
        ))
        .size(12)
        .style(|theme: &iced::Theme| TextStyle {
            color: Some(theme::tokens_for(theme).color.muted_foreground),
        }),
        text(format!(
            "Overlap review: {}  \u{2022}  Alert context: {}",
            overlap_label(record),
            alert_label(record)
        ))
        .size(12)
        .style(|theme: &iced::Theme| TextStyle {
            color: Some(theme::tokens_for(theme).color.muted_foreground),
        }),
    ]
    .spacing(4);

    section("Summary").push(badges).push(meta).build().into()
}

fn detail_actions_section(
    record: &ClipRecord,
    storage_tier: StorageTier,
    post_process_block: Option<&str>,
    can_export_timeline: bool,
    detail: &ClipDetailRecord,
    deleting: bool,
) -> Element<'static, Message> {
    let clip_id = record.id;

    // Primary row — the clip file itself.
    let open_button = with_tooltip(
        if record.path.is_some() {
            styled_button("Open", ButtonTone::Primary)
                .on_press(Message::OpenRequested(clip_id))
                .into()
        } else {
            styled_button("Open", ButtonTone::Primary).into()
        },
        if record.path.is_some() {
            "Open the saved clip file."
        } else {
            "No saved file attached."
        },
    );
    let delete_label: &'static str = if deleting { "Deleting..." } else { "Delete" };
    let delete_button = with_tooltip(
        if deleting {
            styled_button(delete_label, ButtonTone::Danger).into()
        } else {
            styled_button(delete_label, ButtonTone::Danger)
                .on_press(Message::DeleteRequested(clip_id))
                .into()
        },
        if record.path.is_some() {
            "Delete this clip after confirmation."
        } else {
            "Remove this clip row from history."
        },
    );
    let honu_button: Option<Element<'static, Message>> = record.honu_session_id.map(|id| {
        with_tooltip(
            styled_button("Honu session", ButtonTone::Secondary)
                .on_press(Message::OpenHonuSession(id))
                .into(),
            "Open this session on Honu.",
        )
    });

    let mut primary_row = row![open_button, delete_button]
        .spacing(8)
        .align_y(Alignment::Center);
    if let Some(honu) = honu_button {
        primary_row = primary_row.push(honu);
    }

    // Export group
    let chapter_button = with_tooltip(
        if can_export_timeline {
            styled_button("Chapters", ButtonTone::Secondary)
                .on_press(Message::ExportChaptersRequested(clip_id))
                .into()
        } else {
            styled_button("Chapters", ButtonTone::Secondary).into()
        },
        "Export chapter markers as a sidecar file.",
    );
    let subtitle_button = with_tooltip(
        if can_export_timeline {
            styled_button("Subtitles", ButtonTone::Secondary)
                .on_press(Message::ExportSubtitlesRequested(clip_id))
                .into()
        } else {
            styled_button("Subtitles", ButtonTone::Secondary).into()
        },
        "Export timeline markers as a sidecar SRT file.",
    );

    // Storage tier toggle
    let storage_toggle = storage_tier_toggle(clip_id, storage_tier);

    // Upload group
    let copyparty_state = upload_action_state(detail, UploadProvider::Copyparty);
    let youtube_state = upload_action_state(detail, UploadProvider::YouTube);
    let copyparty_button = upload_button(
        clip_id,
        "Copyparty",
        UploadProvider::Copyparty,
        copyparty_state,
        post_process_block,
    );
    let youtube_button = upload_button(
        clip_id,
        "YouTube",
        UploadProvider::YouTube,
        youtube_state,
        post_process_block,
    );

    section("Actions")
        .push(primary_row)
        .push(
            row![label_chip("Export"), chapter_button, subtitle_button,]
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .push(
            row![label_chip("Storage"), storage_toggle]
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .push(
            row![label_chip("Upload"), copyparty_button, youtube_button]
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .build()
        .into()
}

fn label_chip(label: &'static str) -> Element<'static, Message> {
    container(text(label).size(11).style(|theme: &iced::Theme| TextStyle {
        color: Some(theme::tokens_for(theme).color.muted_foreground),
    }))
    .padding(Padding {
        top: 0.0,
        bottom: 0.0,
        left: 0.0,
        right: 4.0,
    })
    .width(Length::Fixed(72.0))
    .into()
}

fn storage_tier_toggle(clip_id: i64, current: StorageTier) -> Element<'static, Message> {
    row![
        segmented_button(
            "Primary",
            current == StorageTier::Primary,
            Message::SetStorageTier(clip_id, StorageTier::Primary),
            "Keep on primary (fast) storage.",
        ),
        segmented_button(
            "Archive",
            current == StorageTier::Archive,
            Message::SetStorageTier(clip_id, StorageTier::Archive),
            "Move to archive (cold) storage.",
        ),
    ]
    .spacing(0)
    .align_y(Alignment::Center)
    .into()
}

fn segmented_button(
    label: &'static str,
    active: bool,
    message: Message,
    tooltip: &'static str,
) -> Element<'static, Message> {
    let content = container(text(label).size(12).style(move |theme: &iced::Theme| {
        let c = &theme::tokens_for(theme).color;
        TextStyle {
            color: Some(if active {
                c.primary_foreground
            } else {
                c.foreground
            }),
        }
    }))
    .padding([6, 14]);

    let btn = button(content).padding(0).style(
        move |theme: &iced::Theme, status: iced::widget::button::Status| {
            let c = &theme::tokens_for(theme).color;
            let (bg, border_color) = if active {
                (Some(Background::Color(c.primary)), c.primary)
            } else {
                match status {
                    iced::widget::button::Status::Hovered => {
                        (Some(Background::Color(c.accent)), c.border_strong)
                    }
                    iced::widget::button::Status::Pressed => {
                        (Some(Background::Color(c.muted)), c.border_strong)
                    }
                    _ => (None, c.border),
                }
            };
            iced::widget::button::Style {
                background: bg,
                text_color: if active {
                    c.primary_foreground
                } else {
                    c.foreground
                },
                border: theme::border(border_color, 1.0, theme::RADIUS.md),
                shadow: Default::default(),
                snap: false,
            }
        },
    );

    with_tooltip(
        if active {
            btn.into()
        } else {
            btn.on_press(message).into()
        },
        tooltip,
    )
}

fn upload_button(
    clip_id: i64,
    label: &'static str,
    provider: UploadProvider,
    state: Option<ClipUploadState>,
    post_process_block: Option<&str>,
) -> Element<'static, Message> {
    let (button_label, tone, enabled, tooltip): (String, ButtonTone, bool, String) =
        match (post_process_block, state) {
            (Some(reason), _) => (
                label.into(),
                ButtonTone::Secondary,
                false,
                reason.to_string(),
            ),
            (None, Some(ClipUploadState::Running)) => (
                format!("{label} uploading..."),
                ButtonTone::Secondary,
                false,
                format!("{label} upload in progress."),
            ),
            (None, Some(ClipUploadState::Succeeded)) => (
                format!("{label} \u{2713}"),
                ButtonTone::Success,
                false,
                format!("Already uploaded to {label}."),
            ),
            (None, _) => (
                label.into(),
                ButtonTone::Secondary,
                true,
                format!("Upload to {label}."),
            ),
        };

    let message = match provider {
        UploadProvider::Copyparty => Message::UploadToCopypartyRequested(clip_id),
        UploadProvider::YouTube => Message::UploadToYouTubeRequested(clip_id),
    };

    let button: Element<'static, Message> = if enabled {
        styled_button(button_label, tone).on_press(message).into()
    } else {
        styled_button(button_label, tone).into()
    };
    with_tooltip(button, tooltip)
}

fn upload_action_state(
    detail: &ClipDetailRecord,
    provider: UploadProvider,
) -> Option<ClipUploadState> {
    detail
        .uploads
        .iter()
        .filter(|upload| upload.provider == provider)
        .find_map(|upload| match upload.state {
            ClipUploadState::Running | ClipUploadState::Succeeded => Some(upload.state),
            ClipUploadState::Failed | ClipUploadState::Cancelled => None,
        })
}

// ---------------------------------------------------------------------------
// Detail sections (optional)
// ---------------------------------------------------------------------------

fn section_is_collapsed(app: &App, section: DetailSection) -> bool {
    app.clip_collapsed_detail_sections.contains(&section)
}

fn collapsible_header(
    title: impl Into<String>,
    description: Option<&'static str>,
    count: usize,
    section: DetailSection,
    collapsed: bool,
) -> Element<'static, Message> {
    let arrow = if collapsed { "\u{25B8}" } else { "\u{25BE}" };
    let title_text = format!("{arrow}  {}", title.into());
    let count_badge_tone = if count > 0 {
        BadgeTone::Outline
    } else {
        BadgeTone::Neutral
    };
    let count_badge_label = if count > 0 {
        count.to_string()
    } else {
        "None".to_string()
    };
    let count_badge: Element<'static, Message> = badge(count_badge_label)
        .tone(count_badge_tone)
        .build()
        .into();

    let title_column: Element<'static, Message> = if let Some(desc) = description {
        column![
            text(title_text)
                .size(14)
                .style(|theme: &iced::Theme| TextStyle {
                    color: Some(theme::tokens_for(theme).color.foreground),
                }),
            text(desc).size(11).style(|theme: &iced::Theme| TextStyle {
                color: Some(theme::tokens_for(theme).color.muted_foreground),
            }),
        ]
        .spacing(2)
        .into()
    } else {
        text(title_text)
            .size(14)
            .style(|theme: &iced::Theme| TextStyle {
                color: Some(theme::tokens_for(theme).color.foreground),
            })
            .into()
    };

    let head = row![title_column, Space::new().width(Length::Fill), count_badge,]
        .spacing(8)
        .align_y(Alignment::Center);

    let btn = button(container(head).padding([6, 8]))
        .padding(0)
        .style(
            |theme: &iced::Theme, status: iced::widget::button::Status| {
                let c = &theme::tokens_for(theme).color;
                let bg = match status {
                    iced::widget::button::Status::Hovered => Some(Background::Color(c.accent)),
                    iced::widget::button::Status::Pressed => Some(Background::Color(c.muted)),
                    _ => None,
                };
                iced::widget::button::Style {
                    background: bg,
                    text_color: c.foreground,
                    border: theme::border(c.border, 1.0, theme::RADIUS.md),
                    shadow: Default::default(),
                    snap: false,
                }
            },
        )
        .on_press(Message::ToggleDetailSection(section));
    btn.into()
}

fn audio_tracks_section(app: &App, detail: &ClipDetailRecord) -> Element<'static, Message> {
    let count = detail.audio_tracks.len();
    let collapsed = section_is_collapsed(app, DetailSection::AudioTracks);

    let header = collapsible_header(
        "Audio tracks",
        Some("Per-stream gain, mute, and source info recorded during save."),
        count,
        DetailSection::AudioTracks,
        collapsed,
    );

    let mut wrapper: Column<'static, Message> = column![header].spacing(6);
    if !collapsed {
        if count == 0 {
            wrapper = wrapper.push(
                text("No explicit audio track metadata is recorded for this clip.")
                    .size(12)
                    .style(|theme: &iced::Theme| TextStyle {
                        color: Some(theme::tokens_for(theme).color.muted_foreground),
                    }),
            );
        } else {
            let mut body: Column<'static, Message> = column![].spacing(6);
            for track in &detail.audio_tracks {
                body = body.push(
                    card()
                        .body(
                            text(format!(
                                "a:{}  |  {}  |  {}  |  gain {:+.1} dB{}  |  {}",
                                track.stream_index,
                                track.role,
                                track.label,
                                track.gain_db,
                                if track.muted {
                                    "  |  muted in premix"
                                } else {
                                    ""
                                },
                                track.source_value,
                            ))
                            .size(12),
                        )
                        .width(Length::Fill)
                        .build(),
                );
            }
            wrapper = wrapper.push(body);
        }
    }
    wrapper.into()
}

fn uploads_section(app: &App, detail: &ClipDetailRecord) -> Element<'static, Message> {
    let count = detail.uploads.len();
    let collapsed = section_is_collapsed(app, DetailSection::Uploads);

    let header = collapsible_header(
        "Upload history",
        Some("Every attempted or completed upload across providers."),
        count,
        DetailSection::Uploads,
        collapsed,
    );

    let mut wrapper: Column<'static, Message> = column![header].spacing(6);
    if !collapsed {
        if count == 0 {
            wrapper = wrapper.push(
                text("No upload history recorded for this clip yet.")
                    .size(12)
                    .style(|theme: &iced::Theme| TextStyle {
                        color: Some(theme::tokens_for(theme).color.muted_foreground),
                    }),
            );
        } else {
            let mut body: Column<'static, Message> = column![].spacing(6);
            for upload in &detail.uploads {
                let upload_body: Element<'static, Message> = match &upload.clip_url {
                    Some(url) => row![
                        text(format!(
                            "{}  |  {}",
                            upload.provider.label(),
                            upload.state.as_str()
                        ))
                        .size(12),
                        with_tooltip(
                            mouse_area(text(url.clone()).size(12).style(|theme: &iced::Theme| {
                                TextStyle {
                                    color: Some(theme::tokens_for(theme).color.primary),
                                }
                            }))
                            .on_press(Message::OpenUploadUrl(url.clone()))
                            .interaction(iced::mouse::Interaction::Pointer)
                            .into(),
                            "Open the uploaded clip URL.",
                        ),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into(),
                    None => text(format!(
                        "{}  |  {}{}",
                        upload.provider.label(),
                        upload.state.as_str(),
                        upload
                            .error_message
                            .as_ref()
                            .map(|error| format!("  |  {error}"))
                            .unwrap_or_default()
                    ))
                    .size(12)
                    .into(),
                };

                body = body.push(card().body(upload_body).width(Length::Fill).build());
            }
            wrapper = wrapper.push(body);
        }
    }
    wrapper.into()
}

fn alerts_section(app: &App, detail: &ClipDetailRecord) -> Element<'static, Message> {
    let count = detail.alerts.len();
    let collapsed = section_is_collapsed(app, DetailSection::Alerts);

    let header = collapsible_header(
        "Alert context",
        Some("Alerts the clip was captured during."),
        count,
        DetailSection::Alerts,
        collapsed,
    );

    let mut wrapper: Column<'static, Message> = column![header].spacing(6);
    if !collapsed {
        if count == 0 {
            wrapper = wrapper.push(
                text("No linked alert context was captured for this clip.")
                    .size(12)
                    .style(|theme: &iced::Theme| TextStyle {
                        color: Some(theme::tokens_for(theme).color.muted_foreground),
                    }),
            );
        } else {
            let mut body: Column<'static, Message> = column![].spacing(6);
            for alert in &detail.alerts {
                let outcome = alert
                    .winner_faction
                    .as_ref()
                    .map(|winner| format!("winner {winner}"))
                    .unwrap_or_else(|| alert.state_name.clone());

                body = body.push(
                    card()
                        .body(
                            text(format!(
                                "{}  |  {}  |  started {}{}",
                                alert.label,
                                outcome,
                                format_timestamp(alert.started_at),
                                alert
                                    .ended_at
                                    .map(|ended_at| format!(
                                        "  |  ended {}",
                                        format_timestamp(ended_at)
                                    ))
                                    .unwrap_or_default()
                            ))
                            .size(12),
                        )
                        .width(Length::Fill)
                        .build(),
                );
            }
            wrapper = wrapper.push(body);
        }
    }
    wrapper.into()
}

fn overlaps_section(app: &App, detail: &ClipDetailRecord) -> Element<'static, Message> {
    let count = detail.overlaps.len();
    let collapsed = section_is_collapsed(app, DetailSection::Overlaps);

    let header = collapsible_header(
        "Overlap review",
        Some("Other saved clips whose timelines intersect this one."),
        count,
        DetailSection::Overlaps,
        collapsed,
    );

    let mut wrapper: Column<'static, Message> = column![header].spacing(6);
    if !collapsed {
        if count == 0 {
            wrapper = wrapper.push(
                text("No overlapping saved clips were linked to this record.")
                    .size(12)
                    .style(|theme: &iced::Theme| TextStyle {
                        color: Some(theme::tokens_for(theme).color.muted_foreground),
                    }),
            );
        } else {
            let mut body: Column<'static, Message> = column![].spacing(6);
            for overlap in &detail.overlaps {
                body = body.push(
                    card()
                        .body(
                            row![
                                text(format!(
                                    "Clip #{}  |  {}  |  {} overlap",
                                    overlap.clip_id,
                                    format_timestamp(overlap.trigger_event_at),
                                    format_duration_ms(overlap.overlap_duration_ms)
                                ))
                                .size(12)
                                .width(Length::Fill),
                                styled_button("Inspect", ButtonTone::Secondary)
                                    .on_press(Message::RowSelected(overlap.clip_id)),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center),
                        )
                        .width(Length::Fill)
                        .build(),
                );
            }
            wrapper = wrapper.push(body);
        }
    }
    wrapper.into()
}

fn raw_events_section(app: &App, detail: &ClipDetailRecord) -> Element<'static, Message> {
    let record = &detail.clip;
    let filter = app.clip_raw_event_filter.trim().to_lowercase();
    let filtered: Vec<&crate::db::ClipRawEventRecord> = detail
        .raw_events
        .iter()
        .filter(|event| {
            if filter.is_empty() {
                return true;
            }
            let target = event
                .other_character_name
                .clone()
                .unwrap_or_default()
                .to_lowercase();
            let weapon = event
                .attacker_weapon_name
                .clone()
                .unwrap_or_default()
                .to_lowercase();
            event.event_kind.to_lowercase().contains(&filter)
                || target.contains(&filter)
                || weapon.contains(&filter)
        })
        .collect();

    let total_count = detail.raw_events.len();
    let visible_count = filtered.len();
    let collapsed = section_is_collapsed(app, DetailSection::RawEvents);

    let header = collapsible_header(
        "Captured raw events",
        Some("Timeline offsets relative to the clip start."),
        total_count,
        DetailSection::RawEvents,
        collapsed,
    );

    let mut wrapper: Column<'static, Message> = column![header].spacing(6);
    if !collapsed {
        if total_count == 0 {
            wrapper = wrapper.push(
                text("No raw event markers were captured for this clip.")
                    .size(12)
                    .style(|theme: &iced::Theme| TextStyle {
                        color: Some(theme::tokens_for(theme).color.muted_foreground),
                    }),
            );
        } else {
            wrapper = wrapper.push(
                row![
                    with_tooltip(
                        text_input(
                            "Filter event kind, target, weapon",
                            &app.clip_raw_event_filter
                        )
                        .on_input(Message::RawEventFilterChanged)
                        .width(Length::Fill)
                        .into(),
                        "Filter this clip's event list.",
                    ),
                    clips_badge(
                        format!("{visible_count}/{total_count}"),
                        if visible_count == 0 {
                            BadgeTone::Warning
                        } else {
                            BadgeTone::Outline
                        },
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );

            if filtered.is_empty() {
                wrapper = wrapper.push(text("No events match the filter.").size(12).style(
                    |theme: &iced::Theme| TextStyle {
                        color: Some(theme::tokens_for(theme).color.muted_foreground),
                    },
                ));
            } else {
                let mut event_column: Column<'static, Message> = column![].spacing(4);
                for event in &filtered {
                    event_column = event_column.push(
                        container(text(timeline_line(record, event)).size(12))
                            .padding([4, 8])
                            .width(Length::Fill)
                            .style(|theme: &iced::Theme| {
                                let c = &theme::tokens_for(theme).color;
                                ContainerStyle {
                                    text_color: Some(c.foreground),
                                    background: Some(Background::Color(c.muted)),
                                    border: theme::border(c.border, 1.0, theme::RADIUS.sm),
                                    ..Default::default()
                                }
                            }),
                    );
                }
                wrapper = wrapper.push(
                    scrollable(event_column)
                        .height(Length::Fixed(240.0))
                        .width(Length::Fill),
                );
            }
        }
    }
    wrapper.into()
}

// ---------------------------------------------------------------------------
// Modals
// ---------------------------------------------------------------------------

fn calendar_dropdown(app: &App, field: CalendarField) -> Element<'static, Message> {
    let label = match field {
        CalendarField::Start => "Start date",
        CalendarField::End => "End date",
    };
    let selected = calendar_seed_date(calendar_input(app, field));
    let picker = date_picker(
        app.clip_calendar_month,
        selected,
        Message::CalendarDaySelected,
        Message::CalendarMonthChanged,
    );

    column![
        text(label).size(16),
        picker,
        row![styled_button("Close", ButtonTone::Secondary).on_press(Message::DismissCalendar),]
            .spacing(8)
            .align_y(Alignment::Center),
    ]
    .spacing(12)
    .into()
}

fn montage_queue_dialog(app: &App) -> Element<'static, Message> {
    let confirm_button: Element<'static, Message> = if app.montage_selection.len() >= 2 {
        styled_button("Confirm montage", ButtonTone::Primary)
            .on_press(Message::ConfirmMontageCreation)
            .into()
    } else {
        with_tooltip(
            styled_button("Confirm montage", ButtonTone::Primary).into(),
            "Select at least two clips to build a montage.",
        )
    };

    let mut content = column![
        text("Create montage").size(20),
        text("Reorder or remove clips, then confirm to start a background montage job.")
            .size(13)
            .style(|theme: &iced::Theme| TextStyle {
                color: Some(theme::tokens_for(theme).color.muted_foreground),
            }),
        row![
            clips_badge(
                format!("{} selected", app.montage_selection.len()),
                if app.montage_selection.len() >= 2 {
                    BadgeTone::Success
                } else {
                    BadgeTone::Warning
                },
            ),
            clips_badge("Modal queue", BadgeTone::Outline),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(12);

    if app.montage_selection.is_empty() {
        content = content.push(
            empty_state("No clips selected")
                .description("Select clips in the history table first.")
                .build(),
        );
    } else {
        let last_index = app.montage_selection.len().saturating_sub(1);
        let mut queue = column![].spacing(8);
        for (index, clip_id) in app.montage_selection.iter().enumerate() {
            let maybe_record = app
                .clip_history_source
                .iter()
                .find(|record| record.id == *clip_id);

            let title = maybe_record
                .map(|record| rule_label(app, record))
                .unwrap_or_else(|| format!("Clip #{clip_id}"));
            let subtitle = maybe_record
                .map(|record| {
                    format!(
                        "#{}  |  {}  |  {}",
                        record.id,
                        format_timestamp(record.trigger_event_at),
                        character_label(app, record)
                    )
                })
                .unwrap_or_else(|| "No longer present in the loaded history.".into());

            let badges = row![
                clips_badge(format!("Position {}", index + 1), BadgeTone::Primary),
                clips_badge(
                    maybe_record
                        .map(duration_label)
                        .unwrap_or_else(|| "Unavailable".into()),
                    BadgeTone::Info,
                ),
                clips_badge(
                    maybe_record
                        .map(size_label)
                        .unwrap_or_else(|| "\u{2013}".into()),
                    BadgeTone::Outline,
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center);

            let up_btn: Element<'static, Message> = if index > 0 {
                styled_button("\u{2191} Up", ButtonTone::Secondary)
                    .on_press(Message::MontageMoveUp(*clip_id))
                    .into()
            } else {
                styled_button("\u{2191} Up", ButtonTone::Secondary).into()
            };
            let down_btn: Element<'static, Message> = if index < last_index {
                styled_button("\u{2193} Down", ButtonTone::Secondary)
                    .on_press(Message::MontageMoveDown(*clip_id))
                    .into()
            } else {
                styled_button("\u{2193} Down", ButtonTone::Secondary).into()
            };
            let remove_btn: Element<'static, Message> = styled_button("Remove", ButtonTone::Danger)
                .on_press(Message::MontageRemove(*clip_id))
                .into();

            let actions = row![up_btn, down_btn, remove_btn]
                .spacing(8)
                .align_y(Alignment::Center);

            queue = queue.push(
                card()
                    .title(title)
                    .description(subtitle)
                    .body(badges)
                    .footer(actions)
                    .width(Length::Fill)
                    .build(),
            );
        }
        content = content.push(scrollable(queue).height(Length::Fixed(320.0)));
    }

    content = content.push(
        row![
            styled_button("Cancel", ButtonTone::Secondary).on_press(Message::CancelMontageModal),
            confirm_button,
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );

    content.width(Length::Fixed(720.0)).into()
}

fn delete_dialog(pending: &crate::app::PendingClipDelete) -> Element<'static, Message> {
    let (title, prompt, path_label, confirm_label) = match &pending.path {
        Some(path) => {
            let filename = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("this clip file");
            let size = pending
                .file_size_bytes
                .map(format_file_size)
                .unwrap_or_else(|| "unknown size".into());
            (
                "Delete clip file",
                format!("Delete {filename} ({size}) from disk and remove this clip row?"),
                Some(path.display().to_string()),
                "Delete file",
            )
        }
        None => (
            "Delete clip row",
            format!(
                "Remove clip #{} from history? No saved file attached.",
                pending.clip_id
            ),
            None,
            "Delete row",
        ),
    };

    let path_widget: Element<'static, Message> = match path_label {
        Some(path) => container(
            text(path)
                .size(11)
                .style(|theme: &iced::Theme| TextStyle {
                    color: Some(theme::tokens_for(theme).color.muted_foreground),
                })
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
        )
        .padding([6, 8])
        .width(Length::Fill)
        .style(|theme: &iced::Theme| {
            let c = &theme::tokens_for(theme).color;
            ContainerStyle {
                text_color: Some(c.muted_foreground),
                background: Some(Background::Color(c.muted)),
                border: theme::border(c.border, 1.0, theme::RADIUS.md),
                ..Default::default()
            }
        })
        .into(),
        None => text("The saved file is already missing.").size(12).into(),
    };

    column![
        text(title).size(20),
        text(prompt).size(14),
        path_widget,
        row![
            styled_button("Cancel", ButtonTone::Secondary).on_press(Message::DeleteCanceled),
            styled_button(confirm_label, ButtonTone::Danger).on_press(Message::DeleteConfirmed),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(12)
    .width(Length::Fixed(520.0))
    .into()
}

// ---------------------------------------------------------------------------
// Helpers: badges, filter options, sorting, labels
// ---------------------------------------------------------------------------

fn clips_badge(label: impl Into<String>, tone: BadgeTone) -> Element<'static, Message> {
    badge(label.into()).tone(tone).build().into()
}

fn active_filter_count(app: &App) -> usize {
    let filters = &app.clip_filters;

    usize::from(!filters.search.trim().is_empty())
        + usize::from(filters.event_after_ts.is_some() || filters.event_before_ts.is_some())
        + usize::from(!filters.target.trim().is_empty())
        + usize::from(!filters.weapon.trim().is_empty())
        + usize::from(!filters.alert.trim().is_empty())
        + usize::from(filters.overlap_state != OverlapFilterState::All)
        + usize::from(!filters.profile.trim().is_empty())
        + usize::from(!filters.rule.trim().is_empty())
        + usize::from(!filters.character.trim().is_empty())
        + usize::from(!filters.server.trim().is_empty())
        + usize::from(!filters.continent.trim().is_empty())
        + usize::from(!filters.base.trim().is_empty())
}

fn total_pages(app: &App) -> usize {
    let len = app.clip_history.len();
    let size = app.clip_history_page_size.max(1);
    if len == 0 { 1 } else { (len + size - 1) / size }
}

pub(in crate::app) fn rebuild_history(app: &mut App) {
    let mut filtered: Vec<ClipRecord> = app
        .clip_history_source
        .iter()
        .filter(|record| clip_matches_filters(app, record))
        .cloned()
        .collect();

    sort_clip_history(
        &mut filtered,
        app.clip_sort_column,
        app.clip_sort_descending,
        &app.config,
        &app.state,
    );

    app.clip_history = filtered;
    app.clip_history_viewport = None;

    // Clamp page to valid range after filtering.
    let pages = total_pages(app);
    if app.clip_history_page >= pages {
        app.clip_history_page = pages.saturating_sub(1);
    }
}

fn sort_clip_history(
    records: &mut [ClipRecord],
    column: ClipSortColumn,
    descending: bool,
    config: &crate::config::Config,
    state: &AppState,
) {
    records.sort_by(|a, b| {
        let order = match column {
            ClipSortColumn::When => a.trigger_event_at.cmp(&b.trigger_event_at),
            ClipSortColumn::Rule => rule_label_from(config, a).cmp(&rule_label_from(config, b)),
            ClipSortColumn::Character => {
                character_label_from(config, state, a).cmp(&character_label_from(config, state, b))
            }
            ClipSortColumn::Score => a.score.cmp(&b.score),
            ClipSortColumn::Duration => a.clip_duration_secs.cmp(&b.clip_duration_secs),
        };
        if descending { order.reverse() } else { order }
    });
}

fn location_summary(record: &ClipRecord) -> String {
    record
        .facility_name
        .clone()
        .or_else(|| census::base_name(record.facility_id))
        .unwrap_or_else(|| "Unknown".into())
}

fn build_filter_options(app: &App) -> ClipFilterOptions {
    let mut profiles = std::collections::BTreeSet::new();
    let mut rules = std::collections::BTreeSet::new();
    let mut characters = std::collections::BTreeSet::new();
    let mut servers = std::collections::BTreeSet::new();
    let mut continents = std::collections::BTreeSet::new();
    let mut bases = std::collections::BTreeSet::new();
    let mut targets = std::collections::BTreeSet::new();
    let mut weapons = std::collections::BTreeSet::new();
    let mut alerts = std::collections::BTreeSet::new();

    for profile in &app.config.rule_profiles {
        profiles.insert(profile.name.clone());
    }
    for rule in &app.config.rule_definitions {
        rules.insert(rule.name.clone());
    }
    for character in &app.config.characters {
        characters.insert(character.name.clone());
    }

    for record in &app.clip_history_source {
        profiles.insert(profile_label(app, record));
        rules.insert(rule_label(app, record));
        characters.insert(character_label(app, record));
        servers.insert(server_label(record));
        continents.insert(continent_label(record));
        let base = location_summary(record);
        if base != "Unknown" {
            bases.insert(base);
        }
    }
    for target in &app.clip_filter_options.targets {
        targets.insert(target.clone());
    }
    for weapon in &app.clip_filter_options.weapons {
        weapons.insert(weapon.clone());
    }
    for alert in &app.clip_filter_options.alerts {
        alerts.insert(alert.clone());
    }

    ClipFilterOptions {
        profiles: profiles
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect(),
        rules: rules
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect(),
        characters: characters
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect(),
        servers: servers
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect(),
        continents: continents
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect(),
        bases: bases.into_iter().collect(),
        targets: targets
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect(),
        weapons: weapons
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect(),
        alerts: alerts
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect(),
    }
}

fn clip_matches_filters(app: &App, record: &ClipRecord) -> bool {
    let profile = profile_label(app, record);
    let rule = rule_label(app, record);
    let character = character_label(app, record);
    let server = server_label(record);
    let continent = continent_label(record);
    let location = location_summary(record);

    exact_filter_matches(&app.clip_filters.profile, &profile)
        && exact_filter_matches(&app.clip_filters.rule, &rule)
        && exact_filter_matches(&app.clip_filters.character, &character)
        && exact_filter_matches(&app.clip_filters.server, &server)
        && exact_filter_matches(&app.clip_filters.continent, &continent)
        && exact_filter_matches(&app.clip_filters.base, &location)
        && quick_search_matches(
            &app.clip_filters.search,
            record,
            &profile,
            &rule,
            &character,
            &server,
            &continent,
            &location,
        )
}

fn exact_filter_matches(filter: &str, value: &str) -> bool {
    let filter = normalize(filter);
    filter.is_empty() || filter == normalize(value)
}

fn quick_search_matches(
    search: &str,
    record: &ClipRecord,
    profile: &str,
    rule: &str,
    character: &str,
    server: &str,
    continent: &str,
    location: &str,
) -> bool {
    let search = normalize(search);
    if search.is_empty() {
        return true;
    }

    let haystack = [
        profile.to_string(),
        rule.to_string(),
        character.to_string(),
        server.to_string(),
        continent.to_string(),
        location.to_string(),
        contribution_summary(record),
        record.profile_id.clone(),
        record.rule_id.clone(),
        record.character_id.to_string(),
        record.world_id.to_string(),
        record.zone_id.map(|id| id.to_string()).unwrap_or_default(),
        record
            .facility_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
    ]
    .join(" ");

    normalize(&haystack).contains(&search)
}

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

fn profile_label(app: &App, record: &ClipRecord) -> String {
    profile_label_from(&app.config, record)
}

fn profile_label_from(config: &crate::config::Config, record: &ClipRecord) -> String {
    config
        .rule_profiles
        .iter()
        .find(|profile| profile.id == record.profile_id)
        .map(|profile| profile.name.clone())
        .unwrap_or_else(|| record.profile_id.clone())
}

fn rule_label(app: &App, record: &ClipRecord) -> String {
    rule_label_from(&app.config, record)
}

fn rule_label_from(config: &crate::config::Config, record: &ClipRecord) -> String {
    if record.origin == crate::db::ClipOrigin::Manual {
        return "Manual clip".into();
    }
    if record.origin == crate::db::ClipOrigin::Imported {
        return "Imported clip".into();
    }

    config
        .rule_definitions
        .iter()
        .find(|rule| rule.id == record.rule_id)
        .map(|rule| rule.name.clone())
        .unwrap_or_else(|| record.rule_id.clone())
}

fn character_label(app: &App, record: &ClipRecord) -> String {
    character_label_from(&app.config, &app.state, record)
}

fn character_label_from(
    config: &crate::config::Config,
    state: &AppState,
    record: &ClipRecord,
) -> String {
    if record.character_id == 0 {
        return "Unassigned".into();
    }

    config
        .characters
        .iter()
        .find(|character| character.character_id == Some(record.character_id))
        .map(|character| character.name.clone())
        .or_else(|| match state {
            AppState::Monitoring {
                character_name,
                character_id,
            } if *character_id == record.character_id => Some(character_name.clone()),
            _ => None,
        })
        .unwrap_or_else(|| format!("Character {}", record.character_id))
}

fn server_label(record: &ClipRecord) -> String {
    if record.world_id == 0 {
        return "Unknown".into();
    }
    census::world_name(record.world_id)
}

fn continent_label(record: &ClipRecord) -> String {
    record
        .zone_name
        .clone()
        .unwrap_or_else(|| census::continent_name(record.zone_id))
}

fn duration_label(record: &ClipRecord) -> String {
    format!("{} seconds", record.clip_duration_secs)
}

fn overlap_label(record: &ClipRecord) -> String {
    match record.overlap_count {
        0 => "None".into(),
        1 => "1 clip".into(),
        count => format!("{count} clips"),
    }
}

fn alert_label(record: &ClipRecord) -> String {
    match record.alert_count {
        0 => "None".into(),
        1 => "1 alert".into(),
        count => format!("{count} alerts"),
    }
}

fn size_label(record: &ClipRecord) -> String {
    record
        .file_size_bytes
        .map(format_file_size)
        .unwrap_or_else(|| "\u{2013}".into())
}

fn post_process_status_label(record: &ClipRecord) -> String {
    match record.post_process_status {
        crate::db::PostProcessStatus::NotRequired => "Audio: not required".into(),
        crate::db::PostProcessStatus::Pending => "Audio: pending".into(),
        crate::db::PostProcessStatus::Completed => "Audio: ok".into(),
        crate::db::PostProcessStatus::Failed => record
            .post_process_error
            .as_ref()
            .filter(|message| !message.trim().is_empty())
            .map(|message| format!("Audio failed | {message}"))
            .unwrap_or_else(|| "Audio failed".into()),
        crate::db::PostProcessStatus::Legacy => "Audio: legacy layout".into(),
    }
}

fn contribution_summary(record: &ClipRecord) -> String {
    if record.events.is_empty() {
        return "No contributions".into();
    }

    record
        .events
        .iter()
        .map(|event| {
            format!(
                "{} x{} = {}",
                event.event_kind, event.occurrences, event.points
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn short_duration_label(record: &ClipRecord) -> String {
    format!("{}s", record.clip_duration_secs)
}

fn format_smart_timestamp(timestamp: chrono::DateTime<Utc>) -> String {
    let local = timestamp.with_timezone(&Local);
    let now = Local::now();
    if local.year() == now.year() {
        local.format("%m-%d %H:%M").to_string()
    } else {
        local.format("%Y-%m-%d %H:%M").to_string()
    }
}

fn timeline_line(record: &ClipRecord, event: &crate::db::ClipRawEventRecord) -> String {
    let offset = event
        .event_at
        .signed_duration_since(record.clip_start_at)
        .num_milliseconds()
        .max(0);
    let seconds = offset as f64 / 1000.0;
    let target = event
        .other_character_name
        .clone()
        .unwrap_or_else(|| format_optional_id("Character", event.other_character_id));
    let weapon = event
        .attacker_weapon_name
        .clone()
        .unwrap_or_else(|| format_optional_id("Weapon", event.attacker_weapon_id.map(u64::from)));

    let mut parts = vec![format!("+{seconds:.1}s"), event.event_kind.clone()];
    if event.is_headshot {
        parts.push("headshot".into());
    }
    if event.other_character_id.is_some() {
        parts.push(format!("target {target}"));
    }
    if event.attacker_weapon_id.is_some() {
        parts.push(format!("weapon {weapon}"));
    }
    parts.join("  |  ")
}

fn format_optional_id(prefix: &str, value: Option<u64>) -> String {
    value
        .map(|id| format!("{prefix} #{id}"))
        .unwrap_or_else(|| "\u{2013}".into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlapFilterChoice {
    All,
    Overlapping,
    UniqueOnly,
}

impl OverlapFilterChoice {
    const ALL: [Self; 3] = [Self::All, Self::Overlapping, Self::UniqueOnly];

    fn from_state(state: OverlapFilterState) -> Self {
        match state {
            OverlapFilterState::All => Self::All,
            OverlapFilterState::Overlapping => Self::Overlapping,
            OverlapFilterState::UniqueOnly => Self::UniqueOnly,
        }
    }

    fn into_state(self) -> OverlapFilterState {
        match self {
            Self::All => OverlapFilterState::All,
            Self::Overlapping => OverlapFilterState::Overlapping,
            Self::UniqueOnly => OverlapFilterState::UniqueOnly,
        }
    }
}

impl std::fmt::Display for OverlapFilterChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::All => "All clips",
            Self::Overlapping => "Only overlaps",
            Self::UniqueOnly => "Only unique",
        })
    }
}

fn filter_pick_list_options(all_label: &str, values: &[String]) -> Vec<String> {
    let mut options = Vec::with_capacity(values.len() + 1);
    options.push(all_label.to_string());
    options.extend(values.iter().cloned());
    options
}

fn selected_filter_option(current: &str, all_label: &str) -> String {
    if current.trim().is_empty() {
        all_label.to_string()
    } else {
        current.to_string()
    }
}

fn filter_value_from_selection(selection: String, all_label: &str) -> String {
    if selection == all_label {
        String::new()
    } else {
        selection
    }
}

pub(in crate::app) fn format_timestamp(timestamp: chrono::DateTime<Utc>) -> String {
    timestamp
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

fn format_file_size(size_bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

    let mut value = size_bytes as f64;
    let mut unit_index = 0usize;
    while value >= 1024.0 && unit_index < UNITS.len() - 1 {
        value /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{size_bytes} {}", UNITS[unit_index])
    } else {
        format!("{value:.1} {}", UNITS[unit_index])
    }
}

fn format_duration_ms(duration_ms: i64) -> String {
    if duration_ms <= 0 {
        return "0.0s".into();
    }
    format!("{:.1}s", duration_ms as f64 / 1000.0)
}

// ---------------------------------------------------------------------------
// Date parsing helpers
// ---------------------------------------------------------------------------

fn date_range_summary(app: &App) -> String {
    match app.clip_date_range_preset {
        DateRangePreset::AllTime => "All saved clips".into(),
        DateRangePreset::Custom => {
            let start = if app.clip_date_range_start.trim().is_empty() {
                "any start"
            } else {
                app.clip_date_range_start.trim()
            };
            let end = if app.clip_date_range_end.trim().is_empty() {
                "any end"
            } else {
                app.clip_date_range_end.trim()
            };
            format!("Custom: {start} \u{2192} {end}")
        }
        preset => format!("{preset}"),
    }
}

fn parse_range_input(value: &str, is_end: bool) -> Result<Option<chrono::DateTime<Utc>>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    if let Ok(datetime) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Ok(Some(datetime.with_timezone(&Utc)));
    }

    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, format) {
            return local_naive_to_utc(naive).map(Some);
        }
    }

    if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        let naive = if is_end {
            date.and_hms_milli_opt(23, 59, 59, 999)
        } else {
            date.and_hms_opt(0, 0, 0)
        }
        .ok_or_else(|| format!("Invalid date value: {trimmed}"))?;

        return local_naive_to_utc(naive).map(Some);
    }

    Err(format!(
        "Invalid date/time `{trimmed}`. Use YYYY-MM-DD, YYYY-MM-DD HH:MM, YYYY-MM-DD HH:MM:SS, or RFC3339."
    ))
}

fn calendar_seed_date(value: &str) -> Option<NaiveDate> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(datetime) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Some(datetime.with_timezone(&Local).date_naive());
    }

    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, format) {
            return Some(naive.date());
        }
    }

    NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").ok()
}

pub(in crate::app) fn today_local_date() -> NaiveDate {
    Local::now()
        .date_naive()
        .with_day(1)
        .unwrap_or_else(|| Local::now().date_naive())
}

fn merge_calendar_date(existing: &str, date: NaiveDate) -> String {
    let trimmed = existing.trim();
    let date_text = date.format("%Y-%m-%d").to_string();

    if trimmed.is_empty() {
        return date_text;
    }

    if let Ok(datetime) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        let time = datetime.time();
        let offset = datetime.offset().to_string();
        return format!(
            "{}T{:02}:{:02}:{:02}{}",
            date_text,
            time.hour(),
            time.minute(),
            time.second(),
            offset
        );
    }

    if let Some((_, time)) = trimmed.split_once(' ') {
        return format!("{date_text} {}", time.trim());
    }

    date_text
}

fn local_day_start(date: NaiveDate) -> Option<chrono::DateTime<Utc>> {
    local_naive_to_utc(date.and_hms_opt(0, 0, 0)?).ok()
}

fn local_naive_to_utc(naive: NaiveDateTime) -> Result<chrono::DateTime<Utc>, String> {
    Local
        .from_local_datetime(&naive)
        .single()
        .or_else(|| Local.from_local_datetime(&naive).earliest())
        .map(|value| value.with_timezone(&Utc))
        .ok_or_else(|| format!("Local date/time `{}` is ambiguous or invalid.", naive))
}

// ---------------------------------------------------------------------------
// Keyboard subscription router
// ---------------------------------------------------------------------------

pub(in crate::app) fn subscription_event_handler(
    event: iced::Event,
    status: iced::event::Status,
) -> Option<Message> {
    let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event else {
        return None;
    };

    use iced::keyboard::Key;
    use iced::keyboard::key::Named;

    let captured = matches!(status, iced::event::Status::Captured);

    let action = match (&key, captured) {
        (Key::Named(Named::ArrowDown), _) => KeyNav::SelectNext,
        (Key::Named(Named::ArrowUp), _) => KeyNav::SelectPrevious,
        (Key::Named(Named::PageDown), false) => KeyNav::NextPage,
        (Key::Named(Named::PageUp), false) => KeyNav::PreviousPage,
        (Key::Named(Named::Home), false) => KeyNav::First,
        (Key::Named(Named::End), false) => KeyNav::Last,
        (Key::Named(Named::Enter), false) => KeyNav::OpenSelected,
        (Key::Named(Named::Delete), false) => KeyNav::DeleteSelected,
        (Key::Named(Named::Escape), _) => KeyNav::Escape,
        (Key::Character(c), false) if c.as_str() == " " => KeyNav::ToggleMontageSelected,
        _ => return None,
    };

    Some(Message::KeyNav(action))
}

#[cfg(test)]
mod tests {
    use super::{HistoryViewportState, history_page_row_is_visible, history_scroll_ratio};

    #[test]
    fn history_scroll_ratio_returns_none_for_empty_pages() {
        assert_eq!(history_scroll_ratio(0, 0), None);
    }

    #[test]
    fn history_scroll_ratio_keeps_single_row_pages_at_top() {
        assert_eq!(history_scroll_ratio(0, 1), Some(0.0));
    }

    #[test]
    fn history_scroll_ratio_maps_first_middle_and_last_rows() {
        assert_eq!(history_scroll_ratio(0, 5), Some(0.0));
        assert_eq!(history_scroll_ratio(2, 5), Some(0.5));
        assert_eq!(history_scroll_ratio(4, 5), Some(1.0));
    }

    #[test]
    fn history_scroll_ratio_clamps_rows_past_the_end() {
        assert_eq!(history_scroll_ratio(9, 5), Some(1.0));
    }

    #[test]
    fn visible_history_row_does_not_require_scroll() {
        let viewport = HistoryViewportState {
            offset_y: 64.0,
            viewport_height: 96.0,
            content_height: 238.0,
        };

        assert!(history_page_row_is_visible(viewport, 2, 7));
    }

    #[test]
    fn row_above_viewport_requires_scroll() {
        let viewport = HistoryViewportState {
            offset_y: 96.0,
            viewport_height: 96.0,
            content_height: 238.0,
        };

        assert!(!history_page_row_is_visible(viewport, 1, 7));
    }

    #[test]
    fn row_below_viewport_requires_scroll() {
        let viewport = HistoryViewportState {
            offset_y: 0.0,
            viewport_height: 96.0,
            content_height: 238.0,
        };

        assert!(!history_page_row_is_visible(viewport, 4, 7));
    }

    #[test]
    fn non_scrollable_history_always_keeps_rows_visible() {
        let viewport = HistoryViewportState {
            offset_y: 0.0,
            viewport_height: 220.0,
            content_height: 180.0,
        };

        assert!(history_page_row_is_visible(viewport, 3, 5));
    }
}
