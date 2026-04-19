mod options;

use std::path::PathBuf;

use chrono::Utc;
use iced::Element;
use iced::Length;
use iced::Padding;

use super::super::shared::{
    ButtonTone, field_label, settings_pick_list_field, settings_stepper_field, settings_text_field,
    settings_text_field_with_button, styled_button, with_tooltip,
};
use super::super::{
    App, AudioSourceDraft, Message as AppMessage, UpdateMessage, audio_source_drafts_from_config,
};
use crate::capture::{DiscoveredAudioKind, DiscoveredAudioSource};
use crate::command_runner;
use crate::config::{
    AudioSourceConfig, CaptureBackend, ManualClipConfig, ObsManagementMode, UpdateChannel,
    YouTubePrivacyStatus, legacy_audio_source_kind_from_value,
};
use crate::secure_store::SecretKey;
use crate::ui::app::{
    checkbox, column, container, mouse_area, pick_list, rounded_box, row, scrollable, stack, text,
    text_input,
};
use crate::ui::layout::card::card;
use crate::ui::layout::empty_state::empty_state;
use crate::ui::layout::page_header::page_header;
use crate::ui::layout::panel::panel;
use crate::ui::layout::section::section;
use crate::ui::layout::sidebar::{SidebarItem, sidebar};
use crate::ui::layout::toolbar::toolbar;
use crate::ui::overlay::banner::banner;
use crate::ui::primitives::badge::{Tone as BadgeTone, badge};
use crate::ui::primitives::switch::switch as toggle_switch;
use crate::update::{AvailableRelease, UpdateInstallBehavior, UpdatePhase, UpdatePrimaryAction};
use options::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AvailableAudioSourceOption {
    pub label: String,
    pub source: String,
}

impl std::fmt::Display for AvailableAudioSourceOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsSubView {
    #[default]
    General,
    CaptureAudio,
    Clips,
    Delivery,
    System,
}

#[derive(Debug, Clone)]
pub enum Message {
    SetSubView(SettingsSubView),
    ServiceIdChanged(String),
    CaptureBackendSelected(CaptureBackendPreset),
    LaunchAtLoginToggled(bool),
    AutoStartMonitoringToggled(bool),
    StartMinimizedToggled(bool),
    MinimizeToTrayToggled(bool),
    UpdateAutoCheckToggled(bool),
    UpdateChannelSelected(UpdateChannel),
    UpdateInstallBehaviorSelected(UpdateInstallBehavior),
    CaptureSourceChanged(String),
    CaptureSourcePresetSelected(CaptureSourcePreset),
    SaveDirChanged(String),
    PickSaveDirectory,
    SaveDirectoryPicked(Result<Option<String>, String>),
    FramerateStepped(i32),
    CodecChanged(String),
    CodecPresetSelected(CodecPreset),
    QualityStepped(i32),
    ContainerChanged(String),
    ContainerPresetSelected(ContainerPreset),
    ObsWebsocketUrlChanged(String),
    ObsPasswordChanged(String),
    ClearObsPassword,
    ObsManagementModeSelected(ObsManagementMode),
    ObsTestConnection,
    ObsConnectionTested(Result<String, String>),
    BufferSecsStepped(i32),
    SaveDelayStepped(i32),
    ClipSavedNotificationsToggled(bool),
    ClipNamingTemplateChanged(String),
    ManualClipEnabledToggled(bool),
    BeginHotkeyCapture,
    CancelHotkeyCapture,
    HotkeyCaptureEvent(iced::keyboard::Event),
    ManualClipDurationStepped(i32),
    StorageTieringEnabledToggled(bool),
    StorageTierDirectoryChanged(String),
    PickStorageTierDirectory,
    StorageTierDirectoryPicked(Result<Option<String>, String>),
    StorageMinAgeDaysStepped(i32),
    StorageMaxScoreStepped(i32),
    RunStorageTieringSweep,
    CopypartyEnabledToggled(bool),
    CopypartyUploadUrlChanged(String),
    CopypartyPublicBaseUrlChanged(String),
    CopypartyUsernameChanged(String),
    CopypartyPasswordChanged(String),
    ClearCopypartyPassword,
    YouTubeEnabledToggled(bool),
    YouTubeClientIdChanged(String),
    YouTubeClientSecretChanged(String),
    YouTubePrivacyStatusSelected(YouTubePrivacyStatus),
    ConnectYouTube,
    DisconnectYouTube,
    DiscordWebhookEnabledToggled(bool),
    DiscordMinScoreStepped(i32),
    DiscordIncludeThumbnailToggled(bool),
    DiscordWebhookUrlChanged(String),
    ClearDiscordWebhook,
    AddAudioSource(DiscoveredAudioKind),
    RemoveAudioSource(usize),
    MoveAudioSourceUp(usize),
    MoveAudioSourceDown(usize),
    AudioSourceLabelChanged(usize, String),
    AudioSourceValueChanged(usize, String),
    AudioSourceGainStepped(usize, i32),
    AudioSourceMutedInPremixToggled(usize, bool),
    AudioSourceIncludedInPremixToggled(usize, bool),
    SelectAvailableAudioSource(DiscoveredAudioKind, AvailableAudioSourceOption),
    AudioSourcesDiscovered(Result<(Vec<DiscoveredAudioSource>, Option<String>), String>),
    BackupDatabase,
    ExportJson,
    ExportCsv,
    BackupCompleted(Result<String, String>),
    ExportCompleted(Result<String, String>),
    CheckForUpdates,
    RefreshRollbackCatalog,
    UpdatePrimaryActionSelected(UpdatePrimaryAction),
    RunSelectedUpdateAction,
    RollbackReleaseSelected(Box<AvailableRelease>),
    DownloadSelectedRollbackVersion,
    RollbackToPreviousInstalledVersion,
    ViewUpdateDetails,
    RevertDraft,
    Save,
}

