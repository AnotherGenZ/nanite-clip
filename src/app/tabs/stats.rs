use std::time::Instant;

use chrono::Utc;
use iced::{Background, Color, Element, Length, Task};

use crate::db::{ClipStatsSnapshot, CountByLabel};
use crate::ui::app::{
    Column, ContainerStyle, column, container, row, scrollable, text, text_non_selectable,
};
use crate::ui::layout::empty_state::empty_state;
use crate::ui::layout::page_header::page_header;
use crate::ui::layout::panel::panel;
use crate::ui::layout::stat::stat;
use crate::ui::layout::toolbar::toolbar;
use crate::ui::overlay::banner::banner;
use crate::ui::primitives::badge::{Tone as BadgeTone, badge};
use crate::ui::theme::{self, Tokens};

use super::super::shared::{ButtonTone, styled_button, with_tooltip};
use super::super::{App, Message as AppMessage, View};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const STAT_ROW_HEIGHT: f32 = 36.0;
const STAT_HEADER_HEIGHT: f32 = 32.0;
const COUNT_COLUMN_WIDTH: f32 = 72.0;
const PCT_COLUMN_WIDTH: f32 = 56.0;
const BAR_WIDTH_PORTIONS: u16 = 100;
const MAX_VISIBLE_ROWS: usize = 8;

// ---------------------------------------------------------------------------
// Public types shared with app.rs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatsTimeRange {
    Last7Days,
    Last30Days,
    #[default]
    AllTime,
}

impl StatsTimeRange {
    pub const ALL: &[Self] = &[Self::Last7Days, Self::Last30Days, Self::AllTime];

    pub fn label(self) -> &'static str {
        match self {
            Self::Last7Days => "Last 7 days",
            Self::Last30Days => "Last 30 days",
            Self::AllTime => "All time",
        }
    }

    pub fn since_timestamp_ms(self) -> Option<i64> {
        let days = match self {
            Self::Last7Days => 7,
            Self::Last30Days => 30,
            Self::AllTime => return None,
        };
        Some((Utc::now() - chrono::Duration::days(days)).timestamp_millis())
    }
}

