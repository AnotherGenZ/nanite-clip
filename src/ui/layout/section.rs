//! Section composite — titled group inside a [`panel`](super::panel).
//!
//! A section renders a small heading and optional description above a
//! column of children. Use it to cluster related rows of settings or
//! details without the heavier visual weight of a nested card.

use iced::widget::{Column, Container, column, container};
use iced::{Element, Length};

use crate::ui::primitives::label::text;
use crate::ui::theme::{self};

pub fn section<'a, Message: 'a>(title: impl Into<String>) -> SectionBuilder<'a, Message> {
    SectionBuilder {
        title: title.into(),
        description: None,
        children: Vec::new(),
    }
}

pub struct SectionBuilder<'a, Message> {
    title: String,
    description: Option<String>,
    children: Vec<Element<'a, Message>>,
}

impl<'a, Message: 'a> SectionBuilder<'a, Message> {
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn push(mut self, child: impl Into<Element<'a, Message>>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn build(self) -> Container<'a, Message> {
        let space = &theme::SPACE;
        let font = &theme::FONT;

        let title = text(self.title)
            .size(font.size_base)
            .style(|theme: &iced::Theme| iced::widget::text::Style {
                color: Some(theme::tokens_for(theme).color.foreground),
            });

        let mut head: Column<'a, Message> = column![title].spacing(space.xxs);
        if let Some(desc) = self.description.filter(|d| !d.is_empty()) {
            head = head.push(text(desc).size(font.size_sm).style(|theme: &iced::Theme| {
                iced::widget::text::Style {
                    color: Some(theme::tokens_for(theme).color.muted_foreground),
                }
            }));
        }

        let mut content: Column<'a, Message> = column![head].spacing(space.sm);
        for child in self.children {
            content = content.push(child);
        }

        container(content).width(Length::Fill)
    }
}

impl<'a, Message: 'a> From<SectionBuilder<'a, Message>> for Element<'a, Message> {
    fn from(b: SectionBuilder<'a, Message>) -> Self {
        b.build().into()
    }
}
