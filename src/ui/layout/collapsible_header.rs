//! Collapsible header — a full-width toggle row used ahead of collapsible content.
//!
//! The title and description text are rendered with non-selectable labels so the
//! header button does not interfere with pointer interactions elsewhere in the UI.

use iced::widget::button;
use iced::{Background, Element, Length};

use crate::ui::app::{column, container, horizontal_space, row, text_non_selectable};
use crate::ui::theme;

pub fn collapsible_header<'a, Message: Clone + 'a>(
    title: impl Into<String>,
    description: Option<String>,
    collapsed: bool,
    trailing: Option<Element<'a, Message>>,
    on_press: Message,
) -> Element<'a, Message> {
    let arrow = if collapsed { "\u{25B8}" } else { "\u{25BE}" };
    let title_text = format!("{arrow}  {}", title.into());

    let title_column: Element<'a, Message> =
        if let Some(description) = description.filter(|value| !value.is_empty()) {
            column![
                text_non_selectable(title_text)
                    .size(14)
                    .style(|theme: &iced::Theme| iced::widget::text::Style {
                        color: Some(theme::tokens_for(theme).color.foreground),
                    }),
                text_non_selectable(description)
                    .size(11)
                    .style(|theme: &iced::Theme| iced::widget::text::Style {
                        color: Some(theme::tokens_for(theme).color.muted_foreground),
                    }),
            ]
            .spacing(2)
            .into()
        } else {
            text_non_selectable(title_text)
                .size(14)
                .style(|theme: &iced::Theme| iced::widget::text::Style {
                    color: Some(theme::tokens_for(theme).color.foreground),
                })
                .into()
        };

    let mut head = row![title_column, horizontal_space()]
        .spacing(8)
        .align_y(iced::Alignment::Center);
    if let Some(trailing) = trailing {
        head = head.push(trailing);
    }

    button(container(head).padding([6, 8]).width(Length::Fill))
        .padding(0)
        .width(Length::Fill)
        .style(
            |theme: &iced::Theme, status: iced::widget::button::Status| {
                let c = &theme::tokens_for(theme).color;
                let background = match status {
                    iced::widget::button::Status::Hovered => Some(Background::Color(c.accent)),
                    iced::widget::button::Status::Pressed => Some(Background::Color(c.muted)),
                    _ => None,
                };
                iced::widget::button::Style {
                    background,
                    text_color: c.foreground,
                    border: theme::border(c.border, 1.0, theme::RADIUS.md),
                    shadow: Default::default(),
                    snap: false,
                }
            },
        )
        .on_press(on_press)
        .into()
}
