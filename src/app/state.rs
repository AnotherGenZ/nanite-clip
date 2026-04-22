use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::time::Instant;

use chrono::NaiveDate;
use iced::widget::{combo_box, image};
use iced::window;

use crate::capture;
use crate::config::{CaptureBackend, ObsManagementMode, UpdateChannel, YouTubePrivacyStatus};
use crate::db::{
    AlertInstanceRecord, ClipDetailRecord, ClipFilterOptions, ClipFilters, ClipRecord,
    ClipStatsSnapshot, SessionSummary,
};
use crate::hotkey::HotkeyManager;
use crate::post_process::FfmpegCapabilities;
use crate::recorder::VideoResolution;
use crate::tray::TrayController;
use crate::update::{UpdatePrimaryAction, UpdateState};

use super::tabs;
use super::{
    ActiveClipCapture, AppState, AudioSourceDraft, MonitoringSession, PendingClipDelete,
    PendingClipLink, PendingProfileImport, PendingRecorderStart, PendingRuleImport,
};

pub(crate) struct RuntimeState {
    pub lifecycle: AppState,
    pub hotkeys: HotkeyManager,
    pub tray: Option<TrayController>,
    pub main_window_id: Option<window::Id>,
    pub hotkey_config_generation: u64,
    pub next_clip_sequence: u64,
    pub pending_save_sequences: VecDeque<u64>,
    pub pending_clip_links: BTreeMap<u64, PendingClipLink>,
    pub status_feedback: Option<String>,
    pub status_feedback_expires_at: Option<Instant>,
    pub honu_session_id: Option<i64>,
    pub active_clip_capture: Option<ActiveClipCapture>,
    pub tracked_alerts: BTreeMap<String, AlertInstanceRecord>,
    pub manual_profile_override_profile_id: Option<String>,
    pub last_auto_switch_rule_id: Option<String>,
    pub active_session: Option<MonitoringSession>,
    pub last_session_summary: Option<SessionSummary>,
    pub portal_capture_recovery_notified: bool,
    pub startup_probe_due_at: Option<Instant>,
    pub startup_probe_pending_result: bool,
    pub startup_probe_resolution: Option<VideoResolution>,
    pub ffmpeg_capabilities: FfmpegCapabilities,
    pub obs_connection_status: Option<capture::ObsConnectionStatus>,
    pub pending_recorder_start: Option<PendingRecorderStart>,
    pub next_recorder_start_id: u64,
    pub obs_restart_requires_manual_restart: bool,
}

pub(crate) struct ClipLibraryState {
    pub recent: Vec<ClipRecord>,
    pub history_source: Vec<ClipRecord>,
    pub history: Vec<ClipRecord>,
    pub thumbnail_handles: BTreeMap<String, image::Handle>,
    pub thumbnail_loads_in_flight: BTreeSet<String>,
    pub filter_options: ClipFilterOptions,
    pub tag_editor_options: combo_box::State<String>,
    pub collection_editor_options: combo_box::State<tabs::clips::CollectionSelectOption>,
    pub bulk_collection_editor_options: combo_box::State<tabs::clips::CollectionSelectOption>,
    pub selected_collection_add_id: Option<i64>,
    pub pending_collection_membership: Option<tabs::clips::PendingCollectionMembership>,
    pub active_organization_editor: Option<tabs::clips::OrganizationEditor>,
    pub pending_organization_input_clear: Option<tabs::clips::OrganizationEditor>,
    pub tag_input: String,
    pub new_collection_name: String,
    pub new_collection_description: String,
    pub selected_id: Option<i64>,
    pub selected_detail: Option<ClipDetailRecord>,
    pub detail_loading: bool,
    pub filters: ClipFilters,
    pub query_revision: u64,
    pub sort_column: tabs::clips::ClipSortColumn,
    pub sort_descending: bool,
    pub history_page: usize,
    pub history_page_size: usize,
    pub history_viewport: Option<tabs::clips::HistoryViewportState>,
    pub browser_mode: tabs::clips::ClipBrowserMode,
    pub advanced_filters_open: bool,
    pub search_revision: u64,
    pub raw_event_filter: String,
    pub collapsed_detail_sections: Vec<tabs::clips::DetailSection>,
    pub pending_delete: Option<PendingClipDelete>,
    pub deleting_id: Option<i64>,
    pub error: Option<String>,
    pub error_expires_at: Option<Instant>,
    pub filter_feedback: Option<String>,
    pub filter_feedback_expires_at: Option<Instant>,
    pub montage_selection: Vec<i64>,
    pub selected_montage_clip_id: Option<i64>,
    pub montage_modal_open: bool,
    pub date_range_preset: tabs::clips::DateRangePreset,
    pub date_range_start: String,
    pub date_range_end: String,
    pub active_calendar: Option<tabs::clips::CalendarField>,
    pub calendar_month: NaiveDate,
}

