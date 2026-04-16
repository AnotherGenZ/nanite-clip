//! Switch primitive (wraps `iced::widget::toggler`).

use iced::widget::{Toggler, toggler as iced_toggler};
use iced::{Background, border};

use crate::ui::theme::{self, Tokens, with_alpha};

pub fn switch<'a, Message: 'a>(is_on: bool) -> Toggler<'a, Message> {
    iced_toggler(is_on)
        .size(theme::DARK.font.size_base + 6.0)
        .text_size(theme::DARK.font.size_base)
        .spacing(theme::DARK.space.sm)
        .style(style_for)
}

fn style_for(theme: &iced::Theme, status: iced_toggler::Status) -> iced_toggler::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;

    let (track, knob) = match status {
        iced_toggler::Status::Active { is_toggled } => {
            if is_toggled {
                (c.primary, c.primary_foreground)
            } else {
                (c.muted, c.foreground)
            }
        }
        iced_toggler::Status::Hovered { is_toggled } => {
            if is_toggled {
                (c.primary_hover, c.primary_foreground)
            } else {
                (c.border_strong, c.foreground)
            }
        }
        iced_toggler::Status::Disabled { is_toggled } => {
            if is_toggled {
                (
                    with_alpha(c.primary, 0.5),
                    with_alpha(c.primary_foreground, 0.7),
                )
            } else {
                (with_alpha(c.muted, 0.8), with_alpha(c.foreground, 0.5))
            }
        }
    };

    iced_toggler::Style {
        background: Background::Color(track),
        background_border_width: 0.0,
        background_border_color: track,
        foreground: Background::Color(knob),
        foreground_border_width: 0.0,
        foreground_border_color: knob,
        text_color: Some(c.foreground),
        border_radius: Some(border::Radius::from(tokens.radius.full)),
        padding_ratio: 0.15,
    }
}
