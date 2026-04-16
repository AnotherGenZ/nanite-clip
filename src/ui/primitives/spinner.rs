//! Spinner primitive — a small loading indicator.
//!
//! Phase-1 implementation renders a static FontAwesome circle-notch icon. A
//! proper rotating spinner requires an `iced::time` subscription driving a
//! custom widget; we'll swap this out when the animation helper lands.

use iced::Element;
use iced_font_awesome::fa_icon_solid;

use crate::ui::theme::{self, Tokens};

#[derive(Debug, Clone, Copy)]
pub enum Size {
    Sm,
    Md,
    Lg,
}

impl Size {
    fn px(self) -> f32 {
        match self {
            Size::Sm => 14.0,
            Size::Md => 18.0,
            Size::Lg => 24.0,
        }
    }
}

pub fn spinner<'a, Message: 'a>(size: Size) -> Element<'a, Message> {
    fa_icon_solid("circle-notch")
        .size(size.px())
        .style(|theme: &iced::Theme| {
            let tokens: &Tokens = theme::tokens_for(theme);
            iced::widget::text::Style {
                color: Some(tokens.color.muted_foreground),
            }
        })
        .into()
}
