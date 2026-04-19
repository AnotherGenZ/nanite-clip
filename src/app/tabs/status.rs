use iced::{Background, Element, Length};

use crate::background_jobs::{BackgroundJobRecord, BackgroundJobState};
use crate::ui::app::{Column, column, container, pick_list, row, scrollable, text};
use crate::ui::layout::card::card;
use crate::ui::layout::empty_state::empty_state;
use crate::ui::layout::page_header::page_header;
use crate::ui::layout::panel::panel;
use crate::ui::layout::toolbar::toolbar;
use crate::ui::overlay::banner::banner;
use crate::ui::primitives::badge::{Tone as BadgeTone, badge};
use crate::ui::theme::{self, Tokens};
use crate::update::{UpdatePhase, UpdatePrimaryAction};

use super::super::shared::{ButtonTone, styled_button, with_tooltip};
use super::super::{App, AppState, Message};

const JOB_TABLE_ID_WIDTH: f32 = 78.0;
const JOB_TABLE_KIND_WIDTH: f32 = 116.0;
const JOB_TABLE_STATE_WIDTH: f32 = 112.0;
const JOB_TABLE_CLIPS_WIDTH: f32 = 88.0;
const JOB_TABLE_UPDATED_WIDTH: f32 = 172.0;
const JOB_TABLE_ACTIONS_WIDTH: f32 = 180.0;
const JOB_TABLE_HEIGHT: f32 = 260.0;

fn update_action_tone(action: UpdatePrimaryAction) -> ButtonTone {
    match action {
        UpdatePrimaryAction::DownloadUpdate => ButtonTone::Primary,
        UpdatePrimaryAction::InstallAndRestart => ButtonTone::Success,
        UpdatePrimaryAction::InstallWhenIdle
        | UpdatePrimaryAction::InstallOnNextLaunch
        | UpdatePrimaryAction::RemindLater => ButtonTone::Secondary,
        UpdatePrimaryAction::SkipThisVersion => ButtonTone::Danger,
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
                    button.on_press(Message::ShowUpdateDetails).into()
                } else {
                    button.into()
                }
            },
            "Open the in-app changelog and updater details viewer for the active update target.",
        ),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into()
}