pub(in crate::app) fn update(app: &mut App, message: Message) -> iced::Task<AppMessage> {
    match message {
        Message::SetSubView(sub_view) => {
            app.settings.sub_view = sub_view;
            iced::Task::none()
        }
        Message::ServiceIdChanged(value) => {
            app.settings.service_id = value;
            iced::Task::none()
        }
        Message::CaptureBackendSelected(backend) => {
            app.settings.capture_backend = backend.into_config_backend();
            app.settings.audio_discovery_error = None;
            iced::Task::none()
        }
        Message::LaunchAtLoginToggled(value) => {
            app.settings.launch_at_login = value;
            iced::Task::none()
        }
        Message::AutoStartMonitoringToggled(value) => {
            app.settings.auto_start_monitoring = value;
            iced::Task::none()
        }
        Message::StartMinimizedToggled(value) => {
            app.settings.start_minimized = value;
            iced::Task::none()
        }
        Message::MinimizeToTrayToggled(value) => {
            app.settings.minimize_to_tray = value;
            iced::Task::none()
        }
        Message::UpdateAutoCheckToggled(value) => {
            app.settings.update_auto_check = value;
            iced::Task::none()
        }
        Message::UpdateChannelSelected(value) => {
            app.settings.update_channel = value;
            app.settings.selected_rollback_release = None;
            app.updates.state.rollback_candidates.clear();
            iced::Task::done(AppMessage::updates(UpdateMessage::RefreshRollbackCatalog))
        }
        Message::UpdateInstallBehaviorSelected(value) => {
            app.settings.update_install_behavior = value;
            iced::Task::none()
        }
        Message::UpdatePrimaryActionSelected(value) => {
            app.settings.selected_update_action = value;
            iced::Task::none()
        }
        Message::RollbackReleaseSelected(value) => {
            app.settings.selected_rollback_release = Some(*value);
            iced::Task::none()
        }
        Message::CaptureSourceChanged(value) => {
            app.settings.capture_source = value;
            iced::Task::none()
        }
        Message::CaptureSourcePresetSelected(preset) => {
            app.settings.capture_source =
                apply_preset_string_selection(app.settings.capture_source.as_str(), preset);
            iced::Task::none()
        }
        Message::SaveDirChanged(value) => {
            app.settings.save_dir = value;
            app.clear_settings_feedback();
            iced::Task::none()
        }
        Message::PickSaveDirectory => {
            let current_dir = app.settings.save_dir.clone();
            iced::Task::perform(async move { pick_directory(current_dir).await }, |result| {
                AppMessage::Settings(Message::SaveDirectoryPicked(result))
            })
        }
        Message::SaveDirectoryPicked(result) => {
            match result {
                Ok(Some(path)) => {
                    app.settings.save_dir = path;
                    app.clear_settings_feedback();
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!("Failed to open save-directory picker: {error}");
                    app.set_settings_feedback(error, true);
                }
            }
            iced::Task::none()
        }
        Message::FramerateStepped(delta) => {
            step_numeric_setting(
                &mut app.settings.framerate,
                delta,
                5,
                30,
                240,
                app.config.recorder.gsr().framerate,
            );
            iced::Task::none()
        }
        Message::CodecChanged(value) => {
            app.settings.codec = value;
            iced::Task::none()
        }
        Message::CodecPresetSelected(preset) => {
            app.settings.codec = apply_preset_string_selection(app.settings.codec.as_str(), preset);
            iced::Task::none()
        }
        Message::QualityStepped(delta) => {
            let current_quality = parse_or_default(&app.settings.quality, 40_000);
            step_numeric_setting(
                &mut app.settings.quality,
                delta,
                1_000,
                1_000,
                500_000,
                current_quality,
            );
            iced::Task::none()
        }
        Message::ContainerChanged(value) => {
            app.settings.container = value;
            iced::Task::none()
        }
        Message::ContainerPresetSelected(preset) => {
            app.settings.container =
                apply_preset_string_selection(app.settings.container.as_str(), preset);
            iced::Task::none()
        }
        Message::ObsWebsocketUrlChanged(value) => {
            app.settings.obs_websocket_url = value;
            iced::Task::none()
        }
        Message::ObsPasswordChanged(value) => {
            app.settings.obs_password_input = value;
            iced::Task::none()
        }
        Message::ClearObsPassword => {
            match app
                .platform
                .secure_store()
                .delete(SecretKey::ObsWebsocketPassword)
            {
                Ok(()) => {
                    app.settings.obs_password_present = false;
                    app.settings.obs_password_input.clear();
                    app.config.recorder.obs_mut().websocket_password = None;
                    app.clear_settings_feedback();
                }
                Err(error) => app.set_settings_feedback(error, false),
            }
            iced::Task::none()
        }
        Message::ObsManagementModeSelected(mode) => {
            app.settings.obs_management_mode = mode;
            iced::Task::none()
        }
        Message::ObsTestConnection => {
            let mut config = app.config.recorder.obs().clone();
            config.websocket_url = app.settings.obs_websocket_url.clone();
            if !app.settings.obs_password_input.trim().is_empty() {
                config.websocket_password =
                    Some(app.settings.obs_password_input.trim().to_string());
            }
            iced::Task::perform(crate::capture::obs::test_connection(config), |result| {
                AppMessage::Settings(Message::ObsConnectionTested(result))
            })
        }
        Message::ObsConnectionTested(result) => {
            match result {
                Ok(message) => app.set_settings_feedback(message, false),
                Err(error) => app.set_settings_feedback(error, false),
            }
            iced::Task::none()
        }
        Message::BufferSecsStepped(delta) => {
            step_numeric_setting(
                &mut app.settings.buffer_secs,
                delta,
                10,
                30,
                3_600,
                app.config.recorder.replay_buffer_secs,
            );
            iced::Task::none()
        }
        Message::SaveDelayStepped(delta) => {
            step_numeric_setting(
                &mut app.settings.save_delay_secs,
                delta,
                1,
                0,
                10,
                app.config.recorder.save_delay_secs,
            );
            iced::Task::none()
        }
        Message::ClipSavedNotificationsToggled(value) => {
            app.settings.clip_saved_notifications = value;
            iced::Task::none()
        }
        Message::ClipNamingTemplateChanged(value) => {
            app.settings.clip_naming_template = value;
            iced::Task::none()
        }
        Message::ManualClipEnabledToggled(value) => {
            app.settings.manual_clip_enabled = value;
            iced::Task::none()
        }
        Message::BeginHotkeyCapture => {
            app.settings.hotkey_capture_active = true;
            app.clear_settings_feedback();
            iced::Task::none()
        }
        Message::CancelHotkeyCapture => {
            app.settings.hotkey_capture_active = false;
            iced::Task::none()
        }
        Message::HotkeyCaptureEvent(event) => {
            match crate::hotkey::capture_binding(&event) {
                crate::hotkey::BindingCapture::Captured(binding) => {
                    app.settings.manual_clip_hotkey = binding;
                    app.settings.hotkey_capture_active = false;
                    app.clear_settings_feedback();
                }
                crate::hotkey::BindingCapture::Unsupported => {
                    app.set_settings_feedback(
                        "That key combination is not supported for manual clip hotkeys.",
                        false,
                    );
                }
                crate::hotkey::BindingCapture::Ignored => {}
            }
            iced::Task::none()
        }
        Message::ManualClipDurationStepped(delta) => {
            step_numeric_setting(
                &mut app.settings.manual_clip_duration_secs,
                delta,
                5,
                5,
                300,
                app.config.manual_clip.duration_secs,
            );
            iced::Task::none()
        }
        Message::StorageTieringEnabledToggled(value) => {
            app.settings.storage_tiering_enabled = value;
            iced::Task::none()
        }
        Message::StorageTierDirectoryChanged(value) => {
            app.settings.storage_tier_directory = value;
            app.clear_settings_feedback();
            iced::Task::none()
        }
        Message::PickStorageTierDirectory => {
            let current_dir = app.settings.storage_tier_directory.clone();
            iced::Task::perform(async move { pick_directory(current_dir).await }, |result| {
                AppMessage::Settings(Message::StorageTierDirectoryPicked(result))
            })
        }
        Message::StorageTierDirectoryPicked(result) => {
            match result {
                Ok(Some(path)) => {
                    app.settings.storage_tier_directory = path;
                    app.clear_settings_feedback();
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!("Failed to open archive-directory picker: {error}");
                    app.set_settings_feedback(error, true);
                }
            }
            iced::Task::none()
        }
        Message::StorageMinAgeDaysStepped(delta) => {
            step_numeric_setting(
                &mut app.settings.storage_min_age_days,
                delta,
                1,
                1,
                3650,
                app.config.storage_tiering.min_age_days,
            );
            iced::Task::none()
        }
        Message::StorageMaxScoreStepped(delta) => {
            step_numeric_setting(
                &mut app.settings.storage_max_score,
                delta,
                5,
                1,
                10_000,
                app.config.storage_tiering.max_score,
            );
            iced::Task::none()
        }
        Message::RunStorageTieringSweep => iced::Task::done(AppMessage::RunStorageTieringSweep),
        Message::CopypartyEnabledToggled(value) => {
            app.settings.copyparty_enabled = value;
            iced::Task::none()
        }
        Message::CopypartyUploadUrlChanged(value) => {
            app.settings.copyparty_upload_url = value;
            iced::Task::none()
        }
        Message::CopypartyPublicBaseUrlChanged(value) => {
            app.settings.copyparty_public_base_url = value;
            iced::Task::none()
        }
        Message::CopypartyUsernameChanged(value) => {
            app.settings.copyparty_username = value;
            iced::Task::none()
        }
        Message::CopypartyPasswordChanged(value) => {
            app.settings.copyparty_password_input = value;
            iced::Task::none()
        }
        Message::ClearCopypartyPassword => {
            match app
                .platform
                .secure_store()
                .delete(SecretKey::CopypartyPassword)
            {
                Ok(()) => {
                    app.settings.copyparty_password_present = false;
                    app.settings.copyparty_password_input.clear();
                    app.set_settings_feedback("Cleared the stored Copyparty password.", false);
                }
                Err(error) => app.set_settings_feedback(error, false),
            }
            iced::Task::none()
        }
        Message::YouTubeEnabledToggled(value) => {
            app.settings.youtube_enabled = value;
            iced::Task::none()
        }
        Message::YouTubeClientIdChanged(value) => {
            app.settings.youtube_client_id = value;
            iced::Task::none()
        }
        Message::YouTubeClientSecretChanged(value) => {
            app.settings.youtube_client_secret_input = value;
            iced::Task::none()
        }
        Message::YouTubePrivacyStatusSelected(value) => {
            app.settings.youtube_privacy_status = value;
            iced::Task::none()
        }
        Message::ConnectYouTube => app.start_youtube_oauth(),
        Message::DisconnectYouTube => {
            let refresh_result = app
                .platform
                .secure_store()
                .delete(SecretKey::YoutubeRefreshToken);
            let secret_result = app
                .platform
                .secure_store()
                .delete(SecretKey::YoutubeClientSecret);
            match (refresh_result, secret_result) {
                (Ok(()), Ok(())) => {
                    app.settings.youtube_refresh_token_present = false;
                    app.settings.youtube_client_secret_present = false;
                    app.settings.youtube_client_secret_input.clear();
                    app.set_settings_feedback("Disconnected the stored YouTube account.", false);
                }
                (Err(error), _) | (_, Err(error)) => app.set_settings_feedback(error, false),
            }
            iced::Task::none()
        }
        Message::DiscordWebhookEnabledToggled(value) => {
            app.settings.discord_enabled = value;
            iced::Task::none()
        }
        Message::DiscordMinScoreStepped(delta) => {
            step_numeric_setting(
                &mut app.settings.discord_min_score,
                delta,
                5,
                1,
                10_000,
                app.config.discord_webhook.min_score,
            );
            iced::Task::none()
        }
        Message::DiscordIncludeThumbnailToggled(value) => {
            app.settings.discord_include_thumbnail = value;
            iced::Task::none()
        }
        Message::DiscordWebhookUrlChanged(value) => {
            app.settings.discord_webhook_input = value;
            iced::Task::none()
        }
        Message::ClearDiscordWebhook => {
            match app
                .platform
                .secure_store()
                .delete(SecretKey::DiscordWebhookUrl)
            {
                Ok(()) => {
                    app.settings.discord_webhook_present = false;
                    app.settings.discord_webhook_input.clear();
                    app.set_settings_feedback("Cleared the stored Discord webhook URL.", false);
                }
                Err(error) => app.set_settings_feedback(error, false),
            }
            iced::Task::none()
        }
        Message::AddAudioSource(kind) => {
            let selected = match kind {
                DiscoveredAudioKind::Device => app.settings.selected_device_audio_source.clone(),
                DiscoveredAudioKind::Application => {
                    app.settings.selected_application_audio_source.clone()
                }
            };
            let Some(selected) = selected else {
                return iced::Task::none();
            };

            let discovered = app
                .settings
                .discovered_audio_sources
                .iter()
                .find(|source| {
                    source.kind == kind
                        && source
                            .kind_hint
                            .config_display_value()
                            .eq_ignore_ascii_case(selected.source.as_str())
                })
                .cloned()
                .unwrap_or(DiscoveredAudioSource {
                    kind_hint: legacy_audio_source_kind_from_value(&selected.source),
                    display_label: selected.label.clone(),
                    kind,
                    available: true,
                });

            match apply_discovered_audio_source(&mut app.settings.audio_sources, &discovered) {
                ApplyDiscoveredAudioSource::Added => {
                    app.set_settings_feedback(
                        format!("Added audio source `{}`.", selected.source),
                        true,
                    );
                }
                ApplyDiscoveredAudioSource::UpdatedExisting => {
                    app.set_settings_feedback(
                        format!(
                            "Updated label for existing audio source `{}`.",
                            selected.source
                        ),
                        true,
                    );
                }
                ApplyDiscoveredAudioSource::Unchanged => {
                    app.set_settings_feedback(
                        format!("Audio source `{}` is already configured.", selected.source),
                        true,
                    );
                }
            }

            sync_selected_audio_sources(app);
            iced::Task::none()
        }
        Message::RemoveAudioSource(index) => {
            if app.settings.audio_sources.len() > 1 {
                app.settings.audio_sources.remove(index);
            } else if let Some(audio_source) = app.settings.audio_sources.get_mut(0) {
                *audio_source = AudioSourceDraft::default();
            }
            sync_selected_audio_sources(app);
            iced::Task::none()
        }
        Message::MoveAudioSourceUp(index) => {
            if index > 0 && index < app.settings.audio_sources.len() {
                app.settings.audio_sources.swap(index, index - 1);
            }
            iced::Task::none()
        }
        Message::MoveAudioSourceDown(index) => {
            if index + 1 < app.settings.audio_sources.len() {
                app.settings.audio_sources.swap(index, index + 1);
            }
            iced::Task::none()
        }
        Message::AudioSourceLabelChanged(index, value) => {
            if let Some(audio_source) = app.settings.audio_sources.get_mut(index) {
                audio_source.label = value;
            }
            iced::Task::none()
        }
        Message::AudioSourceValueChanged(index, value) => {
            if let Some(audio_source) = app.settings.audio_sources.get_mut(index) {
                audio_source.source = value;
            }
            sync_selected_audio_sources(app);
            iced::Task::none()
        }
        Message::AudioSourceGainStepped(index, delta) => {
            if let Some(audio_source) = app.settings.audio_sources.get_mut(index) {
                audio_source.gain_db =
                    (audio_source.gain_db + (delta as f32 * 0.5)).clamp(-60.0, 12.0);
            }
            iced::Task::none()
        }
        Message::AudioSourceMutedInPremixToggled(index, value) => {
            if let Some(audio_source) = app.settings.audio_sources.get_mut(index) {
                audio_source.muted_in_premix = value;
            }
            iced::Task::none()
        }
        Message::AudioSourceIncludedInPremixToggled(index, value) => {
            if let Some(audio_source) = app.settings.audio_sources.get_mut(index) {
                audio_source.included_in_premix = value;
            }
            iced::Task::none()
        }
        Message::SelectAvailableAudioSource(kind, option) => {
            match kind {
                DiscoveredAudioKind::Device => {
                    app.settings.selected_device_audio_source = Some(option);
                }
                DiscoveredAudioKind::Application => {
                    app.settings.selected_application_audio_source = Some(option);
                }
            }
            iced::Task::none()
        }
        Message::AudioSourcesDiscovered(result) => {
            app.settings.audio_discovery_running = false;

            match result {
                Ok((discovered, warning)) => {
                    app.settings.discovered_audio_sources = discovered;
                    app.settings.audio_discovery_error = warning;
                    sync_selected_audio_sources(app);
                }
                Err(error) => {
                    app.settings.audio_discovery_error = Some(error);
                }
            }

            iced::Task::none()
        }
        Message::BackupDatabase => run_database_action(
            app,
            "database backup",
            backup_destination(app.settings.save_dir.as_str(), "sqlite3"),
            |store, destination| async move {
                store
                    .backup_to(&destination)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(destination.display().to_string())
            },
        ),
        Message::ExportJson => run_database_action(
            app,
            "JSON export",
            backup_destination(app.settings.save_dir.as_str(), "json"),
            |store, destination| async move {
                store
                    .export_json_to(&destination)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(destination.display().to_string())
            },
        ),
        Message::ExportCsv => run_database_action(
            app,
            "CSV export",
            backup_destination(app.settings.save_dir.as_str(), "csv"),
            |store, destination| async move {
                store
                    .export_csv_to(&destination)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(destination.display().to_string())
            },
        ),
        Message::BackupCompleted(result) | Message::ExportCompleted(result) => {
            match result {
                Ok(path) => app.set_settings_feedback(format!("Saved to {path}"), false),
                Err(error) => app.set_settings_feedback(error, false),
            }
            iced::Task::none()
        }
        Message::CheckForUpdates => {
            iced::Task::done(AppMessage::updates(UpdateMessage::CheckForUpdates {
                manual: true,
            }))
        }
        Message::RefreshRollbackCatalog => {
            iced::Task::done(AppMessage::updates(UpdateMessage::RefreshRollbackCatalog))
        }
        Message::RunSelectedUpdateAction => {
            iced::Task::done(AppMessage::updates(UpdateMessage::RunSelectedUpdateAction))
        }
        Message::DownloadSelectedRollbackVersion => iced::Task::done(AppMessage::updates(
            UpdateMessage::DownloadSelectedRollbackVersion,
        )),
        Message::RollbackToPreviousInstalledVersion => iced::Task::done(AppMessage::updates(
            UpdateMessage::RollbackToPreviousInstalledVersion,
        )),
        Message::ViewUpdateDetails => {
            iced::Task::done(AppMessage::updates(UpdateMessage::ShowUpdateDetails))
        }
        Message::RevertDraft => {
            apply_settings_draft_from_config(app);
            iced::Task::batch([
                refresh_audio_sources(app),
                iced::Task::done(AppMessage::updates(UpdateMessage::RefreshRollbackCatalog)),
            ])
        }
        Message::Save => {
            app.settings.hotkey_capture_active = false;
            let manual_clip_settings_changed = manual_clip_settings_dirty(app);
            if let Err(error) =
                crate::clip_naming::validate_template(app.settings.clip_naming_template.as_str())
            {
                app.set_settings_feedback(error, false);
                return iced::Task::none();
            }

            app.config.service_id = app.settings.service_id.clone();
            app.config.launch_at_login.enabled = app.settings.launch_at_login;
            app.config.auto_start_monitoring = app.settings.auto_start_monitoring;
            app.config.start_minimized = app.settings.start_minimized;
            app.config.minimize_to_tray = app.settings.minimize_to_tray;
            app.config.updates.auto_check = app.settings.update_auto_check;
            app.config.updates.channel = app.settings.update_channel;
            app.config.updates.install_behavior = app.settings.update_install_behavior;
            app.config.clip_naming_template = non_empty_or_default(
                app.settings.clip_naming_template.as_str(),
                "{timestamp}_{source}_{character}_{rule}_{score}",
            );
            app.config.manual_clip.enabled = app.settings.manual_clip_enabled;
            app.config.manual_clip.hotkey =
                non_empty_or_default(app.settings.manual_clip_hotkey.as_str(), "Ctrl+Shift+F8");
            app.config.manual_clip.duration_secs = app
                .settings
                .manual_clip_duration_secs
                .parse()
                .unwrap_or(app.config.manual_clip.duration_secs);
            app.config.manual_clip.normalize();
            app.settings.manual_clip_hotkey = app.config.manual_clip.hotkey.clone();
            app.settings.manual_clip_duration_secs =
                app.config.manual_clip.duration_secs.to_string();
            app.config.storage_tiering.enabled = app.settings.storage_tiering_enabled;
            app.config.storage_tiering.tier_directory =
                PathBuf::from(app.settings.storage_tier_directory.trim());
            app.config.storage_tiering.min_age_days = app
                .settings
                .storage_min_age_days
                .parse()
                .unwrap_or(app.config.storage_tiering.min_age_days);
            app.config.storage_tiering.max_score = app
                .settings
                .storage_max_score
                .parse()
                .unwrap_or(app.config.storage_tiering.max_score);
            app.config.uploads.copyparty.enabled = app.settings.copyparty_enabled;
            app.config.uploads.copyparty.upload_url = app.settings.copyparty_upload_url.clone();
            app.config.uploads.copyparty.public_base_url =
                app.settings.copyparty_public_base_url.clone();
            app.config.uploads.copyparty.username = app.settings.copyparty_username.clone();
            app.config.uploads.youtube.enabled = app.settings.youtube_enabled;
            app.config.uploads.youtube.client_id = app.settings.youtube_client_id.clone();
            app.config.uploads.youtube.privacy_status = app.settings.youtube_privacy_status;
            app.config.discord_webhook.enabled = app.settings.discord_enabled;
            app.config.discord_webhook.min_score = app
                .settings
                .discord_min_score
                .parse()
                .unwrap_or(app.config.discord_webhook.min_score);
            app.config.discord_webhook.include_thumbnail = app.settings.discord_include_thumbnail;

            app.config.capture.backend = app.settings.capture_backend;
            app.config.recorder.gsr_mut().capture_source =
                non_empty_or_default(app.settings.capture_source.as_str(), "planetside2");
            app.config.recorder.obs_mut().websocket_url = non_empty_or_default(
                app.settings.obs_websocket_url.as_str(),
                "ws://127.0.0.1:4455",
            );
            app.config.recorder.obs_mut().management_mode = app.settings.obs_management_mode;
            app.config.recorder.save_directory = app.settings.save_dir.clone().into();
            let default_framerate = app.config.recorder.gsr().framerate;
            app.config.recorder.gsr_mut().framerate =
                app.settings.framerate.parse().unwrap_or(default_framerate);
            app.config.recorder.gsr_mut().codec =
                non_empty_or_default(app.settings.codec.as_str(), "h264");
            app.config.recorder.gsr_mut().quality = app.settings.quality.clone();
            app.config.recorder.audio_sources = app
                .settings
                .audio_sources
                .iter()
                .filter_map(|audio_source| {
                    let source = audio_source.source.trim();
                    if source.is_empty() {
                        return None;
                    }

                    let mut config = AudioSourceConfig::new(
                        audio_source.label.trim(),
                        legacy_audio_source_kind_from_value(source),
                    );
                    config.gain_db = audio_source.gain_db;
                    config.muted_in_premix = audio_source.muted_in_premix;
                    config.included_in_premix = audio_source.included_in_premix;
                    Some(config)
                })
                .collect();
            app.config.recorder.gsr_mut().container =
                non_empty_or_default(app.settings.container.as_str(), "mkv");
            app.config.recorder.replay_buffer_secs = app
                .settings
                .buffer_secs
                .parse()
                .unwrap_or(app.config.recorder.replay_buffer_secs);
            app.config.recorder.save_delay_secs = app
                .settings
                .save_delay_secs
                .parse()
                .unwrap_or(app.config.recorder.save_delay_secs);
            app.config.recorder.clip_saved_notifications = app.settings.clip_saved_notifications;

            if !app.settings.obs_password_input.trim().is_empty() {
                if let Err(error) = app.platform.secure_store().set(
                    SecretKey::ObsWebsocketPassword,
                    app.settings.obs_password_input.trim(),
                ) {
                    app.set_settings_feedback(error, false);
                    return iced::Task::none();
                }
                app.settings.obs_password_present = true;
                app.config.recorder.obs_mut().websocket_password =
                    Some(app.settings.obs_password_input.trim().to_string());
                app.settings.obs_password_input.clear();
            }

            app.config.normalize();
            app.event_log
                .set_retention_secs(super::super::clip_log_retention_secs(&app.config));
            app.cancel_pending_recorder_start();
            app.recorder
                .update_config(app.config.capture.clone(), app.config.recorder.clone());

            if !app.settings.copyparty_password_input.trim().is_empty() {
                if let Err(error) = app.platform.secure_store().set(
                    SecretKey::CopypartyPassword,
                    app.settings.copyparty_password_input.trim(),
                ) {
                    app.set_settings_feedback(error, false);
                    return iced::Task::none();
                }
                app.settings.copyparty_password_present = true;
                app.settings.copyparty_password_input.clear();
            }

            if !app.settings.youtube_client_secret_input.trim().is_empty() {
                if let Err(error) = app.platform.secure_store().set(
                    SecretKey::YoutubeClientSecret,
                    app.settings.youtube_client_secret_input.trim(),
                ) {
                    app.set_settings_feedback(error, false);
                    return iced::Task::none();
                }
                app.settings.youtube_client_secret_present = true;
                app.settings.youtube_client_secret_input.clear();
            }

            if !app.settings.discord_webhook_input.trim().is_empty() {
                if let Err(error) = app.platform.secure_store().set(
                    SecretKey::DiscordWebhookUrl,
                    app.settings.discord_webhook_input.trim(),
                ) {
                    app.set_settings_feedback(error, false);
                    return iced::Task::none();
                }
                app.settings.discord_webhook_present = true;
                app.settings.discord_webhook_input.clear();
            }

            match app.config.save() {
                Ok(()) => {
                    app.set_settings_feedback_silent("Settings saved.", false);
                    iced::Task::batch([
                        app.configure_hotkeys(manual_clip_settings_changed),
                        app.sync_tray_snapshot(),
                        app.sync_launch_at_login_task(),
                    ])
                }
                Err(error) => {
                    app.set_settings_feedback(format!("Failed to save settings: {error}"), false);
                    iced::Task::none()
                }
            }
        }
    }
}

