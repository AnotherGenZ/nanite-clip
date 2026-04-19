use super::*;

pub(in crate::app) fn hydrate_update_state_from_config(
    config: &mut Config,
    install_channel: update::InstallChannel,
    current_version: semver::Version,
) -> UpdateState {
    record_running_version(config, &current_version);
    config.updates.ensure_install_id();
    let mut update_state = UpdateState::new(install_channel, current_version.clone());
    update_state.system_update_plan = update::detect_system_update_plan(install_channel);
    update_state.last_checked_at = config.updates.last_check_utc;
    update_state.previous_installed_version = config
        .updates
        .installed_version_history
        .first()
        .and_then(|version| semver::Version::parse(version).ok());
    update_state.last_apply_report = config.updates.last_apply_report.clone();

    let prepared_update = config.updates.prepared_update.take().and_then(|prepared| {
        let prepared_version = prepared.parsed_version()?;
        if prepared_version == current_version || !prepared.asset_path.exists() {
            return None;
        }
        Some(prepared)
    });

    if let Some(prepared) = prepared_update {
        update_state.phase = UpdatePhase::ReadyToInstall;
        update_state.prepared_update = Some(prepared.clone());
        config.updates.prepared_update = Some(prepared);
    }

    if config
        .updates
        .remind_later_until_utc
        .is_some_and(|until| until <= Utc::now())
    {
        config.updates.remind_later_until_utc = None;
        config.updates.remind_later_version = None;
    }

    update_state
}

pub(in crate::app) fn record_running_version(
    config: &mut Config,
    current_version: &semver::Version,
) {
    let current_version = current_version.to_string();
    if config.updates.current_version.as_deref() == Some(current_version.as_str()) {
        return;
    }

    if let Some(previous_current) = config
        .updates
        .current_version
        .take()
        .filter(|version| version != &current_version)
    {
        config
            .updates
            .installed_version_history
            .retain(|version| version != &previous_current);
        config
            .updates
            .installed_version_history
            .insert(0, previous_current);
    }

    config
        .updates
        .installed_version_history
        .retain(|version| version != &current_version);
    config.updates.current_version = Some(current_version);
    config.updates.installed_version_history.truncate(10);
}

pub(in crate::app) fn classify_update_error(detail: &str, phase: UpdatePhase) -> UpdateErrorKind {
    let normalized = detail.to_ascii_lowercase();
    if matches!(phase, UpdatePhase::Applying) {
        return UpdateErrorKind::Install;
    }
    if normalized.contains("signature")
        || normalized.contains("checksum")
        || normalized.contains("manifest")
        || normalized.contains("verification")
    {
        return UpdateErrorKind::Verification;
    }
    if normalized.contains("download")
        || normalized.contains("http")
        || normalized.contains("github")
        || normalized.contains("network")
        || normalized.contains("timed out")
        || normalized.contains("connect")
    {
        return UpdateErrorKind::Network;
    }
    UpdateErrorKind::Unknown
}

pub(in crate::app) fn next_automatic_update_check_at(app: &App) -> Option<DateTime<Utc>> {
    app.config.updates.auto_check.then_some(())?;
    app.updates
        .state
        .last_checked_at
        .or(app.config.updates.last_check_utc)
        .map(|checked_at| checked_at + chrono::Duration::hours(12))
}

pub(in crate::app) fn system_update_plan_summary(plan: &update::SystemUpdatePlan) -> String {
    match &plan.command_display {
        Some(command) => format!("{}: {} Command: `{command}`.", plan.label, plan.detail),
        None => format!("{}: {}", plan.label, plan.detail),
    }
}

pub(in crate::app) fn release_banner_title(
    release: &update::AvailableRelease,
    current_version: &Version,
) -> String {
    match release.policy.availability {
        update::UpdateAvailability::DeferredByRollout => {
            format!("Update {} is rolling out", release.version)
        }
        update::UpdateAvailability::RequiresManualUpgrade => {
            format!("Update {} needs a newer base install", release.version)
        }
        update::UpdateAvailability::Available if release.policy.requires_attention() => {
            format!(
                "{} {} requires attention",
                release_action_title(&release.version, current_version),
                release.version
            )
        }
        update::UpdateAvailability::Available => format!("Update {} is available", release.version),
    }
}

pub(in crate::app) fn release_policy_summary(
    release: &update::AvailableRelease,
    current_version: &Version,
    system_update_plan: Option<&update::SystemUpdatePlan>,
) -> String {
    let mut parts = Vec::new();

    match release.policy.availability {
        update::UpdateAvailability::Available => {
            if release.policy.requires_attention() {
                parts.push(format!(
                    "Current version {} requires attention before you keep using it.",
                    current_version
                ));
            } else if release.supports_download() {
                parts.push(format!(
                    "{} {} is ready for this install.",
                    release_action_title(&release.version, current_version),
                    release.version
                ));
            }
        }
        update::UpdateAvailability::DeferredByRollout => {
            parts.push(format!(
                "{} is still in a staged rollout and this install is outside the active cohort.",
                release.version
            ));
        }
        update::UpdateAvailability::RequiresManualUpgrade => {
            let minimum_version = release
                .policy
                .minimum_version
                .as_deref()
                .unwrap_or("a newer supported version");
            parts.push(format!(
                "{} requires at least NaniteClip {} before it can be offered to this install.",
                release.version, minimum_version
            ));
        }
    }

    if release.policy.blocked_current_version {
        parts.push(format!(
            "Current version {} is blocked by this release manifest.",
            current_version
        ));
    }
    if release.policy.mandatory {
        parts.push("This release is marked mandatory.".into());
    }
    if let Some(percentage) = release.policy.rollout_percentage {
        parts.push(if release.policy.rollout_eligible {
            format!("Rollout bucket: eligible within the current {percentage}% rollout.")
        } else {
            format!("Rollout bucket: outside the current {percentage}% rollout.")
        });
    }
    if let Some(message) = release.policy.message.as_deref() {
        parts.push(message.into());
    }
    if !release.install_channel.supports_self_update() {
        if let Some(plan) = system_update_plan {
            parts.push(system_update_plan_summary(plan));
        } else {
            parts.push(release.install_channel.update_instructions().into());
        }
    }

    if parts.is_empty() {
        release.install_channel.update_instructions().into()
    } else {
        parts.join(" ")
    }
}

const DOWNLOADABLE_UPDATE_ACTIONS: [UpdatePrimaryAction; 3] = [
    UpdatePrimaryAction::DownloadUpdate,
    UpdatePrimaryAction::RemindLater,
    UpdatePrimaryAction::SkipThisVersion,
];
const REQUIRED_DOWNLOADABLE_UPDATE_ACTIONS: [UpdatePrimaryAction; 1] =
    [UpdatePrimaryAction::DownloadUpdate];
