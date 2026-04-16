//! Modal helper — dimmed-backdrop dialog over a base element.
//!
//! Iced 0.14 has no portal/overlay API for application-level popups, so we
//! synthesize one with [`iced::widget::Stack`]: layer #1 is the base view,
//! layer #2 is a fill-the-window dim backdrop wired through a `mouse_area`
//! for click-outside dismissal, layer #3 is a centered card that hosts the
//! actual content.
//!
//! The base element is still interactive underneath the modal — if you
//! need to block interaction entirely, the backdrop is a `mouse_area`
//! spanning the full window and will eat most events. For keyboard events,
//! apps should gate their own handlers on a `modal_open` flag.

use iced::widget::{Container, MouseArea, Stack, container, mouse_area, stack};
use iced::{Background, Element, Length, Padding};

use crate::ui::theme::{self, Tokens};

/// Wrap `base` with a centered modal card hosting `content`.
///
/// When `on_dismiss` is `Some`, clicking the dimmed backdrop emits that
/// message. Passing `None` produces a forced-choice modal (no outside click
/// dismissal — the content must provide its own button).
pub fn modal<'a, Message>(
    base: impl Into<Element<'a, Message>>,
    content: impl Into<Element<'a, Message>>,
    on_dismiss: Option<Message>,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let backdrop_container: Container<'a, Message> = container(iced::widget::Space::new())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(backdrop_style);

    let backdrop: Element<'a, Message> = if let Some(msg) = on_dismiss {
        let area: MouseArea<'a, Message> = mouse_area(backdrop_container).on_press(msg);
        area.into()
    } else {
        backdrop_container.into()
    };

    let card: Container<'a, Message> = container(content.into())
        .padding(Padding {
            top: theme::SPACE.xl,
            bottom: theme::SPACE.xl,
            left: theme::SPACE.xl,
            right: theme::SPACE.xl,
        })
        .max_width(520.0)
        .style(card_style);

    let centered: Container<'a, Message> = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(theme::SPACE.xl);

    let centered_el: Element<'a, Message> = centered.into();
    let layered: Stack<'a, Message> = stack![base.into(), backdrop, centered_el];
    layered.into()
}

fn backdrop_style(theme: &iced::Theme) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    iced::widget::container::Style {
        text_color: None,
        background: Some(Background::Color(tokens.color.overlay)),
        border: iced::border::Border::default(),
        shadow: Default::default(),
        snap: false,
    }
}

fn card_style(theme: &iced::Theme) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    iced::widget::container::Style {
        text_color: Some(c.popover_foreground),
        background: Some(Background::Color(c.popover)),
        border: iced::border::Border {
            color: c.border,
            width: 1.0,
            radius: tokens.radius.xl.into(),
        },
        shadow: tokens.shadow.xl,
        snap: false,
    }
}