impl std::fmt::Display for StatsTimeRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsSection {
    ClipsPerDay,
    ClipsPerRule,
    ScoreDistribution,
    TopBases,
    TopWeapons,
    TopTargets,
    RawEventKinds,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Message {
    Refresh,
    TimeRangeChanged(StatsTimeRange),
    ToggleSection(StatsSection),
    ExportStats,
    NavigateToClipsWithRule(String),
    NavigateToClipsWithBase(String),
    NavigateToClipsWithTarget(String),
    NavigateToClipsWithWeapon(String),
    /// Navigate to clips filtered to a specific day (label format: "YYYY-MM-DD").
    NavigateToClipsOnDay(String),
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

pub(in crate::app) fn update(app: &mut App, message: Message) -> Task<AppMessage> {
    match message {
        Message::Refresh => app.load_stats(),
        Message::TimeRangeChanged(range) => {
            app.stats.time_range = range;
            app.load_stats()
        }
        Message::ToggleSection(section) => {
            if app.stats.collapsed_sections.contains(&section) {
                app.stats.collapsed_sections.retain(|s| *s != section);
            } else {
                app.stats.collapsed_sections.push(section);
            }
            Task::none()
        }
        Message::ExportStats => {
            if let Some(snapshot) = &app.stats.snapshot {
                let md = export_stats_markdown(snapshot, app.stats.time_range);
                let save_dir = app.config.recorder.save_directory.clone();
                Task::perform(
                    async move {
                        let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                        let path = save_dir.join(format!("stats_{ts}.md"));
                        tokio::fs::write(&path, md)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok::<String, String>(path.display().to_string())
                    },
                    |result| match result {
                        Ok(path) => AppMessage::StatsExported(Ok(path)),
                        Err(e) => AppMessage::StatsExported(Err(e)),
                    },
                )
            } else {
                Task::none()
            }
        }
        Message::NavigateToClipsWithRule(rule) => {
            app.clips.filters.rule = rule;
            navigate_to_filtered_clips(app)
        }
        Message::NavigateToClipsWithBase(base) => {
            app.clips.filters.base = base;
            navigate_to_filtered_clips(app)
        }
        Message::NavigateToClipsWithTarget(target) => {
            app.clips.filters.target = target;
            navigate_to_filtered_clips(app)
        }
        Message::NavigateToClipsWithWeapon(weapon) => {
            app.clips.filters.weapon = weapon;
            navigate_to_filtered_clips(app)
        }
        Message::NavigateToClipsOnDay(day_label) => {
            // day_label is "YYYY-MM-DD" in local time. Parse to start/end-of-day
            // timestamps (ms) in UTC so the Clips date filter works correctly.
            if let Ok(date) = chrono::NaiveDate::parse_from_str(&day_label, "%Y-%m-%d") {
                let local_tz = chrono::Local::now().timezone();
                let start = date
                    .and_hms_opt(0, 0, 0)
                    .and_then(|dt| dt.and_local_timezone(local_tz).single())
                    .map(|dt| dt.timestamp_millis());
                let end = date
                    .and_hms_opt(23, 59, 59)
                    .and_then(|dt| dt.and_local_timezone(local_tz).single())
                    .map(|dt| dt.timestamp_millis());
                app.clips.filters.event_after_ts = start;
                app.clips.filters.event_before_ts = end;
                app.clips.date_range_preset = super::clips::DateRangePreset::Custom;
                app.clips.date_range_start = day_label.clone();
                app.clips.date_range_end = day_label;
                app.clips.active_calendar = None;
            }
            navigate_to_filtered_clips(app)
        }
    }
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

pub(in crate::app) fn view(app: &App) -> Element<'_, Message> {
    let header = page_header("Stats")
        .action(
            row![
                with_tooltip(
                    styled_button("Export", ButtonTone::Secondary)
                        .on_press(Message::ExportStats)
                        .into(),
                    "Copy a markdown summary to the clipboard.",
                ),
                with_tooltip(
                    styled_button("Refresh", ButtonTone::Secondary)
                        .on_press(Message::Refresh)
                        .into(),
                    "Reload from the database.",
                ),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        );

    // Time-range toolbar
    let mut time_bar = toolbar();
    for &range in StatsTimeRange::ALL {
        let tone = if app.stats.time_range == range {
            ButtonTone::Primary
        } else {
            ButtonTone::Secondary
        };
        time_bar = time_bar
            .push(styled_button(range.label(), tone).on_press(Message::TimeRangeChanged(range)));
    }

    // Staleness indicator
    let staleness_badge = staleness_indicator(app.stats.last_refreshed_at);
    let loading_badge: Option<Element<'_, Message>> = if app.stats.loading {
        Some(badge("Loading...").tone(BadgeTone::Info).build().into())
    } else {
        None
    };
    if let Some(b) = staleness_badge {
        time_bar = time_bar.trailing(b);
    }
    if let Some(b) = loading_badge {
        time_bar = time_bar.trailing(b);
    }

    let mut content = column![header, time_bar.build()].spacing(12);

    // Error state
    if let Some(error) = &app.stats.error {
        content = content.push(
            banner(format!("Failed to load stats: {error}"))
                .error()
                .build(),
        );
    }

    if app.stats.loading && app.stats.snapshot.is_none() {
        content = content.push(text("Loading stats...").size(13));
    } else if let Some(snapshot) = &app.stats.snapshot {
        content = content.push(summary_row(snapshot));

        // Clips per day — chronological order
        if !section_is_collapsed(app, StatsSection::ClipsPerDay) {
            let mut days = snapshot.clips_per_day.clone();
            days.reverse();
            content = content.push(histogram_panel(
                "Clips per day",
                "",
                StatsSection::ClipsPerDay,
                app,
                "Day",
                "Clips",
                120.0,
                &days,
                snapshot.total_clips,
                BarColor::Primary,
                Some(ClickAction::Day),
            ));
        } else {
            content = content.push(collapsed_panel_header(
                "Clips per day",
                StatsSection::ClipsPerDay,
            ));
        }

        // Clips per rule
        if !section_is_collapsed(app, StatsSection::ClipsPerRule) {
            content = content.push(histogram_panel(
                "Clips per rule",
                "",
                StatsSection::ClipsPerRule,
                app,
                "Rule",
                "Clips",
                180.0,
                &snapshot.clips_per_rule,
                snapshot.total_clips,
                BarColor::Success,
                Some(ClickAction::Rule),
            ));
        } else {
            content = content.push(collapsed_panel_header(
                "Clips per rule",
                StatsSection::ClipsPerRule,
            ));
        }

        // Score distribution
        if !section_is_collapsed(app, StatsSection::ScoreDistribution) {
            content = content.push(histogram_panel(
                "Score distribution",
                "",
                StatsSection::ScoreDistribution,
                app,
                "Range",
                "Clips",
                88.0,
                &snapshot.score_distribution,
                snapshot.total_clips,
                BarColor::Warm,
                None,
            ));
        } else {
            content = content.push(collapsed_panel_header(
                "Score distribution",
                StatsSection::ScoreDistribution,
            ));
        }

        // Top bases
        if !section_is_collapsed(app, StatsSection::TopBases) {
            let base_rows: Vec<CountByLabel> = snapshot
                .top_bases
                .iter()
                .map(|b| CountByLabel {
                    label: b.label.clone(),
                    count: b.count,
                })
                .collect();
            content = content.push(histogram_panel(
                "Top bases",
                "",
                StatsSection::TopBases,
                app,
                "Base",
                "Clips",
                180.0,
                &base_rows,
                snapshot.total_clips,
                BarColor::Info,
                Some(ClickAction::Base),
            ));
        } else {
            content = content.push(collapsed_panel_header("Top bases", StatsSection::TopBases));
        }

        // Top weapons
        if !section_is_collapsed(app, StatsSection::TopWeapons) {
            content = content.push(histogram_panel(
                "Top weapons",
                "",
                StatsSection::TopWeapons,
                app,
                "Weapon",
                "Uses",
                180.0,
                &snapshot.top_weapons,
                0, // no meaningful total for event-level counts
                BarColor::Warning,
                Some(ClickAction::Weapon),
            ));
        } else {
            content = content.push(collapsed_panel_header(
                "Top weapons",
                StatsSection::TopWeapons,
            ));
        }

        // Top targets
        if !section_is_collapsed(app, StatsSection::TopTargets) {
            content = content.push(histogram_panel(
                "Top targets",
                "",
                StatsSection::TopTargets,
                app,
                "Character",
                "Kills",
                180.0,
                &snapshot.top_targets,
                0,
                BarColor::Destructive,
                Some(ClickAction::Target),
            ));
        } else {
            content = content.push(collapsed_panel_header(
                "Top targets",
                StatsSection::TopTargets,
            ));
        }

        // Raw event kinds — collapsed by default (advanced)
        let raw_events_collapsed = section_is_collapsed(app, StatsSection::RawEventKinds);
        if !raw_events_collapsed {
            content = content.push(histogram_panel(
                "Raw event kinds (advanced)",
                "For debugging rules.",
                StatsSection::RawEventKinds,
                app,
                "Event kind",
                "Count",
                180.0,
                &snapshot.raw_event_kinds,
                0,
                BarColor::Muted,
                None,
            ));
        } else {
            content = content.push(collapsed_panel_header(
                "Raw event kinds (advanced)",
                StatsSection::RawEventKinds,
            ));
        }
    } else if !app.stats.loading {
        content = content.push(
            empty_state("No stats yet.")
                .description("Clips will populate this.")
                .build(),
        );
    }

    scrollable(container(content).width(Length::Fill))
        .height(Length::Fill)
        .into()
}

// ---------------------------------------------------------------------------
// Summary KPI row
// ---------------------------------------------------------------------------

fn summary_row(snapshot: &ClipStatsSnapshot) -> Element<'static, Message> {
    let avg_duration = if snapshot.total_clips > 0 {
        format_duration_human(snapshot.total_duration_secs / snapshot.total_clips as u64)
    } else {
        "—".into()
    };

    let avg_score = if snapshot.total_clips > 0 {
        format!(
            "{:.1}",
            snapshot.total_score_sum as f64 / snapshot.total_clips as f64
        )
    } else {
        "—".into()
    };

    let unique_rules = snapshot.clips_per_rule.len().to_string();

    let most_active_day = snapshot
        .clips_per_day
        .iter()
        .max_by_key(|d| d.count)
        .map(|d| format!("{} ({})", d.label, d.count))
        .unwrap_or_else(|| "—".into());

    row![
        summary_card("Total clips", snapshot.total_clips.to_string()),
        summary_card(
            "Total duration",
            format_duration_human(snapshot.total_duration_secs),
        ),
        summary_card("Avg duration", avg_duration),
        summary_card("Avg score", avg_score),
        summary_card("Rules active", unique_rules),
        summary_card("Best day", most_active_day),
    ]
    .spacing(12)
    .into()
}

fn summary_card(label: impl Into<String>, value: impl Into<String>) -> Element<'static, Message> {
    stat(label.into(), value.into())
        .width(Length::FillPortion(1))
        .into()
}