pub(in crate::app) fn refresh_audio_sources(app: &mut App) -> iced::Task<AppMessage> {
    if obs_audio_is_backend_owned(app) {
        app.settings.audio_discovery_running = false;
        app.settings.audio_discovery_error = None;
        app.settings.discovered_audio_sources.clear();
        app.settings.selected_device_audio_source = None;
        app.settings.selected_application_audio_source = None;
        return iced::Task::none();
    }

    if app.settings.audio_discovery_running {
        return iced::Task::none();
    }

    app.settings.audio_discovery_running = true;
    app.settings.audio_discovery_error = None;
    let backend = app.recorder.backend_handle();
    iced::Task::perform(
        async move {
            match backend.discover_audio_sources().await {
                Ok(discovered) => Ok((discovered, None)),
                Err(crate::capture::AudioSourceError::PerAppUnavailable { reason, partial }) => {
                    Ok((
                        partial,
                        Some(format!(
                            "Per-application audio discovery unavailable: {reason}"
                        )),
                    ))
                }
                Err(error) => Err(error.to_string()),
            }
        },
        |result| AppMessage::Settings(Message::AudioSourcesDiscovered(result)),
    )
}

pub(in crate::app) fn view(app: &App) -> Element<'_, Message> {
    let header = page_header("Settings")
        .subtitle("Capture, automation, delivery, and maintenance.")
        .build();

    let status_bar = toolbar()
        .push(settings_status_badge(
            format!("Secure Store: {}", app.settings.secure_store_backend_label),
            BadgeTone::Outline,
        ))
        .push(settings_status_badge(
            if obs_audio_is_backend_owned(app) {
                "Audio: OBS-managed".into()
            } else {
                format!("Audio Tracks: {}", configured_audio_source_count(app))
            },
            BadgeTone::Info,
        ))
        .push(settings_status_badge(
            if app.settings.clip_saved_notifications {
                "Toasts On"
            } else {
                "Toasts Off"
            },
            if app.settings.clip_saved_notifications {
                BadgeTone::Success
            } else {
                BadgeTone::Neutral
            },
        ))
        .push(settings_status_badge(
            if app.settings.youtube_refresh_token_present {
                "YouTube Connected"
            } else {
                "YouTube Disconnected"
            },
            if app.settings.youtube_refresh_token_present {
                BadgeTone::Success
            } else {
                BadgeTone::Neutral
            },
        ))
        .push(settings_status_badge(
            if app.settings.discord_webhook_present {
                "Discord Ready"
            } else {
                "Discord Unconfigured"
            },
            if app.settings.discord_webhook_present {
                BadgeTone::Success
            } else {
                BadgeTone::Neutral
            },
        ))
        .push(settings_status_badge(
            format!("Updates: {}", app.settings.update_channel),
            BadgeTone::Primary,
        ))
        .build();

    let mut detail_body = column![].spacing(12);
    if let Some(feedback) = settings_feedback_banner(app) {
        detail_body = detail_body.push(feedback);
    }
    detail_body = detail_body.push(settings_detail_pane(app));

    let sidebar_element: Element<'_, Message> = scrollable(settings_sidebar(app))
        .height(Length::Fill)
        .into();

    let detail_base: Element<'_, Message> = container(detail_body)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    let detail_overlay: Element<'_, Message> = if settings_dirty(app) {
        container(settings_draft_bar(app))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Bottom)
            .padding([16, 20])
            .into()
    } else {
        container(iced::widget::Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    };
    let detail_element: Element<'_, Message> = container(stack![detail_base, detail_overlay])
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

    column![
        header,
        status_bar,
        row![sidebar_element, detail_element]
            .spacing(12)
            .height(Length::Fill)
    ]
    .spacing(12)
    .height(Length::Fill)
    .into()
}

fn settings_sidebar(app: &App) -> Element<'_, Message> {
    let active = app.settings.sub_view;
    let delivery_badge = {
        let enabled = usize::from(app.settings.storage_tiering_enabled)
            + usize::from(app.settings.copyparty_enabled)
            + usize::from(app.settings.youtube_enabled)
            + usize::from(app.settings.discord_enabled);
        format!("{enabled} on")
    };
    let capture_badge = if selected_capture_backend(app) == CaptureBackendPreset::Obs {
        format!("{} · OBS", selected_capture_backend(app))
    } else {
        format!("{} tracks", configured_audio_source_count(app))
    };

    sidebar(active, Message::SetSubView)
        .width(240.0)
        .header(
            column![
                text("Settings Areas").size(13),
                text("Pick a section to edit on the right.").size(12),
            ]
            .spacing(4),
        )
        .push(SidebarItem::new(SettingsSubView::General, "General").badge("Runtime"))
        .push(
            SidebarItem::new(SettingsSubView::CaptureAudio, "Capture & Audio").badge(capture_badge),
        )
        .push(SidebarItem::new(SettingsSubView::Clips, "Clips").badge(
            if app.settings.manual_clip_enabled {
                "Hotkey on"
            } else {
                "Manual only"
            },
        ))
        .push(SidebarItem::new(SettingsSubView::Delivery, "Delivery").badge(delivery_badge))
        .push(
            SidebarItem::new(SettingsSubView::System, "System")
                .badge(format!("Updates: {}", app.settings.update_channel)),
        )
        .build()
        .into()
}

