//! Tooltip primitive — hover-reveal label anchored to a trigger element.
//!
//! Wraps `iced::widget::tooltip` with a themed container card and a short
//! default delay so tooltips feel intentional rather than snappy.

use std::time::Duration;

use iced::widget::{Tooltip, container, text, tooltip as iced_tooltip};
use iced::{Background, Element, Padding};

pub use iced::widget::tooltip::Position;

use crate::ui::theme::{self, Tokens, border};

const DEFAULT_DELAY: Duration = Duration::from_millis(350);

/// Attach a simple text tooltip to `content`.
pub fn tooltip<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    label: impl Into<String>,
    position: Position,
) -> Tooltip<'a, Message> {
    let space = &theme::SPACE;
    let font = &theme::FONT;

    let label_el = text(label.into())
        .size(font.size_sm)
        .style(|theme: &iced::Theme| iced::widget::text::Style {
            color: Some(theme::tokens_for(theme).color.popover_foreground),
        });

    let card = container(label_el).padding(Padding {
        top: space.xs,
        bottom: space.xs,
        left: space.sm,
        right: space.sm,
    });

    iced_tooltip(content, card, position)
        .gap(space.xs)
        .padding(0.0)
        .snap_within_viewport(true)
        .delay(DEFAULT_DELAY)
        .style(tooltip_style)
}

fn tooltip_style(theme: &iced::Theme) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    iced::widget::container::Style {
        text_color: Some(c.popover_foreground),
        background: Some(Background::Color(c.popover)),
        border: border(c.border_strong, 1.0, tokens.radius.sm),
        shadow: tokens.shadow.md,
        snap: false,
    }
}