// ---------------------------------------------------------------------------
// Collapsible section helpers
// ---------------------------------------------------------------------------

fn section_is_collapsed(app: &App, section: StatsSection) -> bool {
    app.stats.collapsed_sections.contains(&section)
}

fn collapsed_panel_header(title: &str, section: StatsSection) -> Element<'static, Message> {
    let title_owned = title.to_string();
    iced::widget::button(
        container(
            row![
                text(format!("\u{25B8}  {title_owned}"))
                    .size(16)
                    .style(|theme: &iced::Theme| iced::widget::text::Style {
                        color: Some(theme::tokens_for(theme).color.foreground),
                    }),
                iced::widget::Space::new().width(Length::Fill),
                badge("collapsed").tone(BadgeTone::Neutral).build(),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        )
        .padding([12, 16])
        .width(Length::Fill)
        .style(collapsed_panel_style),
    )
    .padding(0)
    .width(Length::Fill)
    .style(clickable_row_button_style)
    .on_press(Message::ToggleSection(section))
    .into()
}

fn navigate_to_filtered_clips(app: &mut App) -> Task<AppMessage> {
    Task::batch([
        Task::done(AppMessage::SwitchView(View::Clips)),
        super::clips::refresh_history(app),
    ])
}

// ---------------------------------------------------------------------------
// Histogram panel (themed, with percentages and click navigation)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum BarColor {
    Primary,
    Success,
    Info,
    Warning,
    Destructive,
    Warm,
    Muted,
}

