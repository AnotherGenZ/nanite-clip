//! Toolbar composite — horizontal action bar with optional title and
//! clustered action groups. Not a wrapper around a specific widget; just a
//! styled [`container`] around a [`row`] with separator helpers.

use iced::widget::{Row, container, row};
use iced::{Background, Element, Length, Padding};

use crate::ui::theme::{self, Tokens, border};

pub fn toolbar<'a, Message: 'a + Clone>() -> ToolbarBuilder<'a, Message> {
    ToolbarBuilder {
        leading: Vec::new(),
        trailing: Vec::new(),
    }
}

pub struct ToolbarBuilder<'a, Message> {
    leading: Vec<Element<'a, Message>>,
    trailing: Vec<Element<'a, Message>>,
}

impl<'a, Message: 'a + Clone> ToolbarBuilder<'a, Message> {
    pub fn push(mut self, child: impl Into<Element<'a, Message>>) -> Self {
        self.leading.push(child.into());
        self
    }

    pub fn trailing(mut self, child: impl Into<Element<'a, Message>>) -> Self {
        self.trailing.push(child.into());
        self
    }

    pub fn separator(self) -> Self {
        self.push(divider::<Message>())
    }

    pub fn build(self) -> Element<'a, Message> {
        let space = &theme::SPACE;

        let mut inner: Row<'a, Message> = row![].spacing(space.sm).align_y(iced::Alignment::Center);

        for child in self.leading {
            inner = inner.push(child);
        }

        if !self.trailing.is_empty() {
            inner = inner.push(iced::widget::Space::new().width(Length::Fill));
            for child in self.trailing {
                inner = inner.push(child);
            }
        }

        container(inner)
            .padding(Padding {
                top: space.xs + 2.0,
                bottom: space.xs + 2.0,
                left: space.md,
                right: space.md,
            })
            .width(Length::Fill)
            .style(toolbar_style)
            .into()
    }
}

fn divider<'a, Message: 'a>() -> Element<'a, Message> {
    container(iced::widget::Space::new().height(Length::Fixed(18.0)))
        .width(Length::Fixed(1.0))
        .style(|theme: &iced::Theme| iced::widget::container::Style {
            text_color: None,
            background: Some(Background::Color(theme::tokens_for(theme).color.border)),
            border: iced::border::Border::default(),
            shadow: Default::default(),
            snap: false,
        })
        .into()
}

fn toolbar_style(theme: &iced::Theme) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    iced::widget::container::Style {
        text_color: Some(c.foreground),
        background: Some(Background::Color(c.card)),
        border: border(c.border, 1.0, tokens.radius.md),
        shadow: Default::default(),
        snap: false,
    }
}
