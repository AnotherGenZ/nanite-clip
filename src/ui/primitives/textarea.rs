//! Textarea styling helper.
//!
//! `iced::widget::text_editor::TextEditor` has two generic type parameters
//! (the `Highlighter` in particular) that don't have a clean single wrapper
//! shape. Instead of boxing those away, we expose [`style`] as a `StyleFn`
//! callers hand to `text_editor(&content).style(ui::textarea::style)`.

use iced::Background;
use iced::widget::text_editor;

use crate::ui::theme::{self, Tokens, border, with_alpha};

pub fn style(theme: &iced::Theme, status: text_editor::Status) -> text_editor::Style {
    style_with(theme, status, false)
}

pub fn style_invalid(theme: &iced::Theme, status: text_editor::Status) -> text_editor::Style {
    style_with(theme, status, true)
}

fn style_with(
    theme: &iced::Theme,
    status: text_editor::Status,
    invalid: bool,
) -> text_editor::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    let radius = tokens.radius.md;

    let border_color = if invalid { c.destructive } else { c.border };

    let base = text_editor::Style {
        background: Background::Color(c.input),
        border: border(border_color, 1.0, radius),
        placeholder: c.muted_foreground,
        value: c.input_foreground,
        selection: with_alpha(c.primary, 0.35),
    };

    match status {
        text_editor::Status::Active => base,
        text_editor::Status::Hovered => text_editor::Style {
            border: border(
                if invalid {
                    c.destructive
                } else {
                    c.border_strong
                },
                1.0,
                radius,
            ),
            ..base
        },
        text_editor::Status::Focused { .. } => text_editor::Style {
            border: border(if invalid { c.destructive } else { c.primary }, 1.5, radius),
            ..base
        },
        text_editor::Status::Disabled => text_editor::Style {
            background: Background::Color(c.muted),
            value: c.muted_foreground,
            placeholder: with_alpha(c.muted_foreground, 0.6),
            ..base
        },
    }
}
