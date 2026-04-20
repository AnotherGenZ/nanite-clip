//! Card composite — the fundamental content container.
//!
//! A card has an optional header (title + description), body (arbitrary
//! content), and footer (typically a row of actions). Use it for any
//! bounded content block that needs a surface, border, and padding.

use iced::widget::{Column, Container, column, container};
use iced::{Background, Element, Length, Padding};

use crate::ui::primitives::label::text;
use crate::ui::theme::{self, Tokens, border};

pub fn card<'a, Message: 'a>() -> CardBuilder<'a, Message> {
    CardBuilder::default()
}

pub struct CardBuilder<'a, Message> {
    title: Option<String>,
    description: Option<String>,
    body: Option<Element<'a, Message>>,
    footer: Option<Element<'a, Message>>,
    width: Length,
    max_width: Option<f32>,
}

impl<'a, Message> Default for CardBuilder<'a, Message> {
    fn default() -> Self {
        Self {
            title: None,
            description: None,
            body: None,
            footer: None,
            width: Length::Shrink,
            max_width: None,
        }
    }
}

impl<'a, Message: 'a> CardBuilder<'a, Message> {
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn body(mut self, body: impl Into<Element<'a, Message>>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn footer(mut self, footer: impl Into<Element<'a, Message>>) -> Self {
        self.footer = Some(footer.into());
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn max_width(mut self, max_width: f32) -> Self {
        self.max_width = Some(max_width);
        self
    }

    pub fn build(self) -> Container<'a, Message> {
        let space = &theme::SPACE;
        let font = &theme::FONT;

        let mut col: Column<'a, Message> = column![].spacing(space.md);

        if self.title.is_some() || self.description.is_some() {
            let mut head: Column<'a, Message> = column![].spacing(space.xxs);
            if let Some(title) = self.title {
                head = head.push(text(title).size(font.size_lg).style(|theme: &iced::Theme| {
                    iced::widget::text::Style {
                        color: Some(theme::tokens_for(theme).color.foreground),
                    }
                }));
            }
            if let Some(desc) = self.description.filter(|d| !d.is_empty()) {
                head = head.push(text(desc).size(font.size_sm).style(|theme: &iced::Theme| {
                    iced::widget::text::Style {
                        color: Some(theme::tokens_for(theme).color.muted_foreground),
                    }
                }));
            }
            col = col.push(head);
        }

        if let Some(body) = self.body {
            col = col.push(body);
        }

        if let Some(footer) = self.footer {
            col = col.push(footer);
        }

        let mut c = container(col)
            .padding(Padding {
                top: space.lg,
                bottom: space.lg,
                left: space.lg,
                right: space.lg,
            })
            .width(self.width)
            .style(card_style);

        if let Some(max) = self.max_width {
            c = c.max_width(max);
        }
        c
    }
}

impl<'a, Message: 'a> From<CardBuilder<'a, Message>> for Element<'a, Message> {
    fn from(b: CardBuilder<'a, Message>) -> Self {
        b.build().into()
    }
}

fn card_style(theme: &iced::Theme) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    iced::widget::container::Style {
        text_color: Some(c.card_foreground),
        background: Some(Background::Color(c.card)),
        border: border(c.border, 1.0, tokens.radius.lg),
        shadow: tokens.shadow.sm,
        snap: false,
    }
}