#[derive(Debug, Clone, Copy)]
enum ClickAction {
    Rule,
    Base,
    Target,
    Weapon,
    Day,
}

#[allow(clippy::too_many_arguments)]
fn histogram_panel(
    title: &str,
    description: &str,
    section: StatsSection,
    app: &App,
    label_header: &'static str,
    count_header: &'static str,
    label_width: f32,
    rows: &[CountByLabel],
    total_for_pct: u32,
    bar_color: BarColor,
    click_action: Option<ClickAction>,
) -> Element<'static, Message> {
    let _ = app; // used via section_is_collapsed in caller

    let collapse_btn: Element<'static, Message> =
        iced::widget::button(text_non_selectable("\u{25BE}").size(14).style(
            |theme: &iced::Theme| iced::widget::text::Style {
                color: Some(theme::tokens_for(theme).color.muted_foreground),
            },
        ))
        .padding([2, 6])
        .style(clickable_row_button_style)
        .on_press(Message::ToggleSection(section))
        .into();

    let mut p = panel(title).description(description);

    if rows.is_empty() {
        p = p.push(text("No data yet.").size(13));
    } else {
        let show_pct = total_for_pct > 0;
        let max_count = rows.iter().map(|r| r.count).max().unwrap_or(1);

        let header_el = histogram_header_themed(label_header, count_header, label_width, show_pct);

        let items: Vec<Element<'static, Message>> = rows
            .iter()
            .enumerate()
            .map(|(index, r)| {
                histogram_row_themed(
                    r,
                    max_count,
                    total_for_pct,
                    label_width,
                    index,
                    show_pct,
                    bar_color,
                    click_action,
                )
            })
            .collect();

        let body_height = table_body_height(items.len());

        p = p.push(
            column![
                row![collapse_btn, header_el]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                scrollable(Column::with_children(items))
                    .height(Length::Fixed(body_height))
                    .width(Length::Fill),
            ]
            .spacing(4),
        );
    }

    p.build().into()
}