const SYSTEM_UPDATE_ACTIONS: [UpdatePrimaryAction; 3] = [
    UpdatePrimaryAction::OpenSystemUpdater,
    UpdatePrimaryAction::RemindLater,
    UpdatePrimaryAction::SkipThisVersion,
];
const REQUIRED_SYSTEM_UPDATE_ACTIONS: [UpdatePrimaryAction; 1] =
    [UpdatePrimaryAction::OpenSystemUpdater];
const NOTIFICATION_UPDATE_ACTIONS: [UpdatePrimaryAction; 2] = [
    UpdatePrimaryAction::RemindLater,
    UpdatePrimaryAction::SkipThisVersion,
];
const STAGED_UPDATE_ACTIONS: [UpdatePrimaryAction; 3] = [
    UpdatePrimaryAction::InstallAndRestart,
    UpdatePrimaryAction::InstallWhenIdle,
    UpdatePrimaryAction::InstallOnNextLaunch,
];

pub(in crate::app) fn can_launch_system_updater(app: &App) -> bool {
    app.updates
        .state
        .system_update_plan
        .as_ref()
        .is_some_and(|plan| plan.can_launch())
}

pub(in crate::app) fn update_action_options(app: &App) -> &'static [UpdatePrimaryAction] {
    if app.updates.state.prepared_update.is_some() {
        &STAGED_UPDATE_ACTIONS
    } else if let Some(release) = app.updates.state.latest_release.as_ref() {
        if release.policy.requires_attention() && release.supports_download() {
            &REQUIRED_DOWNLOADABLE_UPDATE_ACTIONS
        } else if release.policy.requires_attention() && can_launch_system_updater(app) {
            &REQUIRED_SYSTEM_UPDATE_ACTIONS
        } else if release.supports_download() {
            &DOWNLOADABLE_UPDATE_ACTIONS
        } else if can_launch_system_updater(app) {
            &SYSTEM_UPDATE_ACTIONS
        } else {
            &NOTIFICATION_UPDATE_ACTIONS
        }
    } else {
        &NOTIFICATION_UPDATE_ACTIONS
    }
}

pub(in crate::app) fn default_update_action(app: &App) -> UpdatePrimaryAction {
    if app.updates.state.prepared_update.is_some() {
        match app.settings.update_install_behavior {
            UpdateInstallBehavior::Manual => UpdatePrimaryAction::InstallAndRestart,
            UpdateInstallBehavior::WhenIdle => UpdatePrimaryAction::InstallWhenIdle,
            UpdateInstallBehavior::OnNextLaunch => UpdatePrimaryAction::InstallOnNextLaunch,
        }
    } else if app
        .updates
        .state
        .latest_release
        .as_ref()
        .is_some_and(|release| release.supports_download())
    {
        UpdatePrimaryAction::DownloadUpdate
    } else if can_launch_system_updater(app) {
        UpdatePrimaryAction::OpenSystemUpdater
    } else {
        UpdatePrimaryAction::RemindLater
    }
}

pub(in crate::app) fn selected_update_action(app: &App) -> UpdatePrimaryAction {
    let selected = app.settings.selected_update_action;
    if update_action_options(app).contains(&selected) {
        selected
    } else {
        default_update_action(app)
    }
}

pub(in crate::app) fn can_run_selected_update_action(app: &App) -> bool {
    match selected_update_action(app) {
        UpdatePrimaryAction::DownloadUpdate => {
            app.updates
                .state
                .latest_release
                .as_ref()
                .is_some_and(|release| release.supports_download())
                && !matches!(
                    app.updates.state.phase,
                    UpdatePhase::Downloading | UpdatePhase::Verifying | UpdatePhase::Applying
                )
        }
        UpdatePrimaryAction::InstallAndRestart
        | UpdatePrimaryAction::InstallWhenIdle
        | UpdatePrimaryAction::InstallOnNextLaunch => {
            app.updates.state.has_downloaded_update()
                && !matches!(
                    app.updates.state.phase,
                    UpdatePhase::Downloading | UpdatePhase::Verifying | UpdatePhase::Applying
                )
        }
        UpdatePrimaryAction::OpenSystemUpdater => {
            app.updates.state.latest_release.is_some() && can_launch_system_updater(app)
        }
        UpdatePrimaryAction::RemindLater | UpdatePrimaryAction::SkipThisVersion => {
            app.updates.state.latest_release.is_some()
        }
    }
}

pub(in crate::app) fn update_apply_report_from_helper_result(
    result: &update::helper_shared::ApplyResult,
) -> UpdateApplyReport {
    UpdateApplyReport {
        target_version: result.target_version.clone(),
        status: match result.status {
            update::helper_shared::ApplyResultStatus::Succeeded => {
                UpdateApplyReportStatus::Succeeded
            }
            update::helper_shared::ApplyResultStatus::Failed => UpdateApplyReportStatus::Failed,
        },
        detail: result.detail.clone(),
        log_path: result.log_path.clone(),
        finished_at: result.finished_at,
    }
}

pub(in crate::app) fn active_release_for_details(app: &App) -> Option<&update::AvailableRelease> {
    app.settings
        .selected_rollback_release
        .as_ref()
        .or(app.updates.state.latest_release.as_ref())
}