fn settings_detail_pane(app: &App) -> Element<'_, Message> {
    let bottom_padding = if settings_dirty(app) { 108.0 } else { 0.0 };
    let body = match app.settings.sub_view {
        SettingsSubView::General => column![settings_overview_cards(app), runtime_panel(app)],
        SettingsSubView::CaptureAudio => {
            let mut body = column![capture_panel(app)].spacing(16);
            if !obs_audio_is_backend_owned(app) {
                body = body.push(audio_panel(app));
            }
            body
        }
        SettingsSubView::Clips => column![clip_output_panel(app)],
        SettingsSubView::Delivery => column![delivery_panel(app)],
        SettingsSubView::System => column![update_panel(app), maintenance_panel(app)],
    }
    .spacing(16);

    scrollable(container(body).width(Length::Fill).padding(Padding {
        top: 0.0,
        right: 0.0,
        bottom: bottom_padding,
        left: 0.0,
    }))
    .height(Length::Fill)
    .into()
}

fn settings_draft_bar(_app: &App) -> Element<'_, Message> {
    let revert_button: Element<'_, Message> = styled_button("Revert", ButtonTone::Secondary)
        .on_press(Message::RevertDraft)
        .into();
    let save_button: Element<'_, Message> = styled_button("Save Settings", ButtonTone::Primary)
        .on_press(Message::Save)
        .into();

    container(
        row![
            column![
                text("Unsaved changes").size(14),
                text(
                    "Save to persist config changes and refresh integrations, or revert this draft."
                )
                .size(12)
                .width(360),
            ]
            .spacing(4)
            .width(Length::Shrink),
            revert_button,
            with_tooltip(
                save_button,
                "Save settings to disk and refresh integrations.",
            ),
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Shrink)
    .padding([12, 14])
    .style(rounded_box)
    .into()
}

fn settings_feedback_banner(app: &App) -> Option<Element<'_, Message>> {
    app.settings.feedback.as_ref().map(|feedback| {
        banner("Settings status")
            .info()
            .description(feedback.clone())
            .build()
            .into()
    })
}

fn settings_dirty(app: &App) -> bool {
    app.settings.launch_at_login != app.config.launch_at_login.enabled
        || app.settings.auto_start_monitoring != app.config.auto_start_monitoring
        || app.settings.start_minimized != app.config.start_minimized
        || app.settings.minimize_to_tray != app.config.minimize_to_tray
        || app.settings.update_auto_check != app.config.updates.auto_check
        || app.settings.update_channel != app.config.updates.channel
        || app.settings.update_install_behavior != app.config.updates.install_behavior
        || app.settings.service_id != app.config.service_id
        || app.settings.capture_backend != app.config.capture.backend
        || app.settings.capture_source != app.config.recorder.gsr().capture_source
        || app.settings.save_dir != app.config.recorder.save_directory.to_string_lossy()
        || app.settings.framerate != app.config.recorder.gsr().framerate.to_string()
        || app.settings.codec != app.config.recorder.gsr().codec
        || app.settings.quality != app.config.recorder.gsr().quality
        || app.settings.audio_sources
            != audio_source_drafts_from_config(&app.config.recorder.audio_sources)
        || app.settings.container != app.config.recorder.gsr().container
        || app.settings.obs_websocket_url != app.config.recorder.obs().websocket_url
        || app.settings.obs_management_mode != app.config.recorder.obs().management_mode
        || !app.settings.obs_password_input.trim().is_empty()
        || app.settings.buffer_secs != app.config.recorder.replay_buffer_secs.to_string()
        || app.settings.save_delay_secs != app.config.recorder.save_delay_secs.to_string()
        || app.settings.clip_saved_notifications != app.config.recorder.clip_saved_notifications
        || app.settings.clip_naming_template != app.config.clip_naming_template
        || manual_clip_settings_dirty(app)
        || app.settings.storage_tiering_enabled != app.config.storage_tiering.enabled
        || app.settings.storage_tier_directory
            != app.config.storage_tiering.tier_directory.to_string_lossy()
        || app.settings.storage_min_age_days != app.config.storage_tiering.min_age_days.to_string()
        || app.settings.storage_max_score != app.config.storage_tiering.max_score.to_string()
        || app.settings.copyparty_enabled != app.config.uploads.copyparty.enabled
        || app.settings.copyparty_upload_url != app.config.uploads.copyparty.upload_url
        || app.settings.copyparty_public_base_url != app.config.uploads.copyparty.public_base_url
        || app.settings.copyparty_username != app.config.uploads.copyparty.username
        || !app.settings.copyparty_password_input.trim().is_empty()
        || app.settings.youtube_enabled != app.config.uploads.youtube.enabled
        || app.settings.youtube_client_id != app.config.uploads.youtube.client_id
        || !app.settings.youtube_client_secret_input.trim().is_empty()
        || app.settings.youtube_privacy_status != app.config.uploads.youtube.privacy_status
        || app.settings.discord_enabled != app.config.discord_webhook.enabled
        || app.settings.discord_min_score != app.config.discord_webhook.min_score.to_string()
        || app.settings.discord_include_thumbnail != app.config.discord_webhook.include_thumbnail
        || !app.settings.discord_webhook_input.trim().is_empty()
}

fn manual_clip_settings_dirty(app: &App) -> bool {
    manual_clip_settings_dirty_values(
        app.settings.manual_clip_enabled,
        app.settings.manual_clip_hotkey.as_str(),
        app.settings.manual_clip_duration_secs.as_str(),
        &app.config.manual_clip,
    )
}

fn manual_clip_settings_dirty_values(
    settings_enabled: bool,
    settings_hotkey: &str,
    settings_duration_secs: &str,
    config: &ManualClipConfig,
) -> bool {
    settings_enabled != config.enabled
        || settings_hotkey != config.hotkey
        || settings_duration_secs != config.duration_secs.to_string()
}

fn apply_settings_draft_from_config(app: &mut App) {
    app.settings.launch_at_login = app.config.launch_at_login.enabled;
    app.settings.auto_start_monitoring = app.config.auto_start_monitoring;
    app.settings.start_minimized = app.config.start_minimized;
    app.settings.minimize_to_tray = app.config.minimize_to_tray;
    app.settings.update_auto_check = app.config.updates.auto_check;
    app.settings.update_channel = app.config.updates.channel;
    app.settings.update_install_behavior = app.config.updates.install_behavior;
    app.settings.selected_update_action = UpdatePrimaryAction::DownloadUpdate;
    app.settings.selected_rollback_release = None;
    app.settings.service_id = app.config.service_id.clone();
    app.settings.capture_backend = app.config.capture.backend;
    app.settings.capture_source = app.config.recorder.gsr().capture_source.clone();
    app.settings.save_dir = app.config.recorder.save_directory.to_string_lossy().into();
    app.settings.framerate = app.config.recorder.gsr().framerate.to_string();
    app.settings.codec = app.config.recorder.gsr().codec.clone();
    app.settings.quality = app.config.recorder.gsr().quality.clone();
    app.settings.audio_sources =
        audio_source_drafts_from_config(&app.config.recorder.audio_sources);
    app.settings.selected_device_audio_source = None;
    app.settings.selected_application_audio_source = None;
    app.settings.audio_discovery_error = None;
    app.settings.container = app.config.recorder.gsr().container.clone();
    app.settings.obs_websocket_url = app.config.recorder.obs().websocket_url.clone();
    app.settings.obs_password_input.clear();
    app.settings.obs_management_mode = app.config.recorder.obs().management_mode;
    app.settings.buffer_secs = app.config.recorder.replay_buffer_secs.to_string();
    app.settings.save_delay_secs = app.config.recorder.save_delay_secs.to_string();
    app.settings.clip_saved_notifications = app.config.recorder.clip_saved_notifications;
    app.settings.clip_naming_template = app.config.clip_naming_template.clone();
    app.settings.manual_clip_enabled = app.config.manual_clip.enabled;
    app.settings.manual_clip_hotkey = app.config.manual_clip.hotkey.clone();
    app.settings.hotkey_capture_active = false;
    app.settings.manual_clip_duration_secs = app.config.manual_clip.duration_secs.to_string();
    app.settings.storage_tiering_enabled = app.config.storage_tiering.enabled;
    app.settings.storage_tier_directory = app
        .config
        .storage_tiering
        .tier_directory
        .to_string_lossy()
        .into();
    app.settings.storage_min_age_days = app.config.storage_tiering.min_age_days.to_string();
    app.settings.storage_max_score = app.config.storage_tiering.max_score.to_string();
    app.settings.copyparty_enabled = app.config.uploads.copyparty.enabled;
    app.settings.copyparty_upload_url = app.config.uploads.copyparty.upload_url.clone();
    app.settings.copyparty_public_base_url = app.config.uploads.copyparty.public_base_url.clone();
    app.settings.copyparty_username = app.config.uploads.copyparty.username.clone();
    app.settings.copyparty_password_input.clear();
    app.settings.youtube_enabled = app.config.uploads.youtube.enabled;
    app.settings.youtube_client_id = app.config.uploads.youtube.client_id.clone();
    app.settings.youtube_client_secret_input.clear();
    app.settings.youtube_oauth_in_flight = false;
    app.settings.youtube_privacy_status = app.config.uploads.youtube.privacy_status;
    app.settings.discord_enabled = app.config.discord_webhook.enabled;
    app.settings.discord_min_score = app.config.discord_webhook.min_score.to_string();
    app.settings.discord_include_thumbnail = app.config.discord_webhook.include_thumbnail;
    app.settings.discord_webhook_input.clear();
    sync_selected_audio_sources(app);
    app.set_settings_feedback_silent("Reverted unsaved settings changes.", true);
}

fn settings_overview_cards(app: &App) -> Element<'_, Message> {
    let startup_summary = if app.settings.auto_start_monitoring {
        "Starts monitoring automatically."
    } else {
        "Launches idle until you start monitoring."
    };
    let capture_summary = if selected_capture_backend(app) == CaptureBackendPreset::Obs {
        format!(
            "OBS · {} · {}s buffer",
            app.settings.obs_management_mode,
            current_buffer_secs(app),
        )
    } else {
        format!(
            "{} fps · {}s buffer · {}",
            current_framerate(app),
            current_buffer_secs(app),
            ContainerPreset::from_value(app.settings.container.as_str())
        )
    };
    let delivery_summary = format!(
        "{} delivery integrations enabled",
        usize::from(app.settings.copyparty_enabled)
            + usize::from(app.settings.youtube_enabled)
            + usize::from(app.settings.discord_enabled)
    );

    row![
        settings_overview_card(
            "Startup",
            startup_summary,
            vec![
                settings_status_badge(
                    if app.settings.launch_at_login {
                        "Launch at login"
                    } else {
                        "Manual launch"
                    },
                    if app.settings.launch_at_login {
                        BadgeTone::Success
                    } else {
                        BadgeTone::Neutral
                    },
                ),
                settings_status_badge(
                    if app.settings.start_minimized {
                        "Starts minimized"
                    } else {
                        "Foreground start"
                    },
                    BadgeTone::Outline,
                ),
            ],
        ),
        settings_overview_card(
            "Recorder",
            capture_summary,
            vec![
                settings_status_badge(
                    selected_capture_backend(app).to_string(),
                    BadgeTone::Primary,
                ),
                settings_status_badge(
                    if selected_capture_backend(app) == CaptureBackendPreset::Obs {
                        obs_management_mode_label(app.settings.obs_management_mode).to_string()
                    } else {
                        CodecPreset::from_value(app.settings.codec.as_str()).to_string()
                    },
                    BadgeTone::Outline,
                ),
            ],
        ),
        settings_overview_card(
            "Delivery",
            delivery_summary,
            vec![
                settings_status_badge(
                    if app.settings.storage_tiering_enabled {
                        "Tiering enabled"
                    } else {
                        "Tiering off"
                    },
                    if app.settings.storage_tiering_enabled {
                        BadgeTone::Warning
                    } else {
                        BadgeTone::Neutral
                    },
                ),
                settings_status_badge(
                    if app.settings.youtube_refresh_token_present {
                        "OAuth ready"
                    } else {
                        "OAuth missing"
                    },
                    if app.settings.youtube_refresh_token_present {
                        BadgeTone::Success
                    } else {
                        BadgeTone::Neutral
                    },
                ),
            ],
        ),
    ]
    .spacing(12)
    .into()
}

fn settings_overview_card<'a>(
    title: &'a str,
    summary: impl Into<String>,
    badges: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut badge_row = row![].spacing(6);
    for badge in badges {
        badge_row = badge_row.push(badge);
    }

    card()
        .title(title)
        .body(column![text(summary.into()).size(14), badge_row].spacing(10))
        .width(Length::FillPortion(1))
        .into()
}

