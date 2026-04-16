//! Checkbox primitive.

use iced::Background;
use iced::widget::{Checkbox, checkbox as iced_checkbox};

use crate::ui::theme::{self, Tokens, border, with_alpha};

pub fn checkbox<'a, Message: 'a>(
    label: impl Into<String>,
    is_checked: bool,
) -> Checkbox<'a, Message> {
    iced_checkbox(is_checked)
        .label(label.into())
        .size(16)
        .spacing(theme::DARK.space.sm)
        .text_size(theme::DARK.font.size_base)
        .style(style_for)
}

fn style_for(theme: &iced::Theme, status: iced_checkbox::Status) -> iced_checkbox::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    let radius = tokens.radius.sm;

    let (background, border_color, icon_color, text_color) = match status {
        iced_checkbox::Status::Active { is_checked } => {
            if is_checked {
                (c.primary, c.primary, c.primary_foreground, c.foreground)
            } else {
                (c.input, c.border_strong, c.primary_foreground, c.foreground)
            }
        }
        iced_checkbox::Status::Hovered { is_checked } => {
            if is_checked {
                (
                    c.primary_hover,
                    c.primary_hover,
                    c.primary_foreground,
                    c.foreground,
                )
            } else {
                (c.input, c.primary, c.primary_foreground, c.foreground)
            }
        }
        iced_checkbox::Status::Disabled { is_checked } => {
            if is_checked {
                (
                    with_alpha(c.primary, 0.5),
                    with_alpha(c.primary, 0.5),
                    with_alpha(c.primary_foreground, 0.7),
                    with_alpha(c.muted_foreground, 0.8),
                )
            } else {
                (
                    c.muted,
                    with_alpha(c.border_strong, 0.6),
                    with_alpha(c.primary_foreground, 0.7),
                    with_alpha(c.muted_foreground, 0.8),
                )
            }
        }
    };

    iced_checkbox::Style {
        background: Background::Color(background),
        icon_color,
        border: border(border_color, 1.0, radius),
        text_color: Some(text_color),
    }
}
