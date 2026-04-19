use std::path::PathBuf;

use chrono::Utc;
use iced::Element;
use iced::Length;

use super::super::shared::{
    ButtonTone, field_label, settings_pick_list_field, settings_stepper_field, settings_text_field,
    settings_text_field_with_button, styled_button, with_tooltip,
};
use super::super::{App, AudioSourceDraft, Message as AppMessage};
use crate::capture::{DiscoveredAudioKind, DiscoveredAudioSource};
use crate::config::{
    AudioSourceConfig, ObsManagementMode, UpdateChannel, YouTubePrivacyStatus,
    legacy_audio_source_kind_from_value,
};
use crate::secure_store::SecretKey;
use crate::ui::app::{
    checkbox, column, container, mouse_area, pick_list, rounded_box, row, scrollable, text,
    text_input,
};
use crate::ui::layout::card::card;
use crate::ui::layout::empty_state::empty_state;
use crate::ui::layout::page_header::page_header;
use crate::ui::layout::panel::panel;
use crate::ui::layout::section::section;
use crate::ui::layout::toolbar::toolbar;
use crate::ui::overlay::banner::banner;
use crate::ui::primitives::badge::{Tone as BadgeTone, badge};
use crate::ui::primitives::switch::switch as toggle_switch;
use crate::update::{AvailableRelease, UpdateInstallBehavior, UpdatePhase, UpdatePrimaryAction};

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

#[derive(Debug, Clone)]
pub enum Message {
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
    Save,
}