fn runtime_panel(app: &App) -> Element<'_, Message> {
    panel("Runtime")
        .push(settings_section_block(
            "Lifecycle",
            "How the app launches and starts monitoring.",
            vec![
                settings_toggle_row(
                    "Launch at Login",
                    "Launch the app at login using the platform backend.",
                    app.settings.launch_at_login,
                    Message::LaunchAtLoginToggled,
                ),
                settings_toggle_row(
                    "Auto-Start Monitoring",
                    "Start monitoring immediately after launch.",
                    app.settings.auto_start_monitoring,
                    Message::AutoStartMonitoringToggled,
                ),
                settings_toggle_row(
                    "Start Minimized",
                    "Start in the background instead of the main window.",
                    app.settings.start_minimized,
                    Message::StartMinimizedToggled,
                ),
                settings_toggle_row(
                    "Minimize to Tray",
                    "Closing the window keeps the app in the tray.",
                    app.settings.minimize_to_tray,
                    Message::MinimizeToTrayToggled,
                ),
            ],
        ))
        .push(settings_section_block(
            "Census Access",
            "Daybreak Census credentials for lookups and live events.",
            vec![settings_text_field(
                "Census Service ID",
                "Daybreak Census service ID for API and realtime events.",
                &app.settings.service_id,
                Message::ServiceIdChanged,
            )],
        ))
        .build()
        .into()
}

fn capture_panel(app: &App) -> Element<'_, Message> {
    let backend_controls: Vec<Element<'_, Message>> = if app.recorder.has_active_session() {
        vec![
            row![
                field_label(
                    "Recorder Backend",
                    "Stop monitoring before switching capture backends.",
                    200.0,
                ),
                text(selected_capture_backend(app).to_string()).size(14),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center)
            .into(),
            text("Stop monitoring before changing capture backends.")
                .size(12)
                .into(),
        ]
    } else {
        vec![settings_pick_list_field(
            "Recorder Backend",
            "Choose the replay-buffer backend NaniteClip controls.",
            CaptureBackendPreset::all(),
            Some(selected_capture_backend(app)),
            Message::CaptureBackendSelected,
        )]
    };

    let backend_panel = settings_section_block(
        "Backend",
        "Select which capture stack NaniteClip controls on this machine.",
        backend_controls,
    );

    let recorder_panel = if selected_capture_backend(app) == CaptureBackendPreset::Obs {
        obs_capture_section(app)
    } else {
        gsr_capture_section(app)
    };

    panel("Capture")
        .push(backend_panel)
        .push(recorder_panel)
        .build()
        .into()
}

fn gsr_capture_section(app: &App) -> Element<'_, Message> {
    let mut video_rows = vec![
        settings_pick_list_field(
            "Capture Source",
            "How gpu-screen-recorder captures the game window.",
            &CaptureSourcePreset::ALL[..],
            Some(CaptureSourcePreset::from_value(
                app.settings.capture_source.as_str(),
            )),
            Message::CaptureSourcePresetSelected,
        ),
        text("Automatic: X11 binds the PS2 window; Wayland uses the desktop portal.")
            .size(12)
            .into(),
        settings_text_field_with_button(
            "Save Directory",
            "Directory where saved clips are written.",
            &app.settings.save_dir,
            Message::SaveDirChanged,
            with_tooltip(
                styled_button("Browse", ButtonTone::Secondary)
                    .on_press(Message::PickSaveDirectory)
                    .into(),
                "Open a folder picker for the clip save directory.",
            ),
        ),
        settings_stepper_field(
            "Framerate",
            "Target recording frame rate for the replay buffer.",
            current_framerate(app),
            "fps",
            Message::FramerateStepped,
        ),
        settings_pick_list_field(
            "Codec",
            "Select the video codec used by gpu-screen-recorder.",
            &CodecPreset::ALL[..],
            Some(CodecPreset::from_value(app.settings.codec.as_str())),
            Message::CodecPresetSelected,
        ),
        settings_stepper_field(
            "Quality (bitrate)",
            "Target encoder bitrate in kilobits per second.",
            current_quality(app),
            "kbps",
            Message::QualityStepped,
        ),
        settings_pick_list_field(
            "Container Format",
            "Choose the output container format for saved clips.",
            &ContainerPreset::ALL[..],
            Some(ContainerPreset::from_value(app.settings.container.as_str())),
            Message::ContainerPresetSelected,
        ),
        settings_stepper_field(
            "Replay Buffer",
            "Seconds of recent gameplay kept available for saving.",
            current_buffer_secs(app),
            "sec",
            Message::BufferSecsStepped,
        ),
        settings_stepper_field(
            "Save Delay",
            "Delay after a trigger to include more post-event footage.",
            current_save_delay(app),
            "sec",
            Message::SaveDelayStepped,
        ),
    ];

    if CaptureSourcePreset::from_value(app.settings.capture_source.as_str())
        == CaptureSourcePreset::Custom
    {
        video_rows.push(settings_text_field(
            "Custom Capture Source",
            "Manual source string passed to gpu-screen-recorder.",
            &app.settings.capture_source,
            Message::CaptureSourceChanged,
        ));
    }

    if CodecPreset::from_value(app.settings.codec.as_str()) == CodecPreset::Custom {
        video_rows.push(settings_text_field(
            "Custom Codec",
            "Manual codec name passed to gpu-screen-recorder.",
            &app.settings.codec,
            Message::CodecChanged,
        ));
    }

    if ContainerPreset::from_value(app.settings.container.as_str()) == ContainerPreset::Custom {
        video_rows.push(settings_text_field(
            "Custom Container",
            "Manual container format passed to gpu-screen-recorder.",
            &app.settings.container,
            Message::ContainerChanged,
        ));
    }

    settings_section_block(
        "gpu-screen-recorder",
        "Core capture and encoding settings.",
        video_rows,
    )
}

fn obs_capture_section(app: &App) -> Element<'_, Message> {
    let mut rows = vec![
        settings_text_field(
            "OBS WebSocket URL",
            "obs-websocket endpoint, usually ws://127.0.0.1:4455.",
            &app.settings.obs_websocket_url,
            Message::ObsWebsocketUrlChanged,
        ),
        settings_text_field(
            if app.settings.obs_password_present {
                "OBS Password (stored)"
            } else {
                "OBS Password"
            },
            "Paste to replace the stored OBS websocket password. Leave blank to keep the current credential.",
            &app.settings.obs_password_input,
            Message::ObsPasswordChanged,
        ),
        row![
            settings_status_badge(
                if app.settings.obs_password_present {
                    "Password stored"
                } else {
                    "No password stored"
                },
                if app.settings.obs_password_present {
                    BadgeTone::Success
                } else {
                    BadgeTone::Neutral
                },
            ),
            with_tooltip(
                styled_button("Clear OBS Password", ButtonTone::Danger)
                    .on_press(Message::ClearObsPassword)
                    .into(),
                "Remove the stored OBS websocket credential.",
            ),
            with_tooltip(
                styled_button("Test Connection", ButtonTone::Secondary)
                    .on_press(Message::ObsTestConnection)
                    .into(),
                "Connect to OBS and verify the websocket settings before saving.",
            ),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .into(),
        settings_pick_list_field(
            "Management Mode",
            "Bring Your Own only triggers saves; Managed Recording also pushes output settings to the active OBS profile.",
            &OBS_MANAGEMENT_MODES[..],
            Some(app.settings.obs_management_mode),
            Message::ObsManagementModeSelected,
        ),
        settings_stepper_field(
            "Save Delay",
            "Delay after a trigger to include more post-event footage.",
            current_save_delay(app),
            "sec",
            Message::SaveDelayStepped,
        ),
    ];

    match app.settings.obs_management_mode {
        ObsManagementMode::BringYourOwn => {
            rows.push(
                banner("OBS owns the scene and replay-buffer setup")
                    .info()
                    .description("NaniteClip will only call SaveReplayBuffer when a rule fires. Configure your scene, audio routing, and replay buffer length in OBS.")
                    .build()
                    .into(),
            );
        }
        ObsManagementMode::ManagedRecording => {
            rows.extend([
                settings_text_field_with_button(
                    "Save Directory",
                    "OBS will record saved clips into this directory.",
                    &app.settings.save_dir,
                    Message::SaveDirChanged,
                    with_tooltip(
                        styled_button("Browse", ButtonTone::Secondary)
                            .on_press(Message::PickSaveDirectory)
                            .into(),
                        "Open a folder picker for the clip save directory.",
                    ),
                ),
                settings_pick_list_field(
                    "Container Format",
                    "OBS-managed recording supports mkv, mp4, mov, flv, and ts.",
                    &ObsContainerPreset::ALL[..],
                    Some(ObsContainerPreset::from_value(app.settings.container.as_str())),
                    |preset| Message::ContainerChanged(preset.config_value().to_string()),
                ),
                settings_stepper_field(
                    "Replay Buffer",
                    "Seconds of recent gameplay kept available for saving.",
                    current_buffer_secs(app),
                    "sec",
                    Message::BufferSecsStepped,
                ),
                banner("NaniteClip will push output settings to OBS")
                    .info()
                    .description("Your scene, capture source, and audio routing remain under your control in OBS. OBS must be using Simple Output mode for NaniteClip to manage replay-buffer settings.")
                    .build()
                    .into(),
            ]);
        }
        ObsManagementMode::FullManagement => {
            rows.push(
                banner("Full OBS management is not available yet")
                    .warning()
                    .description("This mode is reserved for a follow-up release. NaniteClip will reject it at runtime for now.")
                    .build()
                    .into(),
            );
        }
    }

    settings_section_block(
        "OBS Studio",
        "Connect to OBS and use its replay buffer instead of gpu-screen-recorder.",
        rows,
    )
}

fn audio_panel(app: &App) -> Element<'_, Message> {
    let available_device_sources = available_audio_source_options(app, DiscoveredAudioKind::Device);
    let available_application_sources =
        available_audio_source_options(app, DiscoveredAudioKind::Application);
    let selected_device_source = app
        .settings
        .selected_device_audio_source
        .as_ref()
        .filter(|selected| available_device_sources.contains(*selected))
        .cloned();
    let selected_application_source = app
        .settings
        .selected_application_audio_source
        .as_ref()
        .filter(|selected| available_application_sources.contains(*selected))
        .cloned();
    let can_add_selected_device_source = selected_device_source.is_some();
    let can_add_selected_application_source = selected_application_source.is_some();
    let device_source_placeholder = if app.settings.audio_discovery_running {
        "Loading available audio sources..."
    } else if !available_device_sources.is_empty() {
        "Choose a detected audio device"
    } else if app.settings.discovered_audio_sources.is_empty() {
        "No audio sources discovered yet"
    } else {
        "All detected device sources are already configured"
    };
    let application_source_placeholder = if app.settings.audio_discovery_running {
        "Loading detected application streams..."
    } else if !available_application_sources.is_empty() {
        "Choose a detected application stream"
    } else if app.settings.discovered_audio_sources.is_empty() {
        "No audio sources discovered yet"
    } else {
        "No unconfigured application streams are available"
    };

    let device_discovery_row: Element<'_, Message> = row![
        field_label(
            "Detected Devices",
            "Default outputs, inputs, and PipeWire/PulseAudio devices reported by gpu-screen-recorder.",
            200.0,
        ),
        pick_list(
            available_device_sources,
            selected_device_source,
            |option| Message::SelectAvailableAudioSource(DiscoveredAudioKind::Device, option),
        )
        .placeholder(device_source_placeholder)
        .width(420),
        with_tooltip(
            {
                let button = styled_button("Add Device", ButtonTone::Secondary);
                if can_add_selected_device_source {
                    button
                        .on_press(Message::AddAudioSource(DiscoveredAudioKind::Device))
                        .into()
                } else {
                    button.into()
                }
            },
            "Add the selected device or default input/output to the track list.",
        ),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into();
    let application_discovery_row: Element<'_, Message> = row![
        field_label(
            "Detected Apps",
            "Live per-application streams reported by gpu-screen-recorder. Apps only appear while they are producing audio.",
            200.0,
        ),
        pick_list(
            available_application_sources,
            selected_application_source,
            |option| Message::SelectAvailableAudioSource(DiscoveredAudioKind::Application, option),
        )
        .placeholder(application_source_placeholder)
        .width(420),
        with_tooltip(
            {
                let button = styled_button("Add App Stream", ButtonTone::Secondary);
                if can_add_selected_application_source {
                    button
                        .on_press(Message::AddAudioSource(DiscoveredAudioKind::Application))
                        .into()
                } else {
                    button.into()
                }
            },
            "Add the selected application-specific stream to the track list.",
        ),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into();

    let configured_tracks: Element<'_, Message> = if app.settings.audio_sources.is_empty() {
        empty_state("No audio tracks configured.")
            .description("Add a discovered source above to build a track layout.")
            .build()
            .into()
    } else {
        let mut tracks = column![].spacing(12);
        for (index, audio_source) in app.settings.audio_sources.iter().enumerate() {
            tracks = tracks.push(audio_source_row(index, audio_source));
        }
        tracks.into()
    };

    let mut discovery_section = section("Source Discovery")
        .description(
            "Recorder-managed devices and per-application streams you can add to the layout.",
        )
        .push(device_discovery_row)
        .push(application_discovery_row);

    if let Some(error) = &app.settings.audio_discovery_error {
        discovery_section = discovery_section.push(
            banner("Audio source discovery failed")
                .error()
                .description(error.clone())
                .build(),
        );
    }

    panel("Audio")
        .description("Input tracks and premix behavior.")
        .push(discovery_section)
        .push(
            section("Configured Tracks")
                .description("Saved audio layout for new clips.")
                .push(configured_tracks),
        )
        .build()
        .into()
}

