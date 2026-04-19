//! Pagination widget — prev/next and numbered page buttons.
//!
//! Ellipsizes when there are too many pages to show inline. Caller owns the
//! current page and receives a message on each selection.

use iced::widget::{Row, button, container};
use iced::{Background, Element, Length, Padding};

use crate::ui::primitives::label::text_non_selectable as text;
use crate::ui::theme::{self, Tokens, border};

/// Render a pagination row.
///
/// * `current` — 1-indexed current page.
/// * `total` — total number of pages; 0 or 1 produces an empty row.
/// * `sibling_count` — number of pages to show on either side of `current`.
/// * `on_select` — builds a message for a target page number.
pub fn pagination<'a, Message, F>(
    current: usize,
    total: usize,
    sibling_count: usize,
    on_select: F,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
    F: Fn(usize) -> Message + Copy + 'a,
{
    let space = &theme::SPACE;
    let font = &theme::FONT;

    let mut bar: Row<'a, Message> = Row::new()
        .spacing(space.xs)
        .align_y(iced::Alignment::Center);

    if total == 0 {
        return container(iced::widget::Space::new()).into();
    }

    let prev_enabled = current > 1;
    bar = bar.push(nav_button::<Message, F>(
        "\u{2039}",
        prev_enabled,
        on_select,
        current.saturating_sub(1).max(1),
        font,
    ));

    let entries = page_entries(current, total, sibling_count);
    for entry in entries {
        match entry {
            Entry::Page(n) => {
                bar = bar.push(page_button::<Message, F>(n, n == current, on_select, font))
            }
            Entry::Ellipsis => bar = bar.push(ellipsis(font)),
        }
    }

    let next_enabled = current < total;
    bar = bar.push(nav_button::<Message, F>(
        "\u{203A}",
        next_enabled,
        on_select,
        (current + 1).min(total),
        font,
    ));

    container(bar)
        .padding(Padding {
            top: space.xs,
            bottom: space.xs,
            left: 0.0,
            right: 0.0,
        })
        .into()
}

enum Entry {
    Page(usize),
    Ellipsis,
}

fn page_entries(current: usize, total: usize, sibling_count: usize) -> Vec<Entry> {
    let mut out = Vec::new();
    if total <= 1 {
        out.push(Entry::Page(1));
        return out;
    }

    let first = 1;
    let last = total;
    let left = current.saturating_sub(sibling_count).max(first + 1);
    let right = (current + sibling_count).min(last - 1);

    out.push(Entry::Page(first));

    if left > first + 1 {
        out.push(Entry::Ellipsis);
    }

    for p in left..=right {
        out.push(Entry::Page(p));
    }

    if right < last - 1 {
        out.push(Entry::Ellipsis);
    }

    if last != first {
        out.push(Entry::Page(last));
    }

    out
}

fn nav_button<'a, Message, F>(
    label: &'static str,
    enabled: bool,
    on_select: F,
    target: usize,
    font: &crate::ui::theme::FontTokens,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
    F: Fn(usize) -> Message + Copy + 'a,
{
    let content = text(label)
        .size(font.size_base)
        .style(move |theme: &iced::Theme| iced::widget::text::Style {
            color: Some(label_color(theme, false, !enabled)),
        });

    let c = container(content)
        .width(Length::Fixed(32.0))
        .align_x(iced::alignment::Horizontal::Center)
        .padding(Padding {
            top: 4.0,
            bottom: 4.0,
            left: 0.0,
            right: 0.0,
        });

    let mut btn = button(c)
        .padding(0)
        .style(move |theme, status| page_btn_style(theme, status, false, !enabled));
    if enabled {
        btn = btn.on_press(on_select(target));
    }
    btn.into()
}

fn page_button<'a, Message, F>(
    page: usize,
    is_active: bool,
    on_select: F,
    font: &crate::ui::theme::FontTokens,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
    F: Fn(usize) -> Message + Copy + 'a,
{
    let content = text(page.to_string())
        .size(font.size_sm)
        .style(move |theme: &iced::Theme| iced::widget::text::Style {
            color: Some(label_color(theme, is_active, false)),
        });
    let c = container(content)
        .width(Length::Fixed(32.0))
        .align_x(iced::alignment::Horizontal::Center)
        .padding(Padding {
            top: 4.0,
            bottom: 4.0,
            left: 0.0,
            right: 0.0,
        });

    button(c)
        .padding(0)
        .style(move |theme, status| page_btn_style(theme, status, is_active, false))
        .on_press(on_select(page))
        .into()
}

fn ellipsis<'a, Message: 'a>(font: &crate::ui::theme::FontTokens) -> Element<'a, Message> {
    container(
        text("\u{2026}")
            .size(font.size_sm)
            .style(|theme: &iced::Theme| iced::widget::text::Style {
                color: Some(theme::tokens_for(theme).color.muted_foreground),
            }),
    )
    .width(Length::Fixed(24.0))
    .align_x(iced::alignment::Horizontal::Center)
    .into()
}

fn label_color(theme: &iced::Theme, is_active: bool, disabled: bool) -> iced::Color {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    if disabled {
        c.muted_foreground
    } else if is_active {
        c.primary_foreground
    } else {
        c.foreground
    }
}

fn page_btn_style(
    theme: &iced::Theme,
    status: iced::widget::button::Status,
    is_active: bool,
    disabled: bool,
) -> iced::widget::button::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;

    let (bg, fg) = if disabled {
        (None, c.muted_foreground)
    } else if is_active {
        (Some(Background::Color(c.primary)), c.primary_foreground)
    } else {
        match status {
            iced::widget::button::Status::Hovered => {
                (Some(Background::Color(c.muted)), c.foreground)
            }
            iced::widget::button::Status::Pressed => {
                (Some(Background::Color(c.accent)), c.foreground)
            }
            _ => (None, c.foreground),
        }
    };

    iced::widget::button::Style {
        background: bg,
        text_color: fg,
        border: border(
            if is_active { c.primary } else { c.border },
            1.0,
            tokens.radius.md,
        ),
        shadow: Default::default(),
        snap: false,
    }
}