pub(in crate::app) fn update_details_modal(app: &App) -> Element<'_, Message> {
    let active_release = active_release_for_details(app);
    let prepared = app.updates.state.prepared_update.as_ref();
    let title = prepared
        .map(|prepared| format!("Release {}", prepared.version))
        .or_else(|| active_release.map(|release| format!("Release {}", release.version)))
        .unwrap_or_else(|| "Updater Details".into());
    let changelog = active_release
        .map(|release| release.changelog_markdown.as_str())
        .or_else(|| prepared.and_then(|prepared| prepared.changelog_markdown.as_deref()))
        .filter(|text| !text.trim().is_empty())
        .unwrap_or("No changelog text is available for this release yet.");
    let signing_key_id = active_release
        .and_then(|release| release.signature.key_id.as_deref())
        .or_else(|| prepared.and_then(|prepared| prepared.signature.key_id.as_deref()))
        .unwrap_or("Not reported");
    let signing_key_label = active_release
        .and_then(|release| release.signature.key_label.as_deref())
        .or_else(|| prepared.and_then(|prepared| prepared.signature.key_label.as_deref()))
        .unwrap_or("Not reported");
    let signing_algorithm = active_release
        .and_then(|release| release.signature.algorithm.as_deref())
        .or_else(|| prepared.and_then(|prepared| prepared.signature.algorithm.as_deref()))
        .unwrap_or("ed25519");
    let published_summary = active_release
        .and_then(|release| release.published_at)
        .or_else(|| prepared.and_then(|prepared| prepared.published_at))
        .map(tabs::clips::format_timestamp)
        .unwrap_or_else(|| "Not reported".into());
    let release_policy = active_release
        .map(|release| &release.policy)
        .or_else(|| prepared.map(|prepared| &prepared.policy));
    let minimum_version = release_policy
        .and_then(|policy| policy.minimum_version.as_deref())
        .unwrap_or("None");
    let availability_summary = release_policy
        .map(|policy| policy.availability.label())
        .unwrap_or(update::UpdateAvailability::Available.label());
    let rollout_summary = release_policy
        .and_then(|policy| policy.rollout_percentage)
        .map(|percentage| {
            if release_policy.is_some_and(|policy| policy.rollout_eligible) {
                format!("Rollout: eligible within the current {percentage}% rollout")
            } else {
                format!("Rollout: outside the current {percentage}% rollout")
            }
        })
        .unwrap_or_else(|| "Rollout: not configured".into());
    let policy_message = release_policy
        .and_then(|policy| policy.message.as_deref())
        .unwrap_or("No release policy message was provided.");
    let policy_flags = release_policy
        .map(|policy| {
            format!(
                "Mandatory: {} · Current version blocked: {}",
                if policy.mandatory { "yes" } else { "no" },
                if policy.blocked_current_version {
                    "yes"
                } else {
                    "no"
                }
            )
        })
        .unwrap_or_else(|| "Mandatory: no · Current version blocked: no".into());
    let verifier_key_count = update::update_public_keys().len();
    let prepared_summary = prepared.map(|prepared| {
        format!(
            "Staged {} at {}",
            prepared.asset_kind.label(),
            prepared.asset_path.display()
        )
    });
    let last_apply_summary = app
        .updates
        .state
        .last_apply_report
        .as_ref()
        .map(|report| {
            format!(
                "Last apply: {} {} at {}",
                match report.status {
                    UpdateApplyReportStatus::Succeeded => "Succeeded for",
                    UpdateApplyReportStatus::Failed => "Failed for",
                },
                report.target_version,
                tabs::clips::format_timestamp(report.finished_at)
            )
        })
        .unwrap_or_else(|| "Last apply: no helper result has been recorded yet.".into());
    let last_apply_detail = app
        .updates
        .state
        .last_apply_report
        .as_ref()
        .and_then(|report| report.detail.as_deref())
        .unwrap_or("No additional apply detail was recorded.");
    let log_summary = if app.updates.details_log_loading {
        "Loading updater log…".into()
    } else if let Some(error) = &app.updates.details_log_error {
        format!("Could not load updater log: {error}")
    } else if let Some(log_text) = &app.updates.details_log_text {
        summarize_update_log_for_viewer(log_text)
    } else if let Some(report) = &app.updates.state.last_apply_report {
        format!(
            "No updater log preview loaded. Log path: {}",
            report.log_path.display()
        )
    } else {
        "No updater log has been recorded yet.".into()
    };

    let mut details = iced::widget::column![
        text(title).size(24),
        text("Release metadata, signing details, and the current changelog.").size(13),
        text(format!("Current version: {}", app.updates.state.current_version)).size(12),
        text(format!(
            "Install channel: {}",
            app.updates.state.install_channel.label()
        ))
        .size(12),
        text(format!("Published: {published_summary}")).size(12),
        text(format!("Availability: {availability_summary}")).size(12),
        text(format!("Minimum supported version: {minimum_version}")).size(12),
        text(rollout_summary).size(12),
        text(policy_flags).size(12),
        text(format!("Release policy message: {policy_message}")).size(12),
        text(format!(
            "Manifest signature: {signing_algorithm} via key `{signing_key_id}` ({signing_key_label})"
        ))
        .size(12),
        text(format!(
            "Embedded verifier keys in this build: {verifier_key_count}"
        ))
        .size(12),
        text(last_apply_summary).size(12),
        text(format!("Apply detail: {last_apply_detail}")).size(12),
    ]
    .spacing(8);

    if let Some(summary) = prepared_summary {
        details = details.push(text(summary).size(12));
    }
    if let Some(plan) = app.updates.state.system_update_plan.as_ref() {
        details = details.push(text(system_update_plan_summary(plan)).size(12));
    }
    if let Some(report) = &app.updates.state.last_apply_report {
        details =
            details.push(text(format!("Updater log path: {}", report.log_path.display())).size(12));
    }

    details = details
        .push(text("Changelog").size(16))
        .push(container(text(changelog).size(12)).width(Length::Fill))
        .push(text("Updater Log").size(16))
        .push(container(text(log_summary).size(12)).width(Length::Fill));

    let controls = row![
        {
            let button = shared::styled_button("Close", shared::ButtonTone::Secondary);
            button.on_press(Message::updates(UpdateMessage::HideUpdateDetails))
        },
        {
            let button = shared::styled_button("Open on GitHub", shared::ButtonTone::Primary);
            if app.active_update_release_url().is_some() {
                button.on_press(Message::updates(UpdateMessage::OpenUpdateReleaseNotes))
            } else {
                button
            }
        }
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    iced::widget::column![
        scrollable(container(details).width(Length::Fill)).height(Length::Fixed(420.0)),
        controls
    ]
    .spacing(16)
    .width(Length::Fill)
    .into()
}

fn summarize_update_log_for_viewer(log_text: &str) -> String {
    const MAX_LINES: usize = 120;
    let lines: Vec<_> = log_text.lines().collect();
    if lines.len() <= MAX_LINES {
        return log_text.to_string();
    }

    let tail = lines[lines.len() - MAX_LINES..].join("\n");
    format!("Showing the last {MAX_LINES} lines of the updater log.\n\n{tail}")
}

pub(in crate::app) fn release_action_label(
    target_version: &semver::Version,
    current_version: &semver::Version,
) -> &'static str {
    if target_version < current_version {
        "rollback"
    } else if target_version > current_version {
        "update"
    } else {
        "reinstall"
    }
}

pub(in crate::app) fn release_action_title(
    target_version: &semver::Version,
    current_version: &semver::Version,
) -> &'static str {
    if target_version < current_version {
        "Rollback"
    } else if target_version > current_version {
        "Update"
    } else {
        "Reinstall"
    }
}

pub(in crate::app) fn format_update_download_progress(
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
) -> String {
    match total_bytes {
        Some(total_bytes) => format!(
            "{} of {}",
            format_update_bytes(downloaded_bytes),
            format_update_bytes(total_bytes)
        ),
        None => format!("{} downloaded", format_update_bytes(downloaded_bytes)),
    }
}

fn format_update_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let bytes = bytes as f64;
    if bytes >= GB {
        format!("{:.1} GiB", bytes / GB)
    } else if bytes >= MB {
        format!("{:.1} MiB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.1} KiB", bytes / KB)
    } else {
        format!("{} B", bytes as u64)
    }
}