pub(in crate::app) fn view(app: &App) -> Element<'_, Message> {
    let (state_text, state_tone) = match &app.state {
        AppState::Idle => ("Idle", BadgeTone::Neutral),
        AppState::WaitingForGame => ("Waiting for PS2", BadgeTone::Warning),
        AppState::WaitingForLogin => ("Waiting for login", BadgeTone::Warning),
        AppState::Monitoring { .. } => ("Monitoring", BadgeTone::Success),
    };

    let action_button = match &app.state {
        AppState::Idle => with_tooltip(
            styled_button("Start Monitoring", ButtonTone::Success)
                .on_press(Message::StartMonitoring)
                .into(),
            "Watch for PlanetSide 2 and start the recorder.",
        ),
        _ => with_tooltip(
            styled_button("Stop Monitoring", ButtonTone::Danger)
                .on_press(Message::StopMonitoring)
                .into(),
            "Stop monitoring and the recorder.",
        ),
    };

    let header = page_header("Status")
        .subtitle("Application state, sessions, and background jobs.")
        .action(action_button)
        .build();

    let status_bar = toolbar()
        .push(status_badge(state_text, state_tone))
        .push(status_badge(
            if app.recorder.has_active_session() {
                "Recorder running"
            } else {
                "Recorder stopped"
            },
            if app.recorder.has_active_session() {
                BadgeTone::Success
            } else {
                BadgeTone::Neutral
            },
        ))
        .push(status_badge(
            if app.clip_store.is_some() {
                "Database ready"
            } else if app.clip_error.is_some() {
                "Database unavailable"
            } else {
                "Database starting"
            },
            if app.clip_store.is_some() {
                BadgeTone::Success
            } else if app.clip_error.is_some() {
                BadgeTone::Destructive
            } else {
                BadgeTone::Warning
            },
        ))
        .push(status_badge(
            ffmpeg_status_label(app),
            ffmpeg_status_tone(app),
        ))
        .push(status_badge(
            format!("v{}", crate::update::current_version_label()),
            BadgeTone::Outline,
        ))
        .build();

    // System overview panel
    let mut system_panel = panel("System");

    if let Some(status) = &app.obs_connection_status {
        system_panel = system_panel.push(match status {
            crate::capture::ObsConnectionStatus::Connected => banner("OBS reconnected")
                .description("The websocket session was restored and monitoring is running again.")
                .success()
                .build(),
            crate::capture::ObsConnectionStatus::Reconnecting {
                attempt,
                next_retry_in_secs,
            } => banner("OBS disconnected")
                .description(format!(
                    "Reconnecting in the background (attempt {attempt}, retrying in {next_retry_in_secs}s)."
                ))
                .warning()
                .build(),
            crate::capture::ObsConnectionStatus::Failed { reason } => banner("Cannot reconnect to OBS")
                .description(format!("{reason}. NaniteClip will keep retrying in the background."))
                .error()
                .build(),
        });
    }

    // Monitoring state detail
    match &app.state {
        AppState::Monitoring {
            character_name,
            character_id,
        } => {
            system_panel = system_panel.push(
                banner(format!("Monitoring {character_name} ({character_id})"))
                    .success()
                    .build(),
            );
        }
        AppState::Idle => {}
        _ => {
            let detail = match &app.state {
                AppState::WaitingForGame => "Waiting for PlanetSide 2 to launch...",
                AppState::WaitingForLogin => {
                    "PlanetSide 2 detected \u{2014} waiting for character login..."
                }
                _ => "",
            };
            if !detail.is_empty() {
                system_panel = system_panel.push(banner(detail).info().build());
            }
        }
    }

    let mut startup_notes = Vec::new();
    if app.config.launch_at_login.enabled {
        startup_notes.push("Launch-at-login enabled");
    }
    if app.config.auto_start_monitoring {
        startup_notes.push("Auto-start monitoring enabled");
    }
    if !startup_notes.is_empty() {
        system_panel = system_panel.push(
            row(startup_notes
                .iter()
                .map(|note| status_badge(*note, BadgeTone::Info)))
            .spacing(8)
            .align_y(iced::Alignment::Center),
        );
    }

    if let Some(previous_version) = &app.update_state.previous_installed_version {
        system_panel = system_panel.push(
            row![
                status_badge(
                    format!("Previous install {previous_version}"),
                    BadgeTone::Outline
                ),
                with_tooltip(
                    styled_button("Rollback Previous", ButtonTone::Warning)
                        .on_press(Message::RollbackToPreviousInstalledVersion)
                        .into(),
                    "Download the last installed version and stage it as a rollback target.",
                ),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        );
    }

    if let Some(prepared) = &app.update_state.prepared_update {
        let prepared_version = prepared
            .parsed_version()
            .unwrap_or_else(|| app.update_state.current_version.clone());
        let staged_title = if prepared_version < app.update_state.current_version {
            format!("Rollback {} is staged", prepared.version)
        } else {
            format!("Update {} is staged", prepared.version)
        };
        system_panel = system_panel.push(
            banner(staged_title)
                .warning()
                .description(format!(
                    "The downloaded {} is ready. Install behavior: {}.",
                    prepared.asset_kind.label(),
                    app.config.updates.install_behavior.label()
                ))
                .build(),
        );
        system_panel = system_panel.push(
            text(format!(
                "Selected action: {}",
                super::super::selected_update_action(app).description()
            ))
            .size(12),
        );
        system_panel = system_panel.push(text(status_update_detail_summary(app)).size(12));
        system_panel = system_panel.push(update_action_controls(app));
    } else if let Some(release) = &app.update_state.latest_release {
        let next_check_label = super::super::next_automatic_update_check_at(app)
            .map(|next_check| format!("Next check {}", super::clips::format_timestamp(next_check)))
            .unwrap_or_else(|| "Automatic checks off".into());

        let phase_description = match app.update_state.phase {
            UpdatePhase::Checking
            | UpdatePhase::Downloading
            | UpdatePhase::Verifying
            | UpdatePhase::Applying => app
                .update_state
                .progress
                .as_ref()
                .map(|progress| progress.detail.clone())
                .unwrap_or_else(|| app.update_state.phase.label().into()),
            UpdatePhase::ReadyToInstall => app
                .update_state
                .prepared_update
                .as_ref()
                .map(|prepared| format!("{} is downloaded and ready to install.", prepared.version))
                .unwrap_or_else(|| "An update is available.".into()),
            _ => {
                if release.install_channel.supports_self_update() {
                    format!(
                        "{}. Install channel: {}. {}.",
                        release.release_name,
                        release.install_channel.label(),
                        next_check_label
                    )
                } else {
                    format!(
                        "{} {}",
                        release.install_channel.update_instructions(),
                        next_check_label
                    )
                }
            }
        };

        system_panel = system_panel.push(
            banner(format!("Update {} is available", release.version))
                .warning()
                .description(phase_description)
                .build(),
        );
        system_panel = system_panel.push(
            text(format!(
                "Selected action: {}",
                super::super::selected_update_action(app).description()
            ))
            .size(12),
        );
        system_panel = system_panel.push(text(status_update_detail_summary(app)).size(12));
        system_panel = system_panel.push(update_action_controls(app));
    } else if let Some(error) = &app.update_state.last_error {
        system_panel = system_panel.push(
            banner("Update check failed")
                .warning()
                .description(format!("{} issue: {}", error.kind.label(), error.detail))
                .build(),
        );
    }

    if let Some(session) = &app.active_session {
        system_panel = system_panel.push(
            card()
                .title("Active Session")
                .body(
                    row![
                        status_badge(&session.character_name, BadgeTone::Primary),
                        status_badge(format!("ID: {}", session.character_id), BadgeTone::Outline,),
                        status_badge(
                            format!(
                                "Since {}",
                                super::clips::format_timestamp(session.started_at)
                            ),
                            BadgeTone::Outline,
                        ),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                )
                .width(Length::Fill),
        );
    }

    if let Some(line) = app.startup_probe_status_line() {
        system_panel = system_panel.push(text(line).size(13));
    }

    let mut body = column![system_panel.build()].spacing(16);

    // Session summary panel
    if let Some(summary) = &app.last_session_summary {
        let mut summary_panel = panel("Last Session Summary");

        summary_panel = summary_panel.push(
            row![
                status_badge(format!("{} clips", summary.total_clips), BadgeTone::Primary,),
                status_badge(
                    format!("{}s captured", summary.total_duration_secs),
                    BadgeTone::Info,
                ),
                status_badge(
                    format!("{} bases", summary.unique_bases),
                    BadgeTone::Outline,
                ),
                iced::widget::Space::new().width(Length::Fill),
                styled_button("Export Markdown", ButtonTone::Secondary)
                    .on_press(Message::ExportLastSessionSummary),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        );

        if let Some(top_clip) = &summary.top_clip {
            summary_panel = summary_panel.push(
                card()
                    .title("Top Clip")
                    .body(
                        row![
                            status_badge(format!("{} points", top_clip.score), BadgeTone::Success,),
                            status_badge(&top_clip.rule_id, BadgeTone::Outline),
                            status_badge(
                                super::clips::format_timestamp(top_clip.trigger_event_at),
                                BadgeTone::Outline,
                            ),
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center),
                    )
                    .width(Length::Fill),
            );
        }

        if !summary.rule_breakdown.is_empty() {
            let rules_text = summary
                .rule_breakdown
                .iter()
                .map(|item| format!("{} x{}", item.label, item.count))
                .collect::<Vec<_>>()
                .join(", ");
            summary_panel = summary_panel.push(text(format!("Rules: {rules_text}")).size(12));
        }
        if !summary.base_breakdown.is_empty() {
            let bases_text = summary
                .base_breakdown
                .iter()
                .map(|item| format!("{} x{}", item.label, item.count))
                .collect::<Vec<_>>()
                .join(", ");
            summary_panel = summary_panel.push(text(format!("Bases: {bases_text}")).size(12));
        }

        body = body.push(summary_panel.build());
    }

    // Background jobs panel
    let active_jobs = app.background_jobs.active_jobs();
    let recent_jobs = app.background_jobs.recent_jobs();

    let jobs_content: Element<'_, Message> = if active_jobs.is_empty() && recent_jobs.is_empty() {
        empty_state("No background jobs")
            .description("Background jobs like montage rendering and uploads will appear here.")
            .build()
            .into()
    } else {
        let mut rows: Vec<Element<'_, Message>> = active_jobs
            .iter()
            .map(|job| background_job_row(job, true))
            .collect();
        rows.extend(recent_jobs.iter().map(|job| background_job_row(job, false)));
        background_job_table(rows)
    };

    body = body.push(panel("Background Jobs").push(jobs_content).build());

    column![
        header,
        status_bar,
        scrollable(container(body).width(Length::Fill)).height(Length::Fill),
    ]
    .spacing(12)
    .into()
}

fn status_badge<'a>(label: impl Into<String>, tone: BadgeTone) -> Element<'a, Message> {
    badge(label).tone(tone).build().into()
}

fn status_update_detail_summary(app: &App) -> String {
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
    let apply_summary = app
        .update_state
        .last_apply_report
        .as_ref()
        .map(|report| {
            format!(
                "Last apply {} {}.",
                match report.status {
                    crate::update::UpdateApplyReportStatus::Succeeded => "succeeded for",
                    crate::update::UpdateApplyReportStatus::Failed => "failed for",
                },
                report.target_version
            )
        })
        .unwrap_or_else(|| "No apply result recorded yet.".into());

    if let Some(signature) = signature {
        let key_id = signature.key_id.as_deref().unwrap_or("not reported");
        let key_label = signature.key_label.as_deref().unwrap_or("not reported");
        format!(
            "Signed by `{key_id}` ({key_label}). Embedded verifier keys: {verifier_key_count}. {apply_summary}"
        )
    } else {
        format!("Embedded verifier keys: {verifier_key_count}. {apply_summary}")
    }
}

fn ffmpeg_status_label(app: &App) -> &'static str {
    if !app.ffmpeg_capabilities.present {
        "ffmpeg missing"
    } else if !app.ffmpeg_capabilities.meets_floor {
        "ffmpeg too old"
    } else if app.ffmpeg_capabilities.warning.is_some() {
        "ffmpeg warning"
    } else {
        "ffmpeg ready"
    }
}

