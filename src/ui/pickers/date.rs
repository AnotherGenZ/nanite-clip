//! Date picker — calendar month grid.
//!
//! The widget is stateless: the app owns the currently displayed month
//! (as a `NaiveDate` pointing at day 1 of that month) and the currently
//! selected date. `date_picker` returns an [`Element`] that renders a
//! prev/next month header followed by a 7-column weekday grid.

use chrono::{Datelike, Duration, NaiveDate};
use iced::widget::{Column, Row, button, column, container, row};
use iced::{Background, Element, Length, Padding};

use crate::ui::primitives::label::text_non_selectable as text;
use crate::ui::theme::{self, Tokens, border};

/// Render a month-grid date picker.
///
/// * `month` — any `NaiveDate` inside the month to display (only year +
///   month are consumed).
/// * `selected` — the currently selected date, if any.
/// * `on_select` — emitted when the user clicks a day.
/// * `on_change_month` — emitted when the user clicks the prev/next arrow;
///   the caller updates `month` in response.
pub fn date_picker<'a, Message, OnSelect, OnMonth>(
    month: NaiveDate,
    selected: Option<NaiveDate>,
    on_select: OnSelect,
    on_change_month: OnMonth,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
    OnSelect: Fn(NaiveDate) -> Message + Copy + 'a,
    OnMonth: Fn(NaiveDate) -> Message + Copy + 'a,
{
    let space = &theme::SPACE;
    let font = &theme::FONT;

    let first = NaiveDate::from_ymd_opt(month.year(), month.month(), 1).unwrap_or(month);

    let header = header_row::<Message, OnMonth>(first, on_change_month, space, font);

    let weekday_row = weekday_row::<Message>(space, font);
    let grid = grid::<Message, OnSelect>(first, selected, on_select, space, font);

    let body: Column<'a, Message> = column![header, weekday_row, grid].spacing(space.xs);

    container(body)
        .padding(Padding {
            top: space.md,
            bottom: space.md,
            left: space.md,
            right: space.md,
        })
        .width(Length::Fixed(280.0))
        .style(card_style)
        .into()
}

fn header_row<'a, Message, OnMonth>(
    first: NaiveDate,
    on_change_month: OnMonth,
    space: &crate::ui::theme::SpaceTokens,
    font: &crate::ui::theme::FontTokens,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
    OnMonth: Fn(NaiveDate) -> Message + Copy + 'a,
{
    let prev = shift_month(first, -1);
    let next = shift_month(first, 1);

    let prev_btn = button(
        container(
            text("\u{2039}")
                .size(font.size_base)
                .style(|theme: &iced::Theme| iced::widget::text::Style {
                    color: Some(theme::tokens_for(theme).color.foreground),
                }),
        )
        .padding(Padding {
            top: 2.0,
            bottom: 2.0,
            left: space.xs,
            right: space.xs,
        }),
    )
    .padding(0)
    .style(nav_btn_style)
    .on_press(on_change_month(prev));

    let next_btn = button(
        container(
            text("\u{203A}")
                .size(font.size_base)
                .style(|theme: &iced::Theme| iced::widget::text::Style {
                    color: Some(theme::tokens_for(theme).color.foreground),
                }),
        )
        .padding(Padding {
            top: 2.0,
            bottom: 2.0,
            left: space.xs,
            right: space.xs,
        }),
    )
    .padding(0)
    .style(nav_btn_style)
    .on_press(on_change_month(next));

    let label = text(format!("{} {}", month_name(first.month()), first.year()))
        .size(font.size_base)
        .style(|theme: &iced::Theme| iced::widget::text::Style {
            color: Some(theme::tokens_for(theme).color.foreground),
        });

    row![
        prev_btn,
        container(label)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Center),
        next_btn,
    ]
    .align_y(iced::Alignment::Center)
    .spacing(space.xs)
    .into()
}

fn weekday_row<'a, Message: 'a>(
    space: &crate::ui::theme::SpaceTokens,
    font: &crate::ui::theme::FontTokens,
) -> Element<'a, Message> {
    let labels = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
    let mut r: Row<'a, Message> = Row::new().spacing(space.xxs);
    for label in labels {
        let cell = container(text(label).size(font.size_xs).style(|theme: &iced::Theme| {
            iced::widget::text::Style {
                color: Some(theme::tokens_for(theme).color.muted_foreground),
            }
        }))
        .width(Length::Fixed(36.0))
        .align_x(iced::alignment::Horizontal::Center);
        r = r.push(cell);
    }
    r.into()
}