fn clip_output_panel(app: &App) -> Element<'_, Message> {
    panel("Clip Output")
        .push(settings_section_block(
            "Naming & Notifications",
            "How saved clips are announced and named.",
            vec![
                settings_toggle_row(
                    "Overlay Notifications",
                    "Toast clip saves, character confirmation, and profile changes.",
                    app.settings.clip_saved_notifications,
                    Message::ClipSavedNotificationsToggled,
                ),
                settings_text_field(
                    "Clip Naming Template",
                    "Placeholders: {timestamp} {source} {character} {rule} {profile} {server} {continent} {base} {score} {duration}.",
                    &app.settings.clip_naming_template,
                    Message::ClipNamingTemplateChanged,
                ),
                clip_naming_preview_card(app),
            ],
        ))
        .push(settings_section_block(
            "Manual Clip",
            "Global hotkey for manually saving from the replay buffer.",
            vec![
                settings_toggle_row(
                    "Manual Clip Hotkey",
                    "Save a manual clip while the recorder is running.",
                    app.settings.manual_clip_enabled,
                    Message::ManualClipEnabledToggled,
                ),
                hotkey_capture_field(app),
                settings_stepper_field(
                    "Manual Clip Duration",
                    "Clip length when the manual hotkey fires.",
                    current_manual_clip_duration(app),
                    "sec",
                    Message::ManualClipDurationStepped,
                ),
            ],
        ))
        .build()
        .into()
}

fn delivery_panel(app: &App) -> Element<'_, Message> {
    let mut youtube_section = section("YouTube")
        .description("OAuth and upload defaults for YouTube.")
        .push(settings_toggle_row(
            "Enable YouTube Uploads",
            "Allow per-clip YouTube uploads from the Clips tab.",
            app.settings.youtube_enabled,
            Message::YouTubeEnabledToggled,
        ));

    if app.settings.youtube_enabled {
        youtube_section = youtube_section
            .push(settings_text_field(
                "YouTube OAuth Client ID",
                "Google OAuth client ID (Desktop App client recommended).",
                &app.settings.youtube_client_id,
                Message::YouTubeClientIdChanged,
            ))
            .push(settings_text_field(
                if app.settings.youtube_client_secret_present {
                    "YouTube OAuth Client Secret (stored)"
                } else {
                    "YouTube OAuth Client Secret"
                },
                "Required for confidential OAuth clients. Paste to replace.",
                &app.settings.youtube_client_secret_input,
                Message::YouTubeClientSecretChanged,
            ));

        if !app.settings.youtube_client_secret_present
            && app.settings.youtube_client_secret_input.trim().is_empty()
        {
            youtube_section = youtube_section.push(
                banner("Client secret required for some OAuth clients")
                    .warning()
                    .description("Web application OAuth clients won't connect until you enter the matching secret.")
                    .build(),
            );
        }

        youtube_section = youtube_section
            .push(settings_pick_list_field(
                "YouTube Privacy",
                "Privacy status applied to future YouTube uploads.",
                &[
                    YouTubePrivacyStatus::Public,
                    YouTubePrivacyStatus::Unlisted,
                    YouTubePrivacyStatus::Private,
                ][..],
                Some(app.settings.youtube_privacy_status),
                Message::YouTubePrivacyStatusSelected,
            ))
            .push(
                row![
                    settings_status_badge(
                        if app.settings.youtube_refresh_token_present {
                            "Account connected"
                        } else {
                            "No account connected"
                        },
                        if app.settings.youtube_refresh_token_present {
                            BadgeTone::Success
                        } else {
                            BadgeTone::Neutral
                        },
                    ),
                    with_tooltip(
                        {
                            let button = styled_button("Connect YouTube", ButtonTone::Secondary);
                            if app.settings.youtube_oauth_in_flight {
                                button.into()
                            } else {
                                button.on_press(Message::ConnectYouTube).into()
                            }
                        },
                        "Open the Google OAuth flow and store a refresh token.",
                    ),
                    with_tooltip(
                        styled_button("Disconnect YouTube", ButtonTone::Danger)
                            .on_press(Message::DisconnectYouTube)
                            .into(),
                        "Remove stored YouTube OAuth credentials.",
                    ),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            );
    } else {
        youtube_section = youtube_section
            .push(text("Enable YouTube uploads to configure OAuth and upload defaults.").size(12));
    }

    panel("Delivery & Storage")
        .push(settings_section_block(
            "Storage Tiering",
            "Move older, low-score clips to archive storage in the background.",
            {
                let mut rows = vec![settings_toggle_row(
                    "Enable Storage Tiering",
                    "Archive clips that exceed the age and score thresholds.",
                    app.settings.storage_tiering_enabled,
                    Message::StorageTieringEnabledToggled,
                )];

                if app.settings.storage_tiering_enabled {
                    rows.extend([
                        settings_text_field_with_button(
                            "Archive Directory",
                            "Directory used for lower-cost clip storage.",
                            &app.settings.storage_tier_directory,
                            Message::StorageTierDirectoryChanged,
                            with_tooltip(
                                styled_button("Browse", ButtonTone::Secondary)
                                    .on_press(Message::PickStorageTierDirectory)
                                    .into(),
                                "Open a folder picker for the archive directory.",
                            ),
                        ),
                        settings_stepper_field(
                            "Archive After",
                            "Minimum clip age before archive is eligible.",
                            current_storage_min_age_days(app),
                            "days",
                            Message::StorageMinAgeDaysStepped,
                        ),
                        settings_stepper_field(
                            "Archive Score Ceiling",
                            "Only clips at or below this score are archived.",
                            current_storage_max_score(app),
                            "pts",
                            Message::StorageMaxScoreStepped,
                        ),
                        with_tooltip(
                            styled_button("Run Tiering Sweep", ButtonTone::Secondary)
                                .on_press(Message::RunStorageTieringSweep)
                                .into(),
                            "Queue a sweep that archives all currently eligible clips.",
                        ),
                    ]);
                } else {
                    rows.push(
                        text(
                            "Enable storage tiering to configure archive location and thresholds.",
                        )
                        .size(12)
                        .into(),
                    );
                }

                rows
            },
        ))
        .push(settings_section_block(
            "Copyparty",
            "Secrets are stored in the secure credential backend, not config.toml.",
            {
                let mut rows = vec![settings_toggle_row(
                    "Enable Copyparty Uploads",
                    "Allow per-clip Copyparty uploads from the Clips tab.",
                    app.settings.copyparty_enabled,
                    Message::CopypartyEnabledToggled,
                )];

                if app.settings.copyparty_enabled {
                    rows.extend([
                        settings_text_field(
                            "Copyparty Upload URL",
                            "Upload URL, e.g. `https://clips.example.com/up/`.",
                            &app.settings.copyparty_upload_url,
                            Message::CopypartyUploadUrlChanged,
                        ),
                        settings_text_field(
                            "Copyparty Public Base URL",
                            "Optional public base URL when the server returns a relative path.",
                            &app.settings.copyparty_public_base_url,
                            Message::CopypartyPublicBaseUrlChanged,
                        ),
                        settings_text_field(
                            "Copyparty Username",
                            "Optional username for basic auth or `--usernames`.",
                            &app.settings.copyparty_username,
                            Message::CopypartyUsernameChanged,
                        ),
                        settings_text_field(
                            if app.settings.copyparty_password_present {
                                "Copyparty Password (stored)"
                            } else {
                                "Copyparty Password"
                            },
                            "Paste to replace the stored password. Blank keeps the current one.",
                            &app.settings.copyparty_password_input,
                            Message::CopypartyPasswordChanged,
                        ),
                        row![
                            settings_status_badge(
                                if app.settings.copyparty_password_present {
                                    "Password stored"
                                } else {
                                    "No password stored"
                                },
                                if app.settings.copyparty_password_present {
                                    BadgeTone::Success
                                } else {
                                    BadgeTone::Neutral
                                },
                            ),
                            with_tooltip(
                                styled_button("Clear Copyparty Password", ButtonTone::Danger)
                                    .on_press(Message::ClearCopypartyPassword)
                                    .into(),
                                "Remove the stored Copyparty credential.",
                            ),
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center)
                        .into(),
                    ]);
                } else {
                    rows.push(
                        text("Enable Copyparty uploads to configure URLs and credentials.")
                            .size(12)
                            .into(),
                    );
                }

                rows
            },
        ))
        .push(youtube_section)
        .push(settings_section_block(
            "Discord Webhook",
            "Webhook notifications for high-value clips after they save.",
            {
                let mut rows = vec![settings_toggle_row(
                    "Enable Discord Webhook",
                    "Post qualifying clips to a stored Discord webhook.",
                    app.settings.discord_enabled,
                    Message::DiscordWebhookEnabledToggled,
                )];

                if app.settings.discord_enabled {
                    rows.extend([
                        settings_stepper_field(
                            "Minimum Score",
                            "Only clips at or above this score fire the webhook.",
                            current_discord_min_score(app),
                            "pts",
                            Message::DiscordMinScoreStepped,
                        ),
                        settings_toggle_row(
                            "Attach Thumbnail",
                            "Extract a thumbnail with ffmpeg and attach it.",
                            app.settings.discord_include_thumbnail,
                            Message::DiscordIncludeThumbnailToggled,
                        ),
                        settings_text_field(
                            if app.settings.discord_webhook_present {
                                "Discord Webhook URL (stored)"
                            } else {
                                "Discord Webhook URL"
                            },
                            "Paste to store or replace. Blank keeps the current one.",
                            &app.settings.discord_webhook_input,
                            Message::DiscordWebhookUrlChanged,
                        ),
                        row![
                            settings_status_badge(
                                if app.settings.discord_webhook_present {
                                    "Webhook stored"
                                } else {
                                    "No webhook stored"
                                },
                                if app.settings.discord_webhook_present {
                                    BadgeTone::Success
                                } else {
                                    BadgeTone::Neutral
                                },
                            ),
                            with_tooltip(
                                styled_button("Clear Discord Webhook", ButtonTone::Danger)
                                    .on_press(Message::ClearDiscordWebhook)
                                    .into(),
                                "Remove the stored webhook URL.",
                            ),
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center)
                        .into(),
                    ]);
                } else {
                    rows.push(
                        text("Enable the Discord webhook to configure thresholds and credentials.")
                            .size(12)
                            .into(),
                    );
                }

                rows
            },
        ))
        .build()
        .into()
}

fn update_action_tone(action: UpdatePrimaryAction) -> ButtonTone {
    match action {
        UpdatePrimaryAction::DownloadUpdate | UpdatePrimaryAction::OpenSystemUpdater => {
            ButtonTone::Primary
        }
        UpdatePrimaryAction::InstallAndRestart => ButtonTone::Success,
        UpdatePrimaryAction::InstallWhenIdle
        | UpdatePrimaryAction::InstallOnNextLaunch
        | UpdatePrimaryAction::RemindLater => ButtonTone::Secondary,
        UpdatePrimaryAction::SkipThisVersion => ButtonTone::Danger,
    }
}

fn active_release_signature_summary(app: &App) -> String {
    let verifier_key_count = crate::update::update_public_keys().len();
    let signature = app
        .settings
        .selected_rollback_release
        .as_ref()
        .map(|release| &release.signature)
        .or_else(|| {
            app.updates
                .state
                .latest_release
                .as_ref()
                .map(|release| &release.signature)
        })
        .or_else(|| {
            app.updates
                .state
                .prepared_update
                .as_ref()
                .map(|prepared| &prepared.signature)
        });

    if let Some(signature) = signature {
        let key_id = signature.key_id.as_deref().unwrap_or("not reported");
        let key_label = signature.key_label.as_deref().unwrap_or("not reported");
        let algorithm = signature.algorithm.as_deref().unwrap_or("ed25519");
        format!(
            "Signature metadata: {algorithm} via key `{key_id}` ({key_label}). Embedded verifier keys: {verifier_key_count}."
        )
    } else {
        format!(
            "Signature metadata is not loaded yet. Embedded verifier keys: {verifier_key_count}."
        )
    }
}