fn histogram_header_themed(
    label_header: &'static str,
    count_header: &'static str,
    label_width: f32,
    show_pct: bool,
) -> Element<'static, Message> {
    let mut r = row![
        container(text(label_header).size(12).style(|theme: &iced::Theme| {
            iced::widget::text::Style {
                color: Some(theme::tokens_for(theme).color.muted_foreground),
            }
        }))
        .width(Length::Fixed(label_width)),
        themed_separator(),
        text("").size(12).width(Length::Fill),
        themed_separator(),
        container(text(count_header).size(12).style(|theme: &iced::Theme| {
            iced::widget::text::Style {
                color: Some(theme::tokens_for(theme).color.muted_foreground),
            }
        }))
        .width(Length::Fixed(COUNT_COLUMN_WIDTH)),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    if show_pct {
        r = r.push(themed_separator());
        r =
            r.push(
                container(text("%").size(12).style(|theme: &iced::Theme| {
                    iced::widget::text::Style {
                        color: Some(theme::tokens_for(theme).color.muted_foreground),
                    }
                }))
                .width(Length::Fixed(PCT_COLUMN_WIDTH)),
            );
    }

    container(r)
        .padding([6, 10])
        .width(Length::Fill)
        .height(Length::Fixed(STAT_HEADER_HEIGHT))
        .style(header_style)
        .into()
}

#[allow(clippy::too_many_arguments)]
fn histogram_row_themed(
    data: &CountByLabel,
    max_count: u32,
    total_for_pct: u32,
    label_width: f32,
    index: usize,
    show_pct: bool,
    bar_color: BarColor,
    click_action: Option<ClickAction>,
) -> Element<'static, Message> {
    let label = data.label.clone();
    let count = data.count;

    let pct_text = if show_pct && total_for_pct > 0 {
        format!("{:.0}%", count as f64 / total_for_pct as f64 * 100.0)
    } else {
        String::new()
    };

    let label_el: Element<'static, Message> = if let Some(action) = click_action {
        let nav_label = label.clone();
        iced::widget::button(text_non_selectable(label.clone()).size(13).style(
            |theme: &iced::Theme| iced::widget::text::Style {
                color: Some(theme::tokens_for(theme).color.primary),
            },
        ))
        .padding(0)
        .style(clickable_row_button_style)
        .on_press(match action {
            ClickAction::Rule => Message::NavigateToClipsWithRule(nav_label),
            ClickAction::Base => Message::NavigateToClipsWithBase(nav_label),
            ClickAction::Target => Message::NavigateToClipsWithTarget(nav_label),
            ClickAction::Weapon => Message::NavigateToClipsWithWeapon(nav_label),
            ClickAction::Day => Message::NavigateToClipsOnDay(nav_label),
        })
        .into()
    } else {
        text(label)
            .size(13)
            .style(|theme: &iced::Theme| iced::widget::text::Style {
                color: Some(theme::tokens_for(theme).color.foreground),
            })
            .into()
    };

    let mut r = row![
        container(label_el).width(Length::Fixed(label_width)),
        themed_separator(),
        histogram_bar_themed(count, max_count, bar_color),
        themed_separator(),
        container(
            text(count.to_string())
                .size(13)
                .style(|theme: &iced::Theme| iced::widget::text::Style {
                    color: Some(theme::tokens_for(theme).color.foreground),
                })
        )
        .width(Length::Fixed(COUNT_COLUMN_WIDTH)),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    if show_pct {
        r = r.push(themed_separator());
        r = r.push(
            container(text(pct_text).size(12).style(|theme: &iced::Theme| {
                iced::widget::text::Style {
                    color: Some(theme::tokens_for(theme).color.muted_foreground),
                }
            }))
            .width(Length::Fixed(PCT_COLUMN_WIDTH)),
        );
    }

    let striped = index % 2 == 1;
    container(r)
        .padding([8, 10])
        .width(Length::Fill)
        .height(Length::Fixed(STAT_ROW_HEIGHT))
        .style(move |theme| row_style(theme, striped))
        .into()
}