pub(in crate::app) fn main_window_settings() -> window::Settings {
    #[cfg(target_os = "linux")]
    let mut platform_specific = window::settings::PlatformSpecific::default();
    #[cfg(not(target_os = "linux"))]
    let platform_specific = window::settings::PlatformSpecific::default();
    #[cfg(target_os = "linux")]
    {
        platform_specific.application_id = "nanite-clip".into();
    }

    window::Settings {
        exit_on_close_request: false,
        icon: crate::app_icon::window_icon(),
        platform_specific,
        ..window::Settings::default()
    }
}

pub(in crate::app) fn audio_source_drafts_from_config(
    audio_sources: &[AudioSourceConfig],
) -> Vec<AudioSourceDraft> {
    let drafts: Vec<_> = audio_sources
        .iter()
        .map(|audio_source| AudioSourceDraft {
            label: audio_source.label.clone(),
            source: audio_source.kind.config_display_value(),
            gain_db: audio_source.gain_db,
            muted_in_premix: audio_source.muted_in_premix,
            included_in_premix: audio_source.included_in_premix,
        })
        .collect();

    if drafts.is_empty() {
        vec![AudioSourceDraft::default()]
    } else {
        drafts
    }
}

pub(in crate::app) fn clip_post_process_block_reason(record: &ClipRecord) -> Option<String> {
    match record.post_process_status {
        PostProcessStatus::Pending => Some(format!(
            "Clip #{} is still waiting for audio post-processing to finish.",
            record.id
        )),
        PostProcessStatus::Failed => Some(
            record
                .post_process_error
                .clone()
                .filter(|message| !message.trim().is_empty())
                .map(|message| format!("Clip #{} audio post-processing failed: {message}", record.id))
                .unwrap_or_else(|| {
                    format!(
                        "Clip #{} audio post-processing failed. Retry it or use the original clip audio.",
                        record.id
                    )
                }),
        ),
        PostProcessStatus::NotRequired
        | PostProcessStatus::Completed
        | PostProcessStatus::Legacy => None,
    }
}