fn update_action_controls(app: &App) -> Element<'_, Message> {
    let selected_action = super::super::selected_update_action(app);

    row![
        with_tooltip(
            {
                let button =
                    styled_button(selected_action.label(), update_action_tone(selected_action));
                if super::super::can_run_selected_update_action(app) {
                    button.on_press(Message::RunSelectedUpdateAction).into()
                } else {
                    button.into()
                }
            },
            selected_action.description(),
        ),
        pick_list(
            super::super::update_action_options(app),
            Some(selected_action),
            Message::UpdatePrimaryActionSelected,
        )
        .width(220),
        with_tooltip(
            {
                let button = styled_button("View Changelog", ButtonTone::Secondary);
                if app.updates.state.latest_release.is_some()
                    || app.updates.state.prepared_update.is_some()
                    || app.settings.selected_rollback_release.is_some()
                {
                    button.on_press(Message::ViewUpdateDetails).into()
                } else {
                    button.into()
                }
            },
            "Open the in-app changelog and updater details viewer for the active release target.",
        ),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into()
}

fn update_panel(app: &App) -> Element<'_, Message> {
    let last_checked_summary = app
        .updates
        .state
        .last_checked_at
        .map(|checked_at| {
            format!(
                "Last checked: {}",
                super::clips::format_timestamp(checked_at)
            )
        })
        .unwrap_or_else(|| "Last checked: not yet".into());
    let next_check_summary =
        if let Some(next_check) = super::super::next_automatic_update_check_at(app) {
            format!(
                "Next automatic check: {}",
                super::clips::format_timestamp(next_check)
            )
        } else {
            "Next automatic check: automatic checks are off".into()
        };
    let latest_release_summary = app
        .updates
        .state
        .latest_release
        .as_ref()
        .map(|release| {
            format!(
                "{} ({}) · {}",
                release.version,
                release.release_name,
                release.policy.availability.label()
            )
        })
        .or_else(|| {
            app.updates.state.last_checked_at.map(|checked_at| {
                format!(
                    "No newer release found. Last checked {}.",
                    super::clips::format_timestamp(checked_at)
                )
            })
        })
        .unwrap_or_else(|| "No update check has completed yet.".into());
    let phase_summary = match app.updates.state.phase {
        UpdatePhase::Checking
        | UpdatePhase::Downloading
        | UpdatePhase::Verifying
        | UpdatePhase::Applying => app
            .updates
            .state
            .progress
            .as_ref()
            .map(|progress| format!("{}: {}", app.updates.state.phase.label(), progress.detail))
            .unwrap_or_else(|| app.updates.state.phase.label().into()),
        UpdatePhase::ReadyToInstall => app
            .updates
            .state
            .prepared_update
            .as_ref()
            .map(|prepared| {
                let prepared_version = prepared
                    .parsed_version()
                    .unwrap_or_else(|| app.updates.state.current_version.clone());
                format!(
                    "Ready to {}: {}",
                    if prepared_version < app.updates.state.current_version {
                        "roll back"
                    } else {
                        "install"
                    },
                    prepared.version
                )
            })
            .unwrap_or_else(|| app.updates.state.phase.label().into()),
        UpdatePhase::Failed => app
            .updates
            .state
            .last_error
            .as_ref()
            .map(|error| format!("{} issue: {}", error.kind.label(), error.detail))
            .unwrap_or_else(|| "Updater failed.".into()),
        UpdatePhase::Idle => "Updater idle.".into(),
    };
    let reminder_summary = app
        .updates
        .state
        .latest_release
        .as_ref()
        .filter(|release| app.is_update_reminder_deferred(&release.version.to_string()))
        .and_then(|release| {
            app.config.updates.remind_later_until_utc.map(|until| {
                format!(
                    "Reminders for {} are snoozed until {}.",
                    release.version,
                    super::clips::format_timestamp(until)
                )
            })
        });
    let security_summary = active_release_signature_summary(app);
    let last_apply_summary = app
        .updates
        .state
        .last_apply_report
        .as_ref()
        .map(|report| {
            format!(
                "Last apply: {} {} at {}.",
                match report.status {
                    crate::update::UpdateApplyReportStatus::Succeeded => "succeeded for",
                    crate::update::UpdateApplyReportStatus::Failed => "failed for",
                },
                report.target_version,
                super::clips::format_timestamp(report.finished_at)
            )
        })
        .unwrap_or_else(|| "Last apply: no helper result recorded yet.".into());
    let previous_installed_summary = app
        .updates
        .state
        .previous_installed_version
        .as_ref()
        .map(|version| format!("Previously installed version: {version}"))
        .unwrap_or_else(|| "Previously installed version: not recorded yet".into());
    let selected_rollback_summary = app
        .settings
        .selected_rollback_release
        .as_ref()
        .map(|release| format!("Selected rollback target: {}.", release.version))
        .unwrap_or_else(|| "Selected rollback target: none".into());
    let rollback_catalog_summary = if app.updates.state.rollback_catalog_loading {
        "Rollback version list: loading…".into()
    } else if app.updates.state.rollback_candidates.is_empty() {
        "Rollback version list: not loaded yet or no compatible older versions were found.".into()
    } else {
        format!(
            "Rollback version list: {} compatible older release(s) available.",
            app.updates.state.rollback_candidates.len()
        )
    };

    let mut available_release_rows = vec![
        text(phase_summary).size(12).into(),
        text(latest_release_summary).size(12).into(),
        text(
            app.updates
                .state
                .latest_release
                .as_ref()
                .map(|release| {
                    super::super::release_policy_summary(
                        release,
                        &app.updates.state.current_version,
                        app.updates.state.system_update_plan.as_ref(),
                    )
                })
                .unwrap_or_else(|| {
                    app.updates
                        .state
                        .install_channel
                        .update_instructions()
                        .into()
                }),
        )
        .size(12)
        .into(),
        text(security_summary).size(12).into(),
        text(last_apply_summary).size(12).into(),
        text(format!(
            "Install behavior: {}.",
            app.settings.update_install_behavior.description()
        ))
        .size(12)
        .into(),
    ];
    if let Some(plan) = app.updates.state.system_update_plan.as_ref() {
        available_release_rows.push(
            text(super::super::system_update_plan_summary(plan))
                .size(12)
                .into(),
        );
    }
    if let Some(summary) = reminder_summary {
        available_release_rows.push(text(summary).size(12).into());
    }
    available_release_rows.push(
        row![with_tooltip(
            styled_button("Check for Updates", ButtonTone::Secondary)
                .on_press(Message::CheckForUpdates)
                .into(),
            "Query the latest GitHub Release for the selected channel.",
        ),]
        .spacing(8)
        .into(),
    );
    if app.updates.state.latest_release.is_some() || app.updates.state.prepared_update.is_some() {
        available_release_rows.push(
            text(format!(
                "Selected action: {}",
                super::super::selected_update_action(app).description()
            ))
            .size(12)
            .into(),
        );
        available_release_rows.push(update_action_controls(app));
    }

    panel("Application Updates")
        .push(settings_section_block(
            "Preferences",
            "How NaniteClip checks GitHub Releases and which release stream it follows.",
            vec![
                text(format!(
                    "Current version: {} · Install channel: {}",
                    crate::update::current_version_label(),
                    app.updates.state.install_channel.label()
                ))
                .size(12)
                .into(),
                settings_toggle_row(
                    "Automatic Update Checks",
                    "Check GitHub Releases in the background and show an update banner when a newer version is available.",
                    app.settings.update_auto_check,
                    Message::UpdateAutoCheckToggled,
                ),
                settings_pick_list_field(
                    "Release Channel",
                    "Choose which release channel to check. Stable ignores GitHub prereleases. Beta includes them.",
                    &[UpdateChannel::Stable, UpdateChannel::Beta][..],
                    Some(app.settings.update_channel),
                    Message::UpdateChannelSelected,
                ),
                settings_pick_list_field(
                    "Downloaded Update Behavior",
                    "Choose what NaniteClip should do after an update is downloaded.",
                    &[
                        UpdateInstallBehavior::Manual,
                        UpdateInstallBehavior::WhenIdle,
                        UpdateInstallBehavior::OnNextLaunch,
                    ][..],
                    Some(app.settings.update_install_behavior),
                    Message::UpdateInstallBehaviorSelected,
                ),
                text(last_checked_summary).size(12).into(),
                text(next_check_summary).size(12).into(),
            ],
        ))
        .push(settings_section_block(
            "Available Release",
            "Check, download, and apply updates for this installation.",
            available_release_rows,
        ))
        .push(settings_section_block(
            "Rollback",
            "Rollback to the previous installed version or choose a specific older GitHub Release.",
            vec![
                text(previous_installed_summary).size(12).into(),
                text(selected_rollback_summary).size(12).into(),
                text(rollback_catalog_summary).size(12).into(),
                settings_pick_list_field(
                    "Rollback Target",
                    "Choose a specific older release to download and install for this channel.",
                    app.updates.state.rollback_candidates.as_slice(),
                    app.settings.selected_rollback_release.clone(),
                    |value| Message::RollbackReleaseSelected(Box::new(value)),
                ),
                row![
                    with_tooltip(
                        styled_button("Refresh Versions", ButtonTone::Secondary)
                            .on_press(Message::RefreshRollbackCatalog)
                            .into(),
                        "Load older signed releases and their matching assets for this install channel.",
                    ),
                    with_tooltip(
                        {
                            let button = styled_button(
                                "Rollback to Previous",
                                ButtonTone::Warning,
                            );
                            if app.updates.state.previous_installed_version.is_some() {
                                button
                                    .on_press(Message::RollbackToPreviousInstalledVersion)
                                    .into()
                            } else {
                                button.into()
                            }
                        },
                        "Download the last installed version and stage it as a rollback target.",
                    ),
                    with_tooltip(
                        {
                            let button = styled_button(
                                "Download Selected Version",
                                ButtonTone::Warning,
                            );
                            if app.settings.selected_rollback_release.is_some()
                                && !matches!(
                                    app.updates.state.phase,
                                    UpdatePhase::Downloading
                                        | UpdatePhase::Verifying
                                        | UpdatePhase::Applying
                                )
                            {
                                button.on_press(Message::DownloadSelectedRollbackVersion).into()
                            } else {
                                button.into()
                            }
                        },
                        "Download the selected older release and stage it for rollback.",
                    ),
                ]
                .spacing(8)
                .into(),
            ],
        ))
        .build()
        .into()
}

fn maintenance_panel(app: &App) -> Element<'_, Message> {
    panel("Database Maintenance")
        .push(settings_section_block(
            "Maintenance Actions",
            "Schema upgrades create a timestamped SQLite backup first.",
            vec![
                row![
                    with_tooltip(
                        styled_button("Backup Database", ButtonTone::Secondary)
                            .on_press(Message::BackupDatabase)
                            .into(),
                        "Write a SQLite backup next to the save directory.",
                    ),
                    with_tooltip(
                        styled_button("Export JSON", ButtonTone::Secondary)
                            .on_press(Message::ExportJson)
                            .into(),
                        "Export clip metadata and aggregate events as JSON.",
                    ),
                    with_tooltip(
                        styled_button("Export CSV", ButtonTone::Secondary)
                            .on_press(Message::ExportCsv)
                            .into(),
                        "Export clip metadata and aggregate events as CSV.",
                    ),
                ]
                .spacing(8)
                .into(),
                text(format!(
                    "Exports and backups are written under {}.",
                    app.settings.save_dir
                ))
                .size(12)
                .into(),
            ],
        ))
        .build()
        .into()
}

fn settings_section_block<'a>(
    title: &'a str,
    description: &'a str,
    rows: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut block = section(title).description(description);
    for row in rows {
        block = block.push(row);
    }
    block.build().into()
}

fn settings_status_badge<'a>(label: impl Into<String>, tone: BadgeTone) -> Element<'a, Message> {
    badge(label).tone(tone).build().into()
}

fn settings_toggle_row<'a>(
    title: &'a str,
    description: &'a str,
    value: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
) -> Element<'a, Message> {
    row![
        column![
            text(title).size(14),
            text(description).size(12).width(Length::Fill),
        ]
        .spacing(4)
        .width(Length::Fill),
        toggle_switch(value)
            .label(if value { "On" } else { "Off" })
            .on_toggle(on_toggle),
    ]
    .spacing(12)
    .align_y(iced::Alignment::Center)
    .into()
}

