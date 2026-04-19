//! Page header composite — title, optional subtitle, and a trailing
//! actions row. Lives at the top of each page and sets the page context.

use iced::widget::{Column, Container, Row, column, container, row};
use iced::{Element, Length, Padding};

use crate::ui::primitives::label::text;
use crate::ui::theme::{self};

pub fn page_header<'a, Message: 'a + Clone>(
    title: impl Into<String>,
) -> PageHeaderBuilder<'a, Message> {
    PageHeaderBuilder {
        title: title.into(),
        subtitle: None,
        actions: Vec::new(),
    }
}

pub struct PageHeaderBuilder<'a, Message> {
    title: String,
    subtitle: Option<String>,
    actions: Vec<Element<'a, Message>>,
}

impl<'a, Message: 'a + Clone> PageHeaderBuilder<'a, Message> {
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn action(mut self, action: impl Into<Element<'a, Message>>) -> Self {
        self.actions.push(action.into());
        self
    }

    pub fn build(self) -> Container<'a, Message> {
        let space = &theme::SPACE;
        let font = &theme::FONT;

        let title = text(self.title)
            .size(font.size_2xl)
            .style(|theme: &iced::Theme| iced::widget::text::Style {
                color: Some(theme::tokens_for(theme).color.foreground),
            });

        let mut left: Column<'a, Message> = column![title].spacing(space.xxs);
        if let Some(sub) = self.subtitle {
            left = left.push(text(sub).size(font.size_sm).style(|theme: &iced::Theme| {
                iced::widget::text::Style {
                    color: Some(theme::tokens_for(theme).color.muted_foreground),
                }
            }));
        }

        let mut row: Row<'a, Message> = row![left.width(Length::Fill)]
            .spacing(space.md)
            .align_y(iced::Alignment::Center);

        if !self.actions.is_empty() {
            let mut actions_row: Row<'a, Message> = Row::new()
                .spacing(space.sm)
                .align_y(iced::Alignment::Center);
            for action in self.actions {
                actions_row = actions_row.push(action);
            }
            row = row.push(actions_row);
        }

        container(row).padding(Padding {
            top: space.md,
            bottom: space.lg,
            left: 0.0,
            right: 0.0,
        })
    }
}

impl<'a, Message: 'a + Clone> From<PageHeaderBuilder<'a, Message>> for Element<'a, Message> {
    fn from(b: PageHeaderBuilder<'a, Message>) -> Self {
        b.build().into()
    }
}