pub(in crate::app) fn update(app: &mut App, message: Message) -> iced::Task<AppMessage> {
    match message {
        Message::ServiceIdChanged(value) => {
            app.settings_service_id = value;
            iced::Task::none()
        }
        Message::CaptureBackendSelected(backend) => {
            app.settings_capture_backend = backend
                .config_value()
                .expect("capture backend presets always have a config value")
                .to_string();
            app.settings_audio_discovery_error = None;
            iced::Task::none()
        }
        Message::LaunchAtLoginToggled(value) => {
            app.settings_launch_at_login = value;
            iced::Task::none()
        }
        Message::AutoStartMonitoringToggled(value) => {
            app.settings_auto_start_monitoring = value;
            iced::Task::none()
        }
        Message::StartMinimizedToggled(value) => {
            app.settings_start_minimized = value;
            iced::Task::none()
        }
        Message::MinimizeToTrayToggled(value) => {
            app.settings_minimize_to_tray = value;
            iced::Task::none()
        }
        Message::UpdateAutoCheckToggled(value) => {
            app.settings_update_auto_check = value;
            iced::Task::none()
        }
        Message::UpdateChannelSelected(value) => {
            app.settings_update_channel = value;
            app.settings_selected_rollback_release = None;
            app.update_state.rollback_candidates.clear();
            iced::Task::done(AppMessage::RefreshRollbackCatalog)
        }
        Message::UpdateInstallBehaviorSelected(value) => {
            app.settings_update_install_behavior = value;
            iced::Task::none()
        }
        Message::UpdatePrimaryActionSelected(value) => {
            app.settings_selected_update_action = value;
            iced::Task::none()
        }
        Message::RollbackReleaseSelected(value) => {
            app.settings_selected_rollback_release = Some(*value);
            iced::Task::none()
        }
        Message::CaptureSourceChanged(value) => {
            app.settings_capture_source = value;
            iced::Task::none()
        }
        Message::CaptureSourcePresetSelected(preset) => {
            app.settings_capture_source =
                apply_preset_string_selection(app.settings_capture_source.as_str(), preset);
            iced::Task::none()
        }
        Message::SaveDirChanged(value) => {
            app.settings_save_dir = value;
            app.clear_settings_feedback();
            iced::Task::none()
        }
        Message::PickSaveDirectory => {
            let current_dir = app.settings_save_dir.clone();
            iced::Task::perform(async move { pick_directory(current_dir).await }, |result| {
                AppMessage::Settings(Message::SaveDirectoryPicked(result))
            })
        }
        Message::SaveDirectoryPicked(result) => {
            match result {
                Ok(Some(path)) => {
                    app.settings_save_dir = path;
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
                &mut app.settings_framerate,
                delta,
                5,
                30,
                240,
                app.config.recorder.gsr().framerate,
            );
            iced::Task::none()
        }
        Message::CodecChanged(value) => {
            app.settings_codec = value;
            iced::Task::none()
        }
        Message::CodecPresetSelected(preset) => {
            app.settings_codec = apply_preset_string_selection(app.settings_codec.as_str(), preset);
            iced::Task::none()
        }
        Message::QualityStepped(delta) => {
            let current_quality = parse_or_default(&app.settings_quality, 40_000);
            step_numeric_setting(
                &mut app.settings_quality,
                delta,
                1_000,
                1_000,
                500_000,
                current_quality,
            );
            iced::Task::none()
        }
        Message::ContainerChanged(value) => {
            app.settings_container = value;
            iced::Task::none()
        }
        Message::ContainerPresetSelected(preset) => {
            app.settings_container =
                apply_preset_string_selection(app.settings_container.as_str(), preset);
            iced::Task::none()
        }
        Message::ObsWebsocketUrlChanged(value) => {
            app.settings_obs_websocket_url = value;
            iced::Task::none()
        }
        Message::ObsPasswordChanged(value) => {
            app.settings_obs_password_input = value;
            iced::Task::none()
        }
        Message::ClearObsPassword => {
            match app.secure_store.delete(SecretKey::ObsWebsocketPassword) {
                Ok(()) => {
                    app.settings_obs_password_present = false;
                    app.settings_obs_password_input.clear();
                    app.config.recorder.obs_mut().websocket_password = None;
                    app.clear_settings_feedback();
                }
                Err(error) => app.set_settings_feedback(error, false),
            }
            iced::Task::none()
        }
        Message::ObsManagementModeSelected(mode) => {
            app.settings_obs_management_mode = mode;
            iced::Task::none()
        }
        Message::ObsTestConnection => {
            let mut config = app.config.recorder.obs().clone();
            config.websocket_url = app.settings_obs_websocket_url.clone();
            if !app.settings_obs_password_input.trim().is_empty() {
                config.websocket_password =
                    Some(app.settings_obs_password_input.trim().to_string());
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
                &mut app.settings_buffer_secs,
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
                &mut app.settings_save_delay_secs,
                delta,
                1,
                0,
                10,
                app.config.recorder.save_delay_secs,
            );
            iced::Task::none()
        }
        Message::ClipSavedNotificationsToggled(value) => {
            app.settings_clip_saved_notifications = value;
            iced::Task::none()
        }
        Message::ClipNamingTemplateChanged(value) => {
            app.settings_clip_naming_template = value;
            iced::Task::none()
        }
        Message::ManualClipEnabledToggled(value) => {
            app.settings_manual_clip_enabled = value;
            iced::Task::none()
        }
        Message::BeginHotkeyCapture => {
            app.settings_hotkey_capture_active = true;
            app.clear_settings_feedback();
            iced::Task::none()
        }
        Message::CancelHotkeyCapture => {
            app.settings_hotkey_capture_active = false;
            iced::Task::none()
        }
        Message::HotkeyCaptureEvent(event) => {
            match crate::hotkey::capture_binding(&event) {
                crate::hotkey::BindingCapture::Captured(binding) => {
                    app.settings_manual_clip_hotkey = binding;
                    app.settings_hotkey_capture_active = false;
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
                &mut app.settings_manual_clip_duration_secs,
                delta,
                5,
                5,
                300,
                app.config.manual_clip.duration_secs,
            );
            iced::Task::none()
        }
        Message::StorageTieringEnabledToggled(value) => {
            app.settings_storage_tiering_enabled = value;
            iced::Task::none()
        }
        Message::StorageTierDirectoryChanged(value) => {
            app.settings_storage_tier_directory = value;
            app.clear_settings_feedback();
            iced::Task::none()
        }
        Message::PickStorageTierDirectory => {
            let current_dir = app.settings_storage_tier_directory.clone();
            iced::Task::perform(async move { pick_directory(current_dir).await }, |result| {
                AppMessage::Settings(Message::StorageTierDirectoryPicked(result))
            })
        }
        Message::StorageTierDirectoryPicked(result) => {
            match result {
                Ok(Some(path)) => {
                    app.settings_storage_tier_directory = path;
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
                &mut app.settings_storage_min_age_days,
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
                &mut app.settings_storage_max_score,
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
            app.settings_copyparty_enabled = value;
            iced::Task::none()
        }
        Message::CopypartyUploadUrlChanged(value) => {
            app.settings_copyparty_upload_url = value;
            iced::Task::none()
        }
        Message::CopypartyPublicBaseUrlChanged(value) => {
            app.settings_copyparty_public_base_url = value;
            iced::Task::none()
        }
        Message::CopypartyUsernameChanged(value) => {
            app.settings_copyparty_username = value;
            iced::Task::none()
        }
        Message::CopypartyPasswordChanged(value) => {
            app.settings_copyparty_password_input = value;
            iced::Task::none()
        }
        Message::ClearCopypartyPassword => {
            match app.secure_store.delete(SecretKey::CopypartyPassword) {
                Ok(()) => {
                    app.settings_copyparty_password_present = false;
                    app.settings_copyparty_password_input.clear();
                    app.set_settings_feedback("Cleared the stored Copyparty password.", false);
                }
                Err(error) => app.set_settings_feedback(error, false),
            }
            iced::Task::none()
        }
        Message::YouTubeEnabledToggled(value) => {
            app.settings_youtube_enabled = value;
            iced::Task::none()
        }
        Message::YouTubeClientIdChanged(value) => {
            app.settings_youtube_client_id = value;
            iced::Task::none()
        }
        Message::YouTubeClientSecretChanged(value) => {
            app.settings_youtube_client_secret_input = value;
            iced::Task::none()
        }
        Message::YouTubePrivacyStatusSelected(value) => {
            app.settings_youtube_privacy_status = value;
            iced::Task::none()
        }
        Message::ConnectYouTube => app.start_youtube_oauth(),
        Message::DisconnectYouTube => {
            let refresh_result = app.secure_store.delete(SecretKey::YoutubeRefreshToken);
            let secret_result = app.secure_store.delete(SecretKey::YoutubeClientSecret);
            match (refresh_result, secret_result) {
                (Ok(()), Ok(())) => {
                    app.settings_youtube_refresh_token_present = false;
                    app.settings_youtube_client_secret_present = false;
                    app.settings_youtube_client_secret_input.clear();
                    app.set_settings_feedback("Disconnected the stored YouTube account.", false);
                }
                (Err(error), _) | (_, Err(error)) => app.set_settings_feedback(error, false),
            }
            iced::Task::none()
        }
        Message::DiscordWebhookEnabledToggled(value) => {
            app.settings_discord_enabled = value;
            iced::Task::none()
        }
        Message::DiscordMinScoreStepped(delta) => {
            step_numeric_setting(
                &mut app.settings_discord_min_score,
                delta,
                5,
                1,
                10_000,
                app.config.discord_webhook.min_score,
            );
            iced::Task::none()
        }
        Message::DiscordIncludeThumbnailToggled(value) => {
            app.settings_discord_include_thumbnail = value;
            iced::Task::none()
        }
        Message::DiscordWebhookUrlChanged(value) => {
            app.settings_discord_webhook_input = value;
            iced::Task::none()
        }
        Message::ClearDiscordWebhook => {
            match app.secure_store.delete(SecretKey::DiscordWebhookUrl) {
                Ok(()) => {
                    app.settings_discord_webhook_present = false;
                    app.settings_discord_webhook_input.clear();
                    app.set_settings_feedback("Cleared the stored Discord webhook URL.", false);
                }
                Err(error) => app.set_settings_feedback(error, false),
            }
            iced::Task::none()
        }
        Message::AddAudioSource(kind) => {
            let selected = match kind {
                DiscoveredAudioKind::Device => app.settings_selected_device_audio_source.clone(),
                DiscoveredAudioKind::Application => {
                    app.settings_selected_application_audio_source.clone()
                }
            };
            let Some(selected) = selected else {
                return iced::Task::none();
            };

            let discovered = app
                .settings_discovered_audio_sources
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

            match apply_discovered_audio_source(&mut app.settings_audio_sources, &discovered) {
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
            if app.settings_audio_sources.len() > 1 {
                app.settings_audio_sources.remove(index);
            } else if let Some(audio_source) = app.settings_audio_sources.get_mut(0) {
                *audio_source = AudioSourceDraft::default();
            }
            sync_selected_audio_sources(app);
            iced::Task::none()
        }
        Message::MoveAudioSourceUp(index) => {
            if index > 0 && index < app.settings_audio_sources.len() {
                app.settings_audio_sources.swap(index, index - 1);
            }
            iced::Task::none()
        }
        Message::MoveAudioSourceDown(index) => {
            if index + 1 < app.settings_audio_sources.len() {
                app.settings_audio_sources.swap(index, index + 1);
            }
            iced::Task::none()
        }
        Message::AudioSourceLabelChanged(index, value) => {
            if let Some(audio_source) = app.settings_audio_sources.get_mut(index) {
                audio_source.label = value;
            }
            iced::Task::none()
        }
        Message::AudioSourceValueChanged(index, value) => {
            if let Some(audio_source) = app.settings_audio_sources.get_mut(index) {
                audio_source.source = value;
            }
            sync_selected_audio_sources(app);
            iced::Task::none()
        }
        Message::AudioSourceGainStepped(index, delta) => {
            if let Some(audio_source) = app.settings_audio_sources.get_mut(index) {
                audio_source.gain_db =
                    (audio_source.gain_db + (delta as f32 * 0.5)).clamp(-60.0, 12.0);
            }
            iced::Task::none()
        }
        Message::AudioSourceMutedInPremixToggled(index, value) => {
            if let Some(audio_source) = app.settings_audio_sources.get_mut(index) {
                audio_source.muted_in_premix = value;
            }
            iced::Task::none()
        }
        Message::AudioSourceIncludedInPremixToggled(index, value) => {
            if let Some(audio_source) = app.settings_audio_sources.get_mut(index) {
                audio_source.included_in_premix = value;
            }
            iced::Task::none()
        }
        Message::SelectAvailableAudioSource(kind, option) => {
            match kind {
                DiscoveredAudioKind::Device => {
                    app.settings_selected_device_audio_source = Some(option);
                }
                DiscoveredAudioKind::Application => {
                    app.settings_selected_application_audio_source = Some(option);
                }
            }
            iced::Task::none()
        }
        Message::AudioSourcesDiscovered(result) => {
            app.settings_audio_discovery_running = false;

            match result {
                Ok((discovered, warning)) => {
                    app.settings_discovered_audio_sources = discovered;
                    app.settings_audio_discovery_error = warning;
                    sync_selected_audio_sources(app);
                }
                Err(error) => {
                    app.settings_audio_discovery_error = Some(error);
                }
            }

            iced::Task::none()
        }
        Message::BackupDatabase => run_database_action(
            app,
            "database backup",
            backup_destination(app.settings_save_dir.as_str(), "sqlite3"),
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
            backup_destination(app.settings_save_dir.as_str(), "json"),
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
            backup_destination(app.settings_save_dir.as_str(), "csv"),
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
        Message::CheckForUpdates => iced::Task::done(AppMessage::CheckForUpdates { manual: true }),
        Message::RefreshRollbackCatalog => iced::Task::done(AppMessage::RefreshRollbackCatalog),
        Message::RunSelectedUpdateAction => iced::Task::done(AppMessage::RunSelectedUpdateAction),
        Message::DownloadSelectedRollbackVersion => {
            iced::Task::done(AppMessage::DownloadSelectedRollbackVersion)
        }
        Message::RollbackToPreviousInstalledVersion => {
            iced::Task::done(AppMessage::RollbackToPreviousInstalledVersion)
        }
        Message::ViewUpdateDetails => iced::Task::done(AppMessage::ShowUpdateDetails),
        Message::Save => {
            app.settings_hotkey_capture_active = false;
            if let Err(error) =
                crate::clip_naming::validate_template(app.settings_clip_naming_template.as_str())
            {
                app.set_settings_feedback(error, false);
                return iced::Task::none();
            }

            app.config.service_id = app.settings_service_id.clone();
            app.config.launch_at_login.enabled = app.settings_launch_at_login;
            app.config.auto_start_monitoring = app.settings_auto_start_monitoring;
            app.config.start_minimized = app.settings_start_minimized;
            app.config.minimize_to_tray = app.settings_minimize_to_tray;
            app.config.updates.auto_check = app.settings_update_auto_check;
            app.config.updates.channel = app.settings_update_channel;
            app.config.updates.install_behavior = app.settings_update_install_behavior;
            app.config.clip_naming_template = non_empty_or_default(
                app.settings_clip_naming_template.as_str(),
                "{timestamp}_{source}_{character}_{rule}_{score}",
            );
            app.config.manual_clip.enabled = app.settings_manual_clip_enabled;
            app.config.manual_clip.hotkey =
                non_empty_or_default(app.settings_manual_clip_hotkey.as_str(), "Ctrl+Shift+F8");
            app.config.manual_clip.duration_secs = app
                .settings_manual_clip_duration_secs
                .parse()
                .unwrap_or(app.config.manual_clip.duration_secs);
            app.config.manual_clip.normalize();
            app.settings_manual_clip_hotkey = app.config.manual_clip.hotkey.clone();
            app.settings_manual_clip_duration_secs =
                app.config.manual_clip.duration_secs.to_string();
            app.config.storage_tiering.enabled = app.settings_storage_tiering_enabled;
            app.config.storage_tiering.tier_directory =
                PathBuf::from(app.settings_storage_tier_directory.trim());
            app.config.storage_tiering.min_age_days = app
                .settings_storage_min_age_days
                .parse()
                .unwrap_or(app.config.storage_tiering.min_age_days);
            app.config.storage_tiering.max_score = app
                .settings_storage_max_score
                .parse()
                .unwrap_or(app.config.storage_tiering.max_score);
            app.config.uploads.copyparty.enabled = app.settings_copyparty_enabled;
            app.config.uploads.copyparty.upload_url = app.settings_copyparty_upload_url.clone();
            app.config.uploads.copyparty.public_base_url =
                app.settings_copyparty_public_base_url.clone();
            app.config.uploads.copyparty.username = app.settings_copyparty_username.clone();
            app.config.uploads.youtube.enabled = app.settings_youtube_enabled;
            app.config.uploads.youtube.client_id = app.settings_youtube_client_id.clone();
            app.config.uploads.youtube.privacy_status = app.settings_youtube_privacy_status;
            app.config.discord_webhook.enabled = app.settings_discord_enabled;
            app.config.discord_webhook.min_score = app
                .settings_discord_min_score
                .parse()
                .unwrap_or(app.config.discord_webhook.min_score);
            app.config.discord_webhook.include_thumbnail = app.settings_discord_include_thumbnail;

            app.config.capture.backend =
                non_empty_or_default(app.settings_capture_backend.as_str(), "gsr");
            app.config.recorder.gsr_mut().capture_source =
                non_empty_or_default(app.settings_capture_source.as_str(), "planetside2");
            app.config.recorder.obs_mut().websocket_url = non_empty_or_default(
                app.settings_obs_websocket_url.as_str(),
                "ws://127.0.0.1:4455",
            );
            app.config.recorder.obs_mut().management_mode = app.settings_obs_management_mode;
            app.config.recorder.save_directory = app.settings_save_dir.clone().into();
            let default_framerate = app.config.recorder.gsr().framerate;
            app.config.recorder.gsr_mut().framerate =
                app.settings_framerate.parse().unwrap_or(default_framerate);
            app.config.recorder.gsr_mut().codec =
                non_empty_or_default(app.settings_codec.as_str(), "h264");
            app.config.recorder.gsr_mut().quality = app.settings_quality.clone();
            app.config.recorder.audio_sources = app
                .settings_audio_sources
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
                non_empty_or_default(app.settings_container.as_str(), "mkv");
            app.config.recorder.replay_buffer_secs = app
                .settings_buffer_secs
                .parse()
                .unwrap_or(app.config.recorder.replay_buffer_secs);
            app.config.recorder.save_delay_secs = app
                .settings_save_delay_secs
                .parse()
                .unwrap_or(app.config.recorder.save_delay_secs);
            app.config.recorder.clip_saved_notifications = app.settings_clip_saved_notifications;

            if !app.settings_obs_password_input.trim().is_empty() {
                if let Err(error) = app.secure_store.set(
                    SecretKey::ObsWebsocketPassword,
                    app.settings_obs_password_input.trim(),
                ) {
                    app.set_settings_feedback(error, false);
                    return iced::Task::none();
                }
                app.settings_obs_password_present = true;
                app.config.recorder.obs_mut().websocket_password =
                    Some(app.settings_obs_password_input.trim().to_string());
                app.settings_obs_password_input.clear();
            }

            app.config.normalize();
            app.event_log
                .set_retention_secs(super::super::clip_log_retention_secs(&app.config));
            app.cancel_pending_recorder_start();
            app.recorder
                .update_config(app.config.capture.clone(), app.config.recorder.clone());

            if !app.settings_copyparty_password_input.trim().is_empty() {
                if let Err(error) = app.secure_store.set(
                    SecretKey::CopypartyPassword,
                    app.settings_copyparty_password_input.trim(),
                ) {
                    app.set_settings_feedback(error, false);
                    return iced::Task::none();
                }
                app.settings_copyparty_password_present = true;
                app.settings_copyparty_password_input.clear();
            }

            if !app.settings_youtube_client_secret_input.trim().is_empty() {
                if let Err(error) = app.secure_store.set(
                    SecretKey::YoutubeClientSecret,
                    app.settings_youtube_client_secret_input.trim(),
                ) {
                    app.set_settings_feedback(error, false);
                    return iced::Task::none();
                }
                app.settings_youtube_client_secret_present = true;
                app.settings_youtube_client_secret_input.clear();
            }

            if !app.settings_discord_webhook_input.trim().is_empty() {
                if let Err(error) = app.secure_store.set(
                    SecretKey::DiscordWebhookUrl,
                    app.settings_discord_webhook_input.trim(),
                ) {
                    app.set_settings_feedback(error, false);
                    return iced::Task::none();
                }
                app.settings_discord_webhook_present = true;
                app.settings_discord_webhook_input.clear();
            }

            match app.config.save() {
                Ok(()) => {
                    app.set_settings_feedback_silent("Settings saved.", false);
                    iced::Task::batch([
                        app.configure_hotkeys(true),
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
        app.settings_audio_discovery_running = false;
        app.settings_audio_discovery_error = None;
        app.settings_discovered_audio_sources.clear();
        app.settings_selected_device_audio_source = None;
        app.settings_selected_application_audio_source = None;
        return iced::Task::none();
    }

    if app.settings_audio_discovery_running {
        return iced::Task::none();
    }

    app.settings_audio_discovery_running = true;
    app.settings_audio_discovery_error = None;
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
        .action(with_tooltip(
            styled_button("Save Settings", ButtonTone::Primary)
                .on_press(Message::Save)
                .into(),
            "Save settings to disk and refresh integrations.",
        ))
        .build();

    let status_bar = toolbar()
        .push(settings_status_badge(
            format!("Secure Store: {}", app.settings_secure_store_backend_label),
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
            if app.settings_clip_saved_notifications {
                "Toasts On"
            } else {
                "Toasts Off"
            },
            if app.settings_clip_saved_notifications {
                BadgeTone::Success
            } else {
                BadgeTone::Neutral
            },
        ))
        .push(settings_status_badge(
            if app.settings_youtube_refresh_token_present {
                "YouTube Connected"
            } else {
                "YouTube Disconnected"
            },
            if app.settings_youtube_refresh_token_present {
                BadgeTone::Success
            } else {
                BadgeTone::Neutral
            },
        ))
        .push(settings_status_badge(
            if app.settings_discord_webhook_present {
                "Discord Ready"
            } else {
                "Discord Unconfigured"
            },
            if app.settings_discord_webhook_present {
                BadgeTone::Success
            } else {
                BadgeTone::Neutral
            },
        ))
        .push(settings_status_badge(
            format!("Updates: {}", app.settings_update_channel),
            BadgeTone::Primary,
        ))
        .build();

    let mut body = column![
        settings_overview_cards(app),
        runtime_panel(app),
        capture_panel(app),
    ]
    .spacing(16);
    if !obs_audio_is_backend_owned(app) {
        body = body.push(audio_panel(app));
    }
    body = body
        .push(clip_output_panel(app))
        .push(delivery_panel(app))
        .push(update_panel(app))
        .push(maintenance_panel(app));

    column![
        header,
        status_bar,
        scrollable(container(body).width(Length::Fill)).height(Length::Fill)
    ]
    .spacing(12)
    .into()
}

fn settings_overview_cards(app: &App) -> Element<'_, Message> {
    let startup_summary = if app.settings_auto_start_monitoring {
        "Starts monitoring automatically."
    } else {
        "Launches idle until you start monitoring."
    };
    let capture_summary = if selected_capture_backend(app) == CaptureBackendPreset::Obs {
        format!(
            "OBS · {} · {}s buffer",
            app.settings_obs_management_mode,
            current_buffer_secs(app),
        )
    } else {
        format!(
            "{} fps · {}s buffer · {}",
            current_framerate(app),
            current_buffer_secs(app),
            ContainerPreset::from_value(app.settings_container.as_str())
        )
    };
    let delivery_summary = format!(
        "{} delivery integrations enabled",
        usize::from(app.settings_copyparty_enabled)
            + usize::from(app.settings_youtube_enabled)
            + usize::from(app.settings_discord_enabled)
    );

    row![
        settings_overview_card(
            "Startup",
            startup_summary,
            vec![
                settings_status_badge(
                    if app.settings_launch_at_login {
                        "Launch at login"
                    } else {
                        "Manual launch"
                    },
                    if app.settings_launch_at_login {
                        BadgeTone::Success
                    } else {
                        BadgeTone::Neutral
                    },
                ),
                settings_status_badge(
                    if app.settings_start_minimized {
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
                        obs_management_mode_label(app.settings_obs_management_mode).to_string()
                    } else {
                        CodecPreset::from_value(app.settings_codec.as_str()).to_string()
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
                    if app.settings_storage_tiering_enabled {
                        "Tiering enabled"
                    } else {
                        "Tiering off"
                    },
                    if app.settings_storage_tiering_enabled {
                        BadgeTone::Warning
                    } else {
                        BadgeTone::Neutral
                    },
                ),
                settings_status_badge(
                    if app.settings_youtube_refresh_token_present {
                        "OAuth ready"
                    } else {
                        "OAuth missing"
                    },
                    if app.settings_youtube_refresh_token_present {
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
                    app.settings_launch_at_login,
                    Message::LaunchAtLoginToggled,
                ),
                settings_toggle_row(
                    "Auto-Start Monitoring",
                    "Start monitoring immediately after launch.",
                    app.settings_auto_start_monitoring,
                    Message::AutoStartMonitoringToggled,
                ),
                settings_toggle_row(
                    "Start Minimized",
                    "Start in the background instead of the main window.",
                    app.settings_start_minimized,
                    Message::StartMinimizedToggled,
                ),
                settings_toggle_row(
                    "Minimize to Tray",
                    "Closing the window keeps the app in the tray.",
                    app.settings_minimize_to_tray,
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
                &app.settings_service_id,
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
                app.settings_capture_source.as_str(),
            )),
            Message::CaptureSourcePresetSelected,
        ),
        text("Automatic: X11 binds the PS2 window; Wayland uses the desktop portal.")
            .size(12)
            .into(),
        settings_text_field_with_button(
            "Save Directory",
            "Directory where saved clips are written.",
            &app.settings_save_dir,
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
            Some(CodecPreset::from_value(app.settings_codec.as_str())),
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
            Some(ContainerPreset::from_value(app.settings_container.as_str())),
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

    if CaptureSourcePreset::from_value(app.settings_capture_source.as_str())
        == CaptureSourcePreset::Custom
    {
        video_rows.push(settings_text_field(
            "Custom Capture Source",
            "Manual source string passed to gpu-screen-recorder.",
            &app.settings_capture_source,
            Message::CaptureSourceChanged,
        ));
    }

    if CodecPreset::from_value(app.settings_codec.as_str()) == CodecPreset::Custom {
        video_rows.push(settings_text_field(
            "Custom Codec",
            "Manual codec name passed to gpu-screen-recorder.",
            &app.settings_codec,
            Message::CodecChanged,
        ));
    }

    if ContainerPreset::from_value(app.settings_container.as_str()) == ContainerPreset::Custom {
        video_rows.push(settings_text_field(
            "Custom Container",
            "Manual container format passed to gpu-screen-recorder.",
            &app.settings_container,
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
            &app.settings_obs_websocket_url,
            Message::ObsWebsocketUrlChanged,
        ),
        settings_text_field(
            if app.settings_obs_password_present {
                "OBS Password (stored)"
            } else {
                "OBS Password"
            },
            "Paste to replace the stored OBS websocket password. Leave blank to keep the current credential.",
            &app.settings_obs_password_input,
            Message::ObsPasswordChanged,
        ),
        row![
            settings_status_badge(
                if app.settings_obs_password_present {
                    "Password stored"
                } else {
                    "No password stored"
                },
                if app.settings_obs_password_present {
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
            Some(app.settings_obs_management_mode),
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

    match app.settings_obs_management_mode {
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
                    &app.settings_save_dir,
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
                    Some(ObsContainerPreset::from_value(app.settings_container.as_str())),
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
        .settings_selected_device_audio_source
        .as_ref()
        .filter(|selected| available_device_sources.contains(*selected))
        .cloned();
    let selected_application_source = app
        .settings_selected_application_audio_source
        .as_ref()
        .filter(|selected| available_application_sources.contains(*selected))
        .cloned();
    let can_add_selected_device_source = selected_device_source.is_some();
    let can_add_selected_application_source = selected_application_source.is_some();
    let device_source_placeholder = if app.settings_audio_discovery_running {
        "Loading available audio sources..."
    } else if !available_device_sources.is_empty() {
        "Choose a detected audio device"
    } else if app.settings_discovered_audio_sources.is_empty() {
        "No audio sources discovered yet"
    } else {
        "All detected device sources are already configured"
    };
    let application_source_placeholder = if app.settings_audio_discovery_running {
        "Loading detected application streams..."
    } else if !available_application_sources.is_empty() {
        "Choose a detected application stream"
    } else if app.settings_discovered_audio_sources.is_empty() {
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

    let configured_tracks: Element<'_, Message> = if app.settings_audio_sources.is_empty() {
        empty_state("No audio tracks configured.")
            .description("Add a discovered source above to build a track layout.")
            .build()
            .into()
    } else {
        let mut tracks = column![].spacing(12);
        for (index, audio_source) in app.settings_audio_sources.iter().enumerate() {
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

    if let Some(error) = &app.settings_audio_discovery_error {
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
                    app.settings_clip_saved_notifications,
                    Message::ClipSavedNotificationsToggled,
                ),
                settings_text_field(
                    "Clip Naming Template",
                    "Placeholders: {timestamp} {source} {character} {rule} {profile} {server} {continent} {base} {score} {duration}.",
                    &app.settings_clip_naming_template,
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
                    app.settings_manual_clip_enabled,
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
            app.settings_youtube_enabled,
            Message::YouTubeEnabledToggled,
        ))
        .push(settings_text_field(
            "YouTube OAuth Client ID",
            "Google OAuth client ID (Desktop App client recommended).",
            &app.settings_youtube_client_id,
            Message::YouTubeClientIdChanged,
        ))
        .push(settings_text_field(
            if app.settings_youtube_client_secret_present {
                "YouTube OAuth Client Secret (stored)"
            } else {
                "YouTube OAuth Client Secret"
            },
            "Required for confidential OAuth clients. Paste to replace.",
            &app.settings_youtube_client_secret_input,
            Message::YouTubeClientSecretChanged,
        ));

    if !app.settings_youtube_client_secret_present
        && app.settings_youtube_client_secret_input.trim().is_empty()
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
            Some(app.settings_youtube_privacy_status),
            Message::YouTubePrivacyStatusSelected,
        ))
        .push(
            row![
                settings_status_badge(
                    if app.settings_youtube_refresh_token_present {
                        "Account connected"
                    } else {
                        "No account connected"
                    },
                    if app.settings_youtube_refresh_token_present {
                        BadgeTone::Success
                    } else {
                        BadgeTone::Neutral
                    },
                ),
                with_tooltip(
                    {
                        let button = styled_button("Connect YouTube", ButtonTone::Secondary);
                        if app.settings_youtube_oauth_in_flight {
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

    panel("Delivery & Storage")
        .push(settings_section_block(
            "Storage Tiering",
            "Move older, low-score clips to archive storage in the background.",
            vec![
                settings_toggle_row(
                    "Enable Storage Tiering",
                    "Archive clips that exceed the age and score thresholds.",
                    app.settings_storage_tiering_enabled,
                    Message::StorageTieringEnabledToggled,
                ),
                settings_text_field_with_button(
                    "Archive Directory",
                    "Directory used for lower-cost clip storage.",
                    &app.settings_storage_tier_directory,
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
            ],
        ))
        .push(settings_section_block(
            "Copyparty",
            "Secrets are stored in the secure credential backend, not config.toml.",
            vec![
                settings_toggle_row(
                    "Enable Copyparty Uploads",
                    "Allow per-clip Copyparty uploads from the Clips tab.",
                    app.settings_copyparty_enabled,
                    Message::CopypartyEnabledToggled,
                ),
                settings_text_field(
                    "Copyparty Upload URL",
                    "Upload URL, e.g. `https://clips.example.com/up/`.",
                    &app.settings_copyparty_upload_url,
                    Message::CopypartyUploadUrlChanged,
                ),
                settings_text_field(
                    "Copyparty Public Base URL",
                    "Optional public base URL when the server returns a relative path.",
                    &app.settings_copyparty_public_base_url,
                    Message::CopypartyPublicBaseUrlChanged,
                ),
                settings_text_field(
                    "Copyparty Username",
                    "Optional username for basic auth or `--usernames`.",
                    &app.settings_copyparty_username,
                    Message::CopypartyUsernameChanged,
                ),
                settings_text_field(
                    if app.settings_copyparty_password_present {
                        "Copyparty Password (stored)"
                    } else {
                        "Copyparty Password"
                    },
                    "Paste to replace the stored password. Blank keeps the current one.",
                    &app.settings_copyparty_password_input,
                    Message::CopypartyPasswordChanged,
                ),
                row![
                    settings_status_badge(
                        if app.settings_copyparty_password_present {
                            "Password stored"
                        } else {
                            "No password stored"
                        },
                        if app.settings_copyparty_password_present {
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
            ],
        ))
        .push(youtube_section)
        .push(settings_section_block(
            "Discord Webhook",
            "Webhook notifications for high-value clips after they save.",
            vec![
                settings_toggle_row(
                    "Enable Discord Webhook",
                    "Post qualifying clips to a stored Discord webhook.",
                    app.settings_discord_enabled,
                    Message::DiscordWebhookEnabledToggled,
                ),
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
                    app.settings_discord_include_thumbnail,
                    Message::DiscordIncludeThumbnailToggled,
                ),
                settings_text_field(
                    if app.settings_discord_webhook_present {
                        "Discord Webhook URL (stored)"
                    } else {
                        "Discord Webhook URL"
                    },
                    "Paste to store or replace. Blank keeps the current one.",
                    &app.settings_discord_webhook_input,
                    Message::DiscordWebhookUrlChanged,
                ),
                row![
                    settings_status_badge(
                        if app.settings_discord_webhook_present {
                            "Webhook stored"
                        } else {
                            "No webhook stored"
                        },
                        if app.settings_discord_webhook_present {
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
            ],
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
        .settings_selected_rollback_release
        .as_ref()
        .map(|release| &release.signature)
        .or_else(|| {
            app.update_state
                .latest_release
                .as_ref()
                .map(|release| &release.signature)
        })
        .or_else(|| {
            app.update_state
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
                if app.update_state.latest_release.is_some()
                    || app.update_state.prepared_update.is_some()
                    || app.settings_selected_rollback_release.is_some()
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
        .update_state
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
        .update_state
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
            app.update_state.last_checked_at.map(|checked_at| {
                format!(
                    "No newer release found. Last checked {}.",
                    super::clips::format_timestamp(checked_at)
                )
            })
        })
        .unwrap_or_else(|| "No update check has completed yet.".into());
    let phase_summary = match app.update_state.phase {
        UpdatePhase::Checking
        | UpdatePhase::Downloading
        | UpdatePhase::Verifying
        | UpdatePhase::Applying => app
            .update_state
            .progress
            .as_ref()
            .map(|progress| format!("{}: {}", app.update_state.phase.label(), progress.detail))
            .unwrap_or_else(|| app.update_state.phase.label().into()),
        UpdatePhase::ReadyToInstall => app
            .update_state
            .prepared_update
            .as_ref()
            .map(|prepared| {
                let prepared_version = prepared
                    .parsed_version()
                    .unwrap_or_else(|| app.update_state.current_version.clone());
                format!(
                    "Ready to {}: {}",
                    if prepared_version < app.update_state.current_version {
                        "roll back"
                    } else {
                        "install"
                    },
                    prepared.version
                )
            })
            .unwrap_or_else(|| app.update_state.phase.label().into()),
        UpdatePhase::Failed => app
            .update_state
            .last_error
            .as_ref()
            .map(|error| format!("{} issue: {}", error.kind.label(), error.detail))
            .unwrap_or_else(|| "Updater failed.".into()),
        UpdatePhase::Idle => "Updater idle.".into(),
    };
    let reminder_summary = app
        .update_state
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
        .update_state
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
        .update_state
        .previous_installed_version
        .as_ref()
        .map(|version| format!("Previously installed version: {version}"))
        .unwrap_or_else(|| "Previously installed version: not recorded yet".into());
    let selected_rollback_summary = app
        .settings_selected_rollback_release
        .as_ref()
        .map(|release| format!("Selected rollback target: {}.", release.version))
        .unwrap_or_else(|| "Selected rollback target: none".into());
    let rollback_catalog_summary = if app.update_state.rollback_catalog_loading {
        "Rollback version list: loading…".into()
    } else if app.update_state.rollback_candidates.is_empty() {
        "Rollback version list: not loaded yet or no compatible older versions were found.".into()
    } else {
        format!(
            "Rollback version list: {} compatible older release(s) available.",
            app.update_state.rollback_candidates.len()
        )
    };

    let mut available_release_rows = vec![
        text(phase_summary).size(12).into(),
        text(latest_release_summary).size(12).into(),
        text(
            app.update_state
                .latest_release
                .as_ref()
                .map(|release| {
                    super::super::release_policy_summary(
                        release,
                        &app.update_state.current_version,
                        app.update_state.system_update_plan.as_ref(),
                    )
                })
                .unwrap_or_else(|| {
                    app.update_state
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
            app.settings_update_install_behavior.description()
        ))
        .size(12)
        .into(),
    ];
    if let Some(plan) = app.update_state.system_update_plan.as_ref() {
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
    if app.update_state.latest_release.is_some() || app.update_state.prepared_update.is_some() {
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
                    app.update_state.install_channel.label()
                ))
                .size(12)
                .into(),
                settings_toggle_row(
                    "Automatic Update Checks",
                    "Check GitHub Releases in the background and show an update banner when a newer version is available.",
                    app.settings_update_auto_check,
                    Message::UpdateAutoCheckToggled,
                ),
                settings_pick_list_field(
                    "Release Channel",
                    "Choose which release channel to check. Stable ignores GitHub prereleases. Beta includes them.",
                    &[UpdateChannel::Stable, UpdateChannel::Beta][..],
                    Some(app.settings_update_channel),
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
                    Some(app.settings_update_install_behavior),
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
                    app.update_state.rollback_candidates.as_slice(),
                    app.settings_selected_rollback_release.clone(),
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
                            if app.update_state.previous_installed_version.is_some() {
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
                            if app.settings_selected_rollback_release.is_some()
                                && !matches!(
                                    app.update_state.phase,
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
                    app.settings_save_dir
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

    match crate::clip_naming::preview_template(app.settings_clip_naming_template.as_str()) {
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
    app.settings_audio_sources
        .iter()
        .filter(|draft| !audio_source_draft_is_blank(draft))
        .count()
}

fn hotkey_capture_field(app: &App) -> Element<'_, Message> {
    let description = "Click and press a key combination. X11 uses the direct backend; Wayland uses the desktop portal.";
    let binding_label = if app.settings_hotkey_capture_active {
        "Press the desired key combination..."
    } else if app.settings_manual_clip_hotkey.trim().is_empty() {
        "Click to record a hotkey"
    } else {
        app.settings_manual_clip_hotkey.as_str()
    };
    let status = if app.settings_hotkey_capture_active {
        "Listening — click again to cancel."
    } else {
        "Click and press the combination you want."
    };
    let on_press = if app.settings_hotkey_capture_active {
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
    app.settings_discovered_audio_sources
        .iter()
        .filter(|audio_source| audio_source.kind == kind)
        .filter(|audio_source| {
            !app.settings_audio_sources.iter().any(|configured| {
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
    app.settings_selected_device_audio_source = app
        .settings_selected_device_audio_source
        .as_ref()
        .filter(|selected| available_devices.contains(*selected))
        .cloned()
        .or_else(|| available_devices.into_iter().next());

    let available_applications =
        available_audio_source_options(app, DiscoveredAudioKind::Application);
    app.settings_selected_application_audio_source = app
        .settings_selected_application_audio_source
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
    parse_or_default(&app.settings_framerate, app.config.recorder.gsr().framerate)
}

fn current_quality(app: &App) -> u32 {
    parse_or_default(&app.settings_quality, 40_000)
}

fn current_buffer_secs(app: &App) -> u32 {
    parse_or_default(
        &app.settings_buffer_secs,
        app.config.recorder.replay_buffer_secs,
    )
}

fn current_save_delay(app: &App) -> u32 {
    parse_or_default(
        &app.settings_save_delay_secs,
        app.config.recorder.save_delay_secs,
    )
}

fn current_manual_clip_duration(app: &App) -> u32 {
    parse_or_default(
        &app.settings_manual_clip_duration_secs,
        app.config.manual_clip.duration_secs,
    )
}

fn current_storage_min_age_days(app: &App) -> u32 {
    parse_or_default(
        &app.settings_storage_min_age_days,
        app.config.storage_tiering.min_age_days,
    )
}

fn current_storage_max_score(app: &App) -> u32 {
    parse_or_default(
        &app.settings_storage_max_score,
        app.config.storage_tiering.max_score,
    )
}

fn current_discord_min_score(app: &App) -> u32 {
    parse_or_default(
        &app.settings_discord_min_score,
        app.config.discord_webhook.min_score,
    )
}

fn selected_capture_backend(app: &App) -> CaptureBackendPreset {
    CaptureBackendPreset::from_value(app.settings_capture_backend.as_str())
}

fn obs_audio_is_backend_owned(app: &App) -> bool {
    selected_capture_backend(app) == CaptureBackendPreset::Obs
        && app.settings_obs_management_mode != ObsManagementMode::FullManagement
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

#[cfg(not(target_os = "windows"))]
fn pick_directory_impl(current_dir: String) -> Result<Option<String>, String> {
    let dialog_attempts = [
        ("zenity", build_zenity_args(current_dir.as_str())),
        ("qarma", build_zenity_args(current_dir.as_str())),
        ("yad", build_zenity_args(current_dir.as_str())),
        ("kdialog", build_kdialog_args(current_dir.as_str())),
    ];

    for (program, args) in dialog_attempts {
        let output = match std::process::Command::new(program).args(&args).output() {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("{program} failed to start: {error}")),
        };

        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return if path.is_empty() {
                Ok(None)
            } else {
                Ok(Some(path))
            };
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if is_dialog_cancelled(program, output.status.code()) {
            return Ok(None);
        }

        let detail = if stderr.is_empty() {
            format!("exit status {}", output.status)
        } else {
            stderr
        };
        return Err(format!("{program} failed: {detail}"));
    }

    Err(
        "No supported directory picker found. Install `zenity`, `qarma`, `yad`, or `kdialog`."
            .into(),
    )
}

#[cfg(target_os = "windows")]
fn pick_directory_impl(current_dir: String) -> Result<Option<String>, String> {
    let initial_directory = sanitize_windows_dialog_start_dir(current_dir.as_str());
    std::thread::spawn(move || pick_directory_with_windows_shell_dialog(initial_directory))
        .join()
        .map_err(|_| "Windows folder picker thread panicked".to_string())?
}

#[cfg(not(target_os = "windows"))]
fn pick_toml_file_impl(current_path: String, title: String) -> Result<Option<String>, String> {
    let result = run_path_dialog(
        [
            (
                "zenity",
                build_zenity_open_toml_file_args(current_path.as_str(), title.as_str()),
            ),
            (
                "qarma",
                build_zenity_open_toml_file_args(current_path.as_str(), title.as_str()),
            ),
            (
                "yad",
                build_zenity_open_toml_file_args(current_path.as_str(), title.as_str()),
            ),
            (
                "kdialog",
                build_kdialog_open_toml_file_args(current_path.as_str(), title.as_str()),
            ),
        ],
        "No supported file picker found. Install `zenity`, `qarma`, `yad`, or `kdialog`.",
    )?;

    validate_toml_file_selection(result)
}

#[cfg(target_os = "windows")]
fn pick_toml_file_impl(current_path: String, title: String) -> Result<Option<String>, String> {
    let (initial_directory, _) = sanitize_windows_file_dialog_target(current_path.as_str());
    let result = std::thread::spawn(move || {
        pick_toml_file_with_windows_shell_dialog(initial_directory, title)
    })
    .join()
    .map_err(|_| "Windows file picker thread panicked".to_string())??;

    validate_toml_file_selection(result)
}

#[cfg(not(target_os = "windows"))]
fn save_file_impl(initial_path: String, title: String) -> Result<Option<String>, String> {
    run_path_dialog(
        [
            (
                "zenity",
                build_zenity_save_file_args(initial_path.as_str(), title.as_str()),
            ),
            (
                "qarma",
                build_zenity_save_file_args(initial_path.as_str(), title.as_str()),
            ),
            (
                "yad",
                build_zenity_save_file_args(initial_path.as_str(), title.as_str()),
            ),
            (
                "kdialog",
                build_kdialog_save_file_args(initial_path.as_str(), title.as_str()),
            ),
        ],
        "No supported save-file picker found. Install `zenity`, `qarma`, `yad`, or `kdialog`.",
    )
}

#[cfg(target_os = "windows")]
fn save_file_impl(initial_path: String, title: String) -> Result<Option<String>, String> {
    let (initial_directory, suggested_name) =
        sanitize_windows_file_dialog_target(initial_path.as_str());
    std::thread::spawn(move || {
        save_file_with_windows_shell_dialog(initial_directory, suggested_name, title)
    })
    .join()
    .map_err(|_| "Windows save-file picker thread panicked".to_string())?
}

#[cfg(not(target_os = "windows"))]
fn build_zenity_args(current_dir: &str) -> Vec<String> {
    let mut args = vec!["--file-selection".into(), "--directory".into()];
    if let Some(initial) = sanitize_dialog_start_dir(current_dir) {
        args.push("--filename".into());
        args.push(initial);
    }
    args
}

#[cfg(not(target_os = "windows"))]
fn build_kdialog_args(current_dir: &str) -> Vec<String> {
    let mut args = vec!["--getexistingdirectory".into()];
    if let Some(initial) = sanitize_dialog_start_dir(current_dir) {
        args.push(initial);
    }
    args
}

#[cfg(not(target_os = "windows"))]
fn build_zenity_open_file_args(current_path: &str, title: &str) -> Vec<String> {
    let mut args = vec!["--file-selection".into(), "--title".into(), title.into()];
    if let Some(initial) = sanitize_dialog_file_path(current_path, false) {
        args.push("--filename".into());
        args.push(initial);
    }
    args
}

#[cfg(not(target_os = "windows"))]
fn build_zenity_open_toml_file_args(current_path: &str, title: &str) -> Vec<String> {
    let mut args = build_zenity_open_file_args(current_path, title);
    args.push("--file-filter".into());
    args.push("TOML files | *.toml".into());
    args
}

#[cfg(not(target_os = "windows"))]
fn build_zenity_save_file_args(initial_path: &str, title: &str) -> Vec<String> {
    let mut args = vec![
        "--file-selection".into(),
        "--save".into(),
        "--confirm-overwrite".into(),
        "--title".into(),
        title.into(),
    ];
    if let Some(initial) = sanitize_dialog_file_path(initial_path, true) {
        args.push("--filename".into());
        args.push(initial);
    }
    args
}

#[cfg(not(target_os = "windows"))]
fn build_kdialog_open_file_args(current_path: &str, title: &str) -> Vec<String> {
    let mut args = vec!["--title".into(), title.into(), "--getopenfilename".into()];
    if let Some(initial) = sanitize_dialog_file_path(current_path, false) {
        args.push(initial);
    }
    args
}

#[cfg(not(target_os = "windows"))]
fn build_kdialog_open_toml_file_args(current_path: &str, title: &str) -> Vec<String> {
    let mut args = build_kdialog_open_file_args(current_path, title);
    args.push("TOML files (*.toml)".into());
    args
}

#[cfg(not(target_os = "windows"))]
fn build_kdialog_save_file_args(initial_path: &str, title: &str) -> Vec<String> {
    let mut args = vec!["--title".into(), title.into(), "--getsavefilename".into()];
    if let Some(initial) = sanitize_dialog_file_path(initial_path, true) {
        args.push(initial);
    }
    args
}

#[cfg(not(target_os = "windows"))]
fn run_path_dialog<const N: usize>(
    dialog_attempts: [(&str, Vec<String>); N],
    missing_dialog_error: &str,
) -> Result<Option<String>, String> {
    for (program, args) in dialog_attempts {
        let output = match std::process::Command::new(program).args(&args).output() {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("{program} failed to start: {error}")),
        };

        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return if path.is_empty() {
                Ok(None)
            } else {
                Ok(Some(path))
            };
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if is_dialog_cancelled(program, output.status.code()) {
            return Ok(None);
        }

        let detail = if stderr.is_empty() {
            format!("exit status {}", output.status)
        } else {
            stderr
        };
        return Err(format!("{program} failed: {detail}"));
    }

    Err(missing_dialog_error.into())
}

fn validate_toml_file_selection(selection: Option<String>) -> Result<Option<String>, String> {
    let Some(path) = selection else {
        return Ok(None);
    };

    if std::path::Path::new(&path)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
    {
        Ok(Some(path))
    } else {
        Err("Select a `.toml` file to import profiles.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ApplyDiscoveredAudioSource, apply_discovered_audio_source, audio_source_draft_is_blank,
        validate_toml_file_selection,
    };
    use crate::app::AudioSourceDraft;
    use crate::capture::{DiscoveredAudioKind, DiscoveredAudioSource};
    use crate::config::AudioSourceKind;

    #[test]
    fn discovered_audio_source_replaces_blank_placeholder_row() {
        let mut drafts = vec![AudioSourceDraft::default()];
        let discovered = DiscoveredAudioSource {
            kind_hint: AudioSourceKind::DefaultOutput,
            display_label: "Default output".into(),
            kind: DiscoveredAudioKind::Device,
            available: true,
        };

        let outcome = apply_discovered_audio_source(&mut drafts, &discovered);

        assert_eq!(outcome, ApplyDiscoveredAudioSource::Added);
        assert_eq!(
            drafts,
            vec![AudioSourceDraft {
                label: "Default output".into(),
                source: "default_output".into(),
                ..AudioSourceDraft::default()
            }]
        );
    }

    #[test]
    fn discovered_audio_source_does_not_duplicate_existing_source() {
        let mut drafts = vec![AudioSourceDraft {
            label: "Game audio".into(),
            source: "default_output".into(),
            ..AudioSourceDraft::default()
        }];
        let discovered = DiscoveredAudioSource {
            kind_hint: AudioSourceKind::DefaultOutput,
            display_label: "Default output".into(),
            kind: DiscoveredAudioKind::Device,
            available: true,
        };

        let outcome = apply_discovered_audio_source(&mut drafts, &discovered);

        assert_eq!(outcome, ApplyDiscoveredAudioSource::Unchanged);
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].label, "Game audio");
    }

    #[test]
    fn blank_audio_source_helper_requires_empty_label_and_source() {
        assert!(audio_source_draft_is_blank(&AudioSourceDraft::default()));
        assert!(!audio_source_draft_is_blank(&AudioSourceDraft {
            label: "Game audio".into(),
            source: String::new(),
            ..AudioSourceDraft::default()
        }));
    }

    #[test]
    fn toml_picker_validation_accepts_toml_extensions_case_insensitively() {
        assert_eq!(
            validate_toml_file_selection(Some("/tmp/profile-export.TOML".into())).unwrap(),
            Some("/tmp/profile-export.TOML".into())
        );
    }

    #[test]
    fn toml_picker_validation_rejects_non_toml_extensions() {
        let error =
            validate_toml_file_selection(Some("/tmp/profile-export.json".into())).unwrap_err();

        assert!(error.contains("`.toml`"));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn zenity_toml_picker_args_include_file_filter() {
        let args = super::build_zenity_open_toml_file_args("/tmp", "Import");

        assert!(args.iter().any(|arg| arg == "--file-filter"));
        assert!(args.iter().any(|arg| arg == "TOML files | *.toml"));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn kdialog_toml_picker_args_include_file_filter() {
        let args = super::build_kdialog_open_toml_file_args("/tmp", "Import");

        assert!(args.iter().any(|arg| arg == "TOML files (*.toml)"));
    }
}

#[cfg(not(target_os = "windows"))]
fn sanitize_dialog_start_dir(current_dir: &str) -> Option<String> {
    let trimmed = current_dir.trim();
    if trimmed.is_empty() {
        return None;
    }

    let path = std::path::Path::new(trimmed);
    if path.exists() {
        Some(with_trailing_slash(trimmed))
    } else {
        path.parent()
            .map(|parent| with_trailing_slash(parent.to_string_lossy().as_ref()))
    }
}

#[cfg(not(target_os = "windows"))]
fn sanitize_dialog_file_path(path_hint: &str, allow_nonexistent_file_name: bool) -> Option<String> {
    let trimmed = path_hint.trim();
    if trimmed.is_empty() {
        return None;
    }

    let path = std::path::Path::new(trimmed);
    if path.exists() {
        return if path.is_dir() {
            Some(with_trailing_slash(trimmed))
        } else {
            Some(trimmed.to_string())
        };
    }

    if allow_nonexistent_file_name && path.parent().is_some_and(std::path::Path::exists) {
        return Some(path.to_string_lossy().into_owned());
    }

    path.parent()
        .filter(|parent| parent.exists())
        .map(|parent| with_trailing_slash(parent.to_string_lossy().as_ref()))
}

#[cfg(target_os = "windows")]
fn sanitize_windows_dialog_start_dir(current_dir: &str) -> Option<String> {
    let trimmed = current_dir.trim();
    if trimmed.is_empty() {
        return None;
    }

    let path = std::path::Path::new(trimmed);
    if path.exists() {
        Some(trimmed.to_string())
    } else {
        path.parent()
            .map(|parent| parent.to_string_lossy().into_owned())
    }
}

#[cfg(target_os = "windows")]
fn sanitize_windows_file_dialog_target(path_hint: &str) -> (Option<String>, Option<String>) {
    let trimmed = path_hint.trim();
    if trimmed.is_empty() {
        return (None, None);
    }

    let path = std::path::Path::new(trimmed);
    if path.exists() {
        if path.is_dir() {
            return (Some(trimmed.to_string()), None);
        }

        return (
            path.parent()
                .map(|parent| parent.to_string_lossy().into_owned()),
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned()),
        );
    }

    (
        path.parent()
            .map(|parent| parent.to_string_lossy().into_owned()),
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned()),
    )
}

#[cfg(not(target_os = "windows"))]
fn with_trailing_slash(path: &str) -> String {
    let mut value = path.to_string();
    if !value.ends_with('/') {
        value.push('/');
    }
    value
}

#[cfg(not(target_os = "windows"))]
fn is_dialog_cancelled(program: &str, code: Option<i32>) -> bool {
    match program {
        "zenity" | "qarma" | "yad" | "kdialog" => code == Some(1),
        _ => false,
    }
}

#[cfg(target_os = "windows")]
fn pick_directory_with_windows_shell_dialog(
    initial_directory: Option<String>,
) -> Result<Option<String>, String> {
    use windows::Win32::Foundation::ERROR_CANCELLED;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
        CoInitializeEx, CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::UI::Shell::{
        FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog,
        IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
    };
    use windows::core::{HRESULT, HSTRING};

    struct ComApartment;

    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }

    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)
            .ok()
            .map_err(|error| format!("failed to initialize the Windows file picker: {error}"))?;
        let _com_apartment = ComApartment;

        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| format!("failed to create the Windows file picker: {error}"))?;

        dialog
            .SetTitle(&HSTRING::from("Select a folder for NaniteClip"))
            .map_err(|error| format!("failed to set the Windows file picker title: {error}"))?;

        let options = dialog
            .GetOptions()
            .map_err(|error| format!("failed to read Windows file picker options: {error}"))?;
        dialog
            .SetOptions(options | FOS_PICKFOLDERS | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM)
            .map_err(|error| format!("failed to configure the Windows file picker: {error}"))?;

        if let Some(initial_directory) = initial_directory.as_ref() {
            let shell_item: IShellItem =
                SHCreateItemFromParsingName(&HSTRING::from(initial_directory), None).map_err(
                    |error| {
                        format!("failed to resolve the initial directory for the picker: {error}")
                    },
                )?;
            dialog
                .SetFolder(&shell_item)
                .map_err(|error| format!("failed to set the initial picker directory: {error}"))?;
            dialog
                .SetDefaultFolder(&shell_item)
                .map_err(|error| format!("failed to set the default picker directory: {error}"))?;
        }

        match dialog.Show(None) {
            Ok(()) => {}
            Err(error) if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => {
                return Ok(None);
            }
            Err(error) => {
                return Err(format!("Windows folder picker failed: {error}"));
            }
        }

        let selected_item = dialog
            .GetResult()
            .map_err(|error| format!("failed to read the selected folder: {error}"))?;
        let selected_path = selected_item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|error| format!("failed to resolve the selected folder path: {error}"))?;
        let path = selected_path
            .to_string()
            .map_err(|error| format!("selected folder path was not valid UTF-16: {error}"))?;
        CoTaskMemFree(Some(selected_path.0.cast()));

        if path.trim().is_empty() {
            Ok(None)
        } else {
            Ok(Some(path))
        }
    }
}

#[cfg(target_os = "windows")]
fn pick_toml_file_with_windows_shell_dialog(
    initial_directory: Option<String>,
    title: String,
) -> Result<Option<String>, String> {
    use windows::Win32::Foundation::ERROR_CANCELLED;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
        CoInitializeEx, CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FileOpenDialog, IFileOpenDialog,
        IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
    };
    use windows::core::{HRESULT, HSTRING, PCWSTR};

    struct ComApartment;

    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }

    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)
            .ok()
            .map_err(|error| format!("failed to initialize the Windows file picker: {error}"))?;
        let _com_apartment = ComApartment;

        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| format!("failed to create the Windows file picker: {error}"))?;

        dialog
            .SetTitle(&HSTRING::from(title))
            .map_err(|error| format!("failed to set the Windows file picker title: {error}"))?;

        let filter_name = HSTRING::from("TOML files");
        let filter_spec = HSTRING::from("*.toml");
        let file_types = [COMDLG_FILTERSPEC {
            pszName: PCWSTR(filter_name.as_ptr()),
            pszSpec: PCWSTR(filter_spec.as_ptr()),
        }];
        dialog
            .SetFileTypes(&file_types)
            .map_err(|error| format!("failed to set the Windows file picker filter: {error}"))?;

        let options = dialog
            .GetOptions()
            .map_err(|error| format!("failed to read Windows file picker options: {error}"))?;
        dialog
            .SetOptions(options | FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM)
            .map_err(|error| format!("failed to configure the Windows file picker: {error}"))?;

        if let Some(initial_directory) = initial_directory.as_ref() {
            let shell_item: IShellItem =
                SHCreateItemFromParsingName(&HSTRING::from(initial_directory), None).map_err(
                    |error| {
                        format!("failed to resolve the initial directory for the picker: {error}")
                    },
                )?;
            dialog
                .SetFolder(&shell_item)
                .map_err(|error| format!("failed to set the initial picker directory: {error}"))?;
            dialog
                .SetDefaultFolder(&shell_item)
                .map_err(|error| format!("failed to set the default picker directory: {error}"))?;
        }

        match dialog.Show(None) {
            Ok(()) => {}
            Err(error) if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => {
                return Ok(None);
            }
            Err(error) => {
                return Err(format!("Windows file picker failed: {error}"));
            }
        }

        let selected_item = dialog
            .GetResult()
            .map_err(|error| format!("failed to read the selected file: {error}"))?;
        let selected_path = selected_item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|error| format!("failed to resolve the selected file path: {error}"))?;
        let path = selected_path
            .to_string()
            .map_err(|error| format!("selected file path was not valid UTF-16: {error}"))?;
        CoTaskMemFree(Some(selected_path.0.cast()));

        if path.trim().is_empty() {
            Ok(None)
        } else {
            Ok(Some(path))
        }
    }
}

#[cfg(target_os = "windows")]
fn save_file_with_windows_shell_dialog(
    initial_directory: Option<String>,
    suggested_name: Option<String>,
    title: String,
) -> Result<Option<String>, String> {
    use windows::Win32::Foundation::ERROR_CANCELLED;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
        CoInitializeEx, CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::UI::Shell::{
        FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST, FileSaveDialog,
        IFileSaveDialog, IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
    };
    use windows::core::{HRESULT, HSTRING};

    struct ComApartment;

    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }

    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)
            .ok()
            .map_err(|error| format!("failed to initialize the Windows save picker: {error}"))?;
        let _com_apartment = ComApartment;

        let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| format!("failed to create the Windows save picker: {error}"))?;

        dialog
            .SetTitle(&HSTRING::from(title))
            .map_err(|error| format!("failed to set the Windows save picker title: {error}"))?;

        let options = dialog
            .GetOptions()
            .map_err(|error| format!("failed to read Windows save picker options: {error}"))?;
        dialog
            .SetOptions(options | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM | FOS_OVERWRITEPROMPT)
            .map_err(|error| format!("failed to configure the Windows save picker: {error}"))?;

        if let Some(initial_directory) = initial_directory.as_ref() {
            let shell_item: IShellItem =
                SHCreateItemFromParsingName(&HSTRING::from(initial_directory), None).map_err(
                    |error| {
                        format!("failed to resolve the initial directory for the picker: {error}")
                    },
                )?;
            dialog
                .SetFolder(&shell_item)
                .map_err(|error| format!("failed to set the initial picker directory: {error}"))?;
            dialog
                .SetDefaultFolder(&shell_item)
                .map_err(|error| format!("failed to set the default picker directory: {error}"))?;
        }

        if let Some(suggested_name) = suggested_name.as_ref() {
            dialog
                .SetFileName(&HSTRING::from(suggested_name))
                .map_err(|error| format!("failed to set the suggested file name: {error}"))?;
        }

        dialog
            .SetDefaultExtension(&HSTRING::from("toml"))
            .map_err(|error| format!("failed to set the save picker extension: {error}"))?;

        match dialog.Show(None) {
            Ok(()) => {}
            Err(error) if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => {
                return Ok(None);
            }
            Err(error) => {
                return Err(format!("Windows save picker failed: {error}"));
            }
        }

        let selected_item = dialog
            .GetResult()
            .map_err(|error| format!("failed to read the selected save path: {error}"))?;
        let selected_path = selected_item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|error| format!("failed to resolve the selected save path: {error}"))?;
        let path = selected_path
            .to_string()
            .map_err(|error| format!("selected save path was not valid UTF-16: {error}"))?;
        CoTaskMemFree(Some(selected_path.0.cast()));

        if path.trim().is_empty() {
            Ok(None)
        } else {
            Ok(Some(path))
        }
    }
}

trait PresetValue: Sized {
    fn from_value(value: &str) -> Self;
    fn config_value(self) -> Option<&'static str>;
}

fn apply_preset_string_selection<T>(current_value: &str, preset: T) -> String
where
    T: PresetValue + PartialEq + Copy,
{
    match preset.config_value() {
        Some(value) => value.to_string(),
        None if T::from_value(current_value) == preset => current_value.to_string(),
        None => String::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureBackendPreset {
    Gsr,
    Obs,
}

impl CaptureBackendPreset {
    #[cfg(target_os = "linux")]
    const ALL: [Self; 2] = [Self::Gsr, Self::Obs];
    #[cfg(not(target_os = "linux"))]
    const ALL: [Self; 1] = [Self::Obs];

    fn all() -> &'static [Self] {
        &Self::ALL
    }
}

impl PresetValue for CaptureBackendPreset {
    fn from_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "obs" => Self::Obs,
            _ => Self::Gsr,
        }
    }

    fn config_value(self) -> Option<&'static str> {
        Some(match self {
            Self::Gsr => "gsr",
            Self::Obs => "obs",
        })
    }
}

impl std::fmt::Display for CaptureBackendPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Gsr => "gpu-screen-recorder",
            Self::Obs => "OBS Studio",
        })
    }
}

impl std::fmt::Display for ObsManagementMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(obs_management_mode_label(*self))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureSourcePreset {
    Automatic,
    Portal,
    Custom,
}

impl CaptureSourcePreset {
    const ALL: [Self; 3] = [Self::Automatic, Self::Portal, Self::Custom];
}

impl PresetValue for CaptureSourcePreset {
    fn from_value(value: &str) -> Self {
        let value = value.trim();
        if value.is_empty()
            || value.eq_ignore_ascii_case("planetside2")
            || value.eq_ignore_ascii_case("ps2")
            || value.eq_ignore_ascii_case("auto")
            || value.eq_ignore_ascii_case("screen")
        {
            Self::Automatic
        } else if value.eq_ignore_ascii_case("portal") {
            Self::Portal
        } else {
            Self::Custom
        }
    }

    fn config_value(self) -> Option<&'static str> {
        match self {
            Self::Automatic => Some("planetside2"),
            Self::Portal => Some("portal"),
            Self::Custom => None,
        }
    }
}

impl std::fmt::Display for CaptureSourcePreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Automatic => "Automatic (PlanetSide 2)",
            Self::Portal => "Portal/Desktop Picker",
            Self::Custom => "Custom",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecPreset {
    Auto,
    H264,
    Hevc,
    Av1,
    Vp8,
    Vp9,
    HevcHdr,
    Hevc10Bit,
    Av1Hdr,
    Av110Bit,
    Custom,
}

impl CodecPreset {
    const ALL: [Self; 11] = [
        Self::Auto,
        Self::H264,
        Self::Hevc,
        Self::Av1,
        Self::Vp8,
        Self::Vp9,
        Self::HevcHdr,
        Self::Hevc10Bit,
        Self::Av1Hdr,
        Self::Av110Bit,
        Self::Custom,
    ];
}

impl PresetValue for CodecPreset {
    fn from_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Self::Auto,
            "h264" => Self::H264,
            "h265" | "hevc" => Self::Hevc,
            "av1" => Self::Av1,
            "vp8" => Self::Vp8,
            "vp9" => Self::Vp9,
            "hevc_hdr" => Self::HevcHdr,
            "hevc_10bit" => Self::Hevc10Bit,
            "av1_hdr" => Self::Av1Hdr,
            "av1_10bit" => Self::Av110Bit,
            _ => Self::Custom,
        }
    }

    fn config_value(self) -> Option<&'static str> {
        match self {
            Self::Auto => Some("auto"),
            Self::H264 => Some("h264"),
            Self::Hevc => Some("hevc"),
            Self::Av1 => Some("av1"),
            Self::Vp8 => Some("vp8"),
            Self::Vp9 => Some("vp9"),
            Self::HevcHdr => Some("hevc_hdr"),
            Self::Hevc10Bit => Some("hevc_10bit"),
            Self::Av1Hdr => Some("av1_hdr"),
            Self::Av110Bit => Some("av1_10bit"),
            Self::Custom => None,
        }
    }
}

impl std::fmt::Display for CodecPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Auto => "Auto",
            Self::H264 => "H.264",
            Self::Hevc => "HEVC / H.265",
            Self::Av1 => "AV1",
            Self::Vp8 => "VP8",
            Self::Vp9 => "VP9",
            Self::HevcHdr => "HEVC HDR",
            Self::Hevc10Bit => "HEVC 10-bit",
            Self::Av1Hdr => "AV1 HDR",
            Self::Av110Bit => "AV1 10-bit",
            Self::Custom => "Custom",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerPreset {
    Mkv,
    Mp4,
    Mov,
    Custom,
}

impl ContainerPreset {
    const ALL: [Self; 4] = [Self::Mkv, Self::Mp4, Self::Mov, Self::Custom];
}

impl PresetValue for ContainerPreset {
    fn from_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "mkv" => Self::Mkv,
            "mp4" => Self::Mp4,
            "mov" => Self::Mov,
            _ => Self::Custom,
        }
    }

    fn config_value(self) -> Option<&'static str> {
        match self {
            Self::Mkv => Some("mkv"),
            Self::Mp4 => Some("mp4"),
            Self::Mov => Some("mov"),
            Self::Custom => None,
        }
    }
}

impl std::fmt::Display for ContainerPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Mkv => "MKV",
            Self::Mp4 => "MP4",
            Self::Mov => "MOV",
            Self::Custom => "Custom",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObsContainerPreset {
    Mkv,
    Mp4,
    Mov,
    Flv,
    Ts,
}

impl ObsContainerPreset {
    const ALL: [Self; 5] = [Self::Mkv, Self::Mp4, Self::Mov, Self::Flv, Self::Ts];

    fn from_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "mp4" => Self::Mp4,
            "mov" => Self::Mov,
            "flv" => Self::Flv,
            "ts" => Self::Ts,
            _ => Self::Mkv,
        }
    }

    fn config_value(self) -> &'static str {
        match self {
            Self::Mkv => "mkv",
            Self::Mp4 => "mp4",
            Self::Mov => "mov",
            Self::Flv => "flv",
            Self::Ts => "ts",
        }
    }
}

impl std::fmt::Display for ObsContainerPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Mkv => "MKV",
            Self::Mp4 => "MP4",
            Self::Mov => "MOV",
            Self::Flv => "FLV",
            Self::Ts => "TS",
        })
    }
}