fn clip_naming_preview_card(app: &App) -> Element<'_, Message> {
    let mut lines = column![text("Supported placeholders").size(14)].spacing(6);
    for placeholder in crate::clip_naming::SUPPORTED_PLACEHOLDERS {
        lines = lines.push(
            text(format!(
                "{} -> {} ({})",
                placeholder.token, placeholder.example, placeholder.description
            ))
            .size(12),
        );
    }

    match crate::clip_naming::preview_template(app.settings.clip_naming_template.as_str()) {
        Ok(preview) => {
            lines = lines.push(settings_status_badge(
                format!("Example output: {preview}.mkv"),
                BadgeTone::Success,
            ));
        }
        Err(error) => {
            lines = lines.push(settings_status_badge(
                format!("Template preview error: {error}"),
                BadgeTone::Warning,
            ));
        }
    }

    card()
        .title("Template Preview")
        .description("Use this to validate naming tokens before saving.")
        .body(lines)
        .width(Length::Fill)
        .build()
        .into()
}

fn configured_audio_source_count(app: &App) -> usize {
    app.settings
        .audio_sources
        .iter()
        .filter(|draft| !audio_source_draft_is_blank(draft))
        .count()
}

fn hotkey_capture_field(app: &App) -> Element<'_, Message> {
    let description = "Click and press a key combination. X11 uses the direct backend; Wayland uses the desktop portal.";
    let binding_label = if app.settings.hotkey_capture_active {
        "Press the desired key combination..."
    } else if app.settings.manual_clip_hotkey.trim().is_empty() {
        "Click to record a hotkey"
    } else {
        app.settings.manual_clip_hotkey.as_str()
    };
    let status = if app.settings.hotkey_capture_active {
        "Listening — click again to cancel."
    } else {
        "Click and press the combination you want."
    };
    let on_press = if app.settings.hotkey_capture_active {
        Message::CancelHotkeyCapture
    } else {
        Message::BeginHotkeyCapture
    };

    row![
        field_label("Hotkey Binding", description, 200.0),
        mouse_area(
            container(text(binding_label).size(14))
                .width(300)
                .padding([8, 12])
                .style(rounded_box),
        )
        .on_press(on_press),
        text(status).size(12),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into()
}

fn audio_source_row(index: usize, audio_source: &AudioSourceDraft) -> Element<'static, Message> {
    let title = if audio_source.label.trim().is_empty() {
        format!("Track {}", index + 1)
    } else {
        audio_source.label.clone()
    };
    let description = if audio_source.source.trim().is_empty() {
        "Choose a discovered source above or enter a custom backend source string.".into()
    } else {
        format!("Source: {}", audio_source.source)
    };

    card()
        .title(title)
        .description(description)
        .body(
            column![
                row![
                    field_label(
                        "Track Label",
                        "Label written into the saved track metadata.",
                        200.0,
                    ),
                    text_input("Game / Mic / Discord", &audio_source.label)
                        .on_input(move |value| Message::AudioSourceLabelChanged(index, value))
                        .width(260),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
                row![
                    field_label(
                        "Source",
                        "Backend source string, e.g. default_output, app:Discord, device:alsa_output.",
                        200.0,
                    ),
                    text_input("default_output", &audio_source.source)
                        .on_input(move |value| Message::AudioSourceValueChanged(index, value))
                        .width(420),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
                row![
                    field_label(
                        "Premix Gain",
                        "Gain applied when this track is in the premix.",
                        200.0,
                    ),
                    styled_button("-", ButtonTone::Secondary)
                        .on_press(Message::AudioSourceGainStepped(index, -1)),
                    text(format!("{:+.1} dB", audio_source.gain_db)).width(100),
                    styled_button("+", ButtonTone::Secondary)
                        .on_press(Message::AudioSourceGainStepped(index, 1)),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
                row![
                    with_tooltip(
                        checkbox(audio_source.included_in_premix)
                            .label("Include In Premix")
                            .on_toggle(move |value| {
                                Message::AudioSourceIncludedInPremixToggled(index, value)
                            })
                            .into(),
                        "Feed this track into the generated premix stream.",
                    ),
                    with_tooltip(
                        checkbox(audio_source.muted_in_premix)
                            .label("Mute In Premix")
                            .on_toggle(move |value| {
                                Message::AudioSourceMutedInPremixToggled(index, value)
                            })
                            .into(),
                        "Keep the original track but omit it from the premix.",
                    ),
                ]
                .spacing(12)
                .align_y(iced::Alignment::Center),
            ]
            .spacing(10),
        )
        .footer(
            row![
                styled_button("Move Up", ButtonTone::Secondary)
                    .on_press(Message::MoveAudioSourceUp(index)),
                styled_button("Move Down", ButtonTone::Secondary)
                    .on_press(Message::MoveAudioSourceDown(index)),
                styled_button("Remove", ButtonTone::Danger)
                    .on_press(Message::RemoveAudioSource(index)),
            ]
            .spacing(8),
        )
        .width(Length::Fill)
        .build()
        .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApplyDiscoveredAudioSource {
    Added,
    UpdatedExisting,
    Unchanged,
}

fn apply_discovered_audio_source(
    drafts: &mut Vec<AudioSourceDraft>,
    discovered: &DiscoveredAudioSource,
) -> ApplyDiscoveredAudioSource {
    if let Some(existing) = drafts.iter_mut().find(|draft| {
        draft
            .source
            .trim()
            .eq_ignore_ascii_case(discovered.kind_hint.config_display_value().as_str())
    }) {
        if existing.label.trim().is_empty() && !discovered.display_label.trim().is_empty() {
            existing.label = discovered.display_label.clone();
            return ApplyDiscoveredAudioSource::UpdatedExisting;
        }

        return ApplyDiscoveredAudioSource::Unchanged;
    }

    let replacement = AudioSourceDraft {
        label: discovered.display_label.clone(),
        source: discovered.kind_hint.config_display_value(),
        gain_db: 0.0,
        muted_in_premix: false,
        included_in_premix: true,
    };

    if drafts.len() == 1 && drafts.first().is_some_and(audio_source_draft_is_blank) {
        drafts[0] = replacement;
    } else {
        drafts.push(replacement);
    }

    ApplyDiscoveredAudioSource::Added
}

fn audio_source_draft_is_blank(draft: &AudioSourceDraft) -> bool {
    draft.label.trim().is_empty() && draft.source.trim().is_empty()
}

fn available_audio_source_options(
    app: &App,
    kind: DiscoveredAudioKind,
) -> Vec<AvailableAudioSourceOption> {
    app.settings
        .discovered_audio_sources
        .iter()
        .filter(|audio_source| audio_source.kind == kind)
        .filter(|audio_source| {
            !app.settings.audio_sources.iter().any(|configured| {
                configured
                    .source
                    .trim()
                    .eq_ignore_ascii_case(audio_source.kind_hint.config_display_value().as_str())
            })
        })
        .map(|audio_source| AvailableAudioSourceOption {
            label: format_discovered_audio_source_label(audio_source),
            source: audio_source.kind_hint.config_display_value(),
        })
        .collect()
}

fn sync_selected_audio_sources(app: &mut App) {
    let available_devices = available_audio_source_options(app, DiscoveredAudioKind::Device);
    app.settings.selected_device_audio_source = app
        .settings
        .selected_device_audio_source
        .as_ref()
        .filter(|selected| available_devices.contains(*selected))
        .cloned()
        .or_else(|| available_devices.into_iter().next());

    let available_applications =
        available_audio_source_options(app, DiscoveredAudioKind::Application);
    app.settings.selected_application_audio_source = app
        .settings
        .selected_application_audio_source
        .as_ref()
        .filter(|selected| available_applications.contains(*selected))
        .cloned()
        .or_else(|| available_applications.into_iter().next());
}

fn format_discovered_audio_source_label(audio_source: &DiscoveredAudioSource) -> String {
    let kind = match audio_source.kind {
        crate::capture::DiscoveredAudioKind::Device => "Device",
        crate::capture::DiscoveredAudioKind::Application => "App",
    };
    let label = if audio_source.display_label.trim().is_empty() {
        audio_source.kind_hint.config_display_value()
    } else {
        audio_source.display_label.clone()
    };
    if audio_source.available {
        format!("{kind}: {label}")
    } else {
        format!("{kind}: {label} (not currently running)")
    }
}

fn run_database_action<F, Fut>(
    app: &mut App,
    action_label: &str,
    destination: Result<PathBuf, String>,
    operation: F,
) -> iced::Task<AppMessage>
where
    F: FnOnce(crate::db::ClipStore, PathBuf) -> Fut + 'static,
    Fut: std::future::Future<Output = Result<String, String>> + Send + 'static,
{
    let Some(store) = app.clip_store.clone() else {
        app.set_settings_feedback(
            format!("The clip database is unavailable, so the {action_label} cannot run."),
            false,
        );
        return iced::Task::none();
    };

    let destination = match destination {
        Ok(destination) => destination,
        Err(error) => {
            app.set_settings_feedback(error, false);
            return iced::Task::none();
        }
    };

    let is_backup = action_label == "database backup";
    iced::Task::perform(operation(store, destination), move |result| {
        if is_backup {
            AppMessage::Settings(Message::BackupCompleted(result))
        } else {
            AppMessage::Settings(Message::ExportCompleted(result))
        }
    })
}

fn backup_destination(base_directory: &str, extension: &str) -> Result<PathBuf, String> {
    let directory = PathBuf::from(base_directory.trim());
    if directory.as_os_str().is_empty() {
        return Err("Choose a save directory before running backup or export actions.".into());
    }

    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("Failed to create {}: {error}", directory.display()))?;

    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    let file_name = format!("nanite-clip-{timestamp}.{extension}");
    Ok(directory.join(file_name))
}

fn parse_or_default(value: &str, default: u32) -> u32 {
    value.trim().parse().unwrap_or(default)
}

fn step_numeric_setting(
    value: &mut String,
    delta: i32,
    step: u32,
    min: u32,
    max: u32,
    default: u32,
) {
    let current = parse_or_default(value, default);
    let signed_step = step as i64 * i64::from(delta);
    let next = (i64::from(current) + signed_step).clamp(i64::from(min), i64::from(max));
    *value = next.to_string();
}

fn non_empty_or_default(value: &str, default: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

fn current_framerate(app: &App) -> u32 {
    parse_or_default(&app.settings.framerate, app.config.recorder.gsr().framerate)
}

fn current_quality(app: &App) -> u32 {
    parse_or_default(&app.settings.quality, 40_000)
}

fn current_buffer_secs(app: &App) -> u32 {
    parse_or_default(
        &app.settings.buffer_secs,
        app.config.recorder.replay_buffer_secs,
    )
}

fn current_save_delay(app: &App) -> u32 {
    parse_or_default(
        &app.settings.save_delay_secs,
        app.config.recorder.save_delay_secs,
    )
}

fn current_manual_clip_duration(app: &App) -> u32 {
    parse_or_default(
        &app.settings.manual_clip_duration_secs,
        app.config.manual_clip.duration_secs,
    )
}

fn current_storage_min_age_days(app: &App) -> u32 {
    parse_or_default(
        &app.settings.storage_min_age_days,
        app.config.storage_tiering.min_age_days,
    )
}

fn current_storage_max_score(app: &App) -> u32 {
    parse_or_default(
        &app.settings.storage_max_score,
        app.config.storage_tiering.max_score,
    )
}

fn current_discord_min_score(app: &App) -> u32 {
    parse_or_default(
        &app.settings.discord_min_score,
        app.config.discord_webhook.min_score,
    )
}

fn selected_capture_backend(app: &App) -> CaptureBackendPreset {
    CaptureBackendPreset::from_config(app.settings.capture_backend)
}

fn obs_audio_is_backend_owned(app: &App) -> bool {
    selected_capture_backend(app) == CaptureBackendPreset::Obs
        && app.settings.obs_management_mode != ObsManagementMode::FullManagement
}

fn obs_management_mode_label(mode: ObsManagementMode) -> &'static str {
    match mode {
        ObsManagementMode::BringYourOwn => "Bring Your Own",
        ObsManagementMode::ManagedRecording => "Managed Recording",
        ObsManagementMode::FullManagement => "Full Management",
    }
}

const OBS_MANAGEMENT_MODES: [ObsManagementMode; 3] = [
    ObsManagementMode::BringYourOwn,
    ObsManagementMode::ManagedRecording,
    ObsManagementMode::FullManagement,
];

async fn pick_directory(current_dir: String) -> Result<Option<String>, String> {
    pick_directory_impl(current_dir)
}

pub(super) async fn pick_toml_file(
    current_path: String,
    title: String,
) -> Result<Option<String>, String> {
    pick_toml_file_impl(current_path, title)
}

pub(super) async fn save_file(
    initial_path: String,
    title: String,
) -> Result<Option<String>, String> {
    save_file_impl(initial_path, title)
}
