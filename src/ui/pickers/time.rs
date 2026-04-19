//! Time picker — hour/minute/second spinner row.
//!
//! Stateless: the caller owns a [`TimeValue`] and receives a new one on
//! every change. Seconds are optional; pass `show_seconds = false` to hide
//! that column. All values wrap inside their valid ranges.

use iced::widget::{Row, button, column, container};
use iced::{Background, Element, Length, Padding};

use crate::ui::primitives::label::text_non_selectable as text;
use crate::ui::theme::{self, Tokens, border};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TimeValue {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl TimeValue {
    pub fn new(hour: u8, minute: u8, second: u8) -> Self {
        Self {
            hour: hour % 24,
            minute: minute % 60,
            second: second % 60,
        }
    }

    pub fn with_hour(self, hour: u8) -> Self {
        Self {
            hour: hour % 24,
            ..self
        }
    }

    pub fn with_minute(self, minute: u8) -> Self {
        Self {
            minute: minute % 60,
            ..self
        }
    }

    pub fn with_second(self, second: u8) -> Self {
        Self {
            second: second % 60,
            ..self
        }
    }
}

pub fn time_picker<'a, Message, F>(
    value: TimeValue,
    show_seconds: bool,
    on_change: F,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
    F: Fn(TimeValue) -> Message + Copy + 'a,
{
    let space = &theme::SPACE;
    let font = &theme::FONT;

    let hour_col = spinner::<Message>(
        value.hour,
        23,
        move |n| on_change(value.with_hour(n)),
        font,
        space,
    );
    let minute_col = spinner::<Message>(
        value.minute,
        59,
        move |n| on_change(value.with_minute(n)),
        font,
        space,
    );

    let sep = text(":")
        .size(font.size_xl)
        .style(|theme: &iced::Theme| iced::widget::text::Style {
            color: Some(theme::tokens_for(theme).color.muted_foreground),
        });

    let mut row_inner: Row<'a, Message> = Row::new()
        .spacing(space.xs)
        .align_y(iced::Alignment::Center);
    row_inner = row_inner.push(hour_col).push(sep).push(minute_col);

    if show_seconds {
        let sep2 =
            text(":")
                .size(font.size_xl)
                .style(|theme: &iced::Theme| iced::widget::text::Style {
                    color: Some(theme::tokens_for(theme).color.muted_foreground),
                });
        let second_col = spinner::<Message>(
            value.second,
            59,
            move |n| on_change(value.with_second(n)),
            font,
            space,
        );
        row_inner = row_inner.push(sep2).push(second_col);
    }

    container(row_inner)
        .padding(Padding {
            top: space.md,
            bottom: space.md,
            left: space.md,
            right: space.md,
        })
        .style(card_style)
        .into()
}

fn spinner<'a, Message>(
    current: u8,
    max: u8,
    emit: impl Fn(u8) -> Message + Copy + 'a,
    font: &crate::ui::theme::FontTokens,
    space: &crate::ui::theme::SpaceTokens,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let up = step_button::<Message>("\u{25B4}", emit(wrap_up(current, max)), font, space);
    let down = step_button::<Message>("\u{25BE}", emit(wrap_down(current, max)), font, space);

    let value_text =
        text(format!("{:02}", current))
            .size(font.size_2xl)
            .style(|theme: &iced::Theme| iced::widget::text::Style {
                color: Some(theme::tokens_for(theme).color.foreground),
            });

    let value_box = container(value_text)
        .width(Length::Fixed(64.0))
        .align_x(iced::alignment::Horizontal::Center)
        .padding(Padding {
            top: space.xs,
            bottom: space.xs,
            left: 0.0,
            right: 0.0,
        });

    column![up, value_box, down]
        .spacing(space.xxs)
        .align_x(iced::Alignment::Center)
        .into()
}

fn step_button<'a, Message>(
    glyph: &'static str,
    message: Message,
    font: &crate::ui::theme::FontTokens,
    space: &crate::ui::theme::SpaceTokens,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let label =
        text(glyph)
            .size(font.size_sm)
            .style(|theme: &iced::Theme| iced::widget::text::Style {
                color: Some(theme::tokens_for(theme).color.foreground),
            });
    let content = container(label)
        .width(Length::Fixed(64.0))
        .align_x(iced::alignment::Horizontal::Center)
        .padding(Padding {
            top: space.xxs,
            bottom: space.xxs,
            left: 0.0,
            right: 0.0,
        });

    button(content)
        .padding(0)
        .style(step_style)
        .on_press(message)
        .into()
}

fn wrap_up(current: u8, max: u8) -> u8 {
    if current >= max { 0 } else { current + 1 }
}

fn wrap_down(current: u8, max: u8) -> u8 {
    if current == 0 { max } else { current - 1 }
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

fn step_style(
    theme: &iced::Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    let bg = match status {
        iced::widget::button::Status::Hovered => Some(Background::Color(c.muted)),
        iced::widget::button::Status::Pressed => Some(Background::Color(c.accent)),
        _ => Some(Background::Color(c.input)),
    };
    iced::widget::button::Style {
        background: bg,
        text_color: c.foreground,
        border: border(c.border, 1.0, tokens.radius.sm),
        shadow: Default::default(),
        snap: false,
    }
}
