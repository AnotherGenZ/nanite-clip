//! Slider styling helper.
//!
//! iced's `Slider` has a generic numeric parameter with a `num_traits` bound
//! that would force us to depend on `num_traits` directly if we wanted to
//! return a built-in widget. Instead, expose [`style`] as a `StyleFn` that
//! callers hand to `slider(...).style(ui::slider::style)`.

use iced::widget::slider as iced_slider;
use iced::{Background, border};

use crate::ui::theme::{self, Tokens};

pub fn style(theme: &iced::Theme, status: iced_slider::Status) -> iced_slider::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;

    let (filled, handle) = match status {
        iced_slider::Status::Active => (c.primary, c.primary),
        iced_slider::Status::Hovered => (c.primary_hover, c.primary_hover),
        iced_slider::Status::Dragged => (c.primary_active, c.primary_active),
    };

    iced_slider::Style {
        rail: iced_slider::Rail {
            backgrounds: (Background::Color(filled), Background::Color(c.muted)),
            width: 5.0,
            border: border::Border {
                radius: tokens.radius.full.into(),
                width: 0.0,
                color: iced::Color::TRANSPARENT,
            },
        },
        handle: iced_slider::Handle {
            shape: iced_slider::HandleShape::Circle { radius: 8.0 },
            background: Background::Color(handle),
            border_color: c.primary_foreground,
            border_width: 2.0,
        },
    }
}