pub(crate) struct StatsState {
    pub snapshot: Option<ClipStatsSnapshot>,
    pub loading: bool,
    pub error: Option<String>,
    pub revision: u64,
    pub time_range: tabs::stats::StatsTimeRange,
    pub collapsed_sections: Vec<tabs::stats::StatsSection>,
    pub last_refreshed_at: Option<Instant>,
}

pub(crate) struct RuleEditorState {
    pub vehicle_options: Vec<tabs::rules::LookupOption>,
    pub vehicle_browse_categories:
        BTreeMap<tabs::rules::VehicleBrowseKey, tabs::rules::VehicleBrowseCategory>,
    pub weapon_options: Vec<tabs::rules::WeaponLookupOption>,
    pub weapon_browse_groups:
        BTreeMap<tabs::rules::WeaponBrowseKey, tabs::rules::WeaponBrowseGroup>,
    pub weapon_browse_categories:
        BTreeMap<tabs::rules::WeaponBrowseKey, tabs::rules::WeaponBrowseCategory>,
    pub weapon_browse_factions:
        BTreeMap<tabs::rules::WeaponBrowseKey, tabs::rules::WeaponBrowseFaction>,
    pub filter_text_drafts: BTreeMap<tabs::rules::FilterTextDraftKey, String>,
    pub drag_state: Option<tabs::rules::RuleDragState>,
    pub feedback: Option<String>,
    pub feedback_expires_at: Option<Instant>,
    pub pending_profile_import: Option<PendingProfileImport>,
    pub pending_profile_import_shake_started_at: Option<Instant>,
    pub pending_rule_import: Option<PendingRuleImport>,
    pub pending_rule_import_shake_started_at: Option<Instant>,
    pub resolving_characters: BTreeSet<String>,
    pub resolving_lookups: BTreeSet<(crate::db::LookupKind, i64)>,
    pub selected_rule_id: Option<String>,
    pub sub_view: tabs::rules::RulesSubView,
    pub expanded_events: HashSet<(String, usize)>,
    pub expanded_filters: HashSet<(String, usize)>,
}

pub(crate) struct SettingsState {
    pub feedback: Option<String>,
    pub feedback_expires_at: Option<Instant>,
    pub sub_view: tabs::settings::SettingsSubView,
    pub launch_at_login: bool,
    pub auto_start_monitoring: bool,
    pub start_minimized: bool,
    pub minimize_to_tray: bool,
    pub update_auto_check: bool,
    pub update_channel: UpdateChannel,
    pub update_install_behavior: crate::update::UpdateInstallBehavior,
    pub selected_update_action: UpdatePrimaryAction,
    pub selected_rollback_release: Option<crate::update::AvailableRelease>,
    pub pending_hotkey_binding_label: Option<String>,
    pub pending_hotkey_success_toast: bool,
    pub service_id: String,
    pub capture_backend: CaptureBackend,
    pub capture_source: String,
    pub save_dir: String,
    pub framerate: String,
    pub codec: String,
    pub quality: String,
    pub audio_sources: Vec<AudioSourceDraft>,
    pub discovered_audio_sources: Vec<capture::DiscoveredAudioSource>,
    pub selected_device_audio_source: Option<tabs::settings::AvailableAudioSourceOption>,
    pub selected_application_audio_source: Option<tabs::settings::AvailableAudioSourceOption>,
    pub audio_discovery_running: bool,
    pub audio_discovery_error: Option<String>,
    pub container: String,
    pub obs_websocket_url: String,
    pub obs_password_input: String,
    pub obs_password_present: bool,
    pub obs_management_mode: ObsManagementMode,
    pub buffer_secs: String,
    pub save_delay_secs: String,
    pub clip_saved_notifications: bool,
    pub auto_generate_thumbnails: bool,
    pub clip_naming_template: String,
    pub manual_clip_enabled: bool,
    pub manual_clip_hotkey: String,
    pub hotkey_capture_active: bool,
    pub manual_clip_duration_secs: String,
    pub storage_tiering_enabled: bool,
    pub storage_tier_directory: String,
    pub storage_min_age_days: String,
    pub storage_max_score: String,
    pub copyparty_enabled: bool,
    pub copyparty_upload_url: String,
    pub copyparty_public_base_url: String,
    pub copyparty_username: String,
    pub copyparty_password_input: String,
    pub copyparty_password_present: bool,
    pub youtube_enabled: bool,
    pub youtube_client_id: String,
    pub youtube_client_secret_input: String,
    pub youtube_client_secret_present: bool,
    pub youtube_refresh_token_present: bool,
    pub youtube_oauth_in_flight: bool,
    pub youtube_privacy_status: YouTubePrivacyStatus,
    pub discord_enabled: bool,
    pub discord_min_score: String,
    pub discord_include_thumbnail: bool,
    pub discord_webhook_input: String,
    pub discord_webhook_present: bool,
    pub secure_store_backend_label: String,
}

pub(crate) struct UpdateUiState {
    pub state: UpdateState,
    pub details_modal_open: bool,
    pub details_log_text: Option<String>,
    pub details_log_error: Option<String>,
    pub details_log_loading: bool,
}
