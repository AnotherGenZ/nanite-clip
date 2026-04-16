//! Panel composite — a loose card with a persistent title + description,
//! intended to host multiple [`section`](super::section)s of configuration
//! or detail content. Panels omit the card's shadow and use a subtler
//! border so nested cards still read as distinct.

use iced::widget::{Column, Container, column, container, text};
use iced::{Background, Element, Length, Padding};

use crate::ui::theme::{self, Tokens, border};

pub fn panel<'a, Message: 'a>(title: impl Into<String>) -> PanelBuilder<'a, Message> {
    PanelBuilder {
        title: title.into(),
        description: None,
        children: Vec::new(),
        width: Length::Fill,
    }
}

pub struct PanelBuilder<'a, Message> {
    title: String,
    description: Option<String>,
    children: Vec<Element<'a, Message>>,
    width: Length,
}

impl<'a, Message: 'a> PanelBuilder<'a, Message> {
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn push(mut self, child: impl Into<Element<'a, Message>>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn build(self) -> Container<'a, Message> {
        let space = &theme::SPACE;
        let font = &theme::FONT;

        let mut head: Column<'a, Message> = column![].spacing(space.xxs);
        head = head.push(
            text(self.title)
                .size(font.size_xl)
                .style(|theme: &iced::Theme| iced::widget::text::Style {
                    color: Some(theme::tokens_for(theme).color.foreground),
                }),
        );
        if let Some(desc) = self.description {
            head = head.push(text(desc).size(font.size_sm).style(|theme: &iced::Theme| {
                iced::widget::text::Style {
                    color: Some(theme::tokens_for(theme).color.muted_foreground),
                }
            }));
        }

        let mut body: Column<'a, Message> = column![head].spacing(space.lg);
        for child in self.children {
            body = body.push(child);
        }

        container(body)
            .padding(Padding {
                top: space.xl,
                bottom: space.xl,
                left: space.xl,
                right: space.xl,
            })
            .width(self.width)
            .style(panel_style)
    }
}

impl<'a, Message: 'a> From<PanelBuilder<'a, Message>> for Element<'a, Message> {
    fn from(b: PanelBuilder<'a, Message>) -> Self {
        b.build().into()
    }
}

fn panel_style(theme: &iced::Theme) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    iced::widget::container::Style {
        text_color: Some(c.foreground),
        background: Some(Background::Color(c.background)),
        border: border(c.border, 1.0, tokens.radius.lg),
        shadow: Default::default(),
        snap: false,
    }
}