fn ffmpeg_status_tone(app: &App) -> BadgeTone {
    if !app.ffmpeg_capabilities.present {
        BadgeTone::Neutral
    } else if !app.ffmpeg_capabilities.meets_floor {
        BadgeTone::Destructive
    } else if app.ffmpeg_capabilities.warning.is_some() {
        BadgeTone::Warning
    } else {
        BadgeTone::Success
    }
}

fn background_job_table<'a>(rows: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    column![
        background_job_header(),
        scrollable(Column::with_children(rows).spacing(4))
            .height(Length::Fixed(JOB_TABLE_HEIGHT))
            .width(Length::Fill)
    ]
    .spacing(6)
    .into()
}

fn background_job_header<'a>() -> Element<'a, Message> {
    container(
        row![
            container(text("Job ID").size(12)).width(Length::Fixed(JOB_TABLE_ID_WIDTH)),
            container(text("Kind").size(12)).width(Length::Fixed(JOB_TABLE_KIND_WIDTH)),
            container(text("Job").size(12)).width(Length::FillPortion(3)),
            container(text("State").size(12)).width(Length::Fixed(JOB_TABLE_STATE_WIDTH)),
            container(text("Clips").size(12)).width(Length::Fixed(JOB_TABLE_CLIPS_WIDTH)),
            container(text("Updated").size(12)).width(Length::Fixed(JOB_TABLE_UPDATED_WIDTH)),
            container(text("Detail").size(12)).width(Length::FillPortion(4)),
            container(text("Actions").size(12)).width(Length::Fixed(JOB_TABLE_ACTIONS_WIDTH)),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center),
    )
    .padding([8, 10])
    .width(Length::Fill)
    .style(job_header_style)
    .into()
}