fn grid<'a, Message, OnSelect>(
    first: NaiveDate,
    selected: Option<NaiveDate>,
    on_select: OnSelect,
    space: &crate::ui::theme::SpaceTokens,
    font: &crate::ui::theme::FontTokens,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
    OnSelect: Fn(NaiveDate) -> Message + Copy + 'a,
{
    let weekday_from_mon = first.weekday().num_days_from_monday() as i64;
    let grid_start = first - Duration::days(weekday_from_mon);

    let today = chrono::Local::now().date_naive();
    let mut col: Column<'a, Message> = Column::new().spacing(space.xxs);

    for week in 0..6 {
        let mut r: Row<'a, Message> = Row::new().spacing(space.xxs);
        for day in 0..7 {
            let date = grid_start + Duration::days(week * 7 + day);
            let in_month = date.month() == first.month();
            let is_today = date == today;
            let is_selected = selected == Some(date);
            let cell = day_cell::<Message, OnSelect>(
                date,
                in_month,
                is_today,
                is_selected,
                on_select,
                font,
            );
            r = r.push(cell);
        }
        col = col.push(r);
    }

    col.into()
}

fn day_cell<'a, Message, OnSelect>(
    date: NaiveDate,
    in_month: bool,
    is_today: bool,
    is_selected: bool,
    on_select: OnSelect,
    font: &crate::ui::theme::FontTokens,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
    OnSelect: Fn(NaiveDate) -> Message + Copy + 'a,
{
    let day = date.day();
    let label = text(day.to_string())
        .size(font.size_sm)
        .style(move |theme: &iced::Theme| iced::widget::text::Style {
            color: Some(day_color(theme, in_month, is_selected)),
        });

    let content = container(label)
        .width(Length::Fixed(36.0))
        .align_x(iced::alignment::Horizontal::Center)
        .padding(Padding {
            top: 6.0,
            bottom: 6.0,
            left: 0.0,
            right: 0.0,
        });

    button(content)
        .padding(0)
        .style(move |theme, status| day_style(theme, status, in_month, is_today, is_selected))
        .on_press(on_select(date))
        .into()
}

fn day_color(theme: &iced::Theme, in_month: bool, is_selected: bool) -> iced::Color {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    if is_selected {
        c.primary_foreground
    } else if in_month {
        c.foreground
    } else {
        c.muted_foreground
    }
}

fn day_style(
    theme: &iced::Theme,
    status: iced::widget::button::Status,
    _in_month: bool,
    is_today: bool,
    is_selected: bool,
) -> iced::widget::button::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;

    let (bg, border_color) = if is_selected {
        (Some(Background::Color(c.primary)), c.primary)
    } else if is_today {
        (None, c.primary)
    } else {
        match status {
            iced::widget::button::Status::Hovered => {
                (Some(Background::Color(c.muted)), iced::Color::TRANSPARENT)
            }
            iced::widget::button::Status::Pressed => {
                (Some(Background::Color(c.accent)), iced::Color::TRANSPARENT)
            }
            _ => (None, iced::Color::TRANSPARENT),
        }
    };

    iced::widget::button::Style {
        background: bg,
        text_color: c.foreground,
        border: border(
            border_color,
            if is_today || is_selected { 1.0 } else { 0.0 },
            tokens.radius.sm,
        ),
        shadow: Default::default(),
        snap: false,
    }
}

fn nav_btn_style(
    theme: &iced::Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    let bg = match status {
        iced::widget::button::Status::Hovered => Some(Background::Color(c.muted)),
        iced::widget::button::Status::Pressed => Some(Background::Color(c.accent)),
        _ => None,
    };
    iced::widget::button::Style {
        background: bg,
        text_color: c.foreground,
        border: border(iced::Color::TRANSPARENT, 0.0, tokens.radius.sm),
        shadow: Default::default(),
        snap: false,
    }
}

fn card_style(theme: &iced::Theme) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    iced::widget::container::Style {
        text_color: Some(c.popover_foreground),
        background: Some(Background::Color(c.popover)),
        border: border(c.border_strong, 1.0, tokens.radius.lg),
        shadow: tokens.shadow.lg,
        snap: false,
    }
}

fn shift_month(date: NaiveDate, delta: i32) -> NaiveDate {
    let mut y = date.year();
    let mut m = date.month() as i32 + delta;
    while m > 12 {
        m -= 12;
        y += 1;
    }
    while m < 1 {
        m += 12;
        y -= 1;
    }
    NaiveDate::from_ymd_opt(y, m as u32, 1).unwrap_or(date)
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}
