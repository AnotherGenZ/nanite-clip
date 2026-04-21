mod helpers;
mod runtime;
mod shared;
mod state;
mod subscriptions;
mod tabs;

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use iced::{Element, Length, Subscription, Task, Theme, window};
use semver::Version;
use tracing::{info, warn};

use crate::background_jobs::{
    BackgroundJobId, BackgroundJobKind, BackgroundJobManager, BackgroundJobNotification,
    BackgroundJobRecord, BackgroundJobSuccess,
};
use crate::capture;
use crate::census::{self, AlertLifecycle, AlertUpdate, StreamEvent};
use crate::config::AudioSourceConfig;
use crate::config::Config;
use crate::db::{
    AlertInstanceRecord, CharacterOutfitCacheEntry, ClipAudioTrackDraft, ClipDetailRecord,
    ClipDraft, ClipEventContribution, ClipFilterOptions, ClipFilters, ClipOrigin,
    ClipRawEventDraft, ClipRecord, ClipStatsSnapshot, ClipStore, ClipUploadDraft, ClipUploadState,
    LookupKind, PostProcessStatus, SessionSummary, UploadProvider,
};
use crate::discord::DiscordWebhookRequest;
use crate::event_log::EventLog;
use crate::honu::HonuClient;
use crate::hotkey::{HotkeyEvent, HotkeyManager};
use crate::montage::MontageClip;
use crate::notifications::NotificationCenter;
use crate::platform::PlatformServices;
use crate::post_process::{
    self, PostProcessRequest, PostProcessResult, SavedPostProcessMetadata, TrimSpec,
};
use crate::process;
use crate::profile_transfer::{
    ProfileTransferBundle, ProfileTransferConflicts, RuleTransferBundle, RuleTransferConflicts,
};
use crate::recorder::{Recorder, SavePollResult, VideoResolution};
use crate::rules::engine::RuleEngine;
use crate::rules::switching::choose_runtime_rule;
use crate::rules::{ClassifiedEvent, ClipAction, ClipActionLifecycle, ClipLength, RuleProfile};
use crate::secure_store::SecretKey;
use crate::storage_tiering::{self, StorageTier};
use crate::tray::{TrayEvent, TrayProfileOption, TraySnapshot};
use crate::ui::app::{column, container, row, scrollable, stack, text};
use crate::ui::layout::tabs::{Tab as NavTab, tabs as nav_tabs};
use crate::ui::overlay::modal::modal;
use crate::ui::overlay::toast::{self, ToastId, ToastStack, Tone as ToastTone};
use crate::update::{
    self, UpdateApplyReport, UpdateApplyReportStatus, UpdateErrorKind, UpdateErrorState,
    UpdateInstallBehavior, UpdatePhase, UpdatePrimaryAction, UpdateProgressState, UpdateState,
};
use crate::uploads::{
    self, CopypartyUploadCredentials, YouTubeOAuthClient, YouTubeUploadCredentials,
};
use helpers::*;
use state::{
    ClipLibraryState, RuleEditorState, RuntimeState, SettingsState, StatsState, UpdateUiState,
};

pub(in crate::app) use helpers::{
    audio_source_drafts_from_config, clip_post_process_block_reason, release_policy_summary,
    system_update_plan_summary,
};

