//! Keyboard key primitive — renders a small pill styled to resemble a key cap.

use iced::widget::{Container, container, text};
use iced::{Background, Padding};

use crate::ui::theme::{self, Tokens, border};

pub fn kbd<'a, Message: 'a>(key: impl Into<String>) -> Container<'a, Message> {
    let space = &theme::DARK.space;
    container(text(key.into()).size(theme::DARK.font.size_xs))
        .padding(Padding {
            top: 1.0,
            bottom: 1.0,
            left: space.xs + 1.0,
            right: space.xs + 1.0,
        })
        .style(|theme| {
            let tokens: &Tokens = theme::tokens_for(theme);
            let c = &tokens.color;
            iced::widget::container::Style {
                text_color: Some(c.foreground),
                background: Some(Background::Color(c.muted)),
                border: border(c.border_strong, 1.0, tokens.radius.sm),
                shadow: tokens.shadow.sm,
                snap: false,
            }
        })
}