fn background_job_row<'a>(job: &BackgroundJobRecord, active: bool) -> Element<'a, Message> {
    let mut actions = row![].spacing(6).align_y(iced::Alignment::Center);
    if active {
        if job.cancellable {
            actions = actions.push(
                styled_button("Cancel", ButtonTone::Danger)
                    .on_press(Message::CancelBackgroundJob(job.id)),
            );
        }
    } else {
        if job.state == BackgroundJobState::Failed {
            actions = actions.push(
                styled_button("Retry", ButtonTone::Warning)
                    .on_press(Message::RetryBackgroundJob(job.id)),
            );
        }
        actions = actions.push(
            styled_button("Remove", ButtonTone::Secondary)
                .on_press(Message::RemoveBackgroundJob(job.id)),
        );
    }

    let state_badge = match job.state {
        BackgroundJobState::Running => {
            status_badge(background_job_state_label(job), BadgeTone::Primary)
        }
        BackgroundJobState::Succeeded => {
            status_badge(background_job_state_label(job), BadgeTone::Success)
        }
        BackgroundJobState::Failed => {
            status_badge(background_job_state_label(job), BadgeTone::Destructive)
        }
        _ => status_badge(background_job_state_label(job), BadgeTone::Neutral),
    };

    container(
        row![
            container(text(job.id.to_string()).size(12)).width(Length::Fixed(JOB_TABLE_ID_WIDTH)),
            container(text(job.kind.label()).size(12)).width(Length::Fixed(JOB_TABLE_KIND_WIDTH)),
            container(text(job.label.clone()).size(12)).width(Length::FillPortion(3)),
            container(state_badge).width(Length::Fixed(JOB_TABLE_STATE_WIDTH)),
            container(text(background_job_related_clips_label(job)).size(12))
                .width(Length::Fixed(JOB_TABLE_CLIPS_WIDTH)),
            container(text(super::clips::format_timestamp(job.updated_at)).size(12))
                .width(Length::Fixed(JOB_TABLE_UPDATED_WIDTH)),
            container(text(background_job_detail_label(job, active)).size(12))
                .width(Length::FillPortion(4)),
            container(actions).width(Length::Fixed(JOB_TABLE_ACTIONS_WIDTH)),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center),
    )
    .padding([8, 10])
    .width(Length::Fill)
    .style(if active {
        job_active_row_style
    } else {
        job_row_style
    })
    .into()
}

