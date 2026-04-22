mod library;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration as StdDuration;

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc};
use iced::widget::{
    Row, Space, button, image, operation as widget_operation, scrollable as widget_scrollable,
};
use iced::{Alignment, Background, Color, ContentFit, Element, Length, Padding, Task};

use crate::census;
use crate::db::{
    ClipCollectionRecord, ClipDetailRecord, ClipFilterOptions, ClipRecord, ClipUploadState,
    OverlapFilterState, UploadProvider,
};
use crate::storage_tiering::{self, StorageTier};
use crate::ui::app::{
    Column, ContainerStyle, TextStyle, center, checkbox, column, container, mouse_area, pick_list,
    row, scrollable, text, text_input, text_non_selectable,
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
use crate::ui::primitives::switch::switch as toggle_switch;
use crate::ui::primitives::tag::tag;
use crate::ui::theme;
use library::*;

use super::super::shared::{ButtonTone, styled_button, with_tooltip};
use super::super::{App, AppState, Message as AppMessage, RuntimeMessage};

pub(in crate::app) use library::{
    format_timestamp, rebuild_history, subscription_event_handler, today_local_date,
};

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
pub const ALL_TAGS_LABEL: &str = "All tags";
pub const ALL_COLLECTIONS_LABEL: &str = "All collections";

pub const DEFAULT_PAGE_SIZE: usize = 50;
pub const PAGE_SIZE_OPTIONS: [usize; 4] = [25, 50, 100, 200];
const SEARCH_DEBOUNCE_MS: u64 = 180;
const CLIP_HISTORY_SCROLLABLE_ID: &str = "clips-history-scrollable";
const CLIP_HISTORY_ROW_SPACING: f32 = 2.0;
const GALLERY_CARD_WIDTH: f32 = 320.0;
const GALLERY_CARD_SPACING: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HistoryViewportState {
    offset_y: f32,
    viewport_width: f32,
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
            viewport_width: bounds.width,
            viewport_height: bounds.height,
            content_height: content_bounds.height,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    // Data loading
    Loaded(u64, Result<Vec<ClipRecord>, String>),
    ThumbnailHandleLoaded {
        path: String,
        result: Result<image::Handle, String>,
    },

    // Filters
    SearchChanged(String),
    SearchDebounceFired(u64),
    TargetFilterChanged(String),
    WeaponFilterChanged(String),
    AlertFilterChanged(String),
    TagFilterChanged(String),
    FavoritesFilterToggled(bool),
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
    BrowserModeToggled(bool),
    HistoryScrolled(HistoryViewportState),

    // Selection and row actions
    RowSelected(i64),
    OpenRequested(i64),
    ToggleFavorite(i64),
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
    TagInputChanged(String),
    AddTagRequested(i64),
    RemoveTagRequested {
        clip_id: i64,
        tag_name: String,
    },
    TagOptionSelected(String),
    CollectionAddSelectionChanged(CollectionSelectOption),
    DetailCollectionNameChanged(String),
    CollectionOptionSelected(CollectionSelectOption),
    AddClipToCollectionRequested(i64),
    RemoveClipFromCollectionRequested {
        clip_id: i64,
        collection_id: i64,
    },
    NewCollectionNameChanged(String),
    CreateCollectionRequested,
    SubmitCollectionMembershipRequested(i64),
    CollectionSelected(Option<i64>),
    FavoriteSelectedRequested,
    UnfavoriteSelectedRequested,
    AddSelectedToCurrentCollectionRequested,
    ToggleDetailSection(DetailSection),

    // Async results
    OrganizationSaved(Result<(), String>),
    CollectionCreated(Result<ClipCollectionRecord, String>),

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClipSortColumn {
    #[default]
    When,
    Rule,
    Character,
    Score,
    Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailSection {
    Organization,
    AudioTracks,
    Uploads,
    Alerts,
    Overlaps,
    RawEvents,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClipBrowserMode {
    #[default]
    List,
    Gallery,
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
    SubmitActiveOrganizationInput,
    Escape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrganizationEditor {
    Tag,
    Collection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingCollectionMembership {
    Clip(i64),
    SelectedClips,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionSelectOption {
    pub id: i64,
    pub label: String,
}

impl std::fmt::Display for CollectionSelectOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionFilterOption {
    pub id: Option<i64>,
    pub label: String,
}

impl std::fmt::Display for CollectionFilterOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
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

    app.clips.query_revision += 1;
    let revision = app.clips.query_revision;
    let filters = app.clips.filters.clone();

    Task::perform(
        async move { store.search_clips(&filters, 1_000).await },
        move |result| {
            AppMessage::Clips(Message::Loaded(revision, result.map_err(|e| e.to_string())))
        },
    )
}

pub(in crate::app) fn prime_thumbnail_cache_for_visible_items(app: &mut App) -> Task<AppMessage> {
    retain_thumbnail_handles_for_current_view(app);

    let mut wanted_paths = BTreeSet::new();

    for record in visible_history_slice(app) {
        if let Some(path) = existing_thumbnail_path(record) {
            wanted_paths.insert(path.to_string());
        }
    }

    if let Some(detail) = app.clips.selected_detail.as_ref()
        && let Some(path) = existing_thumbnail_path(&detail.clip)
    {
        wanted_paths.insert(path.to_string());
    }

    let tasks: Vec<Task<AppMessage>> = wanted_paths
        .into_iter()
        .filter_map(|path| queue_thumbnail_handle_load(app, path))
        .collect();

    Task::batch(tasks)
}

fn visible_history_slice(app: &App) -> &[ClipRecord] {
    let page_size = app.clips.history_page_size.max(1);
    let start = app.clips.history_page * page_size;
    let end = (start + page_size).min(app.clips.history.len());

    if start >= end {
        &[]
    } else {
        &app.clips.history[start..end]
    }
}

fn existing_thumbnail_path(record: &ClipRecord) -> Option<&str> {
    let path = record.thumbnail_path.as_deref()?;
    std::path::Path::new(path).exists().then_some(path)
}

fn retain_thumbnail_handles_for_current_view(app: &mut App) {
    let mut referenced_paths = BTreeSet::new();

    for record in visible_history_slice(app) {
        if let Some(path) = existing_thumbnail_path(record) {
            referenced_paths.insert(path.to_string());
        }
    }

    if let Some(detail) = app.clips.selected_detail.as_ref()
        && let Some(path) = existing_thumbnail_path(&detail.clip)
    {
        referenced_paths.insert(path.to_string());
    }

    app.clips
        .thumbnail_handles
        .retain(|path, _| referenced_paths.contains(path));
    app.clips
        .thumbnail_loads_in_flight
        .retain(|path| referenced_paths.contains(path));
}

fn queue_thumbnail_handle_load(app: &mut App, path: String) -> Option<Task<AppMessage>> {
    if app.clips.thumbnail_handles.contains_key(&path)
        || app.clips.thumbnail_loads_in_flight.contains(&path)
    {
        return None;
    }

    app.clips.thumbnail_loads_in_flight.insert(path.clone());

    Some(Task::perform(
        {
            let path = path.clone();
            async move {
                tokio::task::spawn_blocking(move || {
                    let bytes = std::fs::read(&path)
                        .map_err(|error| format!("failed to read thumbnail bytes: {error}"))?;
                    let decoded = ::image::load_from_memory(&bytes)
                        .map_err(|error| format!("failed to decode thumbnail image: {error}"))?
                        .into_rgba8();
                    let (width, height) = decoded.dimensions();
                    Ok(image::Handle::from_rgba(width, height, decoded.into_raw()))
                })
                .await
                .map_err(|error| format!("thumbnail decode task failed: {error}"))?
            }
        },
        move |result| {
            AppMessage::Clips(Message::ThumbnailHandleLoaded {
                path: path.clone(),
                result,
            })
        },
    ))
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

pub(in crate::app) fn update(app: &mut App, message: Message) -> Task<AppMessage> {
    match message {
        Message::Loaded(revision, result) => {
            if revision != app.clips.query_revision {
                return Task::none();
            }
            match result {
                Ok(clips) => {
                    app.clips.history_source = clips;
                    rebuild_history(app);
                    app.clear_clip_error();
                    let lookup_clips = app.clips.history_source.clone();
                    return Task::batch([
                        app.schedule_clip_record_lookup_resolutions(&lookup_clips),
                        prime_thumbnail_cache_for_visible_items(app),
                    ]);
                }
                Err(error) => {
                    app.set_clip_error(error.clone());
                    tracing::error!("Failed to load clip history: {error}");
                }
            }
            Task::none()
        }
        Message::ThumbnailHandleLoaded { path, result } => {
            app.clips.thumbnail_loads_in_flight.remove(&path);
            match result {
                Ok(handle) => {
                    app.clips.thumbnail_handles.insert(path, handle);
                }
                Err(error) => {
                    app.clips.thumbnail_handles.remove(&path);
                    tracing::warn!("Failed to preload thumbnail {path}: {error}");
                }
            }
            Task::none()
        }

        Message::SearchChanged(value) => {
            app.clips.filters.search = value;
            app.clips.search_revision = app.clips.search_revision.wrapping_add(1);
            let revision = app.clips.search_revision;
            Task::perform(
                async move {
                    tokio::time::sleep(StdDuration::from_millis(SEARCH_DEBOUNCE_MS)).await;
                    revision
                },
                |revision| AppMessage::Clips(Message::SearchDebounceFired(revision)),
            )
        }
        Message::SearchDebounceFired(revision) => {
            if revision == app.clips.search_revision {
                rebuild_history(app);
                return prime_thumbnail_cache_for_visible_items(app);
            }
            Task::none()
        }

        Message::TargetFilterChanged(value) => {
            app.clips.filters.target = filter_value_from_selection(value, ALL_TARGETS_LABEL);
            refresh_history(app)
        }
        Message::WeaponFilterChanged(value) => {
            app.clips.filters.weapon = filter_value_from_selection(value, ALL_WEAPONS_LABEL);
            refresh_history(app)
        }
        Message::AlertFilterChanged(value) => {
            app.clips.filters.alert = filter_value_from_selection(value, ALL_ALERTS_LABEL);
            refresh_history(app)
        }
        Message::TagFilterChanged(value) => {
            app.clips.filters.tag = filter_value_from_selection(value, ALL_TAGS_LABEL);
            refresh_history(app)
        }
        Message::FavoritesFilterToggled(value) => {
            app.clips.filters.favorites_only = value;
            refresh_history(app)
        }
        Message::OverlapFilterChanged(value) => {
            app.clips.filters.overlap_state = value.into_state();
            refresh_history(app)
        }

        Message::ProfileFilterChanged(value) => {
            app.clips.filters.profile = filter_value_from_selection(value, ALL_PROFILES_LABEL);
            rebuild_history(app);
            prime_thumbnail_cache_for_visible_items(app)
        }
        Message::RuleFilterChanged(value) => {
            app.clips.filters.rule = filter_value_from_selection(value, ALL_RULES_LABEL);
            rebuild_history(app);
            prime_thumbnail_cache_for_visible_items(app)
        }
        Message::CharacterFilterChanged(value) => {
            app.clips.filters.character = filter_value_from_selection(value, ALL_CHARACTERS_LABEL);
            rebuild_history(app);
            prime_thumbnail_cache_for_visible_items(app)
        }
        Message::ServerFilterChanged(value) => {
            app.clips.filters.server = filter_value_from_selection(value, ALL_SERVERS_LABEL);
            rebuild_history(app);
            prime_thumbnail_cache_for_visible_items(app)
        }
        Message::ContinentFilterChanged(value) => {
            app.clips.filters.continent = filter_value_from_selection(value, ALL_CONTINENTS_LABEL);
            rebuild_history(app);
            prime_thumbnail_cache_for_visible_items(app)
        }
        Message::BaseFilterChanged(value) => {
            app.clips.filters.base = filter_value_from_selection(value, ALL_BASES_LABEL);
            rebuild_history(app);
            prime_thumbnail_cache_for_visible_items(app)
        }
        Message::CollectionSelected(collection_id) => {
            app.clips.filters.collection_id = collection_id;
            if let Some(collection_id) = collection_id {
                app.clips.selected_collection_add_id = Some(collection_id);
            }
            refresh_history(app)
        }

        Message::ClearFilters => {
            app.clips.filters = crate::db::ClipFilters::default();
            app.clips.date_range_preset = DateRangePreset::AllTime;
            app.clips.date_range_start.clear();
            app.clips.date_range_end.clear();
            app.clips.active_calendar = None;
            app.clear_clip_filter_feedback();
            refresh_history(app)
        }
        Message::ToggleAdvancedFilters => {
            app.clips.advanced_filters_open = !app.clips.advanced_filters_open;
            Task::none()
        }

        Message::DateRangePresetChanged(preset) => set_date_range_preset(app, preset),
        Message::DateRangeStartChanged(value) => {
            app.clips.date_range_start = value;
            app.clear_clip_filter_feedback();
            Task::none()
        }
        Message::DateRangeEndChanged(value) => {
            app.clips.date_range_end = value;
            app.clear_clip_filter_feedback();
            Task::none()
        }
        Message::ApplyDateRange => apply_date_range(app),
        Message::ToggleCalendar(field) => {
            toggle_calendar(app, field);
            Task::none()
        }
        Message::DismissCalendar => {
            app.clips.active_calendar = None;
            Task::none()
        }
        Message::CalendarMonthChanged(month) => {
            app.clips.calendar_month = month.with_day(1).unwrap_or(month);
            Task::none()
        }
        Message::CalendarDaySelected(date) => {
            select_calendar_day(app, date);
            Task::none()
        }

        Message::SortColumnClicked(column) => {
            if app.clips.sort_column == column {
                app.clips.sort_descending = !app.clips.sort_descending;
            } else {
                app.clips.sort_column = column;
                app.clips.sort_descending = matches!(
                    column,
                    ClipSortColumn::When | ClipSortColumn::Score | ClipSortColumn::Duration
                );
            }
            rebuild_history(app);
            prime_thumbnail_cache_for_visible_items(app)
        }
        Message::PageChanged(page) => {
            let total_pages = total_pages(app);
            app.clips.history_page = page.saturating_sub(1).min(total_pages.saturating_sub(1));
            app.clips.history_viewport = None;
            Task::batch([
                scroll_history_to_top(),
                prime_thumbnail_cache_for_visible_items(app),
            ])
        }
        Message::PageSizeChanged(size) => {
            app.clips.history_page_size = size.max(1);
            app.clips.history_page = 0;
            app.clips.history_viewport = None;
            Task::batch([
                scroll_history_to_top(),
                prime_thumbnail_cache_for_visible_items(app),
            ])
        }
        Message::BrowserModeToggled(is_gallery) => {
            app.clips.browser_mode = if is_gallery {
                ClipBrowserMode::Gallery
            } else {
                ClipBrowserMode::List
            };
            app.clips.history_viewport = None;
            let scroll_task = app
                .clips
                .selected_id
                .and_then(|clip_id| {
                    app.clips
                        .history
                        .iter()
                        .position(|record| record.id == clip_id)
                })
                .map(|index| scroll_history_to_index(app, index))
                .unwrap_or_else(Task::none);
            Task::batch([prime_thumbnail_cache_for_visible_items(app), scroll_task])
        }
        Message::HistoryScrolled(viewport) => {
            app.clips.history_viewport = Some(viewport);
            Task::none()
        }

        Message::RowSelected(clip_id) => {
            app.clear_clip_error();
            app.load_clip_detail(Some(clip_id))
        }
        Message::OpenRequested(clip_id) => {
            let Some(record) = app.clips.history.iter().find(|record| record.id == clip_id) else {
                app.set_clip_error(format!("Clip #{clip_id} is no longer available."));
                return Task::none();
            };
            let Some(path) = record.path.clone() else {
                app.set_clip_error("File not saved yet.");
                return Task::none();
            };
            app.clear_clip_error();
            Task::done(AppMessage::OpenClipRequested(PathBuf::from(path)))
        }
        Message::ToggleFavorite(clip_id) => {
            let Some(record) = app
                .clips
                .history_source
                .iter()
                .find(|record| record.id == clip_id)
            else {
                app.set_clip_error(format!("Clip #{clip_id} is no longer available."));
                return Task::none();
            };
            let Some(store) = app.clip_store.clone() else {
                return Task::none();
            };
            let favorited = !record.favorited;
            Task::perform(
                async move { store.set_clip_favorited(clip_id, favorited).await },
                |result| {
                    AppMessage::Clips(Message::OrganizationSaved(
                        result.map_err(|e| e.to_string()),
                    ))
                },
            )
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
                .clips
                .history_source
                .iter()
                .find(|record| record.id == clip_id)
            else {
                app.set_clip_error(format!("Clip #{clip_id} is no longer available."));
                return Task::none();
            };
            let Some(path) = record.path.clone() else {
                app.set_clip_error("No file to reprocess.");
                return Task::none();
            };
            app.queue_post_process_retry_for_clip(clip_id, PathBuf::from(path))
        }
        Message::UseOriginalAudioRequested(clip_id) => app.use_original_clip_audio(clip_id),
        Message::OpenUploadUrl(url) => {
            let platform = app.platform.clone();
            Task::perform(async move { platform.open_url(&url) }, |result| {
                if let Err(error) = result {
                    tracing::warn!("Failed to open uploaded clip URL: {error}");
                }
                AppMessage::runtime(RuntimeMessage::Tick)
            })
        }
        Message::OpenHonuSession(session_id) => {
            let url = format!("https://wt.honu.pw/s/{session_id}");
            let platform = app.platform.clone();
            Task::perform(async move { platform.open_url(&url) }, |result| {
                if let Err(error) = result {
                    tracing::warn!("Failed to open Honu session URL: {error}");
                }
                AppMessage::runtime(RuntimeMessage::Tick)
            })
        }

        Message::ToggleMontageSelection(clip_id) => {
            if let Some(record) = app
                .clips
                .history_source
                .iter()
                .find(|record| record.id == clip_id)
                && let Some(reason) = super::super::clip_post_process_block_reason(record)
            {
                app.set_clip_error(reason);
                return Task::none();
            }
            if let Some(index) = app
                .clips
                .montage_selection
                .iter()
                .position(|id| *id == clip_id)
            {
                app.clips.montage_selection.remove(index);
                if app.clips.selected_montage_clip_id == Some(clip_id) {
                    app.clips.selected_montage_clip_id =
                        app.clips.montage_selection.first().copied();
                }
            } else {
                app.clips.montage_selection.push(clip_id);
                app.clips.selected_montage_clip_id = Some(clip_id);
            }
            Task::none()
        }
        Message::MontageMoveUp(clip_id) => {
            if let Some(index) = app
                .clips
                .montage_selection
                .iter()
                .position(|id| *id == clip_id)
                && index > 0
            {
                app.clips.montage_selection.swap(index, index - 1);
            }
            Task::none()
        }
        Message::MontageMoveDown(clip_id) => {
            if let Some(index) = app
                .clips
                .montage_selection
                .iter()
                .position(|id| *id == clip_id)
                && index + 1 < app.clips.montage_selection.len()
            {
                app.clips.montage_selection.swap(index, index + 1);
            }
            Task::none()
        }
        Message::MontageRemove(clip_id) => {
            app.clips.montage_selection.retain(|id| *id != clip_id);
            if app.clips.selected_montage_clip_id == Some(clip_id) {
                app.clips.selected_montage_clip_id = app.clips.montage_selection.first().copied();
            }
            Task::none()
        }
        Message::ClearMontageSelection => {
            app.clips.montage_selection.clear();
            app.clips.selected_montage_clip_id = None;
            app.clips.montage_modal_open = false;
            Task::none()
        }
        Message::CreateMontage => {
            if app.clips.montage_selection.len() < 2 {
                app.set_clip_error("Choose at least two clips for a montage.");
                return Task::none();
            }

            app.clips.montage_modal_open = true;
            app.clips.active_calendar = None;
            app.clear_clip_error();
            Task::none()
        }
        Message::CancelMontageModal => {
            app.clips.montage_modal_open = false;
            Task::none()
        }
        Message::ConfirmMontageCreation => {
            if app.clips.montage_selection.len() < 2 {
                app.set_clip_error("Choose at least two clips for a montage.");
                return Task::none();
            }

            app.clips.montage_modal_open = false;
            Task::done(AppMessage::CreateMontageRequested)
        }

        Message::DeleteRequested(clip_id) => {
            let Some(record) = app.clips.history.iter().find(|record| record.id == clip_id) else {
                app.set_clip_error(format!("Clip #{clip_id} is no longer available."));
                return Task::none();
            };

            app.clips.pending_delete = Some(crate::app::PendingClipDelete {
                clip_id,
                path: record.path.as_ref().map(PathBuf::from),
                file_size_bytes: record.file_size_bytes,
            });
            app.clips.active_calendar = None;
            app.clips.montage_modal_open = false;
            app.clear_clip_error();
            Task::none()
        }
        Message::DeleteCanceled => {
            app.clips.pending_delete = None;
            Task::none()
        }
        Message::DeleteConfirmed => {
            let Some(pending) = app.clips.pending_delete.take() else {
                return Task::none();
            };

            Task::done(AppMessage::DeleteClipRequested {
                clip_id: pending.clip_id,
                path: pending.path,
            })
        }

        Message::RawEventFilterChanged(value) => {
            app.clips.raw_event_filter = value;
            Task::none()
        }
        Message::TagInputChanged(value) => {
            app.clips.active_organization_editor = Some(OrganizationEditor::Tag);
            app.clips.tag_input = value;
            Task::none()
        }
        Message::TagOptionSelected(tag_name) => {
            app.clips.active_organization_editor = Some(OrganizationEditor::Tag);
            app.clips.tag_input = tag_name;
            let Some(clip_id) = app.clips.selected_id else {
                return Task::none();
            };
            update(app, Message::AddTagRequested(clip_id))
        }
        Message::AddTagRequested(clip_id) => {
            let tag_name = app.clips.tag_input.trim().to_string();
            if tag_name.is_empty() {
                app.set_clip_error("Enter a tag name first.");
                return Task::none();
            }
            let Some(store) = app.clip_store.clone() else {
                return Task::none();
            };
            app.clips.pending_organization_input_clear = Some(OrganizationEditor::Tag);
            Task::perform(
                async move { store.add_tag(clip_id, &tag_name).await },
                |result| {
                    AppMessage::Clips(Message::OrganizationSaved(
                        result.map_err(|e| e.to_string()),
                    ))
                },
            )
        }
        Message::RemoveTagRequested { clip_id, tag_name } => {
            let Some(store) = app.clip_store.clone() else {
                return Task::none();
            };
            Task::perform(
                async move { store.remove_tag(clip_id, &tag_name).await },
                |result| {
                    AppMessage::Clips(Message::OrganizationSaved(
                        result.map_err(|e| e.to_string()),
                    ))
                },
            )
        }
        Message::CollectionAddSelectionChanged(option) => {
            app.clips.selected_collection_add_id = Some(option.id);
            app.clips.new_collection_name = option.label;
            app.clips.active_organization_editor = None;
            Task::none()
        }
        Message::DetailCollectionNameChanged(value) => {
            app.clips.active_organization_editor = Some(OrganizationEditor::Collection);
            app.clips.new_collection_name = value;
            let trimmed = app.clips.new_collection_name.trim();
            if let Some(collection) = app
                .clips
                .filter_options
                .collections
                .iter()
                .find(|collection| collection.name.eq_ignore_ascii_case(trimmed))
            {
                app.clips.selected_collection_add_id = Some(collection.id);
            } else if !trimmed.is_empty() {
                app.clips.selected_collection_add_id = None;
            }
            Task::none()
        }
        Message::CollectionOptionSelected(option) => {
            app.clips.active_organization_editor = Some(OrganizationEditor::Collection);
            app.clips.selected_collection_add_id = Some(option.id);
            app.clips.new_collection_name = option.label;
            let Some(clip_id) = app.clips.selected_id else {
                return Task::none();
            };
            update(app, Message::AddClipToCollectionRequested(clip_id))
        }
        Message::AddClipToCollectionRequested(clip_id) => {
            let Some(collection_id) = app.clips.selected_collection_add_id else {
                app.set_clip_error("Choose a collection first.");
                return Task::none();
            };
            let Some(store) = app.clip_store.clone() else {
                return Task::none();
            };
            if app.clips.active_organization_editor == Some(OrganizationEditor::Collection) {
                app.clips.pending_organization_input_clear = Some(OrganizationEditor::Collection);
            }
            Task::perform(
                async move { store.add_clip_to_collection(collection_id, clip_id).await },
                |result| {
                    AppMessage::Clips(Message::OrganizationSaved(
                        result.map_err(|e| e.to_string()),
                    ))
                },
            )
        }
        Message::SubmitCollectionMembershipRequested(clip_id) => {
            let name = app.clips.new_collection_name.trim().to_string();
            if name.is_empty() {
                app.set_clip_error("Enter a collection name first.");
                return Task::none();
            }

            if let Some((collection_id, collection_name)) = find_collection_by_name(app, &name)
                .map(|collection| (collection.id, collection.name.clone()))
            {
                app.clips.selected_collection_add_id = Some(collection_id);
                app.clips.new_collection_name = collection_name;
                return update(app, Message::AddClipToCollectionRequested(clip_id));
            }

            app.clips.pending_collection_membership =
                Some(PendingCollectionMembership::Clip(clip_id));
            update(app, Message::CreateCollectionRequested)
        }
        Message::RemoveClipFromCollectionRequested {
            clip_id,
            collection_id,
        } => {
            let Some(store) = app.clip_store.clone() else {
                return Task::none();
            };
            Task::perform(
                async move {
                    store
                        .remove_clip_from_collection(collection_id, clip_id)
                        .await
                },
                |result| {
                    AppMessage::Clips(Message::OrganizationSaved(
                        result.map_err(|e| e.to_string()),
                    ))
                },
            )
        }
        Message::NewCollectionNameChanged(value) => {
            app.clips.active_organization_editor = None;
            app.clips.new_collection_name = value;
            let trimmed = app.clips.new_collection_name.trim();
            if let Some(collection) = app
                .clips
                .filter_options
                .collections
                .iter()
                .find(|collection| collection.name.eq_ignore_ascii_case(trimmed))
            {
                app.clips.selected_collection_add_id = Some(collection.id);
            } else if !trimmed.is_empty() {
                app.clips.selected_collection_add_id = None;
            }
            Task::none()
        }
        Message::CreateCollectionRequested => {
            let name = app.clips.new_collection_name.trim().to_string();
            if name.is_empty() {
                app.set_clip_error("Enter a collection name first.");
                return Task::none();
            }
            if let Some(collection) = app
                .clips
                .filter_options
                .collections
                .iter()
                .find(|collection| collection.name.eq_ignore_ascii_case(&name))
            {
                app.clips.selected_collection_add_id = Some(collection.id);
                app.clips.new_collection_name = collection.name.clone();
                app.clear_clip_error();
                return Task::none();
            }
            let description = app.clips.new_collection_description.trim().to_string();
            let Some(store) = app.clip_store.clone() else {
                return Task::none();
            };
            Task::perform(
                async move {
                    store
                        .create_collection(
                            &name,
                            if description.is_empty() {
                                None
                            } else {
                                Some(description.as_str())
                            },
                        )
                        .await
                },
                |result| {
                    AppMessage::Clips(Message::CollectionCreated(
                        result.map_err(|e| e.to_string()),
                    ))
                },
            )
        }
        Message::FavoriteSelectedRequested => {
            let clip_ids = app.clips.montage_selection.clone();
            if clip_ids.is_empty() {
                app.set_clip_error("Select one or more clips first.");
                return Task::none();
            }
            let Some(store) = app.clip_store.clone() else {
                return Task::none();
            };
            Task::perform(
                async move { store.set_clips_favorited(&clip_ids, true).await },
                |result| {
                    AppMessage::Clips(Message::OrganizationSaved(
                        result.map_err(|e| e.to_string()),
                    ))
                },
            )
        }
        Message::UnfavoriteSelectedRequested => {
            let clip_ids = app.clips.montage_selection.clone();
            if clip_ids.is_empty() {
                app.set_clip_error("Select one or more clips first.");
                return Task::none();
            }
            let Some(store) = app.clip_store.clone() else {
                return Task::none();
            };
            Task::perform(
                async move { store.set_clips_favorited(&clip_ids, false).await },
                |result| {
                    AppMessage::Clips(Message::OrganizationSaved(
                        result.map_err(|e| e.to_string()),
                    ))
                },
            )
        }
        Message::AddSelectedToCurrentCollectionRequested => {
            let clip_ids = app.clips.montage_selection.clone();
            if clip_ids.is_empty() {
                app.set_clip_error("Select one or more clips first.");
                return Task::none();
            }
            let collection_id = if let Some(collection_id) = app.clips.selected_collection_add_id {
                collection_id
            } else {
                let name = app.clips.new_collection_name.trim().to_string();
                if name.is_empty() {
                    app.set_clip_error("Choose or type a collection first.");
                    return Task::none();
                }
                if let Some((collection_id, collection_name)) = find_collection_by_name(app, &name)
                    .map(|collection| (collection.id, collection.name.clone()))
                {
                    app.clips.selected_collection_add_id = Some(collection_id);
                    app.clips.new_collection_name = collection_name;
                    collection_id
                } else {
                    app.clips.pending_collection_membership =
                        Some(PendingCollectionMembership::SelectedClips);
                    return update(app, Message::CreateCollectionRequested);
                }
            };
            let Some(store) = app.clip_store.clone() else {
                return Task::none();
            };
            Task::perform(
                async move {
                    store
                        .add_clips_to_collection(collection_id, &clip_ids)
                        .await
                },
                |result| {
                    AppMessage::Clips(Message::OrganizationSaved(
                        result.map_err(|e| e.to_string()),
                    ))
                },
            )
        }
        Message::OrganizationSaved(result) => match result {
            Ok(()) => {
                match app.clips.pending_organization_input_clear.take() {
                    Some(OrganizationEditor::Tag) => app.clips.tag_input.clear(),
                    Some(OrganizationEditor::Collection) => app.clips.new_collection_name.clear(),
                    None => {}
                }
                refresh_clip_organization(app)
            }
            Err(error) => {
                app.clips.pending_organization_input_clear = None;
                app.set_clip_error(error);
                Task::none()
            }
        },
        Message::CollectionCreated(result) => match result {
            Ok(collection) => {
                app.clips.new_collection_name.clear();
                app.clips.new_collection_description.clear();
                app.clips.selected_collection_add_id = Some(collection.id);
                match app.clips.pending_collection_membership.take() {
                    Some(PendingCollectionMembership::Clip(clip_id)) => {
                        update(app, Message::AddClipToCollectionRequested(clip_id))
                    }
                    Some(PendingCollectionMembership::SelectedClips) => {
                        update(app, Message::AddSelectedToCurrentCollectionRequested)
                    }
                    None => refresh_clip_organization(app),
                }
            }
            Err(error) => {
                app.clips.pending_collection_membership = None;
                app.set_clip_error(error);
                Task::none()
            }
        },
        Message::ToggleDetailSection(section) => {
            if app.clips.collapsed_detail_sections.contains(&section) {
                app.clips
                    .collapsed_detail_sections
                    .retain(|s| *s != section);
            } else {
                app.clips.collapsed_detail_sections.push(section);
            }
            Task::none()
        }

        Message::KeyNav(action) => handle_key_nav(app, action),
    }
}

fn handle_key_nav(app: &mut App, action: KeyNav) -> Task<AppMessage> {
    match action {
        KeyNav::Escape => {
            if app.clips.pending_delete.is_some() {
                app.clips.pending_delete = None;
            } else if app.clips.montage_modal_open {
                app.clips.montage_modal_open = false;
            } else if app.clips.active_calendar.is_some() {
                app.clips.active_calendar = None;
            } else if app.clips.selected_id.is_some() {
                return app.load_clip_detail(None);
            }
            Task::none()
        }
        KeyNav::SelectNext => move_selection(app, 1),
        KeyNav::SelectPrevious => move_selection(app, -1),
        KeyNav::First => select_at_offset(app, 0, true),
        KeyNav::Last => {
            let last = app.clips.history.len().saturating_sub(1);
            select_at_offset(app, last as isize, true)
        }
        KeyNav::NextPage => {
            let total = total_pages(app);
            if app.clips.history_page + 1 < total {
                app.clips.history_page += 1;
            }
            scroll_history_to_top()
        }
        KeyNav::PreviousPage => {
            if app.clips.history_page > 0 {
                app.clips.history_page -= 1;
            }
            scroll_history_to_top()
        }
        KeyNav::OpenSelected => {
            let Some(clip_id) = app.clips.selected_id else {
                return Task::none();
            };
            update(app, Message::OpenRequested(clip_id))
        }
        KeyNav::DeleteSelected => {
            let Some(clip_id) = app.clips.selected_id else {
                return Task::none();
            };
            update(app, Message::DeleteRequested(clip_id))
        }
        KeyNav::ToggleMontageSelected => {
            let Some(clip_id) = app.clips.selected_id else {
                return Task::none();
            };
            update(app, Message::ToggleMontageSelection(clip_id))
        }
        KeyNav::SubmitActiveOrganizationInput => {
            let Some(clip_id) = app.clips.selected_id else {
                return Task::none();
            };
            match app.clips.active_organization_editor {
                Some(OrganizationEditor::Tag) => update(app, Message::AddTagRequested(clip_id)),
                Some(OrganizationEditor::Collection) => {
                    update(app, Message::SubmitCollectionMembershipRequested(clip_id))
                }
                None => Task::none(),
            }
        }
    }
}

fn move_selection(app: &mut App, delta: isize) -> Task<AppMessage> {
    if app.clips.history.is_empty() {
        return Task::none();
    }
    let current_index = app
        .clips
        .selected_id
        .and_then(|id| app.clips.history.iter().position(|r| r.id == id));

    let target = match current_index {
        Some(idx) => (idx as isize + delta)
            .max(0)
            .min(app.clips.history.len() as isize - 1),
        None => {
            if delta >= 0 {
                0
            } else {
                app.clips.history.len() as isize - 1
            }
        }
    };

    select_at_offset(app, target, false)
}

fn select_at_offset(app: &mut App, index: isize, force: bool) -> Task<AppMessage> {
    if app.clips.history.is_empty() || index < 0 {
        return Task::none();
    }
    let index = index as usize;
    if index >= app.clips.history.len() {
        return Task::none();
    }

    let page_size = app.clips.history_page_size.max(1);
    let target_page = index / page_size;
    let page_changed = target_page != app.clips.history_page;
    if page_changed {
        app.clips.history_page = target_page;
        app.clips.history_viewport = None;
    }

    let clip_id = app.clips.history[index].id;
    let scroll_task = if force || page_changed || !history_row_is_visible(app, index) {
        scroll_history_to_index(app, index)
    } else {
        Task::none()
    };

    if force || app.clips.selected_id != Some(clip_id) {
        Task::batch([app.load_clip_detail(Some(clip_id)), scroll_task])
    } else {
        scroll_task
    }
}

fn scroll_history_to_index(app: &App, index: usize) -> Task<AppMessage> {
    if app.clips.browser_mode == ClipBrowserMode::Gallery {
        return scroll_gallery_to_index(app, index);
    }

    let page_size = app.clips.history_page_size.max(1);
    let page_start = app.clips.history_page * page_size;
    let page_end = (page_start + page_size).min(app.clips.history.len());
    let page_row_count = page_end.saturating_sub(page_start);
    let page_row_index = index.saturating_sub(page_start);

    scroll_history_to_row(page_row_index, page_row_count)
}

fn scroll_gallery_to_index(app: &App, index: usize) -> Task<AppMessage> {
    let page_size = app.clips.history_page_size.max(1);
    let page_start = app.clips.history_page * page_size;
    let page_end = (page_start + page_size).min(app.clips.history.len());
    let page_item_count = page_end.saturating_sub(page_start);
    if page_item_count == 0 {
        return Task::none();
    }

    let page_item_index = index.saturating_sub(page_start);
    let column_count = gallery_column_count(
        app.clips
            .history_viewport
            .map(|viewport| viewport.viewport_width)
            .unwrap_or(0.0),
    );
    let row_count = page_item_count.div_ceil(column_count);
    let row_index = page_item_index / column_count;

    scroll_history_to_row(row_index, row_count)
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
    let Some(viewport) = app.clips.history_viewport else {
        return false;
    };

    let page_size = app.clips.history_page_size.max(1);
    let page_start = app.clips.history_page * page_size;
    let page_end = (page_start + page_size).min(app.clips.history.len());
    let page_row_count = page_end.saturating_sub(page_start);
    if page_row_count == 0 || index < page_start || index >= page_end {
        return false;
    }

    if app.clips.browser_mode == ClipBrowserMode::Gallery {
        gallery_page_item_is_visible(viewport, index - page_start, page_row_count)
    } else {
        history_page_row_is_visible(viewport, index - page_start, page_row_count)
    }
}

fn history_page_row_is_visible(
    viewport: HistoryViewportState,
    row_index: usize,
    row_count: usize,
) -> bool {
    page_row_is_visible(viewport, row_index, row_count, CLIP_HISTORY_ROW_SPACING)
}

fn gallery_page_item_is_visible(
    viewport: HistoryViewportState,
    item_index: usize,
    item_count: usize,
) -> bool {
    let column_count = gallery_column_count(viewport.viewport_width);
    let row_count = item_count.div_ceil(column_count);
    let row_index = item_index / column_count;

    page_row_is_visible(viewport, row_index, row_count, GALLERY_CARD_SPACING)
}

fn gallery_column_count(viewport_width: f32) -> usize {
    if viewport_width <= 0.0 {
        return 3;
    }

    ((viewport_width + GALLERY_CARD_SPACING) / (GALLERY_CARD_WIDTH + GALLERY_CARD_SPACING))
        .floor()
        .max(1.0) as usize
}

fn page_row_is_visible(
    viewport: HistoryViewportState,
    row_index: usize,
    row_count: usize,
    row_spacing: f32,
) -> bool {
    if row_count == 0 {
        return false;
    }

    if viewport.content_height <= viewport.viewport_height {
        return true;
    }

    let spacing_total = row_spacing * row_count.saturating_sub(1) as f32;
    let row_height = (viewport.content_height - spacing_total) / row_count as f32;
    if row_height <= 0.0 {
        return false;
    }

    let row_pitch = row_height + row_spacing;
    let row_top = row_index.min(row_count.saturating_sub(1)) as f32 * row_pitch;
    let row_bottom = row_top + row_height;
    let visible_top = viewport.offset_y;
    let visible_bottom = viewport.offset_y + viewport.viewport_height;
    let epsilon = 1.0;

    row_bottom > visible_top - epsilon && row_top < visible_bottom + epsilon
}

fn set_date_range_preset(app: &mut App, preset: DateRangePreset) -> Task<AppMessage> {
    app.clips.date_range_preset = preset;
    app.clear_clip_filter_feedback();
    if preset != DateRangePreset::Custom {
        app.clips.active_calendar = None;
    }

    match preset {
        DateRangePreset::AllTime => {
            app.clips.filters.event_after_ts = None;
            app.clips.filters.event_before_ts = None;
            refresh_history(app)
        }
        DateRangePreset::Custom => Task::none(),
        _ => match preset.bounds(Local::now()) {
            Some((start, end)) => {
                app.clips.filters.event_after_ts = Some(start.timestamp_millis());
                app.clips.filters.event_before_ts = Some(end.timestamp_millis());
                refresh_history(app)
            }
            None => Task::none(),
        },
    }
}

fn apply_date_range(app: &mut App) -> Task<AppMessage> {
    let start = match parse_range_input(&app.clips.date_range_start, false) {
        Ok(value) => value,
        Err(error) => {
            app.set_clip_filter_feedback(error, true);
            return Task::none();
        }
    };

    let end = match parse_range_input(&app.clips.date_range_end, true) {
        Ok(value) => value,
        Err(error) => {
            app.set_clip_filter_feedback(error, true);
            return Task::none();
        }
    };

    if let (Some(start), Some(end)) = (start, end)
        && start > end
    {
        app.set_clip_filter_feedback("Custom range start must be before the end.", true);
        return Task::none();
    }

    app.clips.filters.event_after_ts = start.map(|value| value.timestamp_millis());
    app.clips.filters.event_before_ts = end.map(|value| value.timestamp_millis());
    app.clips.active_calendar = None;
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
    if app.clips.active_calendar == Some(field) {
        app.clips.active_calendar = None;
        return;
    }

    app.clips.active_calendar = Some(field);
    app.clips.calendar_month =
        calendar_seed_date(calendar_input(app, field)).unwrap_or_else(today_local_date);
}

fn select_calendar_day(app: &mut App, date: NaiveDate) {
    let Some(field) = app.clips.active_calendar else {
        return;
    };

    let next_value = merge_calendar_date(calendar_input(app, field), date);

    match field {
        CalendarField::Start => app.clips.date_range_start = next_value,
        CalendarField::End => app.clips.date_range_end = next_value,
    }

    app.clear_clip_filter_feedback();
    app.clips.active_calendar = None;
}

fn calendar_input(app: &App, field: CalendarField) -> &str {
    match field {
        CalendarField::Start => &app.clips.date_range_start,
        CalendarField::End => &app.clips.date_range_end,
    }
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

pub(in crate::app) fn view(app: &App) -> Element<'_, Message> {
    let header = page_header("Clips").build();

    let body = column![header, filters_panel(app), clip_workspace(app),]
        .spacing(12)
        .height(Length::Fill);

    let base: Element<'_, Message> = container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

    let with_calendar = if let Some(field) = app.clips.active_calendar {
        modal(
            base,
            calendar_dropdown(app, field),
            Some(Message::DismissCalendar),
        )
    } else {
        base
    };

    let with_montage = if app.clips.montage_modal_open {
        modal(
            with_calendar,
            montage_queue_dialog(app),
            Some(Message::CancelMontageModal),
        )
    } else {
        with_calendar
    };

    if let Some(pending) = &app.clips.pending_delete {
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
    let tag_options = filter_pick_list_options(ALL_TAGS_LABEL, &filter_options.tags);
    let collection_options = collection_filter_options(&filter_options.collections);
    let selected_collection =
        selected_collection_filter_option(&collection_options, app.clips.filters.collection_id);

    let active = active_filter_count(app);

    // Basic row: search + date preset + profile + rule + character
    let basic_row = row![
        text_input("Search...", &app.clips.filters.search,)
            .on_input(Message::SearchChanged)
            .width(Length::FillPortion(3)),
        pick_list(
            &DateRangePreset::ALL[..],
            Some(app.clips.date_range_preset),
            Message::DateRangePresetChanged,
        )
        .width(Length::FillPortion(1)),
        pick_list(
            profile_options,
            Some(selected_filter_option(
                &app.clips.filters.profile,
                ALL_PROFILES_LABEL,
            )),
            Message::ProfileFilterChanged,
        )
        .width(Length::FillPortion(1))
        .placeholder(ALL_PROFILES_LABEL),
        pick_list(
            rule_options,
            Some(selected_filter_option(
                &app.clips.filters.rule,
                ALL_RULES_LABEL
            )),
            Message::RuleFilterChanged,
        )
        .width(Length::FillPortion(1))
        .placeholder(ALL_RULES_LABEL),
        pick_list(
            character_options,
            Some(selected_filter_option(
                &app.clips.filters.character,
                ALL_CHARACTERS_LABEL,
            )),
            Message::CharacterFilterChanged,
        )
        .width(Length::FillPortion(1))
        .placeholder(ALL_CHARACTERS_LABEL),
        row![
            checkbox(app.clips.filters.favorites_only).on_toggle(Message::FavoritesFilterToggled),
            text("Favorites").size(12),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .width(Length::Shrink),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let advanced_label = if app.clips.advanced_filters_open {
        "Hide advanced filters"
    } else {
        "Show advanced filters"
    };
    let advanced_toggle = with_tooltip(
        styled_button(advanced_label, ButtonTone::Secondary)
            .on_press(Message::ToggleAdvancedFilters)
            .into(),
        "More filters.",
    );

    let reset_button: Element<'static, Message> = if active > 0 {
        with_tooltip(
            styled_button("Clear filters", ButtonTone::Secondary)
                .on_press(Message::ClearFilters)
                .into(),
            "Clear all filters.",
        )
    } else {
        styled_button("Clear filters", ButtonTone::Secondary).into()
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

    if app.clips.advanced_filters_open {
        content = content.push(advanced_filters_row(
            app,
            &filter_options,
            tag_options,
            collection_options,
            selected_collection,
        ));
    }

    if app.clips.date_range_preset == DateRangePreset::Custom {
        content = content.push(custom_range_row(app));
    }

    if let Some(feedback) = app.clips.filter_feedback.as_ref() {
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
    tag_options: Vec<String>,
    collection_options: Vec<CollectionFilterOption>,
    selected_collection: Option<CollectionFilterOption>,
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
                    &app.clips.filters.target,
                    ALL_TARGETS_LABEL,
                )),
                Message::TargetFilterChanged,
            )
            .width(Length::FillPortion(1))
            .placeholder(ALL_TARGETS_LABEL),
            pick_list(
                weapon_options,
                Some(selected_filter_option(
                    &app.clips.filters.weapon,
                    ALL_WEAPONS_LABEL,
                )),
                Message::WeaponFilterChanged,
            )
            .width(Length::FillPortion(1))
            .placeholder(ALL_WEAPONS_LABEL),
            pick_list(
                alert_options,
                Some(selected_filter_option(
                    &app.clips.filters.alert,
                    ALL_ALERTS_LABEL
                )),
                Message::AlertFilterChanged,
            )
            .width(Length::FillPortion(1))
            .placeholder(ALL_ALERTS_LABEL),
            pick_list(
                tag_options,
                Some(selected_filter_option(
                    &app.clips.filters.tag,
                    ALL_TAGS_LABEL,
                )),
                Message::TagFilterChanged,
            )
            .width(Length::FillPortion(1))
            .placeholder(ALL_TAGS_LABEL),
            pick_list(
                &OverlapFilterChoice::ALL[..],
                Some(OverlapFilterChoice::from_state(
                    app.clips.filters.overlap_state
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
                    &app.clips.filters.server,
                    ALL_SERVERS_LABEL
                )),
                Message::ServerFilterChanged,
            )
            .width(Length::FillPortion(1))
            .placeholder(ALL_SERVERS_LABEL),
            pick_list(
                continent_options,
                Some(selected_filter_option(
                    &app.clips.filters.continent,
                    ALL_CONTINENTS_LABEL,
                )),
                Message::ContinentFilterChanged,
            )
            .width(Length::FillPortion(1))
            .placeholder(ALL_CONTINENTS_LABEL),
            pick_list(
                base_options,
                Some(selected_filter_option(
                    &app.clips.filters.base,
                    ALL_BASES_LABEL
                )),
                Message::BaseFilterChanged,
            )
            .width(Length::FillPortion(1))
            .placeholder(ALL_BASES_LABEL),
            pick_list(collection_options, selected_collection, |option| {
                Message::CollectionSelected(option.id)
            })
            .width(Length::FillPortion(1))
            .placeholder(ALL_COLLECTIONS_LABEL),
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
            text_input("Start: 2026-04-05 18:30", &app.clips.date_range_start)
                .on_input(Message::DateRangeStartChanged)
                .width(Length::Fill)
                .into(),
            "Local start time. Accepts YYYY-MM-DD, optional HH:MM(:SS), or RFC3339.",
        ),
        styled_button("Pick start", ButtonTone::Secondary)
            .on_press(Message::ToggleCalendar(CalendarField::Start)),
        with_tooltip(
            text_input("End: 2026-04-06 01:15", &app.clips.date_range_end)
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

fn clip_workspace(app: &App) -> Element<'_, Message> {
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

fn clip_history_panel(app: &App) -> Element<'_, Message> {
    let visible = app.clips.history.len();
    let total = app.clips.history_source.len();
    let page = app.clips.history_page;
    let page_size = app.clips.history_page_size.max(1);
    let total_pages = total_pages(app);

    let status_row = {
        let montage_count = app.clips.montage_selection.len();
        let collection_options = collection_select_options(&app.clips.filter_options.collections);
        let selected_collection = collection_options
            .iter()
            .find(|option| Some(option.id) == app.clips.selected_collection_add_id);
        let gallery_enabled = app.clips.browser_mode == ClipBrowserMode::Gallery;
        let detail_label: &'static str = if app.clips.detail_loading {
            "Detail loading"
        } else if app.clips.selected_id.is_some() {
            "Detail ready"
        } else {
            "No clip selected"
        };
        let detail_tone = if app.clips.detail_loading {
            BadgeTone::Warning
        } else if app.clips.selected_id.is_some() {
            BadgeTone::Info
        } else {
            BadgeTone::Neutral
        };

        let bar = toolbar()
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

        let build_montage_button = || -> Element<'static, Message> {
            if montage_count >= 2 {
                styled_button(
                    format!("Create montage ({montage_count})"),
                    ButtonTone::Primary,
                )
                .on_press(Message::CreateMontage)
                .into()
            } else {
                with_tooltip(
                    styled_button("Create montage", ButtonTone::Primary).into(),
                    "Select 2+ clips.",
                )
            }
        };

        let browser_switch: Element<'static, Message> = row![
            text("Gallery view").size(12),
            toggle_switch(gallery_enabled).on_toggle(Message::BrowserModeToggled),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into();

        let summary_bar = if montage_count == 0 {
            bar.trailing(build_montage_button())
                .trailing(browser_switch)
                .build()
        } else {
            bar.push(clips_badge(
                format!("{montage_count} in montage queue"),
                BadgeTone::Success,
            ))
            .trailing(browser_switch)
            .build()
        };

        if montage_count == 0 {
            summary_bar
        } else {
            let collection_editor = iced::widget::combo_box(
                &app.clips.bulk_collection_editor_options,
                "Search or type a collection",
                selected_collection,
                Message::CollectionAddSelectionChanged,
            )
            .on_input(Message::NewCollectionNameChanged)
            .width(Length::FillPortion(3));

            let add_to_collection: Element<'static, Message> =
                if app.clips.selected_collection_add_id.is_some()
                    || !app.clips.new_collection_name.trim().is_empty()
                {
                    styled_button("Add to collection", ButtonTone::Secondary)
                        .on_press(Message::AddSelectedToCurrentCollectionRequested)
                        .into()
                } else {
                    with_tooltip(
                        styled_button("Add to collection", ButtonTone::Secondary).into(),
                        "Choose an existing collection or type a new one first.",
                    )
                };

            column![
                summary_bar,
                row![
                    clips_badge(format!("{montage_count} selected"), BadgeTone::Success),
                    text("Search existing collections or type a new name.").size(12),
                    Space::new().width(Length::Fill),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                row![collection_editor, add_to_collection,]
                    .spacing(8)
                    .align_y(Alignment::Center),
                row![
                    styled_button("Favorite selected", ButtonTone::Secondary)
                        .on_press(Message::FavoriteSelectedRequested),
                    styled_button("Unfavorite selected", ButtonTone::Secondary)
                        .on_press(Message::UnfavoriteSelectedRequested),
                    styled_button("Clear selection", ButtonTone::Secondary)
                        .on_press(Message::ClearMontageSelection),
                    Space::new().width(Length::Fill),
                    build_montage_button()
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            ]
            .spacing(8)
            .into()
        }
    };

    let header_row = if app.clips.browser_mode == ClipBrowserMode::List {
        Some(history_header_row(app))
    } else {
        None
    };
    let body_rows = history_body_rows(app, page, page_size);

    let footer = history_footer_row(app, page, page_size, total_pages);

    let mut panel_builder = panel("Clip history").push(status_row);
    if let Some(header_row) = header_row {
        panel_builder = panel_builder.push(header_row);
    }

    panel_builder
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
    let current = app.clips.sort_column;
    let desc = app.clips.sort_descending;

    container(
        row![
            header_static_cell(" ", Length::Fixed(42.0)),
            header_static_cell("Fav", Length::Fixed(54.0)),
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
            header_static_cell("Flags", Length::Fixed(200.0)),
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
    container(
        text_non_selectable(label)
            .size(11)
            .style(|theme: &iced::Theme| TextStyle {
                color: Some(theme::tokens_for(theme).color.muted_foreground),
            }),
    )
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
    let content = container(
        text_non_selectable(format!("{label}{arrow}"))
            .size(11)
            .style(move |theme: &iced::Theme| {
                let c = &theme::tokens_for(theme).color;
                TextStyle {
                    color: Some(if is_active {
                        c.foreground
                    } else {
                        c.muted_foreground
                    }),
                }
            }),
    )
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
    if app.clips.history.is_empty() {
        return empty_state("No matching clips")
            .description("Try different filters.")
            .build()
            .into();
    }

    let start = page * page_size;
    let end = (start + page_size).min(app.clips.history.len());
    let slice = &app.clips.history[start..end];

    if app.clips.browser_mode == ClipBrowserMode::Gallery {
        return gallery_body_rows(app, slice);
    }

    let rows: Vec<Element<'static, Message>> = slice
        .iter()
        .map(|record| dense_history_row(app, record, app.clips.selected_id == Some(record.id)))
        .collect();

    scrollable(Column::with_children(rows).spacing(2))
        .id(CLIP_HISTORY_SCROLLABLE_ID)
        .height(Length::Fill)
        .on_scroll(|viewport| Message::HistoryScrolled(viewport.into()))
        .into()
}

fn gallery_body_rows(app: &App, slice: &[ClipRecord]) -> Element<'static, Message> {
    let cards: Vec<Element<'static, Message>> = slice
        .iter()
        .map(|record| gallery_card(app, record, app.clips.selected_id == Some(record.id)))
        .collect();

    let gallery = Row::with_children(cards)
        .spacing(GALLERY_CARD_SPACING)
        .width(Length::Fill)
        .wrap()
        .vertical_spacing(GALLERY_CARD_SPACING)
        .align_x(iced::alignment::Horizontal::Center);

    container(
        scrollable(gallery)
            .id(CLIP_HISTORY_SCROLLABLE_ID)
            .height(Length::Fill)
            .on_scroll(|viewport| Message::HistoryScrolled(viewport.into())),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn history_footer_row(
    app: &App,
    page: usize,
    page_size: usize,
    total_pages: usize,
) -> Element<'static, Message> {
    let visible = app.clips.history.len();
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
            Some(app.clips.history_page_size),
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
    let montage_selected = app.clips.montage_selection.contains(&clip_id);
    let post_process_block = super::super::clip_post_process_block_reason(record);

    let checkbox_cell = container(montage_selection_checkbox(
        clip_id,
        montage_selected,
        post_process_block.as_deref(),
    ))
    .padding([6, 10])
    .width(Length::Fixed(42.0))
    .align_y(Alignment::Center);
    let favorite_button = favorite_icon_button(record.favorited, Message::ToggleFavorite(clip_id));
    let favorite_cell = container(favorite_button)
        .padding([6, 10])
        .width(Length::Fixed(54.0))
        .align_y(Alignment::Center);

    let content = row![
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
            .width(Length::Fixed(200.0))
            .align_y(Alignment::Center),
    ]
    .spacing(0)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let row_content = row![
        checkbox_cell,
        favorite_cell,
        mouse_area(
            container(content)
                .width(Length::Fill)
                .style(move |theme: &iced::Theme| clip_list_row_style(theme, selected))
        )
        .on_press(Message::RowSelected(clip_id))
        .interaction(iced::mouse::Interaction::Pointer),
    ]
    .spacing(0)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    container(row_content)
        .width(Length::Fill)
        .style(move |theme: &iced::Theme| clip_list_row_style(theme, selected))
        .into()
}

fn gallery_card(app: &App, record: &ClipRecord, selected: bool) -> Element<'static, Message> {
    let clip_id = record.id;
    let montage_selected = app.clips.montage_selection.contains(&clip_id);
    let post_process_block = super::super::clip_post_process_block_reason(record);

    let header = row![
        montage_selection_checkbox(clip_id, montage_selected, post_process_block.as_deref()),
        Space::new().width(Length::Fill),
        favorite_icon_button(record.favorited, Message::ToggleFavorite(clip_id)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let summary = column![
        text_non_selectable(rule_label(app, record)).size(14),
        text_non_selectable(character_label(app, record))
            .size(12)
            .style(|theme: &iced::Theme| TextStyle {
                color: Some(theme::tokens_for(theme).color.muted_foreground),
            }),
        text_non_selectable(format_smart_timestamp(record.trigger_event_at))
            .size(12)
            .style(|theme: &iced::Theme| TextStyle {
                color: Some(theme::tokens_for(theme).color.muted_foreground),
            }),
        row![
            clips_badge(format!("Score {}", record.score), BadgeTone::Info),
            clips_badge(short_duration_label(record), BadgeTone::Outline),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
        container(flag_badges(record)).width(Length::Fill),
    ]
    .spacing(6);

    let content = column![
        header,
        clip_thumbnail_preview(app, record, Length::Fill, 160.0, ContentFit::Cover, false),
        summary,
    ]
    .spacing(10)
    .width(Length::Fill);

    mouse_area(
        container(content)
            .width(Length::Fixed(GALLERY_CARD_WIDTH))
            .padding([12, 12])
            .style(move |theme: &iced::Theme| gallery_card_style(theme, selected)),
    )
    .on_press(Message::RowSelected(clip_id))
    .interaction(iced::mouse::Interaction::Pointer)
    .into()
}

fn dense_text_cell(value: impl Into<String>, width: Length) -> Element<'static, Message> {
    container(
        text_non_selectable(value.into())
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

fn gallery_card_style(theme: &iced::Theme, selected: bool) -> ContainerStyle {
    let c = &theme::tokens_for(theme).color;
    ContainerStyle {
        text_color: Some(c.foreground),
        background: Some(Background::Color(if selected {
            c.muted
        } else {
            c.background
        })),
        border: theme::border(
            if selected { c.primary } else { c.border },
            1.0,
            theme::RADIUS.lg,
        ),
        ..Default::default()
    }
}

fn clip_thumbnail_preview(
    app: &App,
    record: &ClipRecord,
    width: Length,
    height: f32,
    content_fit: ContentFit,
    allow_path_fallback: bool,
) -> Element<'static, Message> {
    if let Some(path) = record.thumbnail_path.as_deref()
        && std::path::Path::new(path).exists()
    {
        if let Some(handle) = app.clips.thumbnail_handles.get(path) {
            return container(
                image(handle.clone())
                    .width(Length::Fill)
                    .height(height)
                    .content_fit(content_fit),
            )
            .width(width)
            .height(height)
            .style(|theme: &iced::Theme| ContainerStyle {
                background: Some(Background::Color(theme::tokens_for(theme).color.muted)),
                border: theme::border(theme::tokens_for(theme).color.border, 1.0, theme::RADIUS.md),
                ..Default::default()
            })
            .into();
        }

        if allow_path_fallback {
            return container(
                image(image::Handle::from_path(path))
                    .width(Length::Fill)
                    .height(height)
                    .content_fit(content_fit),
            )
            .width(width)
            .height(height)
            .style(|theme: &iced::Theme| ContainerStyle {
                background: Some(Background::Color(theme::tokens_for(theme).color.muted)),
                border: theme::border(theme::tokens_for(theme).color.border, 1.0, theme::RADIUS.md),
                ..Default::default()
            })
            .into();
        }

        if app.clips.thumbnail_loads_in_flight.contains(path) {
            return container(
                center(text_non_selectable("Loading thumbnail...").size(12).style(
                    |theme: &iced::Theme| TextStyle {
                        color: Some(theme::tokens_for(theme).color.muted_foreground),
                    },
                ))
                .width(Length::Fill)
                .height(Length::Fill),
            )
            .width(width)
            .height(height)
            .style(|theme: &iced::Theme| ContainerStyle {
                background: Some(Background::Color(theme::tokens_for(theme).color.muted)),
                border: theme::border(theme::tokens_for(theme).color.border, 1.0, theme::RADIUS.md),
                ..Default::default()
            })
            .into();
        }

        return container(
            center(
                text_non_selectable("Preparing thumbnail...")
                    .size(12)
                    .style(|theme: &iced::Theme| TextStyle {
                        color: Some(theme::tokens_for(theme).color.muted_foreground),
                    }),
            )
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .width(width)
        .height(height)
        .style(|theme: &iced::Theme| ContainerStyle {
            background: Some(Background::Color(theme::tokens_for(theme).color.muted)),
            border: theme::border(theme::tokens_for(theme).color.border, 1.0, theme::RADIUS.md),
            ..Default::default()
        })
        .into();
    }

    container(
        center(
            text_non_selectable("No thumbnail")
                .size(12)
                .style(|theme: &iced::Theme| TextStyle {
                    color: Some(theme::tokens_for(theme).color.muted_foreground),
                }),
        )
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(width)
    .height(height)
    .style(|theme: &iced::Theme| ContainerStyle {
        background: Some(Background::Color(theme::tokens_for(theme).color.muted)),
        border: theme::border(theme::tokens_for(theme).color.border, 1.0, theme::RADIUS.md),
        ..Default::default()
    })
    .into()
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
    with_tooltip(checkbox, block_reason.unwrap_or("Add to montage."))
}

fn favorite_icon_button(message_active: bool, message: Message) -> Element<'static, Message> {
    let label = if message_active {
        "\u{2605}"
    } else {
        "\u{2606}"
    };
    let tone = if message_active {
        ButtonTone::Warning
    } else {
        ButtonTone::Secondary
    };
    styled_button(label, tone).on_press(message).into()
}

fn flag_badges(record: &ClipRecord) -> Element<'static, Message> {
    let mut items: Vec<Element<'static, Message>> = Vec::new();
    for tag_name in record.tags.iter().take(2) {
        items.push(
            tag(tag_name.clone())
                .tone(BadgeTone::Outline)
                .build()
                .into(),
        );
    }
    if record.tags.len() > 2 {
        items.push(
            badge(format!("+{}", record.tags.len() - 2))
                .tone(BadgeTone::Outline)
                .build()
                .into(),
        );
    }
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
    if record.collection_count > 0 {
        items.push(with_tooltip(
            badge(format!("C{}", record.collection_count))
                .tone(BadgeTone::Neutral)
                .build()
                .into(),
            format!(
                "Member of {} collection{}.",
                record.collection_count,
                if record.collection_count == 1 {
                    ""
                } else {
                    "s"
                }
            ),
        ));
    }
    match record.post_process_status {
        crate::db::PostProcessStatus::Failed => {
            items.push(with_tooltip(
                badge("PP").tone(BadgeTone::Destructive).build().into(),
                "Audio processing failed.",
            ));
        }
        crate::db::PostProcessStatus::Pending => {
            items.push(with_tooltip(
                badge("PP").tone(BadgeTone::Warning).build().into(),
                "Audio processing pending.",
            ));
        }
        crate::db::PostProcessStatus::Completed => {
            items.push(badge("PP").tone(BadgeTone::Success).build().into());
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

fn clip_detail_workspace(app: &App) -> Element<'_, Message> {
    if let Some(detail) = app.clips.selected_detail.as_ref() {
        clip_detail_panel(app, detail)
    } else if app.clips.detail_loading {
        panel("Clip detail")
            .push(empty_state("Loading...").build())
            .build()
            .into()
    } else {
        panel("Clip detail")
            .push(
                empty_state("No clip selected")
                    .description("Arrow keys, Enter, Space, Delete")
                    .build(),
            )
            .build()
            .into()
    }
}

fn clip_detail_panel<'a>(app: &'a App, detail: &'a ClipDetailRecord) -> Element<'a, Message> {
    let record = &detail.clip;
    let storage_tier = storage_tiering::clip_storage_tier(record, &app.config.storage_tiering);
    let post_process_block = super::super::clip_post_process_block_reason(record);
    let can_export_timeline = record.path.is_some() && !detail.raw_events.is_empty();

    let mut content: Column<'_, Message> = column![].spacing(12);
    content = content.push(detail_visual_header(app, record));
    content = content.push(detail_summary_card(app, record, storage_tier));

    content = content.push(detail_actions_section(
        record,
        storage_tier,
        post_process_block.as_deref(),
        can_export_timeline,
        detail,
        app.clips.deleting_id == Some(record.id),
    ));

    if matches!(
        record.post_process_status,
        crate::db::PostProcessStatus::Failed
    ) {
        content = content.push(
            section("Audio recovery")
                .description("Audio post-process failed.")
                .push(
                    row![
                        styled_button("Retry audio post-process", ButtonTone::Warning)
                            .on_press(Message::RetryPostProcessRequested(record.id)),
                        styled_button("Use original audio", ButtonTone::Secondary)
                            .on_press(Message::UseOriginalAudioRequested(record.id)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .build(),
        );
    }

    content = content.push(organization_section(app, detail));
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

fn detail_visual_header(app: &App, record: &ClipRecord) -> Element<'static, Message> {
    section("Preview")
        .description("Representative frame captured near the trigger moment.")
        .push(clip_thumbnail_preview(
            app,
            record,
            Length::Fill,
            270.0,
            ContentFit::Contain,
            true,
        ))
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
        clips_badge(
            if record.favorited {
                "Favorited"
            } else {
                "Not favorited"
            },
            if record.favorited {
                BadgeTone::Warning
            } else {
                BadgeTone::Neutral
            },
        ),
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
    let open_button: Element<'static, Message> = if record.path.is_some() {
        styled_button("Open", ButtonTone::Primary)
            .on_press(Message::OpenRequested(clip_id))
            .into()
    } else {
        with_tooltip(
            styled_button("Open", ButtonTone::Primary).into(),
            "No saved file attached.",
        )
    };
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
            "Delete clip."
        } else {
            "Remove from history."
        },
    );
    let honu_button: Option<Element<'static, Message>> = record.honu_session_id.map(|id| {
        styled_button("Honu session", ButtonTone::Secondary)
            .on_press(Message::OpenHonuSession(id))
            .into()
    });

    let mut primary_row = row![open_button, delete_button]
        .spacing(8)
        .align_y(Alignment::Center);
    if let Some(honu) = honu_button {
        primary_row = primary_row.push(honu);
    }
    primary_row = primary_row.push(
        styled_button(
            if record.favorited {
                "\u{2605} Favorited"
            } else {
                "\u{2606} Favorite"
            },
            if record.favorited {
                ButtonTone::Warning
            } else {
                ButtonTone::Secondary
            },
        )
        .on_press(Message::ToggleFavorite(clip_id)),
    );

    // Export group
    let chapter_button = with_tooltip(
        if can_export_timeline {
            styled_button("Chapters", ButtonTone::Secondary)
                .on_press(Message::ExportChaptersRequested(clip_id))
                .into()
        } else {
            styled_button("Chapters", ButtonTone::Secondary).into()
        },
        "Export chapters.",
    );
    let subtitle_button = with_tooltip(
        if can_export_timeline {
            styled_button("Subtitles", ButtonTone::Secondary)
                .on_press(Message::ExportSubtitlesRequested(clip_id))
                .into()
        } else {
            styled_button("Subtitles", ButtonTone::Secondary).into()
        },
        "Export as SRT.",
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
            "Keep on fast storage.",
        ),
        segmented_button(
            "Archive",
            current == StorageTier::Archive,
            Message::SetStorageTier(clip_id, StorageTier::Archive),
            "Move to cold storage.",
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

fn organization_section<'a>(app: &'a App, detail: &'a ClipDetailRecord) -> Element<'a, Message> {
    let clip_id = detail.clip.id;
    let count = detail.tags.len() + detail.collections.len();
    let collapsed = section_is_collapsed(app, DetailSection::Organization);
    let collection_options = collection_select_options(&app.clips.filter_options.collections);

    let tag_tokens: Element<'static, Message> = if detail.tags.is_empty() {
        text("No tags yet.")
            .size(12)
            .style(|theme: &iced::Theme| TextStyle {
                color: Some(theme::tokens_for(theme).color.muted_foreground),
            })
            .into()
    } else {
        let mut items = row![].spacing(6).align_y(Alignment::Center);
        for tag_name in &detail.tags {
            items = items.push(removable_token(
                tag_name.clone(),
                Message::RemoveTagRequested {
                    clip_id,
                    tag_name: tag_name.clone(),
                },
            ));
        }
        scrollable(items).width(Length::Fill).into()
    };

    let collection_tokens: Element<'static, Message> = if detail.collections.is_empty() {
        text("No collection memberships.")
            .size(12)
            .style(|theme: &iced::Theme| TextStyle {
                color: Some(theme::tokens_for(theme).color.muted_foreground),
            })
            .into()
    } else {
        let mut items = row![].spacing(6).align_y(Alignment::Center);
        for membership in &detail.collections {
            items = items.push(removable_token(
                membership.name.clone(),
                Message::RemoveClipFromCollectionRequested {
                    clip_id,
                    collection_id: membership.collection_id,
                },
            ));
        }
        scrollable(items).width(Length::Fill).into()
    };

    let collection_combo_selection = collection_options
        .iter()
        .find(|option| Some(option.id) == app.clips.selected_collection_add_id);
    let tag_combo_selection = if app.clips.tag_input.trim().is_empty() {
        None
    } else {
        app.clips
            .filter_options
            .tags
            .iter()
            .find(|tag_name| tag_name.eq_ignore_ascii_case(app.clips.tag_input.trim()))
    };

    let tag_editor = iced::widget::combo_box(
        &app.clips.tag_editor_options,
        "Search or type a tag",
        tag_combo_selection,
        Message::TagOptionSelected,
    )
    .on_input(Message::TagInputChanged)
    .width(Length::Fill);

    let collection_editor = iced::widget::combo_box(
        &app.clips.collection_editor_options,
        "Search or type a collection",
        collection_combo_selection,
        Message::CollectionOptionSelected,
    )
    .on_input(Message::DetailCollectionNameChanged)
    .width(Length::Fill);

    let header = collapsible_header(
        "Organization",
        Some("Favorite, tag, and place this clip into curated collections."),
        count,
        DetailSection::Organization,
        collapsed,
    );

    let mut wrapper: Column<'a, Message> = column![header].spacing(6);
    if !collapsed {
        wrapper = wrapper
            .push(
                column![
                    text_non_selectable("Tags")
                        .size(16)
                        .style(|theme: &iced::Theme| TextStyle {
                            color: Some(theme::tokens_for(theme).color.foreground),
                        }),
                    text_non_selectable(
                        "Select from the dropdown to add immediately, or type a new tag and press Enter.",
                    )
                    .size(12)
                    .style(|theme: &iced::Theme| TextStyle {
                        color: Some(theme::tokens_for(theme).color.muted_foreground),
                    }),
                    tag_tokens,
                    tag_editor,
                ]
                .spacing(8),
            )
            .push(
                column![
                    text_non_selectable("Collections")
                        .size(16)
                        .style(|theme: &iced::Theme| TextStyle {
                            color: Some(theme::tokens_for(theme).color.foreground),
                        }),
                    text_non_selectable(
                        "Select from the dropdown to add immediately, or type a new collection and press Enter.",
                    )
                    .size(12)
                    .style(|theme: &iced::Theme| TextStyle {
                        color: Some(theme::tokens_for(theme).color.muted_foreground),
                    }),
                    collection_tokens,
                    collection_editor,
                ]
                .spacing(8),
            );
    }
    wrapper.into()
}

fn section_is_collapsed(app: &App, section: DetailSection) -> bool {
    app.clips.collapsed_detail_sections.contains(&section)
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
            text_non_selectable(title_text)
                .size(14)
                .style(|theme: &iced::Theme| TextStyle {
                    color: Some(theme::tokens_for(theme).color.foreground),
                }),
            text_non_selectable(desc)
                .size(11)
                .style(|theme: &iced::Theme| TextStyle {
                    color: Some(theme::tokens_for(theme).color.muted_foreground),
                }),
        ]
        .spacing(2)
        .into()
    } else {
        text_non_selectable(title_text)
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
            wrapper = wrapper.push(text("No audio metadata.").size(12).style(
                |theme: &iced::Theme| TextStyle {
                    color: Some(theme::tokens_for(theme).color.muted_foreground),
                },
            ));
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
            wrapper = wrapper.push(text("No uploads yet.").size(12).style(
                |theme: &iced::Theme| TextStyle {
                    color: Some(theme::tokens_for(theme).color.muted_foreground),
                },
            ));
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
            wrapper = wrapper.push(text("No alert context.").size(12).style(
                |theme: &iced::Theme| TextStyle {
                    color: Some(theme::tokens_for(theme).color.muted_foreground),
                },
            ));
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
            wrapper = wrapper.push(text("No overlaps.").size(12).style(|theme: &iced::Theme| {
                TextStyle {
                    color: Some(theme::tokens_for(theme).color.muted_foreground),
                }
            }));
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
    let filter = app.clips.raw_event_filter.trim().to_lowercase();
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
            wrapper = wrapper.push(text("No events captured.").size(12).style(
                |theme: &iced::Theme| TextStyle {
                    color: Some(theme::tokens_for(theme).color.muted_foreground),
                },
            ));
        } else {
            wrapper = wrapper.push(
                row![
                    with_tooltip(
                        text_input(
                            "Filter event kind, target, weapon",
                            &app.clips.raw_event_filter
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
        app.clips.calendar_month,
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
    let confirm_button: Element<'static, Message> = if app.clips.montage_selection.len() >= 2 {
        styled_button("Confirm montage", ButtonTone::Primary)
            .on_press(Message::ConfirmMontageCreation)
            .into()
    } else {
        with_tooltip(
            styled_button("Confirm montage", ButtonTone::Primary).into(),
            "Select 2+ clips.",
        )
    };

    let mut content = column![
        text("Create montage").size(20),
        text("Reorder clips and confirm.")
            .size(13)
            .style(|theme: &iced::Theme| TextStyle {
                color: Some(theme::tokens_for(theme).color.muted_foreground),
            }),
        row![
            clips_badge(
                format!("{} selected", app.clips.montage_selection.len()),
                if app.clips.montage_selection.len() >= 2 {
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

    if app.clips.montage_selection.is_empty() {
        content = content.push(
            empty_state("No clips selected")
                .description("Select clips first.")
                .build(),
        );
    } else {
        let last_index = app.clips.montage_selection.len().saturating_sub(1);
        let mut queue = column![].spacing(8);
        for (index, clip_id) in app.clips.montage_selection.iter().enumerate() {
            let maybe_record = app
                .clips
                .history_source
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
                .unwrap_or_else(|| "No longer available.".into());

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

fn collection_select_options(collections: &[ClipCollectionRecord]) -> Vec<CollectionSelectOption> {
    collections
        .iter()
        .map(|collection| CollectionSelectOption {
            id: collection.id,
            label: collection.name.clone(),
        })
        .collect()
}

fn collection_filter_options(collections: &[ClipCollectionRecord]) -> Vec<CollectionFilterOption> {
    let mut options = Vec::with_capacity(collections.len() + 1);
    options.push(CollectionFilterOption {
        id: None,
        label: ALL_COLLECTIONS_LABEL.into(),
    });
    options.extend(collections.iter().map(|collection| CollectionFilterOption {
        id: Some(collection.id),
        label: collection.name.clone(),
    }));
    options
}

pub(in crate::app) fn sync_editor_options(app: &mut App) {
    app.clips.tag_editor_options =
        iced::widget::combo_box::State::new(app.clips.filter_options.tags.clone());
    let collection_options = collection_select_options(&app.clips.filter_options.collections);
    app.clips.collection_editor_options =
        iced::widget::combo_box::State::new(collection_options.clone());
    app.clips.bulk_collection_editor_options =
        iced::widget::combo_box::State::new(collection_options);
}

fn find_collection_by_name<'a>(app: &'a App, name: &str) -> Option<&'a ClipCollectionRecord> {
    app.clips
        .filter_options
        .collections
        .iter()
        .find(|collection| collection.name.eq_ignore_ascii_case(name.trim()))
}

fn selected_collection_filter_option(
    options: &[CollectionFilterOption],
    selected_id: Option<i64>,
) -> Option<CollectionFilterOption> {
    options
        .iter()
        .find(|option| option.id == selected_id)
        .cloned()
}

fn removable_token(label: impl Into<String>, message: Message) -> Element<'static, Message> {
    let label = label.into();
    button(
        container(
            row![text(label).size(12), text("\u{00D7}").size(10),]
                .spacing(6)
                .align_y(Alignment::Center),
        )
        .padding(Padding {
            top: 4.0,
            right: 9.0,
            bottom: 4.0,
            left: 10.0,
        })
        .style(|theme: &iced::Theme| {
            let c = &theme::tokens_for(theme).color;
            ContainerStyle {
                text_color: Some(c.foreground),
                background: Some(Background::Color(c.muted)),
                border: theme::border(c.border_strong, 1.0, theme::RADIUS.md),
                ..Default::default()
            }
        }),
    )
    .padding(0)
    .style(
        |theme: &iced::Theme, status: iced::widget::button::Status| {
            let c = &theme::tokens_for(theme).color;
            iced::widget::button::Style {
                background: match status {
                    iced::widget::button::Status::Hovered => Some(Background::Color(c.accent)),
                    iced::widget::button::Status::Pressed => Some(Background::Color(c.muted)),
                    _ => None,
                },
                text_color: c.foreground,
                border: iced::Border::default(),
                shadow: Default::default(),
                snap: false,
            }
        },
    )
    .on_press(message)
    .into()
}

fn refresh_clip_organization(app: &mut App) -> Task<AppMessage> {
    app.clear_clip_error();
    Task::batch([
        reload_views(app),
        app.load_clip_filter_options(),
        app.load_clip_detail(app.clips.selected_id),
    ])
}