pub(in crate::app) fn output_tracks_to_drafts(
    tracks: Vec<crate::post_process::OutputAudioTrack>,
) -> Vec<ClipAudioTrackDraft> {
    tracks
        .into_iter()
        .map(|track| ClipAudioTrackDraft {
            stream_index: track.stream_index,
            role: track.role,
            label: track.label,
            gain_db: track.gain_db,
            muted: track.muted,
            source_kind: track.source_kind,
            source_value: track.source_value,
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::rules::{AutoSwitchCondition, AutoSwitchRule, default_rule_profiles};
    use chrono::{Datelike, Timelike};

    fn sample_app(mut config: Config) -> App {
        let platform = PlatformServices::new();
        let secure_store = platform.secure_store().clone();
        let recorder = Recorder::new(config.capture.clone(), config.recorder.clone());
        let process_watcher = process::default_game_process_watcher();
        let ffmpeg_capabilities = post_process::probe_ffmpeg_capabilities();
        let character_name = config
            .characters
            .first()
            .map(|character| character.name.clone())
            .unwrap_or_else(|| "Example".into());
        let character_id = config
            .characters
            .first()
            .and_then(|character| character.character_id)
            .unwrap_or(42);
        let current_version = update::current_version();
        let update_state = hydrate_update_state_from_config(
            &mut config,
            update::detect_install_channel(),
            current_version,
        );
        let notifications = platform.create_notification_center();
        App {
            rule_engine: RuleEngine::new(
                config.rule_definitions.clone(),
                config.rule_profiles.clone(),
                config.active_profile_id.clone(),
            ),
            view: View::Status,
            recorder,
            notifications,
            toasts: ToastStack::new(),
            process_watcher,
            clip_store: None,
            clip_store_notice: None,
            event_log: EventLog::new(300),
            platform,
            honu_client: HonuClient::new(),
            background_jobs: BackgroundJobManager::new(),
            new_character_name: String::new(),
            runtime: RuntimeState {
                lifecycle: AppState::Monitoring {
                    character_name: character_name.clone(),
                    character_id,
                },
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
                active_session: Some(MonitoringSession {
                    id: format!("{character_id}-1"),
                    started_at: Utc::now(),
                    character_id,
                    character_name,
                }),
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
                feedback: None,
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
                audio_sources: audio_source_drafts_from_config(&config.recorder.audio_sources),
                discovered_audio_sources: Vec::new(),
                selected_device_audio_source: None,
                selected_application_audio_source: None,
                audio_discovery_running: false,
                audio_discovery_error: None,
                container: config.recorder.gsr().container.clone(),
                obs_websocket_url: config.recorder.obs().websocket_url.clone(),
                obs_password_input: String::new(),
                obs_password_present: config.recorder.obs().websocket_password.is_some(),
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
                copyparty_password_present: false,
                youtube_enabled: config.uploads.youtube.enabled,
                youtube_client_id: config.uploads.youtube.client_id.clone(),
                youtube_client_secret_input: String::new(),
                youtube_client_secret_present: false,
                youtube_refresh_token_present: false,
                youtube_oauth_in_flight: false,
                youtube_privacy_status: config.uploads.youtube.privacy_status,
                discord_enabled: config.discord_webhook.enabled,
                discord_min_score: config.discord_webhook.min_score.to_string(),
                discord_include_thumbnail: config.discord_webhook.include_thumbnail,
                discord_webhook_input: String::new(),
                discord_webhook_present: false,
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
        }
    }

    fn sample_clip() -> ClipDraft {
        let event_at = chrono::DateTime::<Utc>::from_timestamp(1_710_000_000, 0).unwrap();
        ClipDraft {
            trigger_event_at: event_at,
            clip_start_at: event_at - chrono::Duration::seconds(30),
            clip_end_at: event_at,
            saved_at: event_at,
            origin: ClipOrigin::Rule,
            profile_id: "profile_1".into(),
            rule_id: "rule_kill_streak".into(),
            clip_duration_secs: 30,
            session_id: Some("session-1".into()),
            character_id: 42,
            world_id: 17,
            zone_id: Some(2),
            facility_id: Some(1234),
            score: 9,
            honu_session_id: None,
            path: None,
            events: vec![ClipEventContribution {
                event_kind: "Kill".into(),
                occurrences: 3,
                points: 6,
            }],
            raw_events: Vec::new(),
            alert_keys: Vec::new(),
        }
    }

    fn sample_prepared_update(asset_path: PathBuf) -> update::PreparedUpdate {
        sample_prepared_update_with_version(asset_path, "9.9.9")
    }

    fn sample_prepared_update_with_version(
        asset_path: PathBuf,
        version: &str,
    ) -> update::PreparedUpdate {
        update::PreparedUpdate {
            version: version.into(),
            tag_name: format!("v{version}"),
            install_channel: update::InstallChannel::WindowsPortable,
            asset_kind: update::types::UpdateAssetKind::Exe,
            asset_name: "nanite-clip.exe".into(),
            asset_path,
            release_notes_url: format!("https://example.invalid/releases/v{version}"),
            release_name: Some(format!("NaniteClip {version}")),
            changelog_markdown: Some("## Highlights\n\n- Sample release".into()),
            published_at: None,
            signature: update::types::UpdateSignatureInfo::default(),
            policy: update::UpdateReleasePolicy::default(),
        }
    }

    fn sample_available_release(
        install_channel: update::InstallChannel,
        policy: update::UpdateReleasePolicy,
        with_asset: bool,
    ) -> update::AvailableRelease {
        update::AvailableRelease {
            version: Version::parse("9.9.9").unwrap(),
            tag_name: "v9.9.9".into(),
            release_name: "NaniteClip 9.9.9".into(),
            html_url: "https://example.invalid/releases/v9.9.9".into(),
            changelog_markdown: "## Highlights".into(),
            published_at: None,
            signature: update::types::UpdateSignatureInfo::default(),
            policy,
            asset: with_asset.then_some(update::types::ManifestAsset {
                channel: install_channel,
                kind: update::types::UpdateAssetKind::Exe,
                filename: "nanite-clip.exe".into(),
                download_url: "https://example.invalid/nanite-clip.exe".into(),
                sha256: "abc123".into(),
                size: Some(42),
            }),
            install_channel,
            skipped: false,
        }
    }

    #[tokio::test]
    async fn deleting_clip_without_saved_file_still_removes_row() {
        let store = ClipStore::open_in_memory().await.unwrap();
        let clip_id = store.insert_clip(sample_clip()).await.unwrap();

        delete_clip_file_and_unlink(store.clone(), clip_id, None)
            .await
            .unwrap();

        let clips = store.recent_clips(10).await.unwrap();
        assert!(clips.is_empty());
    }

    #[test]
    fn auto_start_monitoring_uses_waiting_for_game_initial_state() {
        let mut config = Config::default();
        config.auto_start_monitoring = true;

        assert!(matches!(
            runtime::initial_runtime_state(&config),
            AppState::WaitingForGame
        ));
    }

    #[test]
    fn non_auto_dismiss_feedback_still_gets_a_timeout() {
        let mut app = sample_app(Config::default());

        app.set_settings_feedback("Needs attention", false);

        assert!(app.settings.feedback_expires_at.is_some());
        assert_eq!(app.toasts.len(), 1);
        assert_eq!(
            app.toasts.toasts()[0].duration,
            Some(App::EXTENDED_MESSAGE_TIMEOUT)
        );
    }

    #[test]
    fn hotkey_feedback_ignores_unchanged_binding_labels() {
        assert_eq!(
            hotkey_configuration_feedback(true, Some("Ctrl+Shift+F8"), Some("Ctrl+Shift+F8"), None),
            None
        );
    }

    #[test]
    fn hotkey_feedback_ignores_success_when_not_requested() {
        assert_eq!(
            hotkey_configuration_feedback(false, Some("Ctrl+Shift+F8"), Some("Alt+F8"), None),
            None
        );
    }

    #[test]
    fn hotkey_feedback_reports_changed_binding_labels() {
        assert_eq!(
            hotkey_configuration_feedback(true, Some("Ctrl+Shift+F8"), Some("Alt+F8"), None),
            Some(HotkeyConfigurationFeedback::Success(
                "Manual clip hotkey active: Alt+F8".into()
            ))
        );
    }

    #[test]
    fn hotkey_feedback_reports_configuration_notes_when_requested() {
        assert_eq!(
            hotkey_configuration_feedback(
                true,
                None,
                None,
                Some("Assign a shortcut in the portal.")
            ),
            Some(HotkeyConfigurationFeedback::Note(
                "Assign a shortcut in the portal.".into()
            ))
        );
    }

    #[test]
    fn hydrate_update_state_restores_staged_update_when_asset_exists() {
        let temp_root = std::env::temp_dir().join(format!(
            "nanite-clip-update-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_root).unwrap();
        let asset_path = temp_root.join("nanite-clip.exe");
        std::fs::write(&asset_path, b"updater").unwrap();

        let mut config = Config::default();
        config.updates.prepared_update = Some(sample_prepared_update(asset_path.clone()));
        let update_state = hydrate_update_state_from_config(
            &mut config,
            update::InstallChannel::WindowsPortable,
            update::current_version(),
        );

        assert!(matches!(update_state.phase, UpdatePhase::ReadyToInstall));
        assert_eq!(
            update_state
                .prepared_update
                .as_ref()
                .map(|prepared| &prepared.asset_path),
            Some(&asset_path)
        );
        assert!(config.updates.prepared_update.is_some());

        let _ = std::fs::remove_file(&asset_path);
        let _ = std::fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn hydrate_update_state_keeps_staged_rollback_target() {
        let temp_root = std::env::temp_dir().join(format!(
            "nanite-clip-rollback-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_root).unwrap();
        let asset_path = temp_root.join("nanite-clip.exe");
        std::fs::write(&asset_path, b"rollback").unwrap();

        let mut config = Config::default();
        let rollback_version = "0.1.0";
        config.updates.prepared_update = Some(sample_prepared_update_with_version(
            asset_path.clone(),
            rollback_version,
        ));
        let update_state = hydrate_update_state_from_config(
            &mut config,
            update::InstallChannel::WindowsPortable,
            update::current_version(),
        );

        assert!(matches!(update_state.phase, UpdatePhase::ReadyToInstall));
        assert_eq!(
            update_state
                .prepared_update
                .as_ref()
                .map(|prepared| prepared.version.as_str()),
            Some(rollback_version)
        );

        let _ = std::fs::remove_file(&asset_path);
        let _ = std::fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn record_running_version_tracks_previous_install_history() {
        let current_version = update::current_version();
        let next_version = semver::Version::new(
            current_version.major,
            current_version.minor,
            current_version.patch.saturating_add(1),
        );
        let mut config = Config::default();
        config.updates.current_version = Some(next_version.to_string());

        record_running_version(&mut config, &current_version);
        let current_version_label = current_version.to_string();

        assert_eq!(
            config.updates.current_version.as_deref(),
            Some(current_version_label.as_str())
        );
        assert_eq!(
            config.updates.installed_version_history.first(),
            Some(&next_version.to_string())
        );
    }

    #[test]
    fn when_idle_updates_only_auto_apply_after_monitoring_stops() {
        let mut config = Config::default();
        config.updates.install_behavior = UpdateInstallBehavior::WhenIdle;

        let mut app = sample_app(config);
        app.updates.state.prepared_update =
            Some(sample_prepared_update(PathBuf::from("staged.exe")));
        app.updates.state.phase = UpdatePhase::ReadyToInstall;

        assert!(!app.should_auto_apply_staged_update());

        app.runtime.lifecycle = AppState::Idle;
        app.runtime.active_session = None;

        assert!(app.should_auto_apply_staged_update());
    }

    #[test]
    fn next_automatic_update_check_uses_last_check_timestamp() {
        let mut app = sample_app(Config::default());
        let last_check = Utc::now() - chrono::Duration::hours(3);
        app.updates.state.last_checked_at = Some(last_check);

        let next_check = next_automatic_update_check_at(&app).unwrap();

        assert_eq!(next_check, last_check + chrono::Duration::hours(12));
    }

    #[test]
    fn mandatory_package_managed_release_prefers_system_updater_action() {
        let mut app = sample_app(Config::default());
        app.runtime.lifecycle = AppState::Idle;
        app.runtime.active_session = None;
        app.updates.state.latest_release = Some(sample_available_release(
            update::InstallChannel::LinuxDeb,
            update::UpdateReleasePolicy {
                mandatory: true,
                ..Default::default()
            },
            false,
        ));
        app.updates.state.system_update_plan = Some(update::SystemUpdatePlan {
            label: "PackageKit".into(),
            detail: "Launch PackageKit.".into(),
            command_display: Some("pkcon update".into()),
            command_program: Some("pkcon".into()),
            command_args: vec!["update".into()],
        });

        assert_eq!(
            update_action_options(&app),
            &[UpdatePrimaryAction::OpenSystemUpdater]
        );
        assert_eq!(
            selected_update_action(&app),
            UpdatePrimaryAction::OpenSystemUpdater
        );
        assert!(can_run_selected_update_action(&app));
    }

    #[test]
    fn classify_update_error_prefers_verification_and_install_context() {
        assert_eq!(
            classify_update_error(
                "downloaded update checksum mismatch",
                UpdatePhase::Downloading
            ),
            UpdateErrorKind::Verification
        );
        assert_eq!(
            classify_update_error("failed to launch updater helper", UpdatePhase::Applying),
            UpdateErrorKind::Install
        );
        assert_eq!(
            classify_update_error("GitHub request timed out", UpdatePhase::Checking),
            UpdateErrorKind::Network
        );
    }

    #[tokio::test]
    async fn stats_day_navigation_updates_clip_date_filter_and_triggers_reload() {
        let mut config = Config::default();
        config.characters.push(crate::config::CharacterConfig {
            name: "Example".into(),
            character_id: Some(42),
            world_id: None,
            faction_id: None,
        });

        let mut app = sample_app(config);
        app.clip_store = Some(ClipStore::open_in_memory().await.unwrap());

        let _task = tabs::stats::update(
            &mut app,
            tabs::stats::Message::NavigateToClipsOnDay("2026-04-05".into()),
        );

        assert_eq!(
            app.clips.date_range_preset,
            tabs::clips::DateRangePreset::Custom
        );
        assert_eq!(app.clips.date_range_start, "2026-04-05");
        assert_eq!(app.clips.date_range_end, "2026-04-05");
        assert!(app.clips.filters.event_after_ts.is_some());
        assert!(app.clips.filters.event_before_ts.is_some());
        assert_eq!(app.clips.query_revision, 1);
    }

    #[test]
    fn storage_retry_target_infers_archive_and_primary() {
        let now = Utc::now();
        let archive_job = BackgroundJobRecord {
            id: BackgroundJobId(1),
            kind: BackgroundJobKind::StorageTiering,
            label: "Move clip #42 to archive".into(),
            state: crate::background_jobs::BackgroundJobState::Failed,
            related_clip_ids: vec![42],
            progress: None,
            started_at: now,
            updated_at: now,
            finished_at: Some(now),
            detail: Some("Move failed.".into()),
            cancellable: false,
        };
        let primary_job = BackgroundJobRecord {
            label: "Move clip #42 to primary".into(),
            ..archive_job.clone()
        };

        assert_eq!(
            infer_storage_move_retry_target(&archive_job).unwrap(),
            StorageTier::Archive
        );
        assert_eq!(
            infer_storage_move_retry_target(&primary_job).unwrap(),
            StorageTier::Primary
        );
    }

    #[test]
    fn manual_clip_naming_context_uses_manual_origin() {
        let mut config = Config::default();
        config.characters.push(crate::config::CharacterConfig {
            name: "Example".into(),
            character_id: Some(42),
            world_id: None,
            faction_id: None,
        });
        let app = sample_app(config);

        let context = app.build_clip_naming_context(&ClipSaveRequest {
            origin: ClipOrigin::Manual,
            profile_id: "profile_1".into(),
            rule_id: "manual_clip".into(),
            duration: ClipLength::Seconds(30),
            clip_duration_secs: 30,
            trigger_score: 0,
            score_breakdown: Vec::new(),
            trigger_at: Utc::now(),
            clip_start_at: Utc::now() - chrono::Duration::seconds(30),
            clip_end_at: Utc::now(),
            world_id: 17,
            zone_id: Some(2),
            facility_id: None,
            character_id: 42,
            honu_session_id: None,
            session_id: Some("42-1".into()),
        });

        assert_eq!(context.source, "manual");
        assert_eq!(context.character, "Example");
        assert_eq!(context.rule, "manual_clip");
    }

    #[test]
    fn alert_window_matching_returns_only_overlapping_same_zone_alerts() {
        let mut app = sample_app(Config::default());
        let clip_start_at = Utc::now();
        let clip_end_at = clip_start_at + chrono::Duration::seconds(30);

        app.runtime.tracked_alerts.insert(
            "matching-a".into(),
            AlertInstanceRecord {
                alert_key: "matching-a".into(),
                label: "Indar Alert A".into(),
                world_id: 17,
                zone_id: 2,
                metagame_event_id: 1,
                started_at: clip_start_at - chrono::Duration::minutes(5),
                ended_at: None,
                state_name: "started".into(),
                winner_faction: None,
                faction_nc: 33.0,
                faction_tr: 33.0,
                faction_vs: 34.0,
            },
        );
        app.runtime.tracked_alerts.insert(
            "matching-b".into(),
            AlertInstanceRecord {
                alert_key: "matching-b".into(),
                label: "Indar Alert B".into(),
                world_id: 17,
                zone_id: 2,
                metagame_event_id: 2,
                started_at: clip_start_at - chrono::Duration::seconds(10),
                ended_at: Some(clip_end_at + chrono::Duration::seconds(5)),
                state_name: "started".into(),
                winner_faction: None,
                faction_nc: 20.0,
                faction_tr: 30.0,
                faction_vs: 50.0,
            },
        );
        app.runtime.tracked_alerts.insert(
            "different-zone".into(),
            AlertInstanceRecord {
                alert_key: "different-zone".into(),
                label: "Esamir Alert".into(),
                world_id: 17,
                zone_id: 8,
                metagame_event_id: 3,
                started_at: clip_start_at - chrono::Duration::minutes(1),
                ended_at: None,
                state_name: "started".into(),
                winner_faction: None,
                faction_nc: 0.0,
                faction_tr: 0.0,
                faction_vs: 100.0,
            },
        );

        let alert_keys = app.alert_keys_for_clip_window(17, Some(2), clip_start_at, clip_end_at);

        assert_eq!(
            alert_keys,
            vec!["matching-a".to_string(), "matching-b".to_string()]
        );
    }

    #[test]
    fn manual_override_blocks_auto_switch_until_resumed() {
        let mut config = Config::default();
        config.rule_profiles = default_rule_profiles();
        config.active_profile_id = "profile_1".into();
        config.auto_switch_rules = vec![AutoSwitchRule {
            id: "time-rule".into(),
            name: "Time Rule".into(),
            enabled: true,
            target_profile_id: "profile_2".into(),
            condition: AutoSwitchCondition::LocalTimeRange {
                start_hour: 0,
                end_hour: 24,
            },
        }];

        let mut app = sample_app(config);
        let now = Utc::now();
        app.runtime.manual_profile_override_profile_id = Some("profile_1".into());

        let _ = app.evaluate_runtime_auto_switch(now, Some(42));
        assert_eq!(app.config.active_profile_id, "profile_1");
        assert_eq!(app.runtime.last_auto_switch_rule_id, None);

        app.resume_auto_switching();
        let _ = app.evaluate_runtime_auto_switch(now, Some(42));
        assert_eq!(app.config.active_profile_id, "profile_2");
        assert_eq!(
            app.runtime.last_auto_switch_rule_id.as_deref(),
            Some("time-rule")
        );
    }

    #[test]
    fn active_character_auto_switch_changes_profile_only_for_matching_character() {
        let mut config = Config::default();
        config.rule_profiles = default_rule_profiles();
        config.active_profile_id = "profile_1".into();
        config.auto_switch_rules = vec![AutoSwitchRule {
            id: "character-switch".into(),
            name: "Character Switch".into(),
            enabled: true,
            target_profile_id: "profile_2".into(),
            condition: AutoSwitchCondition::ActiveCharacter {
                character_ids: vec![99, 123],
                character_id: None,
            },
        }];

        let mut app = sample_app(config);

        let _ = app.evaluate_runtime_auto_switch(Utc::now(), Some(42));
        assert_eq!(app.config.active_profile_id, "profile_1");

        let _ = app.evaluate_runtime_auto_switch(Utc::now(), Some(99));
        assert_eq!(app.config.active_profile_id, "profile_2");
        assert_eq!(
            app.runtime.last_auto_switch_rule_id.as_deref(),
            Some("character-switch")
        );
    }

    #[test]
    fn local_schedule_auto_switch_changes_profile_when_schedule_matches() {
        let mut config = Config::default();
        config.rule_profiles = default_rule_profiles();
        config.active_profile_id = "profile_1".into();
        let local = chrono::Local::now();
        config.auto_switch_rules = vec![AutoSwitchRule {
            id: "schedule-switch".into(),
            name: "Schedule Switch".into(),
            enabled: true,
            target_profile_id: "profile_2".into(),
            condition: AutoSwitchCondition::LocalSchedule {
                weekdays: vec![crate::rules::schedule::ScheduleWeekday::from_chrono(
                    local.weekday(),
                )],
                start_hour: local.hour() as u8,
                start_minute: 0,
                end_hour: ((local.hour() + 1) % 24) as u8,
                end_minute: 0,
            },
        }];

        let mut app = sample_app(config);

        let _ = app.evaluate_runtime_auto_switch(local.with_timezone(&Utc), Some(42));

        assert_eq!(app.config.active_profile_id, "profile_2");
        assert_eq!(
            app.runtime.last_auto_switch_rule_id.as_deref(),
            Some("schedule-switch")
        );
    }

    #[test]
    fn local_schedule_auto_switch_runs_during_idle_tick() {
        let mut config = Config::default();
        config.rule_profiles = default_rule_profiles();
        config.active_profile_id = "profile_1".into();
        let local = chrono::Local::now();
        config.auto_switch_rules = vec![AutoSwitchRule {
            id: "schedule-switch".into(),
            name: "Schedule Switch".into(),
            enabled: true,
            target_profile_id: "profile_2".into(),
            condition: AutoSwitchCondition::LocalSchedule {
                weekdays: vec![crate::rules::schedule::ScheduleWeekday::from_chrono(
                    local.weekday(),
                )],
                start_hour: local.hour() as u8,
                start_minute: if local.minute() < 30 { 0 } else { 30 },
                end_hour: if local.minute() < 30 {
                    local.hour() as u8
                } else {
                    ((local.hour() + 1) % 24) as u8
                },
                end_minute: if local.minute() < 30 { 30 } else { 0 },
            },
        }];

        let mut app = sample_app(config);
        app.runtime.lifecycle = AppState::Idle;
        app.runtime.active_session = None;

        let _ = app.update(Message::runtime(RuntimeMessage::Tick));

        assert_eq!(app.config.active_profile_id, "profile_2");
        assert_eq!(
            app.runtime.last_auto_switch_rule_id.as_deref(),
            Some("schedule-switch")
        );
    }

    #[test]
    fn tray_snapshot_lists_profiles_and_active_selection() {
        let mut config = Config::default();
        config.rule_profiles = vec![
            RuleProfile {
                id: "profile_1".into(),
                name: "Default".into(),
                enabled_rule_ids: Vec::new(),
            },
            RuleProfile {
                id: "profile_2".into(),
                name: "Highlights".into(),
                enabled_rule_ids: Vec::new(),
            },
        ];
        config.active_profile_id = "profile_2".into();
        let app = sample_app(config);

        let snapshot = app.tray_snapshot();

        assert_eq!(snapshot.profile_options.len(), 2);
        assert_eq!(snapshot.profile_options[0].name, "Default");
        assert!(!snapshot.profile_options[0].selected);
        assert_eq!(snapshot.profile_options[1].name, "Highlights");
        assert!(snapshot.profile_options[1].selected);
    }

    #[test]
    fn obs_reconnect_status_updates_tray_snapshot_and_clears_on_reconnect() {
        let mut config = Config::default();
        config.capture.backend = crate::config::CaptureBackend::Obs;
        let mut app = sample_app(config);

        assert!(
            app.apply_backend_runtime_event(capture::BackendRuntimeEvent::ObsConnection(
                capture::ObsConnectionStatus::Reconnecting {
                    attempt: 2,
                    next_retry_in_secs: 5,
                },
            ))
        );
        assert_eq!(
            app.tray_snapshot().status_label,
            "OBS reconnecting (attempt 2, retry in 5s)"
        );

        assert!(
            app.apply_backend_runtime_event(capture::BackendRuntimeEvent::ObsConnection(
                capture::ObsConnectionStatus::Connected,
            ))
        );
        assert!(!app.runtime.obs_restart_requires_manual_restart);
        assert_eq!(app.tray_snapshot().status_label, "Monitoring Example");
    }

    #[test]
    fn obs_failed_status_requires_manual_restart() {
        let mut config = Config::default();
        config.capture.backend = crate::config::CaptureBackend::Obs;
        let mut app = sample_app(config);

        assert!(
            app.apply_backend_runtime_event(capture::BackendRuntimeEvent::ObsConnection(
                capture::ObsConnectionStatus::Failed {
                    reason: "socket closed".into(),
                },
            ))
        );

        assert!(app.runtime.obs_restart_requires_manual_restart);
        assert_eq!(
            app.tray_snapshot().status_label,
            "OBS reconnect failed: socket closed"
        );
    }

    #[test]
    fn minimize_to_tray_close_request_clears_main_window_id() {
        let mut config = Config::default();
        config.minimize_to_tray = true;
        let mut app = sample_app(config);
        let window_id = window::Id::unique();
        app.runtime.main_window_id = Some(window_id);

        let _ = app.update(Message::runtime(RuntimeMessage::WindowCloseRequested(
            window_id,
        )));

        assert_eq!(app.runtime.main_window_id, None);
    }

    #[test]
    fn open_main_window_task_reserves_window_id_immediately() {
        let mut app = sample_app(Config::default());

        let _ = app.open_main_window_task();

        assert!(app.runtime.main_window_id.is_some());
    }

    #[test]
    fn repeated_open_main_window_task_reuses_reserved_window_id() {
        let mut app = sample_app(Config::default());

        let _ = app.open_main_window_task();
        let reserved_id = app.runtime.main_window_id;
        let _ = app.open_main_window_task();

        assert_eq!(app.runtime.main_window_id, reserved_id);
    }

    #[test]
    fn raw_event_harvest_uses_computed_clip_window() {
        let trigger_at = Utc::now();
        let mut event_log = EventLog::new(120);
        event_log.append(crate::rules::ClassifiedEvent {
            kind: crate::rules::EventKind::Kill,
            timestamp: trigger_at - chrono::Duration::seconds(15),
            world_id: 17,
            zone_id: Some(2),
            facility_id: Some(1234),
            actor_character_id: Some(42),
            other_character_id: Some(100),
            other_character_outfit_id: None,
            characters_killed: 1,
            attacker_weapon_id: Some(80),
            attacker_vehicle_id: None,
            vehicle_killed_id: None,
            is_headshot: false,
            actor_class: Some(crate::rules::CharacterClass::HeavyAssault),
            experience_id: None,
        });
        event_log.append(crate::rules::ClassifiedEvent {
            kind: crate::rules::EventKind::Headshot,
            timestamp: trigger_at + chrono::Duration::seconds(4),
            world_id: 17,
            zone_id: Some(2),
            facility_id: Some(1234),
            actor_character_id: Some(42),
            other_character_id: Some(101),
            other_character_outfit_id: None,
            characters_killed: 1,
            attacker_weapon_id: Some(81),
            attacker_vehicle_id: None,
            vehicle_killed_id: None,
            is_headshot: true,
            actor_class: Some(crate::rules::CharacterClass::HeavyAssault),
            experience_id: None,
        });
        event_log.append(crate::rules::ClassifiedEvent {
            kind: crate::rules::EventKind::Revive,
            timestamp: trigger_at - chrono::Duration::seconds(40),
            world_id: 17,
            zone_id: Some(2),
            facility_id: Some(1234),
            actor_character_id: Some(42),
            other_character_id: Some(102),
            other_character_outfit_id: None,
            characters_killed: 0,
            attacker_weapon_id: None,
            attacker_vehicle_id: None,
            vehicle_killed_id: None,
            is_headshot: false,
            actor_class: Some(crate::rules::CharacterClass::Medic),
            experience_id: Some(7),
        });

        let request = ClipSaveRequest {
            origin: ClipOrigin::Rule,
            profile_id: "profile_1".into(),
            rule_id: "rule_1".into(),
            duration: ClipLength::Seconds(20),
            clip_duration_secs: 20,
            trigger_score: 5,
            score_breakdown: Vec::new(),
            trigger_at,
            clip_start_at: trigger_at - chrono::Duration::seconds(15),
            clip_end_at: trigger_at + chrono::Duration::seconds(5),
            world_id: 17,
            zone_id: Some(2),
            facility_id: Some(1234),
            character_id: 42,
            honu_session_id: None,
            session_id: Some("session-1".into()),
        };

        let raw_events = raw_events_from_log(&event_log, &request);
        assert_eq!(raw_events.len(), 2);
        assert_eq!(raw_events[0].event_kind, "Kill");
        assert_eq!(raw_events[1].event_kind, "Headshot");
    }

    #[test]
    fn recompute_capture_window_clamps_to_replay_buffer() {
        let trigger_at = chrono::DateTime::<Utc>::from_timestamp(1_710_000_000, 0).unwrap();
        let mut request = ClipSaveRequest {
            origin: ClipOrigin::Rule,
            profile_id: "profile_1".into(),
            rule_id: "rule_1".into(),
            duration: ClipLength::Seconds(20),
            clip_duration_secs: 20,
            trigger_score: 5,
            score_breakdown: Vec::new(),
            trigger_at,
            clip_start_at: trigger_at - chrono::Duration::seconds(20),
            clip_end_at: trigger_at + chrono::Duration::seconds(40),
            world_id: 17,
            zone_id: Some(2),
            facility_id: Some(1234),
            character_id: 42,
            honu_session_id: None,
            session_id: Some("session-1".into()),
        };

        recompute_capture_window(&mut request, trigger_at - chrono::Duration::seconds(20), 30);

        assert_eq!(request.clip_duration_secs, 30);
        assert_eq!(
            request.clip_start_at,
            request.clip_end_at - chrono::Duration::seconds(30)
        );
        assert_eq!(request.duration, ClipLength::Seconds(30));
    }

    #[test]
    fn clip_log_retention_includes_extension_window() {
        let mut config = Config::default();
        config.recorder.replay_buffer_secs = 120;
        config.recorder.save_delay_secs = 3;
        config.manual_clip.duration_secs = 45;
        config.rule_definitions[0].extension.mode = crate::rules::ClipExtensionMode::HoldUntilQuiet;
        config.rule_definitions[0].extension.window_secs = 9;

        assert_eq!(clip_log_retention_secs(&config), 132);
    }
}