fn histogram_bar_themed(
    count: u32,
    max_count: u32,
    bar_color: BarColor,
) -> Element<'static, Message> {
    let fill = histogram_bar_fill_portion(count, max_count);
    let empty = BAR_WIDTH_PORTIONS.saturating_sub(fill);

    let bar_content: Element<'static, Message> = if empty == 0 {
        container(text(""))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |theme: &iced::Theme| ContainerStyle {
                background: Some(Background::Color(theme::with_alpha(
                    bar_accent_color(theme, bar_color),
                    0.78,
                ))),
                ..Default::default()
            })
            .into()
    } else {
        row![
            container(text(""))
                .width(Length::FillPortion(fill))
                .height(Length::Fill)
                .style(move |theme: &iced::Theme| ContainerStyle {
                    background: Some(Background::Color(theme::with_alpha(
                        bar_accent_color(theme, bar_color),
                        0.78,
                    ))),
                    ..Default::default()
                }),
            container(text(""))
                .width(Length::FillPortion(empty))
                .height(Length::Fill)
                .style(|theme: &iced::Theme| {
                    let tokens: &Tokens = theme::tokens_for(theme);
                    ContainerStyle {
                        background: Some(Background::Color(theme::with_alpha(
                            tokens.color.muted,
                            0.3,
                        ))),
                        ..Default::default()
                    }
                }),
        ]
        .into()
    };

    container(bar_content)
        .padding([5, 6])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|theme: &iced::Theme| {
            let tokens: &Tokens = theme::tokens_for(theme);
            ContainerStyle {
                background: Some(Background::Color(theme::with_alpha(
                    tokens.color.background,
                    0.8,
                ))),
                border: iced::border::Border {
                    color: tokens.color.border,
                    width: 1.0,
                    radius: tokens.radius.md.into(),
                },
                ..Default::default()
            }
        })
        .into()
}

// ---------------------------------------------------------------------------
// Staleness indicator
// ---------------------------------------------------------------------------

fn staleness_indicator(last_refreshed: Option<Instant>) -> Option<Element<'static, Message>> {
    let elapsed = last_refreshed?.elapsed();
    let secs = elapsed.as_secs();
    let label = if secs < 60 {
        "Just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else {
        format!("{}h ago", secs / 3600)
    };
    Some(
        badge(format!("Refreshed {label}"))
            .tone(BadgeTone::Outline)
            .build()
            .into(),
    )
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

fn export_stats_markdown(snapshot: &ClipStatsSnapshot, range: StatsTimeRange) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Clip Stats — {}\n\n", range.label()));
    out.push_str(&format!("- **Total clips:** {}\n", snapshot.total_clips));
    out.push_str(&format!(
        "- **Total duration:** {}\n",
        format_duration_human(snapshot.total_duration_secs)
    ));
    if snapshot.total_clips > 0 {
        out.push_str(&format!(
            "- **Avg duration:** {}\n",
            format_duration_human(snapshot.total_duration_secs / snapshot.total_clips as u64)
        ));
        out.push_str(&format!(
            "- **Avg score:** {:.1}\n",
            snapshot.total_score_sum as f64 / snapshot.total_clips as f64
        ));
    }
    out.push_str(&format!(
        "- **Rules active:** {}\n",
        snapshot.clips_per_rule.len()
    ));
    out.push('\n');

    if !snapshot.clips_per_rule.is_empty() {
        out.push_str("## Clips per rule\n\n");
        out.push_str("| Rule | Count | % |\n|---|---:|---:|\n");
        for r in &snapshot.clips_per_rule {
            let pct = if snapshot.total_clips > 0 {
                format!(
                    "{:.0}%",
                    r.count as f64 / snapshot.total_clips as f64 * 100.0
                )
            } else {
                "—".into()
            };
            out.push_str(&format!("| {} | {} | {} |\n", r.label, r.count, pct));
        }
        out.push('\n');
    }

    if !snapshot.top_bases.is_empty() {
        out.push_str("## Top bases\n\n");
        out.push_str("| Base | Count |\n|---|---:|\n");
        for b in &snapshot.top_bases {
            out.push_str(&format!("| {} | {} |\n", b.label, b.count));
        }
        out.push('\n');
    }

    if !snapshot.top_weapons.is_empty() {
        out.push_str("## Top weapons\n\n");
        out.push_str("| Weapon | Uses |\n|---|---:|\n");
        for w in &snapshot.top_weapons {
            out.push_str(&format!("| {} | {} |\n", w.label, w.count));
        }
        out.push('\n');
    }

    if !snapshot.top_targets.is_empty() {
        out.push_str("## Top targets\n\n");
        out.push_str("| Character | Kills |\n|---|---:|\n");
        for t in &snapshot.top_targets {
            out.push_str(&format!("| {} | {} |\n", t.label, t.count));
        }
        out.push('\n');
    }

    out
}

