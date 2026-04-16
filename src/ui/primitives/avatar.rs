//! Avatar primitive — circular container that holds either initials text or
//! an arbitrary child element (like an image).

use iced::widget::{Container, container, text};
use iced::{Background, Element, Length};

use crate::ui::theme::{self, Tokens, border};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Size {
    Sm,
    #[default]
    Md,
    Lg,
    Xl,
}

impl Size {
    fn diameter(self) -> f32 {
        match self {
            Size::Sm => 24.0,
            Size::Md => 32.0,
            Size::Lg => 40.0,
            Size::Xl => 56.0,
        }
    }

    fn font(self) -> f32 {
        let f = &theme::DARK.font;
        match self {
            Size::Sm => f.size_xs,
            Size::Md => f.size_sm,
            Size::Lg => f.size_base,
            Size::Xl => f.size_lg,
        }
    }
}

pub fn avatar<'a, Message: 'a>(initials: impl Into<String>) -> Container<'a, Message> {
    avatar_with_size(initials, Size::default())
}

pub fn avatar_with_size<'a, Message: 'a>(
    initials: impl Into<String>,
    size: Size,
) -> Container<'a, Message> {
    let diameter = size.diameter();
    container(text(initials.into()).size(size.font()))
        .width(Length::Fixed(diameter))
        .height(Length::Fixed(diameter))
        .center_x(Length::Fixed(diameter))
        .center_y(Length::Fixed(diameter))
        .style(|theme| {
            let tokens: &Tokens = theme::tokens_for(theme);
            let c = &tokens.color;
            iced::widget::container::Style {
                text_color: Some(c.foreground),
                background: Some(Background::Color(c.secondary)),
                border: border(c.border, 1.0, tokens.radius.full),
                shadow: Default::default(),
                snap: false,
            }
        })
}

pub fn avatar_element<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    size: Size,
) -> Container<'a, Message> {
    let diameter = size.diameter();
    container(content)
        .width(Length::Fixed(diameter))
        .height(Length::Fixed(diameter))
        .center_x(Length::Fixed(diameter))
        .center_y(Length::Fixed(diameter))
        .clip(true)
        .style(|theme| {
            let tokens: &Tokens = theme::tokens_for(theme);
            let c = &tokens.color;
            iced::widget::container::Style {
                text_color: Some(c.foreground),
                background: Some(Background::Color(c.secondary)),
                border: border(c.border, 1.0, tokens.radius.full),
                shadow: Default::default(),
                snap: false,
            }
        })
}