fn job_header_style(theme: &iced::Theme) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    iced::widget::container::Style {
        text_color: Some(c.muted_foreground),
        background: Some(Background::Color(c.muted)),
        border: theme::border(c.border, 1.0, tokens.radius.md),
        ..Default::default()
    }
}

fn job_row_style(theme: &iced::Theme) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    iced::widget::container::Style {
        text_color: Some(c.card_foreground),
        background: Some(Background::Color(c.card)),
        border: theme::border(c.border, 1.0, tokens.radius.md),
        ..Default::default()
    }
}

fn job_active_row_style(theme: &iced::Theme) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    iced::widget::container::Style {
        text_color: Some(c.card_foreground),
        background: Some(Background::Color(theme::with_alpha(c.success, 0.08))),
        border: theme::border(theme::with_alpha(c.success, 0.3), 1.0, tokens.radius.md),
        ..Default::default()
    }
}

fn background_job_state_label(job: &BackgroundJobRecord) -> String {
    job.progress
        .as_ref()
        .map(|progress| {
            format!(
                "{} {}/{}",
                job.state.label(),
                progress.current_step,
                progress.total_steps
            )
        })
        .unwrap_or_else(|| job.state.label().into())
}

fn background_job_related_clips_label(job: &BackgroundJobRecord) -> String {
    if job.related_clip_ids.is_empty() {
        "All".into()
    } else if job.related_clip_ids.len() <= 3 {
        job.related_clip_ids
            .iter()
            .map(|clip_id| format!("#{clip_id}"))
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        format!(
            "#{}, +{} more",
            job.related_clip_ids[0],
            job.related_clip_ids.len() - 1
        )
    }
}

fn background_job_detail_label(job: &BackgroundJobRecord, active: bool) -> String {
    if active && let Some(progress) = &job.progress {
        return progress.message.clone();
    }

    job.detail
        .clone()
        .filter(|detail| !detail.trim().is_empty())
        .unwrap_or_else(|| job.label.clone())
}