// ---------------------------------------------------------------------------
// Theme-aware styles
// ---------------------------------------------------------------------------

fn bar_accent_color(theme: &iced::Theme, bar_color: BarColor) -> Color {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    match bar_color {
        BarColor::Primary => c.primary,
        BarColor::Success => c.success,
        BarColor::Info => c.info,
        BarColor::Warning => c.warning,
        BarColor::Destructive => c.destructive,
        BarColor::Warm => Color::from_rgb(0.9, 0.55, 0.25),
        BarColor::Muted => c.muted_foreground,
    }
}

fn header_style(theme: &iced::Theme) -> ContainerStyle {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    ContainerStyle {
        text_color: Some(c.muted_foreground),
        background: Some(Background::Color(c.muted)),
        border: iced::border::Border {
            width: 1.0,
            color: c.border,
            radius: tokens.radius.sm.into(),
        },
        ..Default::default()
    }
}

fn row_style(theme: &iced::Theme, striped: bool) -> ContainerStyle {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    let background = if striped {
        Some(Background::Color(c.muted))
    } else {
        Some(Background::Color(theme::with_alpha(c.card, 0.5)))
    };
    ContainerStyle {
        text_color: Some(c.foreground),
        background,
        border: iced::border::Border {
            width: 0.0,
            color: c.border,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn collapsed_panel_style(theme: &iced::Theme) -> ContainerStyle {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    ContainerStyle {
        text_color: Some(c.foreground),
        background: Some(Background::Color(c.background)),
        border: theme::border(c.border, 1.0, tokens.radius.lg),
        ..Default::default()
    }
}

fn clickable_row_button_style(
    theme: &iced::Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;

    let bg = match status {
        iced::widget::button::Status::Hovered => Some(Background::Color(c.accent)),
        iced::widget::button::Status::Pressed => Some(Background::Color(c.muted)),
        _ => None,
    };

    iced::widget::button::Style {
        background: bg,
        text_color: c.foreground,
        border: theme::border(Color::TRANSPARENT, 0.0, 0.0),
        shadow: Default::default(),
        snap: false,
    }
}

fn themed_separator() -> Element<'static, Message> {
    container(text(""))
        .width(Length::Fixed(1.0))
        .height(Length::Fill)
        .style(|theme: &iced::Theme| {
            let tokens: &Tokens = theme::tokens_for(theme);
            ContainerStyle {
                background: Some(Background::Color(tokens.color.border)),
                ..Default::default()
            }
        })
        .into()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn histogram_bar_fill_portion(count: u32, max_count: u32) -> u16 {
    if max_count == 0 {
        return 1;
    }
    let scaled = ((count as f32 / max_count as f32) * BAR_WIDTH_PORTIONS as f32).round() as u16;
    scaled.clamp(1, BAR_WIDTH_PORTIONS)
}

fn table_body_height(row_count: usize) -> f32 {
    let visible_rows = row_count.clamp(1, MAX_VISIBLE_ROWS);
    visible_rows as f32 * STAT_ROW_HEIGHT
}

fn format_duration_human(total_seconds: u64) -> String {
    let days = total_seconds / 86_400;
    let hours = (total_seconds % 86_400) / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;

    if days > 0 {
        format!("{days}d {hours}h {minutes}m {seconds}s")
    } else if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}