pub struct App {
    config: Config,
    view: View,
    recorder: Recorder,
    notifications: NotificationCenter,
    toasts: ToastStack,
    rule_engine: RuleEngine,
    process_watcher: Arc<dyn process::GameProcessWatcher>,
    clip_store: Option<ClipStore>,
    clip_store_notice: Option<String>,
    event_log: EventLog,
    platform: PlatformServices,
    honu_client: HonuClient,
    background_jobs: BackgroundJobManager,
    new_character_name: String,
    runtime: RuntimeState,
    clips: ClipLibraryState,
    stats: StatsState,
    rules: RuleEditorState,
    settings: SettingsState,
    updates: UpdateUiState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum View {
    Status,
    Clips,
    Stats,
    Characters,
    Rules,
    Settings,
}

#[derive(Debug, Clone)]
enum AppState {
    Idle,
    WaitingForGame,
    WaitingForLogin,
    Monitoring {
        character_name: String,
        character_id: u64,
    },
}

#[derive(Debug, Clone)]
enum PendingSaveOutcome {
    Saved {
        path: PathBuf,
        duration: ClipLength,
        audio_layout: Vec<crate::capture::ResolvedAudioSource>,
    },
    Failed,
}

#[derive(Debug, Clone, Default)]
struct PendingClipLink {
    clip_id: Option<i64>,
    save_outcome: Option<PendingSaveOutcome>,
    persist_failed: bool,
    naming_context: Option<crate::clip_naming::ClipNamingContext>,
}

#[derive(Debug, Clone)]
struct PendingClipDelete {
    clip_id: i64,
    path: Option<PathBuf>,
    file_size_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingProfileImport {
    pub source_path: String,
    pub bundle: ProfileTransferBundle,
    pub conflicts: ProfileTransferConflicts,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingRuleImport {
    pub source_path: String,
    pub bundle: RuleTransferBundle,
    pub conflicts: RuleTransferConflicts,
}

type RecorderStartResult =
    Result<Box<dyn crate::capture::CaptureSession>, crate::capture::CaptureError>;

#[cfg(not(target_os = "windows"))]
#[allow(clippy::arc_with_non_send_sync)]
fn share_hotkey_config_result(hotkeys: HotkeyManager) -> Arc<Mutex<HotkeyManager>> {
    Arc::new(Mutex::new(hotkeys))
}

#[cfg(not(target_os = "windows"))]
fn take_hotkey_config_result(hotkeys: Arc<Mutex<HotkeyManager>>) -> HotkeyManager {
    let mut hotkeys = hotkeys.lock().expect("hotkey manager mutex poisoned");
    std::mem::replace(&mut *hotkeys, HotkeyManager::disabled())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HotkeyConfigurationFeedback {
    Success(String),
    Note(String),
}

fn hotkey_configuration_feedback(
    show_success_toast: bool,
    previous_binding_label: Option<&str>,
    binding_label: Option<&str>,
    configuration_note: Option<&str>,
) -> Option<HotkeyConfigurationFeedback> {
    if let Some(binding_label) = binding_label {
        let changed = previous_binding_label != Some(binding_label);
        if changed && show_success_toast {
            return Some(HotkeyConfigurationFeedback::Success(format!(
                "Manual clip hotkey active: {binding_label}"
            )));
        }
        return None;
    }

    if show_success_toast {
        return configuration_note.map(|message| HotkeyConfigurationFeedback::Note(message.into()));
    }

    None
}

struct PendingRecorderStart {
    id: u64,
    capture_plan: process::CaptureSourcePlan,
    result_rx: tokio::sync::oneshot::Receiver<RecorderStartResult>,
    abort_handle: iced::task::Handle,
}

#[derive(Debug, Clone)]
struct MonitoringSession {
    id: String,
    started_at: chrono::DateTime<Utc>,
    character_id: u64,
    character_name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PostUploadDiscordClipLoaded {
    clip_id: i64,
    provider_label: String,
    clip_url: Option<String>,
    clip: ClipRecord,
}

#[derive(Debug, Clone)]
struct ActiveClipCapture {
    request: ClipSaveRequest,
    preferred_start_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct ClipSaveRequest {
    origin: ClipOrigin,
    profile_id: String,
    rule_id: String,
    duration: ClipLength,
    clip_duration_secs: u32,
    trigger_score: u32,
    score_breakdown: Vec<crate::rules::ScoreBreakdown>,
    trigger_at: chrono::DateTime<Utc>,
    clip_start_at: chrono::DateTime<Utc>,
    clip_end_at: chrono::DateTime<Utc>,
    world_id: u32,
    zone_id: Option<u32>,
    facility_id: Option<u32>,
    character_id: u64,
    honu_session_id: Option<i64>,
    session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AudioSourceDraft {
    pub label: String,
    pub source: String,
    pub gain_db: f32,
    pub muted_in_premix: bool,
    pub included_in_premix: bool,
}

impl Default for AudioSourceDraft {
    fn default() -> Self {
        Self {
            label: String::new(),
            source: String::new(),
            gain_db: 0.0,
            muted_in_premix: false,
            included_in_premix: true,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    SwitchView(View),
    Runtime(RuntimeMessage),

    DatabaseReady(Result<ClipStore, String>),
    PostProcessRecoveryCompleted(Result<Vec<i64>, String>),
    BackgroundJobsRecovered(Result<Vec<BackgroundJobRecord>, String>),
    BackgroundJobStored(Result<(), String>),
    RuleClipAction(ClipAction),
    RequestManualClipSave,
    ClipPersisted {
        sequence: u64,
        result: Result<i64, String>,
    },
    ClipDetailLoaded(Result<Option<ClipDetailRecord>, String>),
    ClipFilterOptionsLoaded(Result<ClipFilterOptions, String>),
    StatsLoaded {
        revision: u64,
        result: Result<ClipStatsSnapshot, String>,
    },
    SessionSummaryLoaded {
        session_id: String,
        result: Result<SessionSummary, String>,
    },
    LookupResolved {
        kind: LookupKind,
        lookup_id: i64,
        refreshed: bool,
        result: Result<(), String>,
    },
    ResolvedRuleEngineEvent(ClassifiedEvent),
    AlertStored {
        alert_key: String,
        result: Result<(), String>,
    },
    ExportLastSessionSummary,
    SessionSummaryExported(Result<String, String>),
    StatsExported(Result<String, String>),
    TimelineArtifactExported {
        kind: crate::timeline_export::TimelineExportKind,
        result: Result<String, String>,
    },
    RunStorageTieringSweep,
    MoveClipToTier {
        clip_id: i64,
        target_tier: StorageTier,
    },
    UploadClipRequested {
        clip_id: i64,
        provider: UploadProvider,
    },
    CreateMontageRequested,
    CancelBackgroundJob(BackgroundJobId),
    RetryBackgroundJob(BackgroundJobId),
    RemoveBackgroundJob(BackgroundJobId),
    BackgroundJobRetryPrepared {
        job_id: BackgroundJobId,
        result: Result<BackgroundJobRetryPlan, String>,
    },
    BackgroundJobRemoved {
        job_id: BackgroundJobId,
        result: Result<(), String>,
    },
    ClipPostProcessBypassed {
        clip_id: i64,
        result: Result<(), String>,
    },
    YouTubeOAuthCompleted(Result<(), String>),
    PostUploadDiscordClipLoaded(Result<Option<PostUploadDiscordClipLoaded>, String>),
    ClipPathLinked {
        clip_id: i64,
        path: String,
        trim: Option<TrimSpec>,
        audio_layout: Vec<AudioSourceConfig>,
        result: Result<(), String>,
    },
    ClipResolutionInspected {
        path: String,
        result: Result<Option<VideoResolution>, String>,
    },
    StartupProbeCompleted {
        path: PathBuf,
        result: Result<Option<VideoResolution>, String>,
        delete_result: Result<(), String>,
    },
    OpenClipRequested(PathBuf),
    ClipOpenFinished {
        path: PathBuf,
        result: Result<(), String>,
    },
    DeleteClipRequested {
        clip_id: i64,
        path: Option<PathBuf>,
    },
    ClipDeleted {
        clip_id: i64,
        path: Option<PathBuf>,
        result: Result<(), String>,
    },
    RecentClipsLoaded(Result<Vec<ClipRecord>, String>),
    Clips(tabs::clips::Message),
    Stats(tabs::stats::Message),
    Characters(tabs::characters::Message),
    Rules(tabs::rules::Message),
    Settings(tabs::settings::Message),
    Updates(UpdateMessage),
    ToastDismiss(ToastId),
    ToastToggleExpand(ToastId),
}

#[derive(Debug, Clone)]
pub enum RuntimeMessage {
    StartMonitoring,
    StopMonitoring,
    CensusStream(StreamEvent),
    OnlineStatusChecked(Vec<u64>),
    HonuSessionResolved(Result<Option<i64>, String>),
    RuntimePoll,
    Tick,
    RecorderStartCompleted {
        id: u64,
    },
    MainWindowOpened(window::Id),
    MainWindowClosed(window::Id),
    WindowCloseRequested(window::Id),
    #[cfg(not(target_os = "windows"))]
    HotkeysConfigured {
        generation: u64,
        result: Result<Arc<Mutex<HotkeyManager>>, String>,
    },
}

#[derive(Debug, Clone)]
pub enum UpdateMessage {
    LaunchAtLoginSynced(Result<(), String>),
    CheckForUpdates {
        manual: bool,
    },
    UpdateCheckCompleted {
        manual: bool,
        result: Result<Option<update::AvailableRelease>, String>,
    },
    RefreshRollbackCatalog,
    RollbackCatalogLoaded(Result<Vec<update::AvailableRelease>, String>),
    DownloadSelectedRollbackVersion,
    RollbackToPreviousInstalledVersion,
    RollbackPreviousVersionResolved(Result<Option<update::AvailableRelease>, String>),
    UpdatePrimaryActionSelected(UpdatePrimaryAction),
    RunSelectedUpdateAction,
    InstallDownloadedUpdateWhenIdle,
    SystemUpdaterOpened(Result<(), String>),
    OpenUpdateReleaseNotes,
    UpdateReleaseNotesOpened(Result<(), String>),
    ShowUpdateDetails,
    HideUpdateDetails,
    UpdateDetailsLogLoaded(Result<String, String>),
}

impl Message {
    fn runtime(message: RuntimeMessage) -> Self {
        Self::Runtime(message)
    }

    fn updates(message: UpdateMessage) -> Self {
        Self::Updates(message)
    }
}

#[derive(Debug, Clone)]
pub(crate) enum BackgroundJobRetryPlan {
    StorageTieringSweep,
    StorageMove {
        clip_id: i64,
        target_tier: StorageTier,
    },
    Upload {
        clip_id: i64,
        provider: UploadProvider,
    },
    Montage {
        clip_ids: Vec<i64>,
    },
    DiscordWebhook {
        clip_id: i64,
        provider_label: String,
        clip_url: String,
    },
    PostProcess {
        clip_id: i64,
        path: PathBuf,
    },
    UpdateDownload,
}

impl App {
    const ERROR_MESSAGE_TIMEOUT: Duration = Duration::from_secs(9);
    const EXTENDED_MESSAGE_TIMEOUT: Duration = Duration::from_secs(15);

    pub fn new() -> (Self, Task<Message>) {
        let mut config = Config::load();
        let platform = PlatformServices::new();
        let secure_store = platform.secure_store().clone();
        let copyparty_password_present = secure_store
            .contains(SecretKey::CopypartyPassword)
            .unwrap_or(false);
        let youtube_client_secret_present = secure_store
            .contains(SecretKey::YoutubeClientSecret)
            .unwrap_or(false);
        let youtube_refresh_token_present = secure_store
            .contains(SecretKey::YoutubeRefreshToken)
            .unwrap_or(false);
        let discord_webhook_present = secure_store
            .contains(SecretKey::DiscordWebhookUrl)
            .unwrap_or(false);
        let obs_websocket_password = secure_store
            .get(SecretKey::ObsWebsocketPassword)
            .ok()
            .flatten();
        let obs_websocket_password_present = obs_websocket_password.is_some();
        config.recorder.obs_mut().websocket_password = obs_websocket_password;
        let initial_state = runtime::initial_runtime_state(&config);
        let initial_audio_sources = audio_source_drafts_from_config(&config.recorder.audio_sources);
        let initial_rules_feedback = config.migration_notice.clone();
        let event_log_retention_secs = clip_log_retention_secs(&config);
        let recorder = Recorder::new(config.capture.clone(), config.recorder.clone());
        let process_watcher = process::default_game_process_watcher();
        let ffmpeg_capabilities = post_process::probe_ffmpeg_capabilities();
        let notifications = platform.create_notification_center();
        let rule_engine = RuleEngine::new(
            config.rule_definitions.clone(),
            config.rule_profiles.clone(),
            config.active_profile_id.clone(),
        );
        let current_version = update::current_version();
        let current_version_label = current_version.to_string();
        let original_updates = config.updates.clone();
        let startup_version_changed =
            config.updates.current_version.as_deref() != Some(current_version_label.as_str());
        let update_state = hydrate_update_state_from_config(
            &mut config,
            update::detect_install_channel(),
            current_version,
        );

        let mut app = Self {
            view: View::Status,
            recorder,
            notifications,
            toasts: ToastStack::new(),
            rule_engine,
            process_watcher,
            event_log: EventLog::new(event_log_retention_secs),
            clip_store: None,
            clip_store_notice: None,
            platform,
            honu_client: HonuClient::new(),
            background_jobs: BackgroundJobManager::new(),
            new_character_name: String::new(),
            runtime: RuntimeState {
                lifecycle: initial_state,
                hotkeys: HotkeyManager::disabled(),
                tray: None,
                main_window_id: None,
                hotkey_config_generation: 0,
                next_clip_sequence: 0,
                pending_save_sequences: VecDeque::new(),
                pending_clip_links: BTreeMap::new(),
                status_feedback: None,
                status_feedback_expires_at: None,
                honu_session_id: None,
                active_clip_capture: None,
                tracked_alerts: BTreeMap::new(),
                manual_profile_override_profile_id: None,
                last_auto_switch_rule_id: None,
                active_session: None,
                last_session_summary: None,
                portal_capture_recovery_notified: false,
                startup_probe_due_at: None,
                startup_probe_pending_result: false,
                startup_probe_resolution: None,
                ffmpeg_capabilities,
                obs_connection_status: None,
                pending_recorder_start: None,
                next_recorder_start_id: 0,
                obs_restart_requires_manual_restart: false,
            },
            clips: ClipLibraryState {
                recent: Vec::new(),
                history_source: Vec::new(),
                history: Vec::new(),
                filter_options: ClipFilterOptions::default(),
                tag_editor_options: iced::widget::combo_box::State::new(Vec::new()),
                collection_editor_options: iced::widget::combo_box::State::new(Vec::new()),
                selected_collection_add_id: None,
                pending_collection_membership_clip_id: None,
                active_organization_editor: None,
                pending_organization_input_clear: None,
                tag_input: String::new(),
                new_collection_name: String::new(),
                new_collection_description: String::new(),
                selected_id: None,
                selected_detail: None,
                detail_loading: false,
                filters: ClipFilters::default(),
                query_revision: 0,
                sort_column: tabs::clips::ClipSortColumn::When,
                sort_descending: true,
                history_page: 0,
                history_page_size: tabs::clips::DEFAULT_PAGE_SIZE,
                history_viewport: None,
                advanced_filters_open: false,
                search_revision: 0,
                raw_event_filter: String::new(),
                collapsed_detail_sections: Vec::new(),
                pending_delete: None,
                deleting_id: None,
                error: None,
                error_expires_at: None,
                filter_feedback: None,
                filter_feedback_expires_at: None,
                montage_selection: Vec::new(),
                selected_montage_clip_id: None,
                montage_modal_open: false,
                date_range_preset: tabs::clips::DateRangePreset::AllTime,
                date_range_start: String::new(),
                date_range_end: String::new(),
                active_calendar: None,
                calendar_month: tabs::clips::today_local_date(),
            },
            stats: StatsState {
                snapshot: None,
                loading: false,
                error: None,
                revision: 0,
                time_range: tabs::stats::StatsTimeRange::default(),
                collapsed_sections: vec![tabs::stats::StatsSection::RawEventKinds],
                last_refreshed_at: None,
            },
            rules: RuleEditorState {
                vehicle_options: Vec::new(),
                vehicle_browse_categories: BTreeMap::new(),
                weapon_options: Vec::new(),
                weapon_browse_groups: BTreeMap::new(),
                weapon_browse_categories: BTreeMap::new(),
                weapon_browse_factions: BTreeMap::new(),
                filter_text_drafts: BTreeMap::new(),
                drag_state: None,
                feedback: initial_rules_feedback.clone(),
                feedback_expires_at: None,
                pending_profile_import: None,
                pending_profile_import_shake_started_at: None,
                pending_rule_import: None,
                pending_rule_import_shake_started_at: None,
                resolving_characters: BTreeSet::new(),
                resolving_lookups: BTreeSet::new(),
                selected_rule_id: None,
                sub_view: tabs::rules::RulesSubView::default(),
                expanded_events: HashSet::new(),
                expanded_filters: HashSet::new(),
            },
            settings: SettingsState {
                feedback: None,
                feedback_expires_at: None,
                sub_view: tabs::settings::SettingsSubView::default(),
                launch_at_login: config.launch_at_login.enabled,
                auto_start_monitoring: config.auto_start_monitoring,
                start_minimized: config.start_minimized,
                minimize_to_tray: config.minimize_to_tray,
                update_auto_check: config.updates.auto_check,
                update_channel: config.updates.channel,
                update_install_behavior: config.updates.install_behavior,
                selected_update_action: UpdatePrimaryAction::DownloadUpdate,
                selected_rollback_release: None,
                pending_hotkey_binding_label: None,
                pending_hotkey_success_toast: false,
                service_id: config.service_id.clone(),
                capture_backend: config.capture.backend,
                capture_source: config.recorder.gsr().capture_source.clone(),
                save_dir: config.recorder.save_directory.to_string_lossy().into(),
                framerate: config.recorder.gsr().framerate.to_string(),
                codec: config.recorder.gsr().codec.clone(),
                quality: config.recorder.gsr().quality.clone(),
                audio_sources: initial_audio_sources,
                discovered_audio_sources: Vec::new(),
                selected_device_audio_source: None,
                selected_application_audio_source: None,
                audio_discovery_running: false,
                audio_discovery_error: None,
                container: config.recorder.gsr().container.clone(),
                obs_websocket_url: config.recorder.obs().websocket_url.clone(),
                obs_password_input: String::new(),
                obs_password_present: obs_websocket_password_present,
                obs_management_mode: config.recorder.obs().management_mode,
                buffer_secs: config.recorder.replay_buffer_secs.to_string(),
                save_delay_secs: config.recorder.save_delay_secs.to_string(),
                clip_saved_notifications: config.recorder.clip_saved_notifications,
                clip_naming_template: config.clip_naming_template.clone(),
                manual_clip_enabled: config.manual_clip.enabled,
                manual_clip_hotkey: config.manual_clip.hotkey.clone(),
                hotkey_capture_active: false,
                manual_clip_duration_secs: config.manual_clip.duration_secs.to_string(),
                storage_tiering_enabled: config.storage_tiering.enabled,
                storage_tier_directory: config
                    .storage_tiering
                    .tier_directory
                    .to_string_lossy()
                    .into(),
                storage_min_age_days: config.storage_tiering.min_age_days.to_string(),
                storage_max_score: config.storage_tiering.max_score.to_string(),
                copyparty_enabled: config.uploads.copyparty.enabled,
                copyparty_upload_url: config.uploads.copyparty.upload_url.clone(),
                copyparty_public_base_url: config.uploads.copyparty.public_base_url.clone(),
                copyparty_username: config.uploads.copyparty.username.clone(),
                copyparty_password_input: String::new(),
                copyparty_password_present,
                youtube_enabled: config.uploads.youtube.enabled,
                youtube_client_id: config.uploads.youtube.client_id.clone(),
                youtube_client_secret_input: String::new(),
                youtube_client_secret_present,
                youtube_refresh_token_present,
                youtube_oauth_in_flight: false,
                youtube_privacy_status: config.uploads.youtube.privacy_status,
                discord_enabled: config.discord_webhook.enabled,
                discord_min_score: config.discord_webhook.min_score.to_string(),
                discord_include_thumbnail: config.discord_webhook.include_thumbnail,
                discord_webhook_input: String::new(),
                discord_webhook_present,
                secure_store_backend_label: secure_store.backend().label().into(),
            },
            updates: UpdateUiState {
                state: update_state,
                details_modal_open: false,
                details_log_text: None,
                details_log_error: None,
                details_log_loading: false,
            },
            config,
        };

        if let Some(notice) = initial_rules_feedback.as_ref() {
            app.push_feedback_toast("Rules", notice.clone(), false);
        }
        if (startup_version_changed || app.config.updates != original_updates)
            && let Err(error) = app.config.save()
        {
            tracing::warn!("Failed to persist updater startup state: {error}");
        }
        app.recover_apply_result();

        tabs::rules::ensure_selection(&mut app);
        let capabilities = app.platform.capabilities();
        if capabilities.tray {
            let initial_tray_snapshot = app.tray_snapshot();
            match app.platform.spawn_tray(initial_tray_snapshot) {
                Ok(tray) => {
                    app.runtime.tray = Some(tray);
                }
                Err(error) => {
                    app.push_feedback_toast("Tray", error, true);
                }
            }
        }
        let initial_window_task = if app.config.start_minimized {
            Task::none()
        } else {
            app.open_main_window_task()
        };

        let task = Task::batch([
            Task::perform(async { ClipStore::open_default().await }, |result| {
                Message::DatabaseReady(result.map_err(|e| e.to_string()))
            }),
            app.configure_hotkeys(false),
            app.resolve_unresolved_characters(),
            initial_window_task,
            app.maybe_resume_staged_update_task(),
            app.maybe_auto_check_for_updates_task(),
        ]);

        (app, task)
    }

    pub fn title(&self) -> String {
        "NaniteClip".into()
    }

    pub fn theme(&self) -> Theme {
        crate::ui::theme::Preset::Nanite.iced_theme(crate::ui::theme::Mode::Dark)
    }

    fn maybe_auto_check_for_updates_task(&mut self) -> Task<Message> {
        if self.should_auto_check_for_updates() {
            self.updates.state.checking = true;
            self.updates.state.phase = UpdatePhase::Checking;
            self.updates.state.progress = Some(UpdateProgressState {
                detail: "Checking GitHub Releases for a newer version.".into(),
            });
            self.check_for_updates_task(false)
        } else {
            Task::none()
        }
    }

    fn maybe_resume_staged_update_task(&mut self) -> Task<Message> {
        if self.updates.state.has_downloaded_update() {
            let prepared = self
                .updates
                .state
                .prepared_update
                .as_ref()
                .expect("checked prepared update presence");
            let prepared_version = prepared
                .parsed_version()
                .unwrap_or_else(|| self.updates.state.current_version.clone());
            self.updates.state.phase = UpdatePhase::ReadyToInstall;
            self.updates.state.progress = None;
            self.push_toast(
                ToastTone::Info,
                release_action_title(&prepared_version, &self.updates.state.current_version),
                format!(
                    "NaniteClip {} is staged and ready to install.",
                    prepared.version
                ),
                true,
            );
            if self.should_auto_apply_staged_update() {
                return Task::done(Message::updates(
                    UpdateMessage::InstallDownloadedUpdateWhenIdle,
                ));
            }
        }
        Task::none()
    }

    fn recover_apply_result(&mut self) {
        match update::helper::take_apply_result() {
            Ok(Some(result)) => {
                let report = update_apply_report_from_helper_result(&result);
                self.updates.state.last_apply_report = Some(report.clone());
                self.config.updates.last_apply_report = Some(report);
                if let Err(error) = self.config.save() {
                    tracing::warn!("Failed to persist updater apply result: {error}");
                }

                match result.status {
                    update::helper_shared::ApplyResultStatus::Succeeded => {
                        tracing::info!(
                            target_version = result.target_version,
                            log_path = %result.log_path.display(),
                            finished_at = %result.finished_at,
                            "recovered successful updater apply result"
                        );
                    }
                    update::helper_shared::ApplyResultStatus::Failed => {
                        let detail = result.detail.unwrap_or_else(|| {
                            format!(
                                "The updater could not install NaniteClip {}.",
                                result.target_version
                            )
                        });
                        let message =
                            format!("{detail} See updater log at {}.", result.log_path.display());
                        self.set_update_error(UpdateErrorKind::Install, message.clone());
                        self.set_status_feedback_silent(message.clone(), false);
                        self.push_toast(ToastTone::Error, "Update Failed", message, true);
                    }
                }
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!("Failed to recover updater apply result: {error}");
            }
        }
    }

    fn show_update_details(&mut self) -> Task<Message> {
        self.updates.details_modal_open = true;
        self.updates.details_log_text = None;
        self.updates.details_log_error = None;
        self.updates.details_log_loading = false;

        let Some(log_path) = self
            .updates
            .state
            .last_apply_report
            .as_ref()
            .map(|report| report.log_path.clone())
            .filter(|path| path.exists())
        else {
            return Task::none();
        };

        self.updates.details_log_loading = true;
        Task::perform(
            async move {
                tokio::fs::read_to_string(&log_path).await.map_err(|error| {
                    format!("failed to read updater log {}: {error}", log_path.display())
                })
            },
            |result| Message::updates(UpdateMessage::UpdateDetailsLogLoaded(result)),
        )
    }

    fn active_update_release_url(&self) -> Option<String> {
        self.selected_rollback_release()
            .map(|release| release.html_url)
            .or_else(|| {
                self.updates
                    .state
                    .latest_release
                    .as_ref()
                    .map(|release| release.html_url.clone())
            })
            .or_else(|| {
                self.updates
                    .state
                    .prepared_update
                    .as_ref()
                    .map(|prepared| prepared.release_notes_url.clone())
            })
    }

    fn should_auto_check_for_updates(&self) -> bool {
        if !self.config.updates.auto_check || self.updates.state.checking {
            return false;
        }

        self.config
            .updates
            .last_check_utc
            .is_none_or(|last_checked| Utc::now() - last_checked >= chrono::Duration::hours(12))
    }

    fn can_apply_update_now(&self) -> bool {
        !matches!(self.runtime.lifecycle, AppState::Monitoring { .. })
            && !self.recorder.has_active_session()
    }

    fn should_auto_apply_staged_update(&self) -> bool {
        self.config.updates.install_behavior == UpdateInstallBehavior::WhenIdle
            && self.can_apply_update_now()
            && self.updates.state.has_downloaded_update()
            && matches!(self.updates.state.phase, UpdatePhase::ReadyToInstall)
    }

    fn selected_rollback_release(&self) -> Option<update::AvailableRelease> {
        self.settings
            .selected_rollback_release
            .clone()
            .or_else(|| self.updates.state.rollback_candidates.first().cloned())
    }

    fn run_selected_update_action(&mut self) -> Task<Message> {
        self.run_update_action(selected_update_action(self))
    }

    fn run_update_action(&mut self, action: UpdatePrimaryAction) -> Task<Message> {
        match action {
            UpdatePrimaryAction::DownloadUpdate => self.queue_update_download(),
            UpdatePrimaryAction::InstallAndRestart => self.apply_prepared_update_now(false),
            UpdatePrimaryAction::InstallWhenIdle => self.schedule_downloaded_update_when_idle(),
            UpdatePrimaryAction::InstallOnNextLaunch => {
                self.schedule_downloaded_update_on_next_launch()
            }
            UpdatePrimaryAction::OpenSystemUpdater => self.open_system_updater(),
            UpdatePrimaryAction::RemindLater => self.remind_update_later(),
            UpdatePrimaryAction::SkipThisVersion => self.skip_available_update(),
        }
    }

    fn open_system_updater(&mut self) -> Task<Message> {
        let Some(plan) = self.updates.state.system_update_plan.clone() else {
            self.set_status_feedback_silent(
                "No system updater handoff is available for this install.",
                false,
            );
            return Task::none();
        };
        let Some(program) = plan.command_program.clone() else {
            self.set_status_feedback_silent(plan.command_display.unwrap_or(plan.detail), false);
            return Task::none();
        };
        let args = plan.command_args.clone();
        let command_display = plan
            .command_display
            .clone()
            .unwrap_or_else(|| plan.label.clone());
        let platform = self.platform.clone();
        Task::perform(
            async move { platform.launch_command(&program, &args, &command_display) },
            |result| Message::updates(UpdateMessage::SystemUpdaterOpened(result)),
        )
    }

    fn schedule_downloaded_update_when_idle(&mut self) -> Task<Message> {
        if !self.updates.state.has_downloaded_update() {
            self.set_status_feedback_silent(
                "Download an update before scheduling its install.",
                false,
            );
            return Task::none();
        }
        self.config.updates.install_behavior = UpdateInstallBehavior::WhenIdle;
        self.settings.update_install_behavior = UpdateInstallBehavior::WhenIdle;
        self.persist_update_config();
        if self.can_apply_update_now() {
            self.apply_prepared_update_now(true)
        } else {
            if let Some(prepared) = self.updates.state.prepared_update.as_ref() {
                self.set_status_feedback_silent(
                    format!(
                        "NaniteClip {} will install automatically when monitoring is idle.",
                        prepared.version
                    ),
                    true,
                );
            }
            Task::none()
        }
    }

    fn schedule_downloaded_update_on_next_launch(&mut self) -> Task<Message> {
        if !self.updates.state.has_downloaded_update() {
            self.set_status_feedback_silent(
                "Download an update before scheduling its install.",
                false,
            );
            return Task::none();
        }
        self.config.updates.install_behavior = UpdateInstallBehavior::OnNextLaunch;
        self.settings.update_install_behavior = UpdateInstallBehavior::OnNextLaunch;
        self.persist_update_config();
        if let Some(prepared) = self.updates.state.prepared_update.as_ref() {
            self.set_status_feedback_silent(
                format!(
                    "NaniteClip {} is staged and will be ready on the next launch.",
                    prepared.version
                ),
                true,
            );
        }
        Task::none()
    }

    fn remind_update_later(&mut self) -> Task<Message> {
        let Some(release) = self.updates.state.latest_release.as_ref() else {
            return Task::none();
        };
        let version = release.version.to_string();
        let remind_until = Utc::now() + chrono::Duration::hours(12);
        self.config.updates.remind_later_version = Some(version.clone());
        self.config.updates.remind_later_until_utc = Some(remind_until);
        self.persist_update_config();
        self.set_status_feedback_silent(
            format!(
                "Will remind you about {} again after {}.",
                version,
                tabs::clips::format_timestamp(remind_until)
            ),
            true,
        );
        Task::none()
    }

    fn skip_available_update(&mut self) -> Task<Message> {
        let Some(release) = self.updates.state.latest_release.as_ref() else {
            return Task::none();
        };
        self.config.updates.skipped_version = Some(release.version.to_string());
        self.config.updates.remind_later_version = None;
        self.config.updates.remind_later_until_utc = None;
        if let Err(error) = self.config.save() {
            self.set_status_feedback_silent(
                format!(
                    "Skipped {}, but failed to save the preference: {error}",
                    release.version
                ),
                false,
            );
        } else {
            self.set_status_feedback_silent(
                format!("Skipped update {} for this install.", release.version),
                true,
            );
        }
        if let Some(release) = self.updates.state.latest_release.as_mut() {
            release.skipped = true;
        }
        Task::none()
    }

    fn persist_update_config(&mut self) {
        if let Err(error) = self.config.save() {
            tracing::warn!("Failed to persist updater settings: {error}");
        }
    }

    fn is_update_reminder_deferred(&self, version: &str) -> bool {
        self.config.updates.remind_later_version.as_deref() == Some(version)
            && self
                .config
                .updates
                .remind_later_until_utc
                .is_some_and(|until| until > Utc::now())
    }

    fn set_update_error(&mut self, kind: UpdateErrorKind, detail: impl Into<String>) {
        self.updates.state.phase = UpdatePhase::Failed;
        self.updates.state.progress = None;
        self.updates.state.last_error = Some(UpdateErrorState {
            kind,
            detail: detail.into(),
        });
    }

    fn apply_prepared_update_now(&mut self, _automatic: bool) -> Task<Message> {
        if matches!(
            self.updates.state.phase,
            UpdatePhase::Downloading | UpdatePhase::Verifying | UpdatePhase::Applying
        ) {
            self.set_status_feedback_silent(
                "Wait for the current updater operation to finish before installing.",
                false,
            );
            return Task::none();
        }
        if !self.can_apply_update_now() {
            self.set_status_feedback_silent("Stop monitoring before applying an update.", false);
            return Task::none();
        }

        let Some(prepared) = self.updates.state.prepared_update.clone() else {
            self.set_status_feedback_silent("No downloaded update is ready to install.", false);
            return Task::none();
        };

        self.updates.state.phase = UpdatePhase::Applying;
        self.updates.state.progress = Some(UpdateProgressState {
            detail: format!("Launching the updater helper for {}.", prepared.version),
        });
        self.updates.state.last_error = None;
        self.persist_update_config();
        self.push_toast(
            ToastTone::Info,
            release_action_title(
                &prepared
                    .parsed_version()
                    .unwrap_or_else(|| self.updates.state.current_version.clone()),
                &self.updates.state.current_version,
            ),
            format!("NaniteClip {} is installing.", prepared.version),
            true,
        );

        match update::helper::spawn_apply_helper(&prepared) {
            Ok(()) => iced::exit(),
            Err(error) => {
                let message = format!("Failed to launch the updater helper: {error}");
                self.set_update_error(UpdateErrorKind::Install, message.clone());
                self.push_toast(ToastTone::Error, "Update Failed", message.clone(), true);
                self.set_status_feedback_silent(message, false);
                Task::none()
            }
        }
    }

    fn check_for_updates_task(&self, manual: bool) -> Task<Message> {
        let channel = if manual {
            self.settings.update_channel
        } else {
            self.config.updates.channel
        };
        let install_channel = self.updates.state.install_channel;
        let current_version = self.updates.state.current_version.clone();
        let install_id = self.config.updates.install_id.clone();
        let skipped_version = self.config.updates.skipped_version.clone();
        Task::perform(
            async move {
                update::fetch_available_release(
                    channel,
                    install_channel,
                    &current_version,
                    install_id.as_deref(),
                    skipped_version.as_deref(),
                )
                .await
            },
            move |result| Message::updates(UpdateMessage::UpdateCheckCompleted { manual, result }),
        )
    }

    fn refresh_rollback_catalog_task(&self) -> Task<Message> {
        let channel = self.settings.update_channel;
        let install_channel = self.updates.state.install_channel;
        let current_version = self.updates.state.current_version.clone();
        Task::perform(
            async move {
                update::fetch_rollback_candidates(channel, install_channel, &current_version).await
            },
            |result| Message::updates(UpdateMessage::RollbackCatalogLoaded(result)),
        )
    }

    fn lookup_previous_installed_rollback_task(&self) -> Task<Message> {
        let Some(previous_version) = self.updates.state.previous_installed_version.clone() else {
            return Task::done(Message::updates(
                UpdateMessage::RollbackPreviousVersionResolved(Ok(None)),
            ));
        };
        let channel = self.settings.update_channel;
        let install_channel = self.updates.state.install_channel;
        Task::perform(
            async move {
                update::fetch_release_by_version(channel, install_channel, &previous_version).await
            },
            |result| Message::updates(UpdateMessage::RollbackPreviousVersionResolved(result)),
        )
    }

    pub fn subscription(&self) -> Subscription<Message> {
        subscriptions::build(self)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SwitchView(view) => {
                self.settings.hotkey_capture_active = false;
                self.view = view;
                if matches!(self.view, View::Settings) {
                    let mut tasks = vec![tabs::settings::refresh_audio_sources(self)];
                    if self.updates.state.rollback_candidates.is_empty()
                        || self.settings.update_channel != self.config.updates.channel
                    {
                        tasks.push(Task::done(Message::updates(
                            UpdateMessage::RefreshRollbackCatalog,
                        )));
                    }
                    Task::batch(tasks)
                } else if matches!(self.view, View::Stats) {
                    self.load_stats()
                } else if matches!(self.view, View::Clips) {
                    self.load_clip_filter_options()
                } else if matches!(self.view, View::Rules) {
                    tabs::rules::load_reference_data(self)
                } else {
                    Task::none()
                }
            }

            Message::Runtime(message) => self.handle_runtime_message(message),

            Message::Updates(message) => self.handle_update_message(message),

            Message::ToastDismiss(id) => {
                self.toasts.dismiss(id);
                Task::none()
            }

            Message::ToastToggleExpand(id) => {
                self.toasts.toggle_expand(id);
                Task::none()
            }

            Message::DatabaseReady(result) => match result {
                Ok(store) => {
                    self.clip_store_notice = store.startup_notice().map(str::to_owned);
                    if let Some(notice) = self.clip_store_notice.clone() {
                        self.push_feedback_toast("Database", notice, false);
                    }
                    self.clip_store = Some(store);
                    self.clear_clip_error();
                    Task::batch([
                        self.sweep_interrupted_post_process_clips(),
                        self.load_clip_filter_options(),
                        tabs::rules::load_reference_data(self),
                        self.recover_background_jobs(),
                    ])
                }
                Err(error) => {
                    self.clip_store_notice = None;
                    self.set_clip_error(error.clone());
                    tracing::error!("Failed to initialize clip database: {error}");
                    Task::none()
                }
            },

            Message::PostProcessRecoveryCompleted(result) => match result {
                Ok(recovered_clip_ids) => {
                    if !recovered_clip_ids.is_empty() {
                        self.set_status_feedback(
                            format!(
                                "Marked {} interrupted audio post-process clip(s) as failed.",
                                recovered_clip_ids.len()
                            ),
                            false,
                        );
                    }

                    let detail = if self.clips.selected_id.is_some() {
                        self.load_clip_detail(self.clips.selected_id)
                    } else {
                        Task::none()
                    };

                    Task::batch([tabs::clips::reload_views(self), detail])
                }
                Err(error) => {
                    self.set_status_feedback(
                        format!("Failed to recover interrupted audio post-process jobs: {error}"),
                        false,
                    );
                    tabs::clips::reload_views(self)
                }
            },

            Message::BackgroundJobsRecovered(result) => {
                match result {
                    Ok(records) => self.background_jobs.replace_recent_jobs(records),
                    Err(error) => {
                        tracing::warn!("Failed to recover persisted background jobs: {error}");
                    }
                }
                Task::none()
            }

            Message::BackgroundJobStored(result) => {
                if let Err(error) = result {
                    tracing::warn!("Failed to persist background job state: {error}");
                }
                Task::none()
            }

            Message::RuleClipAction(action) => self.handle_rule_clip_action(action),

            Message::ClipPersisted { sequence, result } => match result {
                Ok(clip_id) => {
                    self.clear_clip_error();
                    self.runtime
                        .pending_clip_links
                        .entry(sequence)
                        .or_default()
                        .clip_id = Some(clip_id);
                    let linked = self.resolve_pending_clip_links();
                    let detail = if self.clips.selected_id == Some(clip_id) {
                        self.load_clip_detail(Some(clip_id))
                    } else {
                        Task::none()
                    };
                    let stats = if matches!(self.view, View::Stats) {
                        self.load_stats()
                    } else {
                        Task::none()
                    };
                    Task::batch([
                        tabs::clips::reload_views(self),
                        self.load_clip_filter_options(),
                        stats,
                        linked,
                        detail,
                    ])
                }
                Err(error) => {
                    self.set_clip_error(error.clone());
                    tracing::error!("Failed to persist clip: {error}");
                    self.runtime
                        .pending_clip_links
                        .entry(sequence)
                        .or_default()
                        .persist_failed = true;
                    self.resolve_pending_clip_links()
                }
            },

            Message::ClipPathLinked {
                clip_id,
                path,
                trim,
                audio_layout,
                result,
            } => match result {
                Ok(()) => {
                    self.clear_clip_error();
                    self.update_clip_path_in_memory(clip_id, Some(&path));
                    Task::batch([
                        self.inspect_saved_clip_resolution(path.clone()),
                        self.queue_post_process_for_clip(
                            clip_id,
                            PathBuf::from(path),
                            trim,
                            audio_layout,
                        ),
                    ])
                }
                Err(error) => {
                    self.set_clip_error(error.clone());
                    tracing::error!("Failed to store clip path for clip #{clip_id}: {error}");
                    Task::none()
                }
            },

            Message::ClipResolutionInspected { path, result } => {
                match result {
                    Ok(Some(resolution)) => {
                        if self.should_reset_portal_capture_after_clip(resolution) {
                            self.runtime.portal_capture_recovery_notified = true;
                            let clear_result = crate::recorder::clear_portal_session_token();
                            let next_step = if matches!(
                                self.runtime.lifecycle,
                                AppState::WaitingForGame
                                    | AppState::WaitingForLogin
                                    | AppState::Monitoring { .. }
                            ) {
                                "Stop and start monitoring to pick the correct portal source again."
                            } else {
                                "Start monitoring again to pick the correct portal source."
                            };
                            let token_detail = match clear_result {
                                Ok(true) => {
                                    " Cleared the saved portal selection for the next recorder start."
                                }
                                Ok(false) => " The saved portal selection was already clear.",
                                Err(error) => {
                                    warn!(error = %error, path = %path, "Failed to clear portal token after suspicious clip resolution");
                                    ""
                                }
                            };
                            self.set_status_feedback(
                                format!(
                                    "Saved clip resolution {}x{} looks much lower than expected for fullscreen portal capture. This can happen if the portal latched onto the launcher window before the game expanded.{token_detail} {next_step}",
                                    resolution.width, resolution.height
                                ),
                                false,
                            );
                        }
                    }
                    Ok(None) => {
                        tracing::debug!("ffprobe returned no video stream for saved clip {path}");
                    }
                    Err(error) => {
                        tracing::debug!(
                            "Failed to inspect saved clip resolution for {path}: {error}"
                        );
                    }
                }
                Task::none()
            }

            Message::StartupProbeCompleted {
                path,
                result,
                delete_result,
            } => {
                if let Err(error) = delete_result {
                    tracing::warn!(
                        "Failed to delete startup recorder probe clip {}: {error}",
                        path.display()
                    );
                }

                match result {
                    Ok(Some(resolution)) => {
                        self.runtime.startup_probe_resolution = Some(resolution);
                        tracing::info!(
                            "Startup recorder probe resolved to {}x{}",
                            resolution.width,
                            resolution.height
                        );
                        if self.should_reset_portal_capture_after_clip(resolution) {
                            self.runtime.portal_capture_recovery_notified = true;
                            let clear_result = crate::recorder::clear_portal_session_token();
                            let token_detail = match clear_result {
                                Ok(true) => {
                                    " Cleared the saved portal selection for the next recorder start."
                                }
                                Ok(false) => " The saved portal selection was already clear.",
                                Err(error) => {
                                    warn!(error = %error, "Failed to clear portal token after startup recorder probe");
                                    ""
                                }
                            };
                            self.set_status_feedback(
                                format!(
                                    "Startup recorder probe detected a suspicious portal capture size of {}x{}. This usually means the portal latched onto the launcher window before the game expanded.{token_detail} Stop and start monitoring to pick the correct portal source again.",
                                    resolution.width, resolution.height
                                ),
                                false,
                            );
                        }
                    }
                    Ok(None) => {
                        self.runtime.startup_probe_resolution = None;
                        tracing::debug!(
                            "Startup recorder probe found no video stream in {}",
                            path.display()
                        );
                    }
                    Err(error) => {
                        self.runtime.startup_probe_resolution = None;
                        tracing::debug!(
                            "Failed to inspect startup recorder probe {}: {error}",
                            path.display()
                        );
                    }
                }

                Task::none()
            }

            Message::RunStorageTieringSweep => self.queue_storage_tiering_sweep(),

            Message::MoveClipToTier {
                clip_id,
                target_tier,
            } => self.queue_clip_storage_move(clip_id, target_tier),

            Message::UploadClipRequested { clip_id, provider } => {
                self.queue_clip_upload(clip_id, provider)
            }

            Message::CreateMontageRequested => self.queue_montage_creation(),

            Message::CancelBackgroundJob(job_id) => {
                if self.background_jobs.cancel(job_id) {
                    self.set_status_feedback(format!("Requested cancellation for {job_id}."), true);
                    self.persist_background_job_snapshot(job_id)
                } else {
                    Task::none()
                }
            }

            Message::RetryBackgroundJob(job_id) => self.retry_background_job(job_id),

            Message::RemoveBackgroundJob(job_id) => self.remove_background_job(job_id),

            Message::BackgroundJobRetryPrepared { job_id, result } => match result {
                Ok(plan) => {
                    self.set_status_feedback(format!("Retrying {job_id}."), true);
                    self.execute_background_job_retry(plan)
                }
                Err(error) => {
                    self.set_status_feedback(format!("Could not retry {job_id}: {error}"), false);
                    Task::none()
                }
            },

            Message::BackgroundJobRemoved { job_id, result } => {
                match result {
                    Ok(()) => self.set_status_feedback(format!("Removed {job_id}."), true),
                    Err(error) => self.set_status_feedback(
                        format!("Removed {job_id} from the UI, but failed to delete it from the clip database: {error}"),
                        false,
                    ),
                }
                Task::none()
            }

            Message::ClipPostProcessBypassed { clip_id, result } => match result {
                Ok(()) => {
                    self.set_clip_filter_feedback(
                        format!("Clip #{clip_id} is now using the original captured audio layout."),
                        false,
                    );
                    let detail = if self.clips.selected_id == Some(clip_id) {
                        self.load_clip_detail(Some(clip_id))
                    } else {
                        Task::none()
                    };
                    Task::batch([tabs::clips::reload_views(self), detail])
                }
                Err(error) => {
                    self.set_clip_error(error);
                    Task::none()
                }
            },

            Message::YouTubeOAuthCompleted(result) => {
                self.settings.youtube_oauth_in_flight = false;
                match result {
                    Ok(()) => {
                        info!("YouTube OAuth flow completed and refresh token is marked as stored");
                        if !self.settings.youtube_client_secret_input.trim().is_empty() {
                            self.settings.youtube_client_secret_present = true;
                            self.settings.youtube_client_secret_input.clear();
                        }
                        self.settings.youtube_refresh_token_present = true;
                        self.set_settings_feedback(
                            "YouTube account connected. Future uploads will use the stored refresh token.",
                            false,
                        );
                    }
                    Err(error) => {
                        warn!(error = %error, "YouTube OAuth flow failed before completion");
                        self.set_settings_feedback(error, false)
                    }
                }
                Task::none()
            }

            Message::PostUploadDiscordClipLoaded(result) => match result {
                Ok(Some(payload)) => self.start_discord_webhook_for_uploaded_clip(payload),
                Ok(None) => {
                    tracing::warn!(
                        "Skipping Discord webhook because the uploaded clip could not be reloaded from the database"
                    );
                    Task::none()
                }
                Err(error) => {
                    tracing::warn!("Failed to prepare Discord webhook after upload: {error}");
                    Task::none()
                }
            },

            Message::OpenClipRequested(path) => {
                let open_path = path.clone();
                let platform = self.platform.clone();
                Task::perform(
                    async move { platform.open_path(&open_path) },
                    move |result| Message::ClipOpenFinished { path, result },
                )
            }

            Message::ClipOpenFinished { path, result } => match result {
                Ok(()) => {
                    self.clear_clip_error();
                    Task::none()
                }
                Err(error) => {
                    self.set_clip_error(error.clone());
                    tracing::error!("Failed to open clip {}: {error}", path.display());
                    Task::none()
                }
            },

            Message::DeleteClipRequested { clip_id, path } => {
                let Some(store) = self.clip_store.clone() else {
                    self.set_clip_error(
                        "Clip database is unavailable, so the clip cannot be deleted.",
                    );
                    return Task::none();
                };

                self.clips.deleting_id = Some(clip_id);
                self.clear_clip_error();

                let result_path = path.clone();
                Task::perform(
                    async move { delete_clip_file_and_unlink(store, clip_id, path.as_deref()).await },
                    move |result| Message::ClipDeleted {
                        clip_id,
                        path: result_path,
                        result,
                    },
                )
            }

            Message::ClipDeleted {
                clip_id,
                path,
                result,
            } => {
                if self.clips.deleting_id == Some(clip_id) {
                    self.clips.deleting_id = None;
                }

                match result {
                    Ok(()) => {
                        self.clear_clip_error();
                        self.remove_clip_from_memory(clip_id);
                        let feedback = match &path {
                            Some(path) => {
                                format!(
                                    "Deleted clip {} and removed its saved file.",
                                    path.display()
                                )
                            }
                            None => format!("Deleted clip #{clip_id} from history."),
                        };
                        self.set_clip_filter_feedback(feedback, true);
                        return self.load_clip_filter_options();
                    }
                    Err(error) => {
                        self.set_clip_error(error.clone());
                        match &path {
                            Some(path) => {
                                tracing::error!("Failed to delete clip {}: {error}", path.display())
                            }
                            None => tracing::error!("Failed to delete clip #{clip_id}: {error}"),
                        }
                    }
                }

                Task::none()
            }

            Message::RecentClipsLoaded(result) => {
                match result {
                    Ok(clips) => {
                        self.clips.recent = clips;
                        self.clear_clip_error();
                        let lookup_clips = self.clips.recent.clone();
                        return Task::batch([
                            self.schedule_clip_record_lookup_resolutions(&lookup_clips),
                            Task::none(),
                        ]);
                    }
                    Err(error) => {
                        self.set_clip_error(error.clone());
                        tracing::error!("Failed to load recent clips: {error}");
                    }
                }
                Task::none()
            }

            Message::Clips(msg) => tabs::clips::update(self, msg),

            Message::Stats(msg) => tabs::stats::update(self, msg),

            Message::Characters(msg) => tabs::characters::update(self, msg),

            Message::Rules(msg) => tabs::rules::update(self, msg),

            Message::Settings(msg) => tabs::settings::update(self, msg),

            Message::RequestManualClipSave => self.request_manual_clip_save(),

            Message::ClipDetailLoaded(result) => {
                self.clips.detail_loading = false;
                match result {
                    Ok(detail) => {
                        self.clips.selected_detail = detail;
                        self.clear_clip_error();
                        if let Some(detail) = self.clips.selected_detail.clone() {
                            return self.schedule_clip_detail_lookup_resolutions(&detail);
                        }
                    }
                    Err(error) => {
                        self.clips.selected_detail = None;
                        self.set_clip_error(error.clone());
                        tracing::error!("Failed to load clip detail: {error}");
                    }
                }
                Task::none()
            }

            Message::ClipFilterOptionsLoaded(result) => {
                match result {
                    Ok(options) => {
                        self.clips.filter_options.targets = options.targets;
                        self.clips.filter_options.weapons = options.weapons;
                        self.clips.filter_options.alerts = options.alerts;
                        self.clips.filter_options.tags = options.tags;
                        self.clips.filter_options.collections = options.collections;
                        tabs::clips::sync_editor_options(self);
                    }
                    Err(error) => {
                        tracing::warn!("Failed to load clip filter options: {error}");
                    }
                }
                Task::none()
            }

            Message::StatsLoaded { revision, result } => {
                if revision != self.stats.revision {
                    return Task::none();
                }

                self.stats.loading = false;
                match result {
                    Ok(snapshot) => {
                        self.stats.error = None;
                        self.stats.snapshot = Some(snapshot);
                        self.stats.last_refreshed_at = Some(Instant::now());
                    }
                    Err(error) => {
                        self.stats.error = Some(error.clone());
                        self.push_toast(ToastTone::Error, "Stats", error.clone(), true);
                        tracing::error!("Failed to load stats: {error}");
                    }
                }
                Task::none()
            }

            Message::SessionSummaryLoaded { session_id, result } => {
                match result {
                    Ok(summary) => {
                        if summary.session_id == session_id {
                            self.runtime.last_session_summary = Some(summary);
                        }
                    }
                    Err(error) => {
                        tracing::warn!("Failed to load session summary {session_id}: {error}");
                    }
                }
                Task::none()
            }

            Message::ExportLastSessionSummary => self.export_last_session_summary_markdown(),

            Message::SessionSummaryExported(result) => {
                match result {
                    Ok(path) => self.set_status_feedback(
                        format!("Exported session summary markdown to {path}"),
                        false,
                    ),
                    Err(error) => self.set_status_feedback(error, false),
                }
                Task::none()
            }

            Message::StatsExported(result) => {
                match result {
                    Ok(path) => {
                        self.push_toast(
                            ToastTone::Success,
                            "Stats",
                            format!("Exported stats to {path}"),
                            true,
                        );
                    }
                    Err(error) => {
                        self.push_toast(ToastTone::Error, "Stats", error, true);
                    }
                }
                Task::none()
            }

            Message::TimelineArtifactExported { kind, result } => {
                match result {
                    Ok(path) => self.set_clip_filter_feedback(
                        format!("Exported {} to {path}", kind.label()),
                        false,
                    ),
                    Err(error) => self.set_clip_error(error),
                }
                Task::none()
            }

            Message::LookupResolved {
                kind,
                lookup_id,
                refreshed,
                result,
            } => {
                self.rules.resolving_lookups.remove(&(kind, lookup_id));
                match result {
                    Ok(()) if refreshed => {
                        let mut tasks = vec![
                            tabs::clips::reload_views(self),
                            self.load_clip_filter_options(),
                        ];
                        if matches!(self.view, View::Stats) {
                            tasks.push(self.load_stats());
                        }
                        if self.clips.selected_id.is_some() {
                            tasks.push(self.load_clip_detail(self.clips.selected_id));
                        }
                        Task::batch(tasks)
                    }
                    Ok(()) => Task::none(),
                    Err(error) => {
                        tracing::warn!(
                            "Lookup resolution failed for {:?} #{lookup_id}: {error}",
                            kind
                        );
                        Task::none()
                    }
                }
            }

            Message::ResolvedRuleEngineEvent(event) => {
                self.ingest_resolved_rule_engine_event(event)
            }

            Message::AlertStored { alert_key, result } => {
                if let Err(error) = result {
                    tracing::warn!("Failed to persist alert {alert_key}: {error}");
                }
                Task::none()
            }
        }
    }

    fn handle_update_message(&mut self, message: UpdateMessage) -> Task<Message> {
        match message {
            UpdateMessage::LaunchAtLoginSynced(result) => {
                if let Err(error) = result {
                    self.set_settings_feedback(
                        format!(
                            "Settings were saved, but launch-at-login could not be updated: {error}"
                        ),
                        false,
                    );
                }
                Task::none()
            }
            UpdateMessage::CheckForUpdates { manual } => {
                if self.updates.state.checking {
                    return Task::none();
                }
                self.updates.state.checking = true;
                self.updates.state.phase = UpdatePhase::Checking;
                self.updates.state.progress = Some(UpdateProgressState {
                    detail: "Checking GitHub Releases for a newer version.".into(),
                });
                self.updates.state.last_error = None;
                self.check_for_updates_task(manual)
            }
            UpdateMessage::RefreshRollbackCatalog => {
                if self.updates.state.rollback_catalog_loading {
                    return Task::none();
                }
                self.updates.state.rollback_catalog_loading = true;
                self.refresh_rollback_catalog_task()
            }
            UpdateMessage::UpdateCheckCompleted { manual, result } => {
                self.updates.state.checking = false;
                self.updates.state.progress = None;
                let checked_at = Utc::now();
                self.updates.state.last_checked_at = Some(checked_at);
                self.config.updates.last_check_utc = Some(checked_at);

                match result {
                    Ok(latest_release) => {
                        self.updates.state.latest_release = latest_release;
                        self.updates.state.last_error = None;
                        self.updates.state.phase = if self.updates.state.has_downloaded_update() {
                            UpdatePhase::ReadyToInstall
                        } else {
                            UpdatePhase::Idle
                        };

                        if let Some(release) = &self.updates.state.latest_release {
                            let release_version = release.version.to_string();
                            if self.config.updates.remind_later_version.as_deref()
                                != Some(release_version.as_str())
                            {
                                self.config.updates.remind_later_version = None;
                                self.config.updates.remind_later_until_utc = None;
                            }
                        } else {
                            self.config.updates.remind_later_version = None;
                            self.config.updates.remind_later_until_utc = None;
                        }
                    }
                    Err(error) => {
                        self.set_update_error(
                            classify_update_error(error.as_str(), UpdatePhase::Checking),
                            error.clone(),
                        );
                        self.push_toast(
                            ToastTone::Error,
                            "Update Failed",
                            format!("Failed to check for updates: {error}"),
                            true,
                        );
                        if !manual {
                            tracing::warn!("Automatic update check failed: {error}");
                        }
                    }
                }

                if let Err(error) = self.config.save() {
                    tracing::warn!("Failed to persist update-check timestamp: {error}");
                }

                Task::none()
            }
            UpdateMessage::RollbackCatalogLoaded(result) => {
                self.updates.state.rollback_catalog_loading = false;
                match result {
                    Ok(candidates) => {
                        self.updates.state.rollback_candidates = candidates.clone();
                        let previous_version =
                            self.updates.state.previous_installed_version.clone();
                        self.settings.selected_rollback_release = previous_version
                            .as_ref()
                            .and_then(|previous| {
                                candidates
                                    .iter()
                                    .find(|release| &release.version == previous)
                                    .cloned()
                            })
                            .or_else(|| candidates.first().cloned());
                    }
                    Err(error) => {
                        self.set_update_error(
                            classify_update_error(error.as_str(), UpdatePhase::Checking),
                            error.clone(),
                        );
                        self.push_toast(
                            ToastTone::Error,
                            "Update Failed",
                            format!("Failed to refresh rollback versions: {error}"),
                            true,
                        );
                    }
                }
                Task::none()
            }
            UpdateMessage::DownloadSelectedRollbackVersion => {
                let Some(release) = self.selected_rollback_release() else {
                    self.set_status_feedback_silent(
                        "Load rollback versions and choose one before downloading it.",
                        false,
                    );
                    return Task::none();
                };
                self.queue_release_download(release)
            }
            UpdateMessage::RollbackToPreviousInstalledVersion => {
                if self.updates.state.previous_installed_version.is_none() {
                    self.set_status_feedback_silent(
                        "No previously installed version is recorded for this install yet.",
                        false,
                    );
                    return Task::none();
                }
                self.lookup_previous_installed_rollback_task()
            }
            UpdateMessage::RollbackPreviousVersionResolved(result) => match result {
                Ok(Some(release)) => {
                    self.settings.selected_rollback_release = Some(release.clone());
                    self.queue_release_download(release)
                }
                Ok(None) => {
                    let version_label = self
                        .updates
                        .state
                        .previous_installed_version
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "the previous version".into());
                    self.set_status_feedback_silent(
                        format!(
                            "GitHub Releases did not expose a downloadable asset for {}.",
                            version_label
                        ),
                        false,
                    );
                    Task::none()
                }
                Err(error) => {
                    self.set_update_error(
                        classify_update_error(error.as_str(), UpdatePhase::Checking),
                        error.clone(),
                    );
                    self.push_toast(
                        ToastTone::Error,
                        "Update Failed",
                        format!("Failed to resolve the previous version: {error}"),
                        true,
                    );
                    Task::none()
                }
            },
            UpdateMessage::UpdatePrimaryActionSelected(action) => {
                self.settings.selected_update_action = action;
                Task::none()
            }
            UpdateMessage::RunSelectedUpdateAction => self.run_selected_update_action(),
            UpdateMessage::InstallDownloadedUpdateWhenIdle => {
                self.schedule_downloaded_update_when_idle()
            }
            UpdateMessage::ShowUpdateDetails => self.show_update_details(),
            UpdateMessage::HideUpdateDetails => {
                self.updates.details_modal_open = false;
                self.updates.details_log_loading = false;
                Task::none()
            }
            UpdateMessage::UpdateDetailsLogLoaded(result) => {
                self.updates.details_log_loading = false;
                match result {
                    Ok(log_text) => {
                        self.updates.details_log_text = Some(log_text);
                        self.updates.details_log_error = None;
                    }
                    Err(error) => {
                        self.updates.details_log_text = None;
                        self.updates.details_log_error = Some(error);
                    }
                }
                Task::none()
            }
            UpdateMessage::SystemUpdaterOpened(result) => {
                if let Err(error) = result {
                    self.set_status_feedback_silent(
                        format!("Failed to launch the system updater: {error}"),
                        false,
                    );
                }
                Task::none()
            }
            UpdateMessage::OpenUpdateReleaseNotes => {
                let Some(url) = self.active_update_release_url() else {
                    return Task::none();
                };
                let platform = self.platform.clone();
                Task::perform(async move { platform.open_url(&url) }, |result| {
                    Message::updates(UpdateMessage::UpdateReleaseNotesOpened(result))
                })
            }
            UpdateMessage::UpdateReleaseNotesOpened(result) => {
                if let Err(error) = result {
                    self.set_status_feedback_silent(
                        format!("Failed to open the release notes: {error}"),
                        false,
                    );
                }
                Task::none()
            }
        }
    }

    fn fetch_honu_session(&self, character_id: u64) -> Task<Message> {
        let client = self.honu_client.clone();
        Task::perform(
            async move { client.fetch_active_session(character_id).await },
            |result| {
                Message::runtime(RuntimeMessage::HonuSessionResolved(
                    result.map_err(|e| e.to_string()),
                ))
            },
        )
    }

    fn check_online_status(&self) -> Task<Message> {
        let ids: Vec<u64> = self
            .config
            .characters
            .iter()
            .filter_map(|c| c.character_id)
            .collect();
        if ids.is_empty() || self.config.service_id.is_empty() {
            return Task::none();
        }
        let service_id = self.config.service_id.clone();
        Task::perform(
            async move { census::fetch_online_status(&service_id, &ids).await },
            |result| match result {
                Ok(online) => Message::runtime(RuntimeMessage::OnlineStatusChecked(online)),
                Err(e) => {
                    tracing::warn!("Online status check failed: {e}");
                    Message::runtime(RuntimeMessage::OnlineStatusChecked(Vec::new()))
                }
            },
        )
    }

    fn handle_census_stream(&mut self, event: StreamEvent) -> Task<Message> {
        match event {
            StreamEvent::Login { character_id } => {
                if matches!(self.runtime.lifecycle, AppState::WaitingForLogin) {
                    let character_name = self
                        .config
                        .characters
                        .iter()
                        .find(|c| c.character_id == Some(character_id))
                        .map(|c| c.name.clone())
                        .unwrap_or_else(|| format!("character {character_id}"));
                    tracing::info!("{character_name} logged in ({character_id})");
                    return self.enter_monitoring(character_name, character_id);
                }
                Task::none()
            }
            StreamEvent::Logout { character_id } => {
                if let AppState::Monitoring {
                    character_id: active,
                    ..
                } = &self.runtime.lifecycle
                    && *active == character_id
                {
                    tracing::info!("character {character_id} logged out");
                    self.runtime.lifecycle = AppState::WaitingForLogin;
                    self.rule_engine.reset();
                    self.runtime.honu_session_id = None;
                    return self.finish_active_session();
                }
                Task::none()
            }
            StreamEvent::Classified {
                character_id,
                event,
            } => {
                let should_handle = match &self.runtime.lifecycle {
                    AppState::Monitoring {
                        character_id: active_id,
                        ..
                    } if *active_id == character_id => true,
                    _ => return Task::none(),
                };

                if !should_handle {
                    return Task::none();
                }

                self.event_log.append(event.clone());
                let _ = character_id;
                self.resolve_rule_engine_event(event)
            }
            StreamEvent::Alert(alert_update) => self.handle_alert_update(alert_update),
            StreamEvent::Disconnected => {
                tracing::debug!("census stream reported disconnect");
                Task::none()
            }
        }
    }

    fn ingest_resolved_rule_engine_event(&mut self, event: ClassifiedEvent) -> Task<Message> {
        let actions = self.rule_engine.ingest(&event);
        let mut tasks = Vec::new();
        if !actions.is_empty() {
            tasks.extend(
                actions
                    .into_iter()
                    .map(|action| Task::done(Message::RuleClipAction(action))),
            );
        }
        Task::batch(tasks)
    }

    fn resolve_rule_engine_event(&mut self, event: ClassifiedEvent) -> Task<Message> {
        let Some(other_character_id) = event.other_character_id else {
            return Task::done(Message::ResolvedRuleEngineEvent(event));
        };

        if !self.active_profile_uses_target_outfit_filters() {
            return Task::done(Message::ResolvedRuleEngineEvent(event));
        }

        let store = self.clip_store.clone();
        let service_id = self.config.service_id.clone();
        Task::perform(
            async move {
                if let Some(store) = &store
                    && let Some(CharacterOutfitCacheEntry { outfit_id, .. }) = store
                        .cached_character_outfit(other_character_id)
                        .await
                        .map_err(|error| error.to_string())?
                {
                    return Ok::<Option<u64>, String>(outfit_id);
                }

                if service_id.trim().is_empty() {
                    return Ok::<Option<u64>, String>(None);
                }

                let resolved =
                    census::resolve_character_outfit_reference(&service_id, other_character_id)
                        .await
                        .map_err(|error| error.to_string())?;
                let outfit_id = resolved.as_ref().map(|lookup| lookup.id as u64);
                let outfit_tag = if let Some(store) = &store {
                    if let Some(lookup) = &resolved {
                        store
                            .cached_lookup(LookupKind::Outfit, lookup.id)
                            .await
                            .map_err(|error| error.to_string())?
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(store) = &store {
                    store
                        .store_character_outfit(
                            other_character_id,
                            outfit_id,
                            outfit_tag.as_deref(),
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                }

                Ok::<Option<u64>, String>(outfit_id)
            },
            move |result| {
                let mut resolved_event = event.clone();
                match result {
                    Ok(outfit_id) => {
                        resolved_event.other_character_outfit_id = outfit_id;
                    }
                    Err(error) => {
                        tracing::warn!(
                            "Failed to resolve target outfit for character #{other_character_id}: {error}"
                        );
                    }
                }
                Message::ResolvedRuleEngineEvent(resolved_event)
            },
        )
    }

    fn active_profile_uses_target_outfit_filters(&self) -> bool {
        let enabled_rule_ids: BTreeSet<_> = self
            .active_profile()
            .map(|profile| {
                profile
                    .enabled_rule_ids
                    .iter()
                    .map(String::as_str)
                    .collect()
            })
            .unwrap_or_default();

        self.config
            .rule_definitions
            .iter()
            .filter(|rule| enabled_rule_ids.contains(rule.id.as_str()))
            .flat_map(|rule| &rule.scored_events)
            .filter(|event| event.filters.is_enabled())
            .any(|event| {
                event.filters.groups().iter().any(|group| {
                    group.clauses.iter().any(|clause| {
                        matches!(clause, crate::rules::ScoredEventFilterClause::TargetOutfit { outfit } if outfit.is_configured())
                    })
                })
            })
    }

    pub fn view(&self) -> Element<'_, Message> {
        let nav: Element<'_, Message> = nav_tabs(self.view.clone(), Message::SwitchView)
            .push(NavTab::new(View::Status, "Status"))
            .push(NavTab::new(View::Clips, "Clips"))
            .push(NavTab::new(View::Stats, "Stats"))
            .push(NavTab::new(View::Characters, "Characters"))
            .push(NavTab::new(View::Rules, "Rules"))
            .push(NavTab::new(View::Settings, "Settings"))
            .build();

        let content: Element<Message> = match self.view {
            View::Status => tabs::status::view(self),
            View::Clips => tabs::clips::view(self).map(Message::Clips),
            View::Stats => tabs::stats::view(self).map(Message::Stats),
            View::Characters => tabs::characters::view(self).map(Message::Characters),
            View::Rules => tabs::rules::view(self).map(Message::Rules),
            View::Settings => tabs::settings::view(self).map(Message::Settings),
        };

        let layout = column![nav, crate::ui::primitives::separator::horizontal(), content]
            .spacing(10)
            .padding(16);

        let base: Element<'_, Message> = container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();

        let overlay: Element<'_, Message> = toast::view(
            &self.toasts,
            toast::Corner::BottomRight,
            Message::ToastDismiss,
            Message::ToastToggleExpand,
        )
        .unwrap_or_else(|| {
            iced::widget::Space::new()
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        });

        let base: Element<'_, Message> = stack![base, overlay].into();
        if self.updates.details_modal_open {
            modal(
                base,
                update_details_modal(self),
                Some(Message::updates(UpdateMessage::HideUpdateDetails)),
            )
        } else {
            base
        }
    }

    fn resolve_unresolved_characters(&mut self) -> Task<Message> {
        let unresolved_names: Vec<String> = self
            .config
            .characters
            .iter()
            .filter(|character| {
                character.character_id.is_none()
                    || character.world_id.is_none()
                    || character.faction_id.is_none()
            })
            .map(|character| character.name.clone())
            .collect();

        let mut tasks = Vec::new();
        for name in unresolved_names {
            tasks.push(self.queue_character_resolution(name));
        }

        Task::batch(tasks)
    }

    pub(in crate::app) fn queue_character_resolution(&mut self, name: String) -> Task<Message> {
        let name = name.trim().to_string();
        if name.is_empty()
            || self.config.service_id.trim().is_empty()
            || self.rules.resolving_characters.contains(&name)
        {
            return Task::none();
        }

        self.rules.resolving_characters.insert(name.clone());
        let service_id = self.config.service_id.clone();
        let resolved_name = name.clone();
        Task::perform(
            async move { census::resolve_character(&service_id, &name).await },
            move |result| match result {
                Ok(character) => Message::Characters(tabs::characters::Message::Resolved(
                    resolved_name,
                    character,
                )),
                Err(error) => Message::Characters(tabs::characters::Message::ResolveFailed(
                    resolved_name,
                    error.to_string(),
                )),
            },
        )
    }

    fn handle_rule_clip_action(&mut self, action: ClipAction) -> Task<Message> {
        match action.lifecycle {
            ClipActionLifecycle::Trigger => self
                .clip_request_from_rule_action(&action)
                .map_or_else(Task::none, |request| {
                    self.queue_immediate_clip_save(request)
                }),
            ClipActionLifecycle::StartExtending { expires_at } => self
                .clip_request_from_rule_action(&action)
                .map_or_else(Task::none, |request| {
                    self.start_active_clip_capture(request, expires_at)
                }),
            ClipActionLifecycle::Extend { expires_at } => {
                self.extend_active_clip_capture(&action, expires_at)
            }
            ClipActionLifecycle::Finalize { finalized_at } => {
                self.finalize_active_clip_capture(&action.rule_id, finalized_at)
            }
        }
    }

    fn clip_request_from_rule_action(&self, action: &ClipAction) -> Option<ClipSaveRequest> {
        let AppState::Monitoring { character_id, .. } = &self.runtime.lifecycle else {
            return None;
        };

        let clip_duration_secs = self.clip_duration_secs(action.clip_length);
        let clip_end_at = action.event.timestamp
            + chrono::Duration::seconds(i64::from(self.config.recorder.save_delay_secs));
        let clip_start_at = clip_end_at - chrono::Duration::seconds(i64::from(clip_duration_secs));

        Some(ClipSaveRequest {
            origin: ClipOrigin::Rule,
            profile_id: self.active_profile_id(),
            rule_id: action.rule_id.clone(),
            duration: action.clip_length,
            clip_duration_secs,
            trigger_score: action.trigger_score,
            score_breakdown: action.score_breakdown.clone(),
            trigger_at: action.event.timestamp,
            clip_start_at,
            clip_end_at,
            world_id: action.event.world_id,
            zone_id: action.event.zone_id,
            facility_id: action.event.facility_id,
            character_id: *character_id,
            honu_session_id: self.runtime.honu_session_id,
            session_id: self
                .runtime
                .active_session
                .as_ref()
                .map(|session| session.id.clone()),
        })
    }

    fn queue_immediate_clip_save(&mut self, request: ClipSaveRequest) -> Task<Message> {
        if self.runtime.active_clip_capture.is_some() {
            tracing::info!("Clip trigger ignored: another pending clip capture is already active");
            return Task::none();
        }

        self.runtime.active_clip_capture = Some(ActiveClipCapture {
            preferred_start_at: request.clip_start_at,
            request,
        });
        self.poll_active_clip_capture().unwrap_or_else(Task::none)
    }

    fn start_active_clip_capture(
        &mut self,
        mut request: ClipSaveRequest,
        expires_at: chrono::DateTime<Utc>,
    ) -> Task<Message> {
        if self.runtime.active_clip_capture.is_some() {
            tracing::info!(
                "Auto-extend trigger ignored for rule `{}` because another clip capture is already pending",
                request.rule_id
            );
            return Task::none();
        }

        let preferred_start_at = request.clip_start_at;
        request.clip_end_at =
            expires_at + chrono::Duration::seconds(i64::from(self.config.recorder.save_delay_secs));
        recompute_capture_window(
            &mut request,
            preferred_start_at,
            self.config.recorder.replay_buffer_secs,
        );
        self.runtime.active_clip_capture = Some(ActiveClipCapture {
            request,
            preferred_start_at,
        });
        Task::none()
    }

    fn extend_active_clip_capture(
        &mut self,
        action: &ClipAction,
        expires_at: chrono::DateTime<Utc>,
    ) -> Task<Message> {
        let snapshot_duration_secs = self.clip_duration_secs(action.clip_length);
        let save_delay_secs = self.config.recorder.save_delay_secs;
        let replay_buffer_secs = self.config.recorder.replay_buffer_secs;

        let Some(active_capture) = self.runtime.active_clip_capture.as_mut() else {
            tracing::debug!(
                "Received extend action for rule `{}` without an active capture",
                action.rule_id
            );
            return Task::none();
        };
        if active_capture.request.rule_id != action.rule_id {
            tracing::debug!(
                "Ignoring extend action for rule `{}` while `{}` is pending",
                action.rule_id,
                active_capture.request.rule_id
            );
            return Task::none();
        }

        let snapshot_end_at =
            action.event.timestamp + chrono::Duration::seconds(i64::from(save_delay_secs));
        let snapshot_start_at =
            snapshot_end_at - chrono::Duration::seconds(i64::from(snapshot_duration_secs));
        active_capture.preferred_start_at =
            active_capture.preferred_start_at.min(snapshot_start_at);
        active_capture.request.duration = action.clip_length;
        active_capture.request.trigger_score = active_capture
            .request
            .trigger_score
            .max(action.trigger_score);
        active_capture.request.score_breakdown = action.score_breakdown.clone();
        active_capture.request.world_id = action.event.world_id;
        active_capture.request.zone_id = action.event.zone_id;
        active_capture.request.facility_id = action.event.facility_id;
        active_capture.request.clip_end_at =
            expires_at + chrono::Duration::seconds(i64::from(save_delay_secs));
        recompute_capture_window(
            &mut active_capture.request,
            active_capture.preferred_start_at,
            replay_buffer_secs,
        );
        Task::none()
    }

    fn finalize_active_clip_capture(
        &mut self,
        rule_id: &str,
        finalized_at: chrono::DateTime<Utc>,
    ) -> Task<Message> {
        let save_delay_secs = self.config.recorder.save_delay_secs;
        let replay_buffer_secs = self.config.recorder.replay_buffer_secs;
        let Some(active_capture) = self.runtime.active_clip_capture.as_mut() else {
            tracing::debug!(
                "Received finalize action for rule `{rule_id}` without an active capture"
            );
            return Task::none();
        };
        if active_capture.request.rule_id != rule_id {
            tracing::debug!(
                "Ignoring finalize action for rule `{rule_id}` while `{}` is pending",
                active_capture.request.rule_id
            );
            return Task::none();
        }

        active_capture.request.clip_end_at =
            finalized_at + chrono::Duration::seconds(i64::from(save_delay_secs));
        recompute_capture_window(
            &mut active_capture.request,
            active_capture.preferred_start_at,
            replay_buffer_secs,
        );
        self.poll_active_clip_capture().unwrap_or_else(Task::none)
    }

    pub(in crate::app) fn active_profile_index(&self) -> Option<usize> {
        self.config
            .rule_profiles
            .iter()
            .position(|profile| profile.id == self.config.active_profile_id)
    }

    pub(in crate::app) fn active_profile(&self) -> Option<&RuleProfile> {
        self.active_profile_index()
            .and_then(|index| self.config.rule_profiles.get(index))
    }

    fn active_profile_id(&self) -> String {
        self.config.active_profile_id.clone()
    }

    pub(in crate::app) fn manual_profile_override_name(&self) -> Option<String> {
        let profile_id = self.runtime.manual_profile_override_profile_id.as_deref()?;
        self.config
            .rule_profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .map(|profile| profile.name.clone())
            .or_else(|| Some(profile_id.to_string()))
    }

    fn set_active_profile_runtime(&mut self, profile_id: String, persist: bool) {
        if self.config.active_profile_id == profile_id {
            return;
        }
        self.config.active_profile_id = profile_id;
        self.rule_engine.update_rules(
            self.config.rule_definitions.clone(),
            self.config.rule_profiles.clone(),
            self.config.active_profile_id.clone(),
        );
        if persist && let Err(error) = self.config.save() {
            tracing::error!("Failed to save config: {error}");
        }
        self.notify_active_profile_activated();
        let _ = self.sync_tray_snapshot();
    }

    pub(in crate::app) fn apply_manual_profile_selection(&mut self, profile_id: String) {
        self.runtime.manual_profile_override_profile_id = Some(profile_id.clone());
        self.runtime.last_auto_switch_rule_id = None;
        self.set_active_profile_runtime(profile_id, true);
    }

    pub(in crate::app) fn resume_auto_switching(&mut self) {
        self.runtime.manual_profile_override_profile_id = None;
    }

    fn evaluate_runtime_auto_switch(
        &mut self,
        now: chrono::DateTime<Utc>,
        active_character_id: Option<u64>,
    ) -> Task<Message> {
        if self.runtime.manual_profile_override_profile_id.is_some() {
            return Task::none();
        }
        let Some(decision) =
            choose_runtime_rule(&self.config.auto_switch_rules, now, active_character_id)
        else {
            return Task::none();
        };
        if self.config.active_profile_id == decision.target_profile_id {
            return Task::none();
        }
        self.runtime.last_auto_switch_rule_id = Some(decision.rule_id);
        self.set_active_profile_runtime(decision.target_profile_id, false);
        Task::none()
    }

    fn handle_alert_update(&mut self, alert_update: AlertUpdate) -> Task<Message> {
        let mut record = self
            .runtime
            .tracked_alerts
            .get(&alert_update.alert_key)
            .cloned()
            .unwrap_or(AlertInstanceRecord {
                alert_key: alert_update.alert_key.clone(),
                label: census::alert_label(alert_update.metagame_event_id, alert_update.zone_id),
                world_id: alert_update.world_id,
                zone_id: alert_update.zone_id,
                metagame_event_id: alert_update.metagame_event_id,
                started_at: alert_update.timestamp,
                ended_at: None,
                state_name: alert_update.state_name.clone(),
                winner_faction: None,
                faction_nc: alert_update.faction_nc,
                faction_tr: alert_update.faction_tr,
                faction_vs: alert_update.faction_vs,
            });

        record.state_name = alert_update.state_name.clone();
        record.faction_nc = alert_update.faction_nc;
        record.faction_tr = alert_update.faction_tr;
        record.faction_vs = alert_update.faction_vs;
        record.started_at = record.started_at.min(alert_update.timestamp);
        if matches!(alert_update.lifecycle, AlertLifecycle::Ended) {
            record.ended_at = Some(alert_update.timestamp);
            record.winner_faction = alert_update.winner_faction.clone();
        }
        self.runtime
            .tracked_alerts
            .insert(alert_update.alert_key.clone(), record.clone());

        let Some(store) = self.clip_store.clone() else {
            return Task::none();
        };
        let alert_key = record.alert_key.clone();
        Task::perform(
            async move { store.upsert_alert(&record).await },
            move |result| Message::AlertStored {
                alert_key,
                result: result.map_err(|error| error.to_string()),
            },
        )
    }

    fn alert_keys_for_clip_window(
        &self,
        world_id: u32,
        zone_id: Option<u32>,
        clip_start_at: chrono::DateTime<Utc>,
        clip_end_at: chrono::DateTime<Utc>,
    ) -> Vec<String> {
        let Some(zone_id) = zone_id else {
            return Vec::new();
        };

        self.runtime
            .tracked_alerts
            .values()
            .filter(|alert| alert.world_id == world_id && alert.zone_id == zone_id)
            .filter(|alert| {
                let alert_end = alert.ended_at.unwrap_or(clip_end_at);
                alert.started_at < clip_end_at && alert_end > clip_start_at
            })
            .map(|alert| alert.alert_key.clone())
            .collect()
    }

    pub(in crate::app) fn notifications_enabled(&self) -> bool {
        self.config.recorder.clip_saved_notifications
    }

    pub(in crate::app) fn sync_launch_at_login_task(&self) -> Task<Message> {
        let config = self.config.launch_at_login.clone();
        let platform = self.platform.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || platform.sync_launch_at_login(&config))
                    .await
                    .map_err(|error| format!("failed to join launch-at-login task: {error}"))?
            },
            |result| Message::updates(UpdateMessage::LaunchAtLoginSynced(result)),
        )
    }

    pub(in crate::app) fn notify_active_profile_activated(&mut self) {
        if !self.notifications_enabled() {
            return;
        }

        if let Some(profile_name) = self.active_profile().map(|profile| profile.name.clone()) {
            self.notifications
                .notify_profile_activated(profile_name.as_str());
        }
    }

    fn clip_duration_secs(&self, duration: ClipLength) -> u32 {
        match duration {
            ClipLength::Seconds(value) => value,
            ClipLength::FullBuffer => self.config.recorder.replay_buffer_secs,
        }
    }

    fn poll_active_clip_capture(&mut self) -> Option<Task<Message>> {
        let active_capture = self.runtime.active_clip_capture.as_ref()?;
        if self.recorder.save_in_progress() {
            return None;
        }
        if Utc::now() < active_capture.request.clip_end_at {
            return None;
        }

        let request = self.runtime.active_clip_capture.take()?.request;
        Some(self.begin_clip_save(request))
    }

    fn poll_startup_probe(&mut self) -> Option<Task<Message>> {
        let due_at = self.runtime.startup_probe_due_at?;
        if Instant::now() < due_at {
            return None;
        }
        if self.runtime.startup_probe_pending_result
            || self.recorder.save_in_progress()
            || self.runtime.active_clip_capture.is_some()
            || !self.recorder.should_probe_saved_clip_resolution()
        {
            return None;
        }

        self.runtime.startup_probe_due_at = None;
        self.runtime.startup_probe_pending_result = true;
        if let Err(error) = self.recorder.save_clip(ClipLength::Seconds(5)) {
            self.runtime.startup_probe_pending_result = false;
            tracing::warn!("Failed to start startup recorder probe save: {error}");
            return None;
        }

        tracing::info!("Started startup recorder probe save");
        Some(Task::none())
    }

    fn begin_clip_save(&mut self, request: ClipSaveRequest) -> Task<Message> {
        if let Err(error) = self.recorder.save_clip(request.duration) {
            self.set_clip_error(format!("Failed to save clip: {error}"));
            tracing::error!("Failed to save clip: {error}");
            return Task::none();
        }

        let raw_events = raw_events_from_log(&self.event_log, &request);
        let alert_keys = self.alert_keys_for_clip_window(
            request.world_id,
            request.zone_id,
            request.clip_start_at,
            request.clip_end_at,
        );
        let draft = clip_draft_from_request(request.clone(), raw_events.clone(), alert_keys);
        let lookup_tasks = self.schedule_raw_event_lookup_resolutions(
            &draft.raw_events,
            draft.zone_id,
            draft.facility_id,
        );

        let Some(store) = self.clip_store.clone() else {
            tracing::warn!("Clip triggered but clip database is not ready yet");
            return lookup_tasks;
        };

        let sequence = self.runtime.next_clip_sequence;
        self.runtime.next_clip_sequence += 1;
        self.runtime.pending_save_sequences.push_back(sequence);
        self.runtime
            .pending_clip_links
            .entry(sequence)
            .or_default()
            .naming_context = Some(self.build_clip_naming_context(&request));

        let persist_task = Task::perform(
            async move { store.insert_clip(draft).await },
            move |result| Message::ClipPersisted {
                sequence,
                result: result.map_err(|e| e.to_string()),
            },
        );

        Task::batch([persist_task, lookup_tasks])
    }

    fn load_clip_detail(&mut self, clip_id: Option<i64>) -> Task<Message> {
        self.clips.selected_id = clip_id;
        self.clips.selected_detail = None;

        let Some(clip_id) = clip_id else {
            self.clips.detail_loading = false;
            return Task::none();
        };
        let Some(store) = self.clip_store.clone() else {
            self.clips.detail_loading = false;
            return Task::none();
        };

        self.clips.detail_loading = true;
        Task::perform(async move { store.clip_detail(clip_id).await }, |result| {
            Message::ClipDetailLoaded(result.map_err(|e| e.to_string()))
        })
    }

    fn load_stats(&mut self) -> Task<Message> {
        let Some(store) = self.clip_store.clone() else {
            self.stats.snapshot = None;
            self.stats.loading = false;
            return Task::none();
        };

        self.stats.revision += 1;
        let revision = self.stats.revision;
        self.stats.loading = true;
        let since_ts = self.stats.time_range.since_timestamp_ms();

        Task::perform(
            async move { store.stats_snapshot(since_ts).await },
            move |result| Message::StatsLoaded {
                revision,
                result: result.map_err(|e| e.to_string()),
            },
        )
    }

    fn load_clip_filter_options(&mut self) -> Task<Message> {
        let Some(store) = self.clip_store.clone() else {
            self.clips.filter_options.targets.clear();
            self.clips.filter_options.weapons.clear();
            self.clips.filter_options.alerts.clear();
            self.clips.filter_options.tags.clear();
            self.clips.filter_options.collections.clear();
            return Task::none();
        };

        Task::perform(
            async move { store.raw_event_filter_options().await },
            |result| Message::ClipFilterOptionsLoaded(result.map_err(|e| e.to_string())),
        )
    }

    fn export_selected_clip_timeline_artifact(
        &mut self,
        clip_id: i64,
        kind: crate::timeline_export::TimelineExportKind,
    ) -> Task<Message> {
        let Some(detail) = self
            .clips
            .selected_detail
            .clone()
            .filter(|detail| detail.clip.id == clip_id)
        else {
            self.set_clip_error(format!("Clip #{clip_id} detail is not loaded yet."));
            return Task::none();
        };

        Task::perform(
            async move { crate::timeline_export::export_timeline_sidecar(&detail, kind) },
            move |result| Message::TimelineArtifactExported {
                kind,
                result: result.map(|path| path.display().to_string()),
            },
        )
    }

    fn export_last_session_summary_markdown(&mut self) -> Task<Message> {
        let Some(summary) = self.runtime.last_session_summary.clone() else {
            self.set_status_feedback("No completed session summary is available to export.", true);
            return Task::none();
        };
        let save_dir = self.config.recorder.save_directory.clone();

        Task::perform(
            async move { crate::session_report::export_session_summary_markdown(&summary, &save_dir) },
            |result| Message::SessionSummaryExported(result.map(|path| path.display().to_string())),
        )
    }

    fn finish_active_session(&mut self) -> Task<Message> {
        self.event_log.clear();
        let Some(session) = self.runtime.active_session.take() else {
            return Task::none();
        };

        let Some(store) = self.clip_store.clone() else {
            self.runtime.last_session_summary = Some(empty_session_summary(&session));
            return Task::none();
        };
        let session_id = session.id.clone();

        Task::perform(
            async move { store.session_summary(&session_id).await },
            move |result| Message::SessionSummaryLoaded {
                session_id: session.id.clone(),
                result: result.map_err(|e| e.to_string()),
            },
        )
    }

    fn schedule_clip_record_lookup_resolutions(&mut self, clips: &[ClipRecord]) -> Task<Message> {
        let mut tasks = Vec::new();
        for clip in clips {
            if let Some(zone_id) = clip.zone_id {
                tasks.push(self.queue_lookup_resolution(LookupKind::Zone, i64::from(zone_id)));
            }
            if let Some(facility_id) = clip.facility_id {
                tasks.push(
                    self.queue_lookup_resolution(LookupKind::Facility, i64::from(facility_id)),
                );
            }
        }
        Task::batch(tasks)
    }

    fn schedule_clip_detail_lookup_resolutions(
        &mut self,
        detail: &ClipDetailRecord,
    ) -> Task<Message> {
        let raw_events: Vec<_> = detail
            .raw_events
            .iter()
            .map(|event| ClipRawEventDraft {
                event_at: event.event_at,
                event_kind: event.event_kind.clone(),
                world_id: event.world_id,
                zone_id: event.zone_id,
                facility_id: event.facility_id,
                actor_character_id: event.actor_character_id,
                other_character_id: event.other_character_id,
                actor_class: event.actor_class.clone(),
                attacker_weapon_id: event.attacker_weapon_id,
                attacker_vehicle_id: event.attacker_vehicle_id,
                vehicle_killed_id: event.vehicle_killed_id,
                characters_killed: event.characters_killed,
                is_headshot: event.is_headshot,
                experience_id: event.experience_id,
            })
            .collect();

        self.schedule_raw_event_lookup_resolutions(
            &raw_events,
            detail.clip.zone_id,
            detail.clip.facility_id,
        )
    }

    fn schedule_raw_event_lookup_resolutions(
        &mut self,
        raw_events: &[ClipRawEventDraft],
        zone_id: Option<u32>,
        facility_id: Option<u32>,
    ) -> Task<Message> {
        let mut tasks = Vec::new();

        if let Some(zone_id) = zone_id {
            tasks.push(self.queue_lookup_resolution(LookupKind::Zone, i64::from(zone_id)));
        }
        if let Some(facility_id) = facility_id {
            tasks.push(self.queue_lookup_resolution(LookupKind::Facility, i64::from(facility_id)));
        }

        for event in raw_events {
            if let Some(actor_character_id) = event.actor_character_id {
                tasks.push(
                    self.queue_lookup_resolution(LookupKind::Character, actor_character_id as i64),
                );
            }
            if let Some(other_character_id) = event.other_character_id {
                tasks.push(
                    self.queue_lookup_resolution(LookupKind::Character, other_character_id as i64),
                );
            }
            if let Some(weapon_id) = event.attacker_weapon_id {
                tasks.push(self.queue_lookup_resolution(LookupKind::Weapon, i64::from(weapon_id)));
            }
            if let Some(vehicle_id) = event.attacker_vehicle_id {
                tasks
                    .push(self.queue_lookup_resolution(LookupKind::Vehicle, i64::from(vehicle_id)));
            }
            if let Some(vehicle_id) = event.vehicle_killed_id {
                tasks
                    .push(self.queue_lookup_resolution(LookupKind::Vehicle, i64::from(vehicle_id)));
            }
            if let Some(zone_id) = event.zone_id {
                tasks.push(self.queue_lookup_resolution(LookupKind::Zone, i64::from(zone_id)));
            }
            if let Some(facility_id) = event.facility_id {
                tasks.push(
                    self.queue_lookup_resolution(LookupKind::Facility, i64::from(facility_id)),
                );
            }
        }

        Task::batch(tasks)
    }

    fn queue_lookup_resolution(&mut self, kind: LookupKind, lookup_id: i64) -> Task<Message> {
        if lookup_id <= 0
            || self.config.service_id.trim().is_empty()
            || self.rules.resolving_lookups.contains(&(kind, lookup_id))
        {
            return Task::none();
        }

        let Some(store) = self.clip_store.clone() else {
            return Task::none();
        };

        self.rules.resolving_lookups.insert((kind, lookup_id));
        let service_id = self.config.service_id.clone();

        Task::perform(
            async move {
                if store
                    .cached_lookup(kind, lookup_id)
                    .await
                    .map_err(|error| error.to_string())?
                    .is_some()
                {
                    return Ok(false);
                }

                let resolved = match kind {
                    LookupKind::Facility => {
                        census::resolve_facility_name(&service_id, lookup_id as u32)
                            .await
                            .map_err(|error| error.to_string())?
                    }
                    LookupKind::Vehicle => {
                        census::resolve_vehicle_name(&service_id, lookup_id as u16)
                            .await
                            .map_err(|error| error.to_string())?
                    }
                    LookupKind::Zone => census::resolve_zone_name(&service_id, lookup_id as u32)
                        .await
                        .map_err(|error| error.to_string())?,
                    LookupKind::Character => {
                        census::resolve_character_name(&service_id, lookup_id as u64)
                            .await
                            .map_err(|error| error.to_string())?
                    }
                    LookupKind::Outfit => {
                        census::resolve_outfit_name(&service_id, lookup_id as u64)
                            .await
                            .map_err(|error| error.to_string())?
                    }
                    LookupKind::Weapon => {
                        census::resolve_weapon_name(&service_id, lookup_id as u32)
                            .await
                            .map_err(|error| error.to_string())?
                    }
                };

                if let Some(name) = resolved {
                    store
                        .store_lookup(kind, lookup_id, &name)
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            },
            move |result: Result<bool, String>| match result {
                Ok(refreshed) => Message::LookupResolved {
                    kind,
                    lookup_id,
                    refreshed,
                    result: Ok(()),
                },
                Err(error) => Message::LookupResolved {
                    kind,
                    lookup_id,
                    refreshed: false,
                    result: Err(error.to_string()),
                },
            },
        )
    }

    fn record_save_outcome(&mut self, outcome: PendingSaveOutcome) -> Task<Message> {
        let Some(sequence) = self.runtime.pending_save_sequences.pop_front() else {
            tracing::warn!("Received a clip save result without a pending clip request");
            return Task::none();
        };

        self.runtime
            .pending_clip_links
            .entry(sequence)
            .or_default()
            .save_outcome = Some(outcome);
        self.resolve_pending_clip_links()
    }

    fn resolve_pending_clip_links(&mut self) -> Task<Message> {
        let naming_template = self.config.clip_naming_template.clone();
        let ready_sequences: Vec<u64> = self
            .runtime
            .pending_clip_links
            .iter()
            .filter_map(|(&sequence, pending)| {
                if pending.save_outcome.is_none() {
                    None
                } else if pending.persist_failed || pending.clip_id.is_some() {
                    Some(sequence)
                } else {
                    None
                }
            })
            .collect();

        let mut tasks = Vec::new();

        for sequence in ready_sequences {
            let Some(pending) = self.runtime.pending_clip_links.remove(&sequence) else {
                continue;
            };

            if pending.persist_failed {
                continue;
            }

            let Some(clip_id) = pending.clip_id else {
                continue;
            };

            match pending.save_outcome {
                Some(PendingSaveOutcome::Saved {
                    path,
                    duration,
                    audio_layout,
                }) => {
                    let Some(store) = self.clip_store.clone() else {
                        tracing::warn!(
                            "Saved clip path {} could not be stored because the database is unavailable",
                            path.display()
                        );
                        continue;
                    };

                    let clip_path = path.to_string_lossy().into_owned();
                    let path_for_store = clip_path.clone();
                    let naming_context = pending.naming_context;
                    let naming_template = naming_template.clone();
                    let trim = match duration {
                        ClipLength::FullBuffer => None,
                        ClipLength::Seconds(tail_secs) => Some(TrimSpec { tail_secs }),
                    };
                    let audio_layout: Vec<AudioSourceConfig> =
                        audio_layout.into_iter().map(|entry| entry.config).collect();
                    tasks.push(Task::perform(
                        async move {
                            let final_path = if let Some(naming_context) = naming_context {
                                crate::clip_naming::rename_saved_clip(
                                    &naming_template,
                                    std::path::Path::new(&clip_path),
                                    &naming_context,
                                )
                                .map(|path| path.to_string_lossy().into_owned())
                                .unwrap_or(clip_path.clone())
                            } else {
                                clip_path.clone()
                            };

                            store.update_clip_path(clip_id, Some(&final_path)).await?;
                            Ok::<String, crate::db::ClipStoreError>(final_path)
                        },
                        move |result| match result {
                            Ok(final_path) => Message::ClipPathLinked {
                                clip_id,
                                path: final_path,
                                trim: trim.clone(),
                                audio_layout: audio_layout.clone(),
                                result: Ok(()),
                            },
                            Err(error) => Message::ClipPathLinked {
                                clip_id,
                                path: path_for_store,
                                trim: trim.clone(),
                                audio_layout: audio_layout.clone(),
                                result: Err(error.to_string()),
                            },
                        },
                    ));
                }
                Some(PendingSaveOutcome::Failed) | None => {}
            }
        }

        Task::batch(tasks)
    }

    fn update_clip_path_in_memory(&mut self, clip_id: i64, path: Option<&str>) {
        let path = path.map(str::to_string);
        let file_size_bytes = path
            .as_deref()
            .and_then(|value| std::fs::metadata(value).ok().map(|metadata| metadata.len()));

        for clip in &mut self.clips.recent {
            if clip.id == clip_id {
                clip.path = path.clone();
                clip.file_size_bytes = file_size_bytes;
            }
        }

        for clip in &mut self.clips.history_source {
            if clip.id == clip_id {
                clip.path = path.clone();
                clip.file_size_bytes = file_size_bytes;
            }
        }

        for clip in &mut self.clips.history {
            if clip.id == clip_id {
                clip.path = path.clone();
                clip.file_size_bytes = file_size_bytes;
            }
        }
    }

    fn inspect_saved_clip_resolution(&self, path: String) -> Task<Message> {
        if self.runtime.portal_capture_recovery_notified
            || !self.recorder.should_probe_saved_clip_resolution()
        {
            return Task::none();
        }

        Task::perform(
            crate::recorder::probe_video_resolution(PathBuf::from(&path)),
            move |result| Message::ClipResolutionInspected { path, result },
        )
    }

    fn inspect_and_delete_startup_probe(&self, path: PathBuf) -> Task<Message> {
        let task_path = path.clone();
        Task::perform(
            async move {
                let result = crate::recorder::probe_video_resolution(task_path.clone()).await;
                let delete_result = tokio::fs::remove_file(&task_path)
                    .await
                    .map_err(|error| format!("failed to delete {}: {error}", task_path.display()));
                (result, delete_result)
            },
            move |(result, delete_result)| Message::StartupProbeCompleted {
                path,
                result,
                delete_result,
            },
        )
    }

    fn queue_post_process_for_clip(
        &mut self,
        clip_id: i64,
        path: PathBuf,
        trim: Option<TrimSpec>,
        audio_layout: Vec<AudioSourceConfig>,
    ) -> Task<Message> {
        self.start_post_process_job(
            clip_id,
            path,
            Some(SavedPostProcessMetadata {
                trim,
                audio_layout,
                post_processing: self.config.recorder.post_processing.clone(),
            }),
        )
    }

    pub(in crate::app) fn queue_post_process_retry_for_clip(
        &mut self,
        clip_id: i64,
        path: PathBuf,
    ) -> Task<Message> {
        self.start_post_process_job(clip_id, path, None)
    }

    pub(in crate::app) fn use_original_clip_audio(&mut self, clip_id: i64) -> Task<Message> {
        let Some(store) = self.clip_store.clone() else {
            self.set_clip_error("Clip database is unavailable.");
            return Task::none();
        };

        Task::perform(
            async move {
                let clip_path = store
                    .clip_detail(clip_id)
                    .await
                    .map_err(|error| error.to_string())?
                    .and_then(|detail| detail.clip.path)
                    .map(PathBuf::from);
                store
                    .delete_audio_tracks(clip_id)
                    .await
                    .map_err(|error| error.to_string())?;
                store
                    .set_post_process_status(clip_id, PostProcessStatus::NotRequired, None)
                    .await
                    .map_err(|error| error.to_string())?;
                if let Some(path) = clip_path {
                    post_process::delete_saved_metadata(&path)
                        .map_err(|error| error.to_string())?;
                }
                Ok(())
            },
            move |result| Message::ClipPostProcessBypassed { clip_id, result },
        )
    }

    fn start_post_process_job(
        &mut self,
        clip_id: i64,
        path: PathBuf,
        metadata: Option<SavedPostProcessMetadata>,
    ) -> Task<Message> {
        let Some(store) = self.clip_store.clone() else {
            self.set_clip_error("Clip database is unavailable.");
            return Task::none();
        };

        if !path.exists() {
            self.set_clip_error(format!(
                "Clip file does not exist for audio post-processing: {}",
                path.display()
            ));
            return Task::none();
        }

        let metadata = match metadata {
            Some(metadata) => {
                if let Err(error) = post_process::write_saved_metadata(&path, &metadata) {
                    self.set_clip_error(format!(
                        "Failed to save post-process metadata for clip #{clip_id}: {error}"
                    ));
                    return Task::none();
                }
                metadata
            }
            None => match post_process::read_saved_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    self.set_clip_error(format!(
                        "Failed to load the original captured audio layout for clip #{clip_id}: {error}"
                    ));
                    return Task::none();
                }
            },
        };

        if !self.runtime.ffmpeg_capabilities.present
            || !self.runtime.ffmpeg_capabilities.meets_floor
        {
            tracing::warn!(
                clip_id,
                path = %path.display(),
                ffmpeg_present = self.runtime.ffmpeg_capabilities.present,
                ffmpeg_meets_floor = self.runtime.ffmpeg_capabilities.meets_floor,
                "Skipping audio post-process because ffmpeg is unavailable or below the supported version floor"
            );
            let _ = post_process::delete_saved_metadata(&path);
            return Task::none();
        }

        let request = PostProcessRequest {
            input: path.clone(),
            output: path.clone(),
            trim: metadata.trim.clone(),
            audio_layout: metadata.audio_layout.clone(),
            post_processing: metadata.post_processing.clone(),
        };
        match post_process::probe_audio_streams_blocking(&path) {
            Ok(probed) if !post_process::needs_post_process(&request, &probed) => {
                tracing::debug!(
                    clip_id,
                    path = %path.display(),
                    "Skipping audio post-process because the saved clip does not need rewriting"
                );
                let _ = post_process::delete_saved_metadata(&path);
                return Task::none();
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(
                    clip_id,
                    path = %path.display(),
                    error = %error,
                    "Failed to preflight audio post-process; queueing the background job so the clip records the failure"
                );
            }
        }

        let ffmpeg_capabilities = self.runtime.ffmpeg_capabilities.clone();
        let job_id = self.background_jobs.start(
            BackgroundJobKind::PostProcess,
            format!("Post-process clip #{clip_id}"),
            vec![clip_id],
            move |ctx| async move {
                ctx.progress(1, 4, "Loading saved audio layout.")?;
                store
                    .set_post_process_status(clip_id, PostProcessStatus::Pending, None)
                    .await
                    .map_err(|error| format!("failed to mark clip #{clip_id} as pending: {error}"))?;

                let request = PostProcessRequest {
                    input: path.clone(),
                    output: path.clone(),
                    trim: metadata.trim.clone(),
                    audio_layout: metadata.audio_layout.clone(),
                    post_processing: metadata.post_processing.clone(),
                };

                ctx.progress(2, 4, "Running audio post-process.")?;
                match post_process::run(request, &ffmpeg_capabilities).await {
                    Ok(PostProcessResult::Unchanged { tracks }) => {
                        let drafts = output_tracks_to_drafts(tracks);
                        store
                            .insert_audio_tracks(clip_id, drafts.clone())
                            .await
                            .map_err(|error| {
                                format!(
                                    "failed to persist audio track metadata for clip #{clip_id}: {error}"
                                )
                            })?;
                        store
                            .set_post_process_status(clip_id, PostProcessStatus::NotRequired, None)
                            .await
                            .map_err(|error| {
                                format!(
                                    "failed to mark clip #{clip_id} as not requiring post-process: {error}"
                                )
                            })?;
                        post_process::delete_saved_metadata(&path).map_err(|error| {
                            format!(
                                "audio post-process completed for clip #{clip_id}, but failed to delete the saved metadata sidecar: {error}"
                            )
                        })?;
                        ctx.progress(4, 4, "Audio post-process was not required.")?;
                        Ok(BackgroundJobSuccess::PostProcess {
                            clip_id,
                            final_path: path.to_string_lossy().into_owned(),
                            plan: post_process::PostProcessPlan {
                                trimmed: metadata.trim.is_some(),
                                premix_stream_index: None,
                                preserved_stream_count: drafts.len(),
                                codec_used: metadata.post_processing.codec,
                            },
                            tracks: drafts,
                            message: "Audio post-processing was not required.".into(),
                        })
                    }
                    Ok(PostProcessResult::Rewritten {
                        output,
                        plan,
                        tracks,
                    }) => {
                        ctx.progress(3, 4, "Saving rewritten audio tracks.")?;
                        let final_path = output.to_string_lossy().into_owned();
                        let drafts = output_tracks_to_drafts(tracks);
                        store
                            .update_clip_path(clip_id, Some(&final_path))
                            .await
                            .map_err(|error| {
                                format!(
                                    "failed to update clip path after post-process for clip #{clip_id}: {error}"
                                )
                            })?;
                        store
                            .insert_audio_tracks(clip_id, drafts.clone())
                            .await
                            .map_err(|error| {
                                format!(
                                    "failed to persist audio track metadata for clip #{clip_id}: {error}"
                                )
                            })?;
                        store
                            .set_post_process_status(clip_id, PostProcessStatus::Completed, None)
                            .await
                            .map_err(|error| {
                                format!(
                                    "failed to mark clip #{clip_id} as post-processed: {error}"
                                )
                            })?;
                        post_process::delete_saved_metadata(&path).map_err(|error| {
                            format!(
                                "audio post-process completed for clip #{clip_id}, but failed to delete the saved metadata sidecar: {error}"
                            )
                        })?;
                        ctx.progress(4, 4, "Audio post-process completed.")?;
                        Ok(BackgroundJobSuccess::PostProcess {
                            clip_id,
                            final_path,
                            plan,
                            tracks: drafts,
                            message: "Audio post-processing completed.".into(),
                        })
                    }
                    Err(error) => {
                        let error_text = error.to_string();
                        store
                            .set_post_process_status(
                                clip_id,
                                PostProcessStatus::Failed,
                                Some(&error_text),
                            )
                            .await
                            .map_err(|store_error| {
                                format!(
                                    "audio post-process failed for clip #{clip_id}: {error_text}; also failed to record the failure: {store_error}"
                                )
                            })?;
                        Err(error_text)
                    }
                }
            },
        );

        self.set_clip_filter_feedback(
            format!("Queued audio post-processing for clip #{clip_id}."),
            true,
        );
        self.persist_background_job_snapshot(job_id)
    }

    fn should_reset_portal_capture_after_clip(&self, resolution: VideoResolution) -> bool {
        !self.runtime.portal_capture_recovery_notified
            && matches!(
                self.recorder.post_save_recovery_hint(Some(resolution)),
                capture::RecoveryHint::ReacquireCaptureTarget
            )
    }

    pub(in crate::app) fn startup_probe_status_line(&self) -> Option<String> {
        if !self.recorder.should_probe_saved_clip_resolution() {
            return None;
        }

        if self.runtime.startup_probe_pending_result || self.runtime.startup_probe_due_at.is_some()
        {
            return Some("Recorder startup probe: pending".into());
        }

        self.runtime.startup_probe_resolution.map(|resolution| {
            format!(
                "Recorder startup probe: {}x{}",
                resolution.width, resolution.height
            )
        })
    }

    fn remove_clip_from_memory(&mut self, clip_id: i64) {
        self.clips.recent.retain(|clip| clip.id != clip_id);
        self.clips.history_source.retain(|clip| clip.id != clip_id);
        self.clips.history.retain(|clip| clip.id != clip_id);

        if self.clips.selected_id == Some(clip_id) {
            self.clips.selected_id = None;
            self.clips.selected_detail = None;
            self.clips.detail_loading = false;
        }
    }

    fn recover_background_jobs(&self) -> Task<Message> {
        let Some(store) = self.clip_store.clone() else {
            return Task::none();
        };

        Task::perform(
            async move {
                store
                    .recover_background_jobs(BackgroundJobManager::HISTORY_LIMIT)
                    .await
                    .map_err(|error| error.to_string())
            },
            Message::BackgroundJobsRecovered,
        )
    }

    fn persist_background_job_record(&self, record: BackgroundJobRecord) -> Task<Message> {
        let Some(store) = self.clip_store.clone() else {
            return Task::none();
        };

        Task::perform(
            async move {
                store
                    .upsert_background_job(&record)
                    .await
                    .map_err(|error| error.to_string())
            },
            Message::BackgroundJobStored,
        )
    }

    fn persist_background_job_snapshot(&self, job_id: BackgroundJobId) -> Task<Message> {
        self.background_jobs
            .record(job_id)
            .map(|record| self.persist_background_job_record(record))
            .unwrap_or_else(Task::none)
    }

    fn delete_background_job_record(&self, job_id: BackgroundJobId) -> Task<Message> {
        let Some(store) = self.clip_store.clone() else {
            return Task::done(Message::BackgroundJobRemoved {
                job_id,
                result: Ok(()),
            });
        };

        Task::perform(
            async move {
                store
                    .delete_background_job(job_id)
                    .await
                    .map_err(|error| error.to_string())
            },
            move |result| Message::BackgroundJobRemoved { job_id, result },
        )
    }

    fn sweep_interrupted_post_process_clips(&self) -> Task<Message> {
        let Some(store) = self.clip_store.clone() else {
            return Task::none();
        };

        Task::perform(
            async move {
                let clip_ids = store
                    .clips_pending_post_process()
                    .await
                    .map_err(|error| error.to_string())?;
                for clip_id in &clip_ids {
                    store
                        .set_post_process_status(
                            *clip_id,
                            PostProcessStatus::Failed,
                            Some("interrupted by shutdown"),
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                }
                Ok(clip_ids)
            },
            Message::PostProcessRecoveryCompleted,
        )
    }

    fn process_background_job_notifications(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();

        for notification in self.background_jobs.drain_notifications() {
            match notification {
                BackgroundJobNotification::Updated(record) => {
                    tasks.push(self.persist_background_job_record(record.clone()));
                    if matches!(record.kind, BackgroundJobKind::StorageTiering) {
                        self.set_status_feedback(
                            format!(
                                "{}: {}",
                                record.label,
                                record
                                    .progress
                                    .as_ref()
                                    .map(|progress| progress.message.as_str())
                                    .unwrap_or(record.state.label())
                            ),
                            true,
                        );
                    } else if matches!(record.kind, BackgroundJobKind::AppUpdateDownload)
                        && let Some(progress) = &record.progress
                    {
                        self.updates.state.phase = if progress.message.contains("Verifying") {
                            UpdatePhase::Verifying
                        } else {
                            UpdatePhase::Downloading
                        };
                        self.updates.state.progress = Some(UpdateProgressState {
                            detail: progress.message.clone(),
                        });
                    }
                }
                BackgroundJobNotification::Finished {
                    record,
                    success,
                    error,
                } => {
                    tasks.push(self.persist_background_job_record(record.clone()));
                    match success.map(|success| *success) {
                        Some(BackgroundJobSuccess::StorageTiering {
                            moved_clip_ids,
                            message,
                        }) => {
                            self.set_status_feedback(message, false);
                            tasks.push(tabs::clips::reload_views(self));
                            if self.clips.selected_id.is_some()
                                && moved_clip_ids
                                    .contains(&self.clips.selected_id.unwrap_or_default())
                            {
                                tasks.push(self.load_clip_detail(self.clips.selected_id));
                            }
                        }
                        Some(BackgroundJobSuccess::Upload {
                            clip_id,
                            provider_label,
                            clip_url,
                            message,
                        }) => {
                            tracing::info!(
                                clip_id,
                                provider = %provider_label,
                                clip_url = ?clip_url,
                                "Upload background job completed successfully"
                            );
                            self.set_clip_filter_feedback(
                                format!("{provider_label}: {message}"),
                                true,
                            );
                            if self.clips.selected_id == Some(clip_id) {
                                tasks.push(self.load_clip_detail(Some(clip_id)));
                            }
                            tasks.push(self.queue_discord_webhook_for_uploaded_clip(
                                clip_id,
                                provider_label,
                                clip_url,
                            ));
                        }
                        Some(BackgroundJobSuccess::Montage {
                            output_path,
                            message,
                            ..
                        }) => {
                            self.set_clip_filter_feedback(
                                format!("{message} Output: {output_path}"),
                                false,
                            );
                        }
                        Some(BackgroundJobSuccess::DiscordWebhook { message, .. }) => {
                            self.set_status_feedback(message, true);
                        }
                        Some(BackgroundJobSuccess::PostProcess {
                            clip_id,
                            final_path,
                            plan,
                            tracks,
                            message,
                        }) => {
                            self.update_clip_path_in_memory(clip_id, Some(&final_path));
                            let track_summary = if tracks.is_empty() {
                                "no saved track metadata".to_string()
                            } else {
                                format!("{} audio track(s)", tracks.len())
                            };
                            let plan_summary = if let Some(mix_index) = plan.premix_stream_index {
                                format!("mixed stream at a:{mix_index}")
                            } else if plan.trimmed {
                                "trim-only rewrite".into()
                            } else {
                                "no premix stream".into()
                            };
                            self.set_clip_filter_feedback(
                                format!("{message} Stored {track_summary}; {plan_summary}."),
                                false,
                            );
                            tasks.push(tabs::clips::reload_views(self));
                            if self.clips.selected_id == Some(clip_id) {
                                tasks.push(self.load_clip_detail(Some(clip_id)));
                            }
                        }
                        Some(BackgroundJobSuccess::AppUpdateDownload { prepared, message }) => {
                            let prepared = *prepared;
                            self.updates.state.prepared_update = Some(prepared.clone());
                            self.updates.state.phase = UpdatePhase::ReadyToInstall;
                            self.updates.state.progress = None;
                            self.updates.state.last_error = None;
                            self.config.updates.prepared_update = Some(prepared.clone());
                            self.persist_update_config();

                            let follow_up = match self.config.updates.install_behavior {
                                UpdateInstallBehavior::Manual => None,
                                UpdateInstallBehavior::WhenIdle if self.can_apply_update_now() => {
                                    Some(Task::done(Message::updates(
                                        UpdateMessage::InstallDownloadedUpdateWhenIdle,
                                    )))
                                }
                                UpdateInstallBehavior::WhenIdle => {
                                    self.push_toast(
                                        ToastTone::Success,
                                        "Ready to Install",
                                        format!(
                                            "{message} It will install automatically when monitoring is idle."
                                        ),
                                        true,
                                    );
                                    None
                                }
                                UpdateInstallBehavior::OnNextLaunch => {
                                    self.push_toast(
                                        ToastTone::Success,
                                        "Ready to Install",
                                        format!("{message} It is staged for the next launch."),
                                        true,
                                    );
                                    None
                                }
                            };

                            if matches!(
                                self.config.updates.install_behavior,
                                UpdateInstallBehavior::Manual
                            ) {
                                self.push_toast(
                                    ToastTone::Success,
                                    "Ready to Install",
                                    message,
                                    true,
                                );
                            }
                            if let Some(task) = follow_up {
                                tasks.push(task);
                            }
                        }
                        None => {
                            let message = error.unwrap_or_else(|| {
                                record
                                    .detail
                                    .unwrap_or_else(|| "Background job failed.".into())
                            });
                            if matches!(record.kind, BackgroundJobKind::PostProcess) {
                                self.set_clip_filter_feedback(message.clone(), false);
                                tasks.push(tabs::clips::reload_views(self));
                                if self.clips.selected_id.is_some_and(|clip_id| {
                                    record.related_clip_ids.contains(&clip_id)
                                }) {
                                    tasks.push(self.load_clip_detail(self.clips.selected_id));
                                }
                            }
                            if matches!(record.kind, BackgroundJobKind::AppUpdateDownload) {
                                self.set_update_error(
                                    classify_update_error(
                                        message.as_str(),
                                        self.updates.state.phase,
                                    ),
                                    message.clone(),
                                );
                                self.updates.state.progress = None;
                                self.push_toast(
                                    ToastTone::Error,
                                    "Update Failed",
                                    message.clone(),
                                    true,
                                );
                                self.set_status_feedback_silent(message, false);
                            } else {
                                self.set_status_feedback(message, false);
                            }
                        }
                    }
                }
            }
        }

        Task::batch(tasks)
    }

    fn retry_background_job(&mut self, job_id: BackgroundJobId) -> Task<Message> {
        let Some(record) = self.background_jobs.record(job_id) else {
            self.set_status_feedback(format!("{job_id} is no longer available."), false);
            return Task::none();
        };

        if self.background_jobs.is_active(job_id) {
            self.set_status_feedback(
                format!("{job_id} is still active. Cancel it before retrying."),
                false,
            );
            return Task::none();
        }

        if record.state != crate::background_jobs::BackgroundJobState::Failed {
            self.set_status_feedback(
                format!("{job_id} can only be retried after it fails."),
                false,
            );
            return Task::none();
        }

        match record.kind {
            BackgroundJobKind::StorageTiering => {
                let result = if record.related_clip_ids.is_empty() {
                    Ok(BackgroundJobRetryPlan::StorageTieringSweep)
                } else if record.related_clip_ids.len() == 1 {
                    infer_storage_move_retry_target(&record).map(|target_tier| {
                        BackgroundJobRetryPlan::StorageMove {
                            clip_id: record.related_clip_ids[0],
                            target_tier,
                        }
                    })
                } else {
                    Err("storage tiering retries only support sweeps and single-clip moves.".into())
                };
                Task::done(Message::BackgroundJobRetryPrepared { job_id, result })
            }
            BackgroundJobKind::Montage => {
                let result = if record.related_clip_ids.len() >= 2 {
                    Ok(BackgroundJobRetryPlan::Montage {
                        clip_ids: record.related_clip_ids.clone(),
                    })
                } else {
                    Err("montage retries need at least two source clips.".into())
                };
                Task::done(Message::BackgroundJobRetryPrepared { job_id, result })
            }
            BackgroundJobKind::AppUpdateDownload => {
                Task::done(Message::BackgroundJobRetryPrepared {
                    job_id,
                    result: Ok(BackgroundJobRetryPlan::UpdateDownload),
                })
            }
            BackgroundJobKind::Upload
            | BackgroundJobKind::DiscordWebhook
            | BackgroundJobKind::PostProcess => {
                let Some(clip_id) = record.related_clip_ids.first().copied() else {
                    self.set_status_feedback(
                        format!("{job_id} does not reference a clip to retry."),
                        false,
                    );
                    return Task::none();
                };
                let Some(store) = self.clip_store.clone() else {
                    self.set_status_feedback("Clip database is unavailable.", false);
                    return Task::none();
                };
                let kind = record.kind;
                Task::perform(
                    async move {
                        let detail = store
                            .clip_detail(clip_id)
                            .await
                            .map_err(|error| format!("failed to load clip #{clip_id}: {error}"))?
                            .ok_or_else(|| format!("clip #{clip_id} no longer exists."))?;
                        match kind {
                            BackgroundJobKind::Upload => detail
                                .uploads
                                .iter()
                                .filter(|upload| {
                                    matches!(
                                        upload.state,
                                        ClipUploadState::Failed | ClipUploadState::Cancelled
                                    )
                                })
                                .max_by_key(|upload| upload.updated_at.timestamp_millis())
                                .map(|upload| BackgroundJobRetryPlan::Upload {
                                    clip_id,
                                    provider: upload.provider,
                                })
                                .ok_or_else(|| {
                                    format!(
                                        "clip #{clip_id} does not have a failed upload record to retry."
                                    )
                                }),
                            BackgroundJobKind::DiscordWebhook => detail
                                .uploads
                                .iter()
                                .filter_map(|upload| {
                                    upload
                                        .clip_url
                                        .as_ref()
                                        .filter(|url| !url.trim().is_empty())
                                        .map(|clip_url| (upload.updated_at, upload.provider, clip_url))
                                })
                                .max_by_key(|(updated_at, _, _)| updated_at.timestamp_millis())
                                .map(|(_, provider, clip_url)| BackgroundJobRetryPlan::DiscordWebhook {
                                    clip_id,
                                    provider_label: provider.label().into(),
                                    clip_url: clip_url.clone(),
                                })
                                .ok_or_else(|| {
                                    format!(
                                        "clip #{clip_id} does not have an uploaded clip URL available for a Discord retry."
                                    )
                                }),
                            BackgroundJobKind::PostProcess => detail
                                .clip
                                .path
                                .as_ref()
                                .filter(|path| !path.trim().is_empty())
                                .map(|path| BackgroundJobRetryPlan::PostProcess {
                                    clip_id,
                                    path: PathBuf::from(path),
                                })
                                .ok_or_else(|| {
                                    format!(
                                        "clip #{clip_id} does not have a saved file path available for audio post-process retry."
                                    )
                                }),
                            _ => Err("unsupported retry kind.".into()),
                        }
                    },
                    move |result| Message::BackgroundJobRetryPrepared { job_id, result },
                )
            }
        }
    }

    fn execute_background_job_retry(&mut self, plan: BackgroundJobRetryPlan) -> Task<Message> {
        match plan {
            BackgroundJobRetryPlan::StorageTieringSweep => self.queue_storage_tiering_sweep(),
            BackgroundJobRetryPlan::StorageMove {
                clip_id,
                target_tier,
            } => self.queue_clip_storage_move(clip_id, target_tier),
            BackgroundJobRetryPlan::Upload { clip_id, provider } => {
                self.queue_clip_upload(clip_id, provider)
            }
            BackgroundJobRetryPlan::Montage { clip_ids } => {
                self.queue_montage_creation_for_clip_ids(clip_ids, false)
            }
            BackgroundJobRetryPlan::DiscordWebhook {
                clip_id,
                provider_label,
                clip_url,
            } => self.queue_discord_webhook_for_uploaded_clip(
                clip_id,
                provider_label,
                Some(clip_url),
            ),
            BackgroundJobRetryPlan::PostProcess { clip_id, path } => {
                self.queue_post_process_retry_for_clip(clip_id, path)
            }
            BackgroundJobRetryPlan::UpdateDownload => self.queue_update_download(),
        }
    }

    fn remove_background_job(&mut self, job_id: BackgroundJobId) -> Task<Message> {
        if self.background_jobs.is_active(job_id) {
            self.set_status_feedback(
                format!("{job_id} is still active. Cancel it before removing it."),
                false,
            );
            return Task::none();
        }

        if self.background_jobs.remove_history(job_id).is_none() {
            self.set_status_feedback(format!("{job_id} is no longer available."), false);
            return Task::none();
        }

        self.delete_background_job_record(job_id)
    }

    fn queue_update_download(&mut self) -> Task<Message> {
        let Some(release) = self.updates.state.latest_release.clone() else {
            self.set_status_feedback_silent("Check for updates before downloading one.", false);
            return Task::none();
        };
        self.queue_release_download(release)
    }

    fn queue_release_download(&mut self, release: update::AvailableRelease) -> Task<Message> {
        if matches!(
            self.updates.state.phase,
            UpdatePhase::Downloading | UpdatePhase::Verifying | UpdatePhase::Applying
        ) {
            self.set_status_feedback_silent("An update operation is already in progress.", true);
            return Task::none();
        }
        if !release.supports_download() {
            self.set_status_feedback_silent(
                release_policy_summary(
                    &release,
                    &self.updates.state.current_version,
                    self.updates.state.system_update_plan.as_ref(),
                ),
                false,
            );
            return Task::none();
        }
        if self
            .updates
            .state
            .prepared_update
            .as_ref()
            .is_some_and(|prepared| prepared.version == release.version.to_string())
        {
            let action_title =
                release_action_title(&release.version, &self.updates.state.current_version);
            self.set_status_feedback_silent(
                format!(
                    "{action_title} target {} is already downloaded.",
                    release.version
                ),
                true,
            );
            return Task::none();
        }

        let action_label =
            release_action_label(&release.version, &self.updates.state.current_version);
        let action_title =
            release_action_title(&release.version, &self.updates.state.current_version);
        self.updates.state.phase = UpdatePhase::Downloading;
        self.updates.state.progress = Some(UpdateProgressState {
            detail: format!("Preparing to download {action_label} {}.", release.version),
        });
        self.updates.state.last_error = None;

        let release_for_job = release.clone();
        let version_label = release.version.to_string();
        let job_id = self.background_jobs.start(
            BackgroundJobKind::AppUpdateDownload,
            format!("Download {action_label} {}", release.version),
            Vec::new(),
            move |ctx| async move {
                ctx.progress(
                    0,
                    100,
                    format!("Starting {action_label} download for {}.", version_label),
                )?;
                let prepared =
                    update::download::download_release_asset(&release_for_job, |progress| {
                        let (message, step) = match progress.step {
                            update::DownloadStep::Downloading => {
                                let message = format!(
                                    "Downloading {} {} ({}; {}).",
                                    action_label,
                                    version_label,
                                    release_for_job
                                        .asset
                                        .as_ref()
                                        .map(|asset| asset.kind.label())
                                        .unwrap_or("asset"),
                                    format_update_download_progress(
                                        progress.downloaded_bytes,
                                        progress.total_bytes,
                                    )
                                );
                                let step = progress
                                    .total_bytes
                                    .map(|total| {
                                        (((progress.downloaded_bytes as f64 / total.max(1) as f64)
                                            * 90.0)
                                            .round()
                                            as u32)
                                            .clamp(1, 90)
                                    })
                                    .unwrap_or(45);
                                (message, step)
                            }
                            update::DownloadStep::Verifying => {
                                ("Verifying the downloaded update checksum.".into(), 95)
                            }
                        };
                        ctx.progress(step, 100, message)
                    })
                    .await?;

                ctx.progress(
                    100,
                    100,
                    format!("Downloaded {action_label} target {}.", prepared.version),
                )?;
                Ok(BackgroundJobSuccess::AppUpdateDownload {
                    prepared: Box::new(prepared),
                    message: format!("Downloaded {action_title} target {}.", version_label),
                })
            },
        );

        self.set_status_feedback(
            format!("Queued {action_label} download for {}.", release.version),
            true,
        );
        self.persist_background_job_snapshot(job_id)
    }

    fn queue_storage_tiering_sweep(&mut self) -> Task<Message> {
        if !self.config.storage_tiering.enabled {
            self.set_settings_feedback(
                "Enable storage tiering and choose an archive directory first.",
                false,
            );
            return Task::none();
        }
        let Some(store) = self.clip_store.clone() else {
            self.set_settings_feedback("Clip database is unavailable.", false);
            return Task::none();
        };

        let config = self.config.storage_tiering.clone();
        let primary_dir = self.config.recorder.save_directory.clone();
        let job_id = self.background_jobs.start(
            BackgroundJobKind::StorageTiering,
            "Storage tiering sweep",
            Vec::new(),
            move |ctx| async move {
                ctx.progress(1, 3, "Loading clip catalog.")?;
                let clips = store
                    .all_clips()
                    .await
                    .map_err(|error| format!("failed to load clips for tiering: {error}"))?;
                let candidates: Vec<_> = clips
                    .iter()
                    .filter_map(storage_tiering::tiering_candidate_from_clip)
                    .filter_map(|candidate| {
                        storage_tiering::plan_archive_move(
                            &config,
                            &primary_dir,
                            Utc::now(),
                            &candidate,
                        )
                    })
                    .collect();

                if candidates.is_empty() {
                    return Ok(BackgroundJobSuccess::StorageTiering {
                        moved_clip_ids: Vec::new(),
                        message: "No clips matched the current storage tiering policy.".into(),
                    });
                }

                let total = candidates.len() as u32;
                let mut moved_clip_ids = Vec::new();
                for (index, plan) in candidates.into_iter().enumerate() {
                    ctx.progress(
                        (index as u32) + 2,
                        total + 1,
                        format!("Moving clip #{} to archive storage.", plan.clip_id),
                    )?;
                    let result = storage_tiering::execute_move_plan(&plan)?;
                    store
                        .update_clip_path(
                            result.clip_id,
                            Some(result.destination_path.to_string_lossy().as_ref()),
                        )
                        .await
                        .map_err(|error| format!("failed to update clip path: {error}"))?;
                    moved_clip_ids.push(result.clip_id);
                }

                Ok(BackgroundJobSuccess::StorageTiering {
                    moved_clip_ids: moved_clip_ids.clone(),
                    message: format!("Moved {} clips to archive storage.", moved_clip_ids.len()),
                })
            },
        );

        self.set_settings_feedback("Queued storage tiering sweep.", true);
        self.persist_background_job_snapshot(job_id)
    }

    fn queue_clip_storage_move(&mut self, clip_id: i64, target_tier: StorageTier) -> Task<Message> {
        let Some(store) = self.clip_store.clone() else {
            self.set_clip_error("Clip database is unavailable.");
            return Task::none();
        };
        let Some(record) = self
            .clips
            .history_source
            .iter()
            .find(|record| record.id == clip_id)
            .cloned()
        else {
            self.set_clip_error(format!("Clip #{clip_id} is no longer available."));
            return Task::none();
        };
        let Some(candidate) = storage_tiering::tiering_candidate_from_clip(&record) else {
            self.set_clip_error("This clip does not have a saved file path yet.");
            return Task::none();
        };

        let config = self.config.storage_tiering.clone();
        let primary_dir = self.config.recorder.save_directory.clone();
        let plan = match target_tier {
            StorageTier::Archive => storage_tiering::plan_archive_move(
                &config,
                &primary_dir,
                Utc::now() + chrono::Duration::days(i64::from(config.min_age_days)),
                &candidate,
            ),
            StorageTier::Primary => {
                storage_tiering::plan_restore_move(&config, &primary_dir, &candidate)
            }
        };
        let Some(plan) = plan else {
            self.set_clip_error(match target_tier {
                StorageTier::Archive => "This clip does not currently qualify for archive storage.",
                StorageTier::Primary => "This clip is already stored on primary storage.",
            });
            return Task::none();
        };

        let job_id = self.background_jobs.start(
            BackgroundJobKind::StorageTiering,
            format!(
                "Move clip #{clip_id} to {}",
                target_tier.label().to_lowercase()
            ),
            vec![clip_id],
            move |ctx| async move {
                ctx.progress(1, 2, "Moving clip file.")?;
                let result = storage_tiering::execute_move_plan(&plan)?;
                store
                    .update_clip_path(
                        result.clip_id,
                        Some(result.destination_path.to_string_lossy().as_ref()),
                    )
                    .await
                    .map_err(|error| format!("failed to update clip path: {error}"))?;
                ctx.progress(2, 2, "Clip move completed.")?;
                Ok(BackgroundJobSuccess::StorageTiering {
                    moved_clip_ids: vec![clip_id],
                    message: format!(
                        "Moved clip #{} to {} storage.",
                        clip_id,
                        result.target_tier.label().to_lowercase()
                    ),
                })
            },
        );

        self.persist_background_job_snapshot(job_id)
    }

    fn queue_clip_upload(&mut self, clip_id: i64, provider: UploadProvider) -> Task<Message> {
        let Some(store) = self.clip_store.clone() else {
            self.set_clip_error("Clip database is unavailable.");
            return Task::none();
        };
        let Some(record) = self
            .clips
            .history_source
            .iter()
            .find(|record| record.id == clip_id)
            .cloned()
        else {
            self.set_clip_error(format!("Clip #{clip_id} is no longer available."));
            return Task::none();
        };
        if let Some(reason) = clip_post_process_block_reason(&record) {
            self.set_clip_error(reason);
            return Task::none();
        }
        let Some(path) = record.path.clone() else {
            self.set_clip_error("This clip does not have a saved file path yet.");
            return Task::none();
        };
        if !PathBuf::from(&path).exists() {
            self.set_clip_error(format!("Clip file does not exist: {path}"));
            return Task::none();
        }

        let title = self.build_upload_title(&record);
        let description = self.build_upload_description(&record);
        tracing::info!(
            clip_id,
            provider = provider.label(),
            path = %path,
            "Queueing clip upload"
        );
        let request = uploads::UploadRequest {
            clip_id,
            clip_path: PathBuf::from(&path),
            title,
            description,
        };
        let secure_store = self.platform.secure_store().clone();
        let copyparty = self.config.uploads.copyparty.clone();
        let youtube = self.config.uploads.youtube.clone();

        let job_id = self.background_jobs.start(
            BackgroundJobKind::Upload,
            format!("Upload clip #{} to {}", clip_id, provider.label()),
            vec![clip_id],
            move |ctx| async move {
                let detail = store
                    .clip_detail(clip_id)
                    .await
                    .map_err(|error| format!("failed to inspect upload history: {error}"))?;
                let already_uploaded = detail.as_ref().is_some_and(|detail| {
                    detail.uploads.iter().any(|upload| {
                        upload.provider == provider
                            && matches!(
                                upload.state,
                                ClipUploadState::Running | ClipUploadState::Succeeded
                            )
                    })
                });
                if already_uploaded {
                    return Err(format!(
                        "Clip #{} has already been uploaded to {}.",
                        clip_id,
                        provider.label()
                    ));
                }

                let running_upload_id = store
                    .insert_clip_upload(ClipUploadDraft {
                        clip_id,
                        provider,
                        state: ClipUploadState::Running,
                        external_id: None,
                        clip_url: None,
                        error_message: None,
                    })
                    .await
                    .map_err(|error| format!("failed to create upload history row: {error}"))?;

                let completion = match provider {
                    UploadProvider::Copyparty => {
                        let password =
                            secure_store
                                .get(SecretKey::CopypartyPassword)?
                                .ok_or_else(|| {
                                    "Store a Copyparty password in Settings before uploading."
                                        .to_string()
                                })?;
                        uploads::upload_to_copyparty(
                            ctx,
                            request,
                            CopypartyUploadCredentials {
                                upload_url: copyparty.upload_url,
                                public_base_url: copyparty.public_base_url,
                                username: copyparty.username,
                                password,
                            },
                        )
                        .await
                    }
                    UploadProvider::YouTube => {
                        let refresh_token = secure_store
                            .get(SecretKey::YoutubeRefreshToken)?
                            .ok_or_else(|| {
                                "Connect a YouTube account in Settings before uploading."
                                    .to_string()
                            })?;
                        let client_secret = secure_store.get(SecretKey::YoutubeClientSecret)?;
                        uploads::upload_to_youtube(
                            ctx,
                            request,
                            detail
                                .as_ref()
                                .map(|detail| detail.audio_tracks.as_slice())
                                .unwrap_or(&[]),
                            YouTubeUploadCredentials {
                                client_id: youtube.client_id,
                                client_secret,
                                refresh_token,
                                privacy_status: youtube.privacy_status,
                            },
                        )
                        .await
                    }
                };

                match completion {
                    Ok(completion) => {
                        store
                            .update_clip_upload(
                                running_upload_id,
                                ClipUploadState::Succeeded,
                                completion.external_id.as_deref(),
                                completion.clip_url.as_deref(),
                                None,
                            )
                            .await
                            .map_err(|error| {
                                format!("failed to finalize upload history: {error}")
                            })?;
                        Ok(BackgroundJobSuccess::Upload {
                            clip_id,
                            provider_label: completion.provider_label,
                            clip_url: completion.clip_url.clone(),
                            message: match (&completion.note, &completion.clip_url) {
                                (Some(note), Some(url)) => format!("{note} {url}"),
                                (Some(note), None) => note.clone(),
                                (None, Some(url)) => url.clone(),
                                (None, None) => "Upload finished.".into(),
                            },
                        })
                    }
                    Err(error) => {
                        let state = if error == "Job cancelled." {
                            ClipUploadState::Cancelled
                        } else {
                            ClipUploadState::Failed
                        };
                        store
                            .update_clip_upload(running_upload_id, state, None, None, Some(&error))
                            .await
                            .map_err(|store_error| {
                                format!("failed to record upload failure: {store_error}")
                            })?;
                        Err(error)
                    }
                }
            },
        );

        self.set_clip_filter_feedback(
            format!("Queued {} upload for clip #{}.", provider.label(), clip_id),
            true,
        );
        self.persist_background_job_snapshot(job_id)
    }

    fn queue_montage_creation(&mut self) -> Task<Message> {
        self.queue_montage_creation_for_clip_ids(self.clips.montage_selection.clone(), true)
    }

    fn queue_montage_creation_for_clip_ids(
        &mut self,
        clip_ids: Vec<i64>,
        clear_selection: bool,
    ) -> Task<Message> {
        if clip_ids.len() < 2 {
            self.set_clip_error("Choose at least two clips for a montage.");
            return Task::none();
        }
        let Some(store) = self.clip_store.clone() else {
            self.set_clip_error("Clip database is unavailable.");
            return Task::none();
        };

        let mut clips = Vec::new();
        for clip_id in &clip_ids {
            let Some(record) = self
                .clips
                .history_source
                .iter()
                .find(|record| record.id == *clip_id)
            else {
                self.set_clip_error(format!("Clip #{clip_id} is no longer available."));
                return Task::none();
            };
            if let Some(reason) = clip_post_process_block_reason(record) {
                self.set_clip_error(reason);
                return Task::none();
            }
            let Some(path) = &record.path else {
                self.set_clip_error(format!("Clip #{clip_id} does not have a saved file path."));
                return Task::none();
            };
            clips.push(MontageClip {
                clip_id: *clip_id,
                path: PathBuf::from(path),
                post_process_status: record.post_process_status,
                audio_tracks: Vec::new(),
            });
        }

        let save_dir = self.config.recorder.save_directory.clone();
        let job_id = self.background_jobs.start(
            BackgroundJobKind::Montage,
            "Create montage",
            clip_ids.clone(),
            move |ctx| async move {
                let mut clips = clips;
                for clip in &mut clips {
                    if let Some(detail) =
                        store.clip_detail(clip.clip_id).await.map_err(|error| {
                            format!("failed to load clip #{} for montage: {error}", clip.clip_id)
                        })?
                    {
                        clip.post_process_status = detail.clip.post_process_status;
                        clip.audio_tracks = detail.audio_tracks;
                    }
                }
                let result = crate::montage::create_concat_montage(ctx, save_dir, clips).await?;
                store
                    .insert_montage(
                        result.output_path.to_string_lossy().as_ref(),
                        &result.source_clip_ids,
                    )
                    .await
                    .map_err(|error| format!("failed to record montage output: {error}"))?;
                Ok(BackgroundJobSuccess::Montage {
                    output_path: result.output_path.to_string_lossy().into_owned(),
                    source_clip_ids: result.source_clip_ids.clone(),
                    message: if result.normalized_clip_count > 0 {
                        format!(
                            "Montage ready. Merged {} clips ({} normalized).",
                            result.source_clip_ids.len(),
                            result.normalized_clip_count
                        )
                    } else {
                        format!(
                            "Created montage from {} clips.",
                            result.source_clip_ids.len()
                        )
                    },
                })
            },
        );

        if clear_selection {
            self.clips.montage_selection.clear();
            self.clips.selected_montage_clip_id = None;
        }

        self.persist_background_job_snapshot(job_id)
    }

    fn queue_discord_webhook_for_uploaded_clip(
        &mut self,
        clip_id: i64,
        provider_label: String,
        clip_url: Option<String>,
    ) -> Task<Message> {
        if !self.config.discord_webhook.enabled {
            tracing::info!(
                clip_id,
                provider = %provider_label,
                "Skipping Discord webhook because the feature is disabled"
            );
            return Task::none();
        }
        let Some(clip_url) = clip_url.filter(|value| !value.trim().is_empty()) else {
            tracing::warn!(
                clip_id,
                provider = %provider_label,
                "Skipping Discord webhook because the upload did not return a clip URL"
            );
            return Task::none();
        };
        let Ok(Some(_)) = self
            .platform
            .secure_store()
            .get(SecretKey::DiscordWebhookUrl)
        else {
            tracing::warn!(
                clip_id,
                provider = %provider_label,
                "Skipping Discord webhook because no webhook URL is configured"
            );
            return Task::none();
        };
        let Some(store) = self.clip_store.clone() else {
            tracing::warn!(
                clip_id,
                provider = %provider_label,
                "Skipping Discord webhook because the clip database is unavailable"
            );
            return Task::none();
        };

        tracing::info!(
            clip_id,
            provider = %provider_label,
            clip_url = %clip_url,
            "Resolving clip metadata for post-upload Discord webhook"
        );
        Task::perform(
            async move {
                let detail = store
                    .clip_detail(clip_id)
                    .await
                    .map_err(|error| format!("failed to load clip #{clip_id}: {error}"))?;
                Ok(detail.map(|detail| PostUploadDiscordClipLoaded {
                    clip_id,
                    provider_label,
                    clip_url: Some(clip_url),
                    clip: detail.clip,
                }))
            },
            Message::PostUploadDiscordClipLoaded,
        )
    }

    fn start_discord_webhook_for_uploaded_clip(
        &mut self,
        payload: PostUploadDiscordClipLoaded,
    ) -> Task<Message> {
        let PostUploadDiscordClipLoaded {
            clip_id,
            provider_label,
            clip_url,
            clip: record,
        } = payload;

        if record.score < self.config.discord_webhook.min_score {
            tracing::info!(
                clip_id,
                provider = %provider_label,
                score = record.score,
                min_score = self.config.discord_webhook.min_score,
                "Skipping Discord webhook because clip score is below the configured threshold"
            );
            return Task::none();
        }

        let webhook_url = match self
            .platform
            .secure_store()
            .get(SecretKey::DiscordWebhookUrl)
        {
            Ok(Some(url)) => url,
            Ok(None) => {
                tracing::warn!(
                    clip_id,
                    provider = %provider_label,
                    "Skipping Discord webhook because no webhook URL is configured"
                );
                return Task::none();
            }
            Err(error) => {
                tracing::warn!(
                    clip_id,
                    provider = %provider_label,
                    "Skipping Discord webhook because the secure store lookup failed: {error}"
                );
                return Task::none();
            }
        };

        let Some(clip_url) = clip_url.filter(|value| !value.trim().is_empty()) else {
            tracing::warn!(
                clip_id,
                provider = %provider_label,
                "Skipping Discord webhook because the resolved clip URL was empty"
            );
            return Task::none();
        };

        let request = DiscordWebhookRequest {
            webhook_url,
            clip_title: self.build_upload_title(&record),
            clip_path: record.path.as_ref().map(PathBuf::from),
            clip_url: Some(clip_url.clone()),
            score: record.score,
            profile_name: self.profile_label_for_record(&record),
            rule_name: self.rule_label_for_record(&record),
            character_name: self.character_label_for_record(&record),
            location_label: record
                .facility_name
                .clone()
                .or_else(|| census::base_name(record.facility_id))
                .unwrap_or_else(|| census::continent_name(record.zone_id)),
            event_timestamp_label: tabs::clips::format_timestamp(record.trigger_event_at),
            include_thumbnail: self.config.discord_webhook.include_thumbnail,
        };

        tracing::info!(
            clip_id,
            provider = %provider_label,
            score = record.score,
            webhook = true,
            "Queueing Discord webhook after upload"
        );

        let job_id = self.background_jobs.start(
            BackgroundJobKind::DiscordWebhook,
            format!(
                "Send Discord webhook for clip #{} after {} upload",
                clip_id, provider_label
            ),
            vec![clip_id],
            move |ctx| async move {
                crate::discord::send_clip_webhook(ctx, request).await?;
                Ok(BackgroundJobSuccess::DiscordWebhook {
                    clip_id,
                    message: format!(
                        "Discord webhook sent for clip #{} after {} upload.",
                        clip_id, provider_label
                    ),
                })
            },
        );

        self.persist_background_job_snapshot(job_id)
    }

    pub(in crate::app) fn start_youtube_oauth(&mut self) -> Task<Message> {
        let client_id = self.settings.youtube_client_id.trim().to_string();
        if client_id.is_empty() {
            self.set_settings_feedback("Enter a YouTube desktop OAuth client ID first.", false);
            return Task::none();
        }

        let client_secret_input = self.settings.youtube_client_secret_input.trim().to_string();
        let platform = self.platform.clone();
        let secure_store = self.platform.secure_store().clone();
        info!(
            secure_store_backend = %secure_store.backend().label(),
            client_secret_input_present = !client_secret_input.is_empty(),
            "Scheduling YouTube OAuth task"
        );
        self.settings.youtube_oauth_in_flight = true;

        Task::perform(
            async move {
                if !client_secret_input.is_empty() {
                    info!("Persisting YouTube client secret before OAuth flow");
                    secure_store.set(SecretKey::YoutubeClientSecret, &client_secret_input)?;
                }
                info!("Resolving YouTube client secret for OAuth flow");
                let tokens = uploads::begin_youtube_oauth(
                    YouTubeOAuthClient {
                        client_id,
                        client_secret: if client_secret_input.is_empty() {
                            secure_store.get(SecretKey::YoutubeClientSecret)?
                        } else {
                            Some(client_secret_input)
                        },
                    },
                    move |url| platform.open_url(url),
                )
                .await?;
                info!(
                    refresh_token_len = tokens.refresh_token.len(),
                    "YouTube OAuth returned a refresh token; attempting to store it"
                );
                secure_store.set(SecretKey::YoutubeRefreshToken, &tokens.refresh_token)?;
                info!("Stored YouTube refresh token successfully");
                Ok::<(), String>(())
            },
            Message::YouTubeOAuthCompleted,
        )
    }

    fn build_upload_title(&self, record: &ClipRecord) -> String {
        format!(
            "{} | {} | {}",
            self.character_label_for_record(record),
            self.rule_label_for_record(record),
            tabs::clips::format_timestamp(record.trigger_event_at)
        )
    }

    fn build_upload_description(&self, record: &ClipRecord) -> String {
        format!(
            "Profile: {}\nRule: {}\nCharacter: {}\nScore: {}\nServer: {}\nContinent: {}\nBase: {}",
            self.profile_label_for_record(record),
            self.rule_label_for_record(record),
            self.character_label_for_record(record),
            record.score,
            census::world_name(record.world_id),
            census::continent_name(record.zone_id),
            record
                .facility_name
                .clone()
                .or_else(|| census::base_name(record.facility_id))
                .unwrap_or_else(|| "Unknown".into()),
        )
    }

    fn profile_label_for_record(&self, record: &ClipRecord) -> String {
        self.config
            .rule_profiles
            .iter()
            .find(|profile| profile.id == record.profile_id)
            .map(|profile| profile.name.clone())
            .unwrap_or_else(|| record.profile_id.clone())
    }

    fn rule_label_for_record(&self, record: &ClipRecord) -> String {
        if record.origin == ClipOrigin::Manual {
            return "Manual Clip".into();
        }
        if record.origin == ClipOrigin::Imported {
            return "Imported Clip".into();
        }
        self.config
            .rule_definitions
            .iter()
            .find(|rule| rule.id == record.rule_id)
            .map(|rule| rule.name.clone())
            .unwrap_or_else(|| record.rule_id.clone())
    }

    fn character_label_for_record(&self, record: &ClipRecord) -> String {
        if record.character_id == 0 {
            return "Unassigned".into();
        }
        self.config
            .characters
            .iter()
            .find(|character| character.character_id == Some(record.character_id))
            .map(|character| character.name.clone())
            .unwrap_or_else(|| format!("Character {}", record.character_id))
    }

    pub(in crate::app) fn set_clip_error(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.clips.error = Some(message.clone());
        self.clips.error_expires_at = Some(Instant::now() + Self::ERROR_MESSAGE_TIMEOUT);
        self.push_toast(ToastTone::Error, "Clips", message, true);
    }

    pub(in crate::app) fn clear_clip_error(&mut self) {
        self.clips.error = None;
        self.clips.error_expires_at = None;
    }

    pub(in crate::app) fn set_clip_filter_feedback(
        &mut self,
        message: impl Into<String>,
        auto_dismiss: bool,
    ) {
        let message = message.into();
        self.clips.filter_feedback = Some(message.clone());
        self.clips.filter_feedback_expires_at =
            Some(Instant::now() + Self::feedback_timeout(auto_dismiss));
        self.push_feedback_toast("Clips", message, auto_dismiss);
    }

    pub(in crate::app) fn clear_clip_filter_feedback(&mut self) {
        self.clips.filter_feedback = None;
        self.clips.filter_feedback_expires_at = None;
    }

    pub(in crate::app) fn set_settings_feedback(
        &mut self,
        message: impl Into<String>,
        auto_dismiss: bool,
    ) {
        let message = message.into();
        self.settings.feedback = Some(message.clone());
        self.settings.feedback_expires_at =
            Some(Instant::now() + Self::feedback_timeout(auto_dismiss));
        self.push_feedback_toast("Settings", message, auto_dismiss);
    }

    pub(in crate::app) fn set_settings_feedback_silent(
        &mut self,
        message: impl Into<String>,
        auto_dismiss: bool,
    ) {
        let message = message.into();
        self.settings.feedback = Some(message);
        self.settings.feedback_expires_at =
            Some(Instant::now() + Self::feedback_timeout(auto_dismiss));
    }

    pub(in crate::app) fn clear_settings_feedback(&mut self) {
        self.settings.feedback = None;
        self.settings.feedback_expires_at = None;
    }

    pub(in crate::app) fn set_status_feedback(
        &mut self,
        message: impl Into<String>,
        auto_dismiss: bool,
    ) {
        let message = message.into();
        self.runtime.status_feedback = Some(message.clone());
        self.runtime.status_feedback_expires_at =
            Some(Instant::now() + Self::feedback_timeout(auto_dismiss));
        self.push_feedback_toast("Status", message, auto_dismiss);
    }

    pub(in crate::app) fn set_status_feedback_silent(
        &mut self,
        message: impl Into<String>,
        auto_dismiss: bool,
    ) {
        let message = message.into();
        self.runtime.status_feedback = Some(message);
        self.runtime.status_feedback_expires_at =
            Some(Instant::now() + Self::feedback_timeout(auto_dismiss));
    }

    pub(in crate::app) fn clear_status_feedback(&mut self) {
        self.runtime.status_feedback = None;
        self.runtime.status_feedback_expires_at = None;
    }

    pub(in crate::app) fn set_rules_feedback(
        &mut self,
        message: impl Into<String>,
        auto_dismiss: bool,
    ) {
        let message = message.into();
        self.rules.feedback = Some(message.clone());
        self.rules.feedback_expires_at =
            Some(Instant::now() + Self::feedback_timeout(auto_dismiss));
        self.push_feedback_toast("Rules", message, auto_dismiss);
    }

    fn push_feedback_toast(&mut self, title: &str, message: String, auto_dismiss: bool) {
        self.push_toast(ToastTone::Info, title, message, auto_dismiss);
    }

    pub(in crate::app) fn push_success_toast(
        &mut self,
        title: &str,
        message: impl Into<String>,
        auto_dismiss: bool,
    ) {
        self.push_toast(ToastTone::Success, title, message.into(), auto_dismiss);
    }

    fn push_toast(&mut self, tone: ToastTone, title: &str, message: String, auto_dismiss: bool) {
        self.toasts.push_with(
            tone,
            title,
            Some(message),
            Some(Self::feedback_timeout(auto_dismiss)),
        );
    }

    fn feedback_timeout(auto_dismiss: bool) -> Duration {
        if auto_dismiss {
            Self::ERROR_MESSAGE_TIMEOUT
        } else {
            Self::EXTENDED_MESSAGE_TIMEOUT
        }
    }

    pub(in crate::app) fn clear_rules_feedback(&mut self) {
        self.rules.feedback = None;
        self.rules.feedback_expires_at = None;
    }

    fn dismiss_expired_feedback(&mut self) {
        let now = Instant::now();

        if self
            .clips
            .error_expires_at
            .is_some_and(|expires_at| now >= expires_at)
        {
            self.clear_clip_error();
        }

        if self
            .clips
            .filter_feedback_expires_at
            .is_some_and(|expires_at| now >= expires_at)
        {
            self.clear_clip_filter_feedback();
        }

        if self
            .settings
            .feedback_expires_at
            .is_some_and(|expires_at| now >= expires_at)
        {
            self.clear_settings_feedback();
        }

        if self
            .runtime
            .status_feedback_expires_at
            .is_some_and(|expires_at| now >= expires_at)
        {
            self.clear_status_feedback();
        }

        if self
            .rules
            .feedback_expires_at
            .is_some_and(|expires_at| now >= expires_at)
        {
            self.clear_rules_feedback();
        }
    }

    fn configure_hotkeys(&mut self, show_success_toast: bool) -> Task<Message> {
        self.runtime.hotkey_config_generation += 1;
        #[cfg(not(target_os = "windows"))]
        let generation = self.runtime.hotkey_config_generation;
        let previous_binding_label = self.runtime.hotkeys.binding_label().map(str::to_string);
        self.settings.pending_hotkey_binding_label = previous_binding_label.clone();
        self.settings.pending_hotkey_success_toast = show_success_toast;
        let config = self.config.manual_clip.clone();
        let display_server = process::detect_display_server();
        let desktop_environment = process::detect_desktop_environment();
        tracing::debug!(
            generation = self.runtime.hotkey_config_generation,
            enabled = config.enabled,
            hotkey = %config.hotkey,
            duration_secs = config.duration_secs,
            ?display_server,
            ?desktop_environment,
            "starting manual clip hotkey configuration"
        );
        if let Some(previous_binding_label) = previous_binding_label {
            tracing::debug!(
                generation = self.runtime.hotkey_config_generation,
                previous_binding_label,
                "dropping existing manual clip hotkey before reconfiguration"
            );
        }
        self.runtime.hotkeys = self.platform.disabled_hotkeys();

        #[cfg(target_os = "windows")]
        {
            let result =
                self.platform
                    .configure_hotkeys_sync(&config, display_server, desktop_environment);
            self.finish_hotkey_configuration(result);
            Task::none()
        }

        #[cfg(not(target_os = "windows"))]
        let platform = self.platform.clone();

        #[cfg(not(target_os = "windows"))]
        Task::perform(
            async move {
                platform
                    .configure_hotkeys(&config, display_server, desktop_environment)
                    .await
                    .map(share_hotkey_config_result)
            },
            move |result| {
                Message::runtime(RuntimeMessage::HotkeysConfigured { generation, result })
            },
        )
    }

    fn finish_hotkey_configuration(&mut self, result: Result<HotkeyManager, String>) {
        let previous_binding_label = self.settings.pending_hotkey_binding_label.take();
        let show_success_toast = std::mem::take(&mut self.settings.pending_hotkey_success_toast);
        match result {
            Ok(hotkeys) => {
                let binding_label = hotkeys.binding_label().map(str::to_string);
                let configuration_note = hotkeys.configuration_note().map(str::to_string);
                tracing::debug!(
                    enabled = self.config.manual_clip.enabled,
                    binding_label = ?binding_label,
                    configuration_note = ?configuration_note,
                    "manual clip hotkey configuration completed"
                );
                self.runtime.hotkeys = hotkeys;
                if self.config.manual_clip.enabled {
                    match hotkey_configuration_feedback(
                        show_success_toast,
                        previous_binding_label.as_deref(),
                        binding_label.as_deref(),
                        configuration_note.as_deref(),
                    ) {
                        Some(HotkeyConfigurationFeedback::Success(message)) => {
                            self.set_settings_feedback(message, true);
                        }
                        Some(HotkeyConfigurationFeedback::Note(message)) => {
                            self.set_settings_feedback_silent(message, false);
                        }
                        None => {}
                    }
                }
            }
            Err(error) => {
                tracing::warn!(enabled = self.config.manual_clip.enabled, %error, "manual clip hotkey configuration failed");
                self.set_settings_feedback(error, false);
                self.runtime.hotkeys = self.platform.disabled_hotkeys();
            }
        }
    }

    fn tray_snapshot(&self) -> TraySnapshot {
        let mut status_label = match &self.runtime.lifecycle {
            AppState::Idle => "Idle".into(),
            AppState::WaitingForGame => "Waiting for PlanetSide 2".into(),
            AppState::WaitingForLogin => "Waiting for character login".into(),
            AppState::Monitoring { character_name, .. } => {
                format!("Monitoring {character_name}")
            }
        };

        if let Some(status) = &self.runtime.obs_connection_status {
            status_label = match status {
                capture::ObsConnectionStatus::Connected => status_label,
                capture::ObsConnectionStatus::Reconnecting {
                    attempt,
                    next_retry_in_secs,
                } => {
                    format!("OBS reconnecting (attempt {attempt}, retry in {next_retry_in_secs}s)")
                }
                capture::ObsConnectionStatus::Failed { reason } => {
                    format!("OBS reconnect failed: {reason}")
                }
            };
        }

        TraySnapshot {
            title: "NaniteClip".into(),
            status_label,
            can_start_monitoring: matches!(self.runtime.lifecycle, AppState::Idle),
            can_stop_monitoring: !matches!(self.runtime.lifecycle, AppState::Idle),
            profile_options: self
                .config
                .rule_profiles
                .iter()
                .map(|profile| TrayProfileOption {
                    id: profile.id.clone(),
                    name: profile.name.clone(),
                    selected: profile.id == self.config.active_profile_id,
                })
                .collect(),
        }
    }

    pub(in crate::app) fn sync_tray_snapshot(&self) -> Task<Message> {
        let Some(tray) = &self.runtime.tray else {
            return Task::none();
        };
        tray.update_snapshot(self.tray_snapshot());
        Task::none()
    }

    fn apply_backend_runtime_event(&mut self, event: capture::BackendRuntimeEvent) -> bool {
        match event {
            capture::BackendRuntimeEvent::ObsConnection(status) => match status {
                capture::ObsConnectionStatus::Connected => {
                    self.runtime.obs_restart_requires_manual_restart = false;
                    let changed = self.runtime.obs_connection_status.take().is_some();
                    if changed {
                        self.set_status_feedback("OBS reconnected.", true);
                    }
                    changed
                }
                ref other @ capture::ObsConnectionStatus::Failed { ref reason } => {
                    let changed = self.runtime.obs_connection_status.as_ref() != Some(other);
                    self.runtime.obs_restart_requires_manual_restart = true;
                    self.runtime.obs_connection_status = Some(other.clone());
                    if changed {
                        self.set_status_feedback(
                            format!(
                                "OBS reconnect attempts were exhausted: {reason}. Restart monitoring after OBS is available again."
                            ),
                            false,
                        );
                    }
                    changed
                }
                other => {
                    let changed = self.runtime.obs_connection_status.as_ref() != Some(&other);
                    self.runtime.obs_connection_status = Some(other);
                    changed
                }
            },
        }
    }

    fn show_window_task(&mut self) -> Task<Message> {
        if let Some(window_id) = self.runtime.main_window_id {
            Task::batch([
                window::minimize(window_id, false),
                window::gain_focus(window_id),
            ])
        } else {
            self.open_main_window_task()
        }
    }

    fn open_main_window_task(&mut self) -> Task<Message> {
        if self.runtime.main_window_id.is_some() {
            return Task::none();
        }

        let (window_id, task) = window::open(main_window_settings());
        self.runtime.main_window_id = Some(window_id);
        task.map(|window_id| Message::runtime(RuntimeMessage::MainWindowOpened(window_id)))
    }

    fn request_manual_clip_save(&mut self) -> Task<Message> {
        if !self.config.manual_clip.enabled {
            tracing::debug!("ignoring manual clip save request because manual clip is disabled");
            return Task::none();
        }
        if !self.recorder.is_running() {
            tracing::debug!(
                state = ?self.runtime.lifecycle,
                save_in_progress = self.recorder.save_in_progress(),
                active_clip_capture = self.runtime.active_clip_capture.is_some(),
                "rejecting manual clip save request because recorder is not running"
            );
            self.set_clip_error(
                "Manual clip save is unavailable because the recorder is not running.",
            );
            return Task::none();
        }
        if self.runtime.active_clip_capture.is_some() || self.recorder.save_in_progress() {
            tracing::debug!(
                state = ?self.runtime.lifecycle,
                save_in_progress = self.recorder.save_in_progress(),
                active_clip_capture = self.runtime.active_clip_capture.is_some(),
                "rejecting manual clip save request because another save is already in progress"
            );
            self.set_clip_error(
                "Manual clip save ignored because another save is already in progress.",
            );
            return Task::none();
        }

        let (character_id, world_id) = match &self.runtime.lifecycle {
            AppState::Monitoring { character_id, .. } => (*character_id, 0),
            _ => (0, 0),
        };

        let trigger_at = Utc::now();
        let clip_end_at =
            trigger_at + chrono::Duration::seconds(i64::from(self.config.recorder.save_delay_secs));
        let clip_start_at = clip_end_at
            - chrono::Duration::seconds(i64::from(self.config.manual_clip.duration_secs));
        tracing::debug!(
            state = ?self.runtime.lifecycle,
            character_id,
            world_id,
            trigger_at = %trigger_at,
            clip_start_at = %clip_start_at,
            clip_end_at = %clip_end_at,
            duration_secs = self.config.manual_clip.duration_secs,
            save_delay_secs = self.config.recorder.save_delay_secs,
            "accepting manual clip save request"
        );

        let request = ClipSaveRequest {
            origin: ClipOrigin::Manual,
            profile_id: self.active_profile_id(),
            rule_id: "manual_clip".into(),
            duration: ClipLength::Seconds(self.config.manual_clip.duration_secs),
            clip_duration_secs: self.config.manual_clip.duration_secs,
            trigger_score: 0,
            score_breakdown: Vec::new(),
            trigger_at,
            clip_start_at,
            clip_end_at,
            world_id,
            zone_id: None,
            facility_id: None,
            character_id,
            honu_session_id: self.runtime.honu_session_id,
            session_id: self
                .runtime
                .active_session
                .as_ref()
                .map(|session| session.id.clone()),
        };

        self.queue_immediate_clip_save(request)
    }

    fn build_clip_naming_context(
        &self,
        request: &ClipSaveRequest,
    ) -> crate::clip_naming::ClipNamingContext {
        let character = self
            .config
            .characters
            .iter()
            .find(|character| character.character_id == Some(request.character_id))
            .map(|character| character.name.clone())
            .unwrap_or_else(|| {
                if request.character_id == 0 {
                    "unassigned".into()
                } else {
                    format!("character_{}", request.character_id)
                }
            });

        crate::clip_naming::ClipNamingContext {
            timestamp: request.trigger_at,
            source: request.origin.as_str().into(),
            profile: request.profile_id.clone(),
            rule: if request.origin == ClipOrigin::Manual {
                "manual_clip".into()
            } else {
                request.rule_id.clone()
            },
            character,
            server: census::world_name(request.world_id),
            continent: census::continent_name(request.zone_id),
            base: census::base_name(request.facility_id).unwrap_or_else(|| "unknown".into()),
            score: request.trigger_score,
            duration_secs: request.clip_duration_secs,
        }
    }
}

fn clip_draft_from_request(
    request: ClipSaveRequest,
    raw_events: Vec<ClipRawEventDraft>,
    alert_keys: Vec<String>,
) -> ClipDraft {
    ClipDraft {
        trigger_event_at: request.trigger_at,
        clip_start_at: request.clip_start_at,
        clip_end_at: request.clip_end_at,
        saved_at: Utc::now(),
        origin: request.origin,
        profile_id: request.profile_id,
        rule_id: request.rule_id,
        clip_duration_secs: request.clip_duration_secs,
        session_id: request.session_id,
        character_id: request.character_id,
        world_id: request.world_id,
        zone_id: request.zone_id,
        facility_id: request.facility_id,
        score: request.trigger_score,
        honu_session_id: request.honu_session_id,
        path: None,
        events: score_contributions_from_breakdown(&request.score_breakdown),
        raw_events,
        alert_keys,
    }
}

async fn delete_clip_file_and_unlink(
    store: ClipStore,
    clip_id: i64,
    path: Option<&std::path::Path>,
) -> Result<(), String> {
    if let Some(path) = path {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "failed to delete clip file {}: {error}",
                    path.display()
                ));
            }
        }
    }

