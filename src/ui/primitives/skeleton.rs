//! Skeleton placeholder — a rounded rect used while content is loading.
//!
//! Static for now (no pulse animation); once the primitives layer grows an
//! animation helper we can revisit.

use iced::widget::{Container, Space, container};
use iced::{Background, Length};

use crate::ui::theme::{self, Tokens, border};

pub fn skeleton<'a, Message: 'a>(width: f32, height: f32) -> Container<'a, Message> {
    container(
        Space::new()
            .width(Length::Fixed(width))
            .height(Length::Fixed(height)),
    )
    .width(Length::Fixed(width))
    .height(Length::Fixed(height))
    .style(|theme| {
        let tokens: &Tokens = theme::tokens_for(theme);
        iced::widget::container::Style {
            text_color: None,
            background: Some(Background::Color(tokens.color.skeleton)),
            border: border(iced::Color::TRANSPARENT, 0.0, tokens.radius.sm),
            shadow: Default::default(),
            snap: false,
        }
    })
}

pub fn text_line<'a, Message: 'a>(width: f32) -> Container<'a, Message> {
    skeleton(width, theme::DARK.font.size_base + 2.0)
}

pub fn circle<'a, Message: 'a>(diameter: f32) -> Container<'a, Message> {
    container(
        Space::new()
            .width(Length::Fixed(diameter))
            .height(Length::Fixed(diameter)),
    )
    .width(Length::Fixed(diameter))
    .height(Length::Fixed(diameter))
    .style(|theme| {
        let tokens: &Tokens = theme::tokens_for(theme);
        iced::widget::container::Style {
            text_color: None,
            background: Some(Background::Color(tokens.color.skeleton)),
            border: border(iced::Color::TRANSPARENT, 0.0, tokens.radius.full),
            shadow: Default::default(),
            snap: false,
        }
    })
}
