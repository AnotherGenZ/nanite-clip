//! Popover — stack-overlaid card anchored to a corner of the window.
//!
//! iced 0.14 does not expose a positional overlay API for application-level
//! widgets, so "popover" here means "a card we layer on top of the base view
//! via [`iced::widget::Stack`] and pin to a corner with `align_x`/`align_y`".
//! It is not anchored to a trigger element — callers that need
//! trigger-relative positioning should compose it into a layout whose
//! geometry places the trigger near the target corner.
//!
//! Despite the coarse positioning, this is still the right primitive for
//! things like "show the settings menu in the top right", "show the quick
//! action list in the bottom right", or "expand a detail card from the
//! header".

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{Container, Stack, container, mouse_area, stack};
use iced::{Background, Element, Length, Padding};

use crate::ui::theme::{self, Tokens, border};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl Anchor {
    fn alignments(self) -> (Horizontal, Vertical) {
        match self {
            Anchor::TopLeft => (Horizontal::Left, Vertical::Top),
            Anchor::TopCenter => (Horizontal::Center, Vertical::Top),
            Anchor::TopRight => (Horizontal::Right, Vertical::Top),
            Anchor::CenterLeft => (Horizontal::Left, Vertical::Center),
            Anchor::Center => (Horizontal::Center, Vertical::Center),
            Anchor::CenterRight => (Horizontal::Right, Vertical::Center),
            Anchor::BottomLeft => (Horizontal::Left, Vertical::Bottom),
            Anchor::BottomCenter => (Horizontal::Center, Vertical::Bottom),
            Anchor::BottomRight => (Horizontal::Right, Vertical::Bottom),
        }
    }
}

/// Overlay `content` on top of `base`, pinned to `anchor`.
///
/// When `on_dismiss` is `Some`, a transparent click-catcher behind the card
/// emits that message so callers can implement click-outside dismissal.
/// Passing `None` leaves the base layer fully interactive.
pub fn popover<'a, Message>(
    base: impl Into<Element<'a, Message>>,
    content: impl Into<Element<'a, Message>>,
    anchor: Anchor,
    on_dismiss: Option<Message>,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let (h, v) = anchor.alignments();

    let card: Container<'a, Message> = container(content.into())
        .padding(Padding {
            top: theme::SPACE.sm,
            bottom: theme::SPACE.sm,
            left: theme::SPACE.sm,
            right: theme::SPACE.sm,
        })
        .style(card_style);

    let anchored: Container<'a, Message> = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(h)
        .align_y(v)
        .padding(theme::SPACE.lg);

    let anchored_el: Element<'a, Message> = anchored.into();

    let overlay: Element<'a, Message> = if let Some(msg) = on_dismiss {
        let catcher: Container<'a, Message> = container(iced::widget::Space::new())
            .width(Length::Fill)
            .height(Length::Fill);
        let catcher_el: Element<'a, Message> = mouse_area(catcher).on_press(msg).into();
        let layered: Stack<'a, Message> = stack![catcher_el, anchored_el];
        layered.into()
    } else {
        anchored_el
    };

    let root: Stack<'a, Message> = stack![base.into(), overlay];
    root.into()
}

fn card_style(theme: &iced::Theme) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    iced::widget::container::Style {
        text_color: Some(c.popover_foreground),
        background: Some(Background::Color(c.popover)),
        border: border(c.border_strong, 1.0, tokens.radius.lg),
        shadow: tokens.shadow.lg,
        snap: false,
    }
}