    store
        .delete_clip(clip_id)
        .await
        .map_err(|error| format!("failed to delete clip record: {error}"))
}

fn score_contributions_from_breakdown(
    breakdown: &[crate::rules::ScoreBreakdown],
) -> Vec<ClipEventContribution> {
    breakdown
        .iter()
        .map(|item| ClipEventContribution {
            event_kind: item.event.to_string(),
            occurrences: item.occurrences,
            points: item.points,
        })
        .collect()
}

fn recompute_capture_window(
    request: &mut ClipSaveRequest,
    preferred_start_at: chrono::DateTime<Utc>,
    replay_buffer_secs: u32,
) {
    let buffer_window = chrono::Duration::seconds(i64::from(replay_buffer_secs));
    let latest_allowed_start = request.clip_end_at - buffer_window;
    request.clip_start_at = preferred_start_at.max(latest_allowed_start);
    request.clip_duration_secs = request
        .clip_end_at
        .signed_duration_since(request.clip_start_at)
        .num_seconds()
        .max(1) as u32;
    if !matches!(request.duration, ClipLength::FullBuffer) {
        request.duration = ClipLength::Seconds(request.clip_duration_secs);
    }
}

fn raw_events_from_log(event_log: &EventLog, request: &ClipSaveRequest) -> Vec<ClipRawEventDraft> {
    event_log
        .query_range(request.clip_start_at, request.clip_end_at)
        .into_iter()
        .map(|event| ClipRawEventDraft {
            event_at: event.timestamp,
            event_kind: event.kind.to_string(),
            world_id: event.world_id,
            zone_id: event.zone_id,
            facility_id: event.facility_id,
            actor_character_id: event.actor_character_id,
            other_character_id: event.other_character_id,
            actor_class: event.actor_class.map(|class| class.to_string()),
            attacker_weapon_id: event.attacker_weapon_id,
            attacker_vehicle_id: event.attacker_vehicle_id,
            vehicle_killed_id: event.vehicle_killed_id,
            characters_killed: event.characters_killed,
            is_headshot: event.is_headshot,
            experience_id: event.experience_id,
        })
        .collect()
}

fn clip_log_retention_secs(config: &Config) -> u32 {
    let max_extension_window_secs = config
        .rule_definitions
        .iter()
        .filter(|rule| rule.extension.is_enabled())
        .map(|rule| rule.extension.window_secs)
        .max()
        .unwrap_or(0);

    config
        .recorder
        .replay_buffer_secs
        .saturating_add(config.recorder.save_delay_secs)
        .saturating_add(max_extension_window_secs)
        .max(
            config
                .manual_clip
                .duration_secs
                .saturating_add(config.recorder.save_delay_secs),
        )
}

fn empty_session_summary(session: &MonitoringSession) -> SessionSummary {
    SessionSummary {
        session_id: session.id.clone(),
        total_clips: 0,
        total_duration_secs: 0,
        unique_bases: 0,
        top_clip: None,
        rule_breakdown: Vec::new(),
        base_breakdown: Vec::new(),
    }
}

fn infer_storage_move_retry_target(record: &BackgroundJobRecord) -> Result<StorageTier, String> {
    let label = record.label.to_ascii_lowercase();
    if label.contains("to archive") {
        Ok(StorageTier::Archive)
    } else if label.contains("to primary") {
        Ok(StorageTier::Primary)
    } else {
        Err("storage move retry could not determine the original target tier.".into())
    }
}
