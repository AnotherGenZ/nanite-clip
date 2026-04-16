//! Separator primitive — a theme-aware rule.

use iced::widget::rule::{self, Rule};

use crate::ui::theme::{self, Tokens};

pub fn horizontal<'a>() -> Rule<'a> {
    rule::horizontal(1).style(style)
}

pub fn vertical<'a>() -> Rule<'a> {
    rule::vertical(1).style(style)
}

fn style(theme: &iced::Theme) -> rule::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    rule::Style {
        color: tokens.color.border,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: false,
    }
}
