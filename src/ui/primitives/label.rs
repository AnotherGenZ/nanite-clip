//! Typography primitives.
//!
//! `text()` applies design-token sizing via shorthand helpers. Use `heading`
//! for section titles, `body` for normal copy, and `caption` for secondary
//! metadata.

use iced::widget::{Text, text as iced_text};

use crate::ui::theme as ui_theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    H1,
    H2,
    H3,
    Body,
    BodySmall,
    Caption,
}

impl Level {
    pub fn size(self) -> f32 {
        let f = &ui_theme::DARK.font;
        match self {
            Level::H1 => f.size_3xl,
            Level::H2 => f.size_2xl,
            Level::H3 => f.size_xl,
            Level::Body => f.size_base,
            Level::BodySmall => f.size_sm,
            Level::Caption => f.size_xs,
        }
    }
}

pub fn text<'a>(label: impl Into<String>) -> Text<'a> {
    iced_text(label.into()).size(Level::Body.size())
}

pub fn heading<'a>(level: Level, label: impl Into<String>) -> Text<'a> {
    iced_text(label.into())
        .size(level.size())
        .style(|theme: &iced::Theme| iced::widget::text::Style {
            color: Some(ui_theme::tokens_for(theme).color.foreground),
        })
}

pub fn muted<'a>(label: impl Into<String>) -> Text<'a> {
    iced_text(label.into())
        .size(Level::BodySmall.size())
        .style(|theme: &iced::Theme| iced::widget::text::Style {
            color: Some(ui_theme::tokens_for(theme).color.muted_foreground),
        })
}

pub fn caption<'a>(label: impl Into<String>) -> Text<'a> {
    iced_text(label.into())
        .size(Level::Caption.size())
        .style(|theme: &iced::Theme| iced::widget::text::Style {
            color: Some(ui_theme::tokens_for(theme).color.muted_foreground),
        })
}
