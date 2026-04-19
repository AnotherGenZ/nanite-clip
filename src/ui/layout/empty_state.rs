//! Empty state composite — centered placeholder with icon, title,
//! description, and optional call-to-action.

use iced::widget::{Column, Container, column, container};
use iced::{Element, Length, Padding};

use crate::ui::primitives::label::text;
use crate::ui::theme::{self};

pub fn empty_state<'a, Message: 'a>(title: impl Into<String>) -> EmptyStateBuilder<'a, Message> {
    EmptyStateBuilder {
        title: title.into(),
        description: None,
        icon: None,
        action: None,
    }
}

pub struct EmptyStateBuilder<'a, Message> {
    title: String,
    description: Option<String>,
    icon: Option<Element<'a, Message>>,
    action: Option<Element<'a, Message>>,
}

impl<'a, Message: 'a> EmptyStateBuilder<'a, Message> {
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn icon(mut self, icon: impl Into<Element<'a, Message>>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn action(mut self, action: impl Into<Element<'a, Message>>) -> Self {
        self.action = Some(action.into());
        self
    }

    pub fn build(self) -> Container<'a, Message> {
        let space = &theme::SPACE;
        let font = &theme::FONT;

        let mut col: Column<'a, Message> =
            column![].spacing(space.md).align_x(iced::Alignment::Center);

        if let Some(icon) = self.icon {
            col = col.push(icon);
        }

        col = col.push(
            text(self.title)
                .size(font.size_lg)
                .style(|theme: &iced::Theme| iced::widget::text::Style {
                    color: Some(theme::tokens_for(theme).color.foreground),
                }),
        );

        if let Some(desc) = self.description {
            col = col.push(text(desc).size(font.size_sm).style(|theme: &iced::Theme| {
                iced::widget::text::Style {
                    color: Some(theme::tokens_for(theme).color.muted_foreground),
                }
            }));
        }

        if let Some(action) = self.action {
            col = col.push(action);
        }

        container(col)
            .padding(Padding {
                top: space.xxl,
                bottom: space.xxl,
                left: space.xl,
                right: space.xl,
            })
            .center_x(Length::Fill)
            .center_y(Length::Fill)
    }
}

impl<'a, Message: 'a> From<EmptyStateBuilder<'a, Message>> for Element<'a, Message> {
    fn from(b: EmptyStateBuilder<'a, Message>) -> Self {
        b.build().into()
    }
}
