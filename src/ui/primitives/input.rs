//! Text input primitive.
//!
//! Wraps `iced::widget::text_input` with design-token styling and sensible
//! padding. Use [`input`] to build one; call the `.invalid(true)` helper to
//! flip it into the error appearance (red ring + border).

use iced::widget::{TextInput, text_input as iced_input};
use iced::{Background, Element, Length};

use crate::ui::theme::{self, Tokens, border, with_alpha};

pub struct NaniteInput<'a, Message>
where
    Message: Clone,
{
    inner: TextInput<'a, Message>,
    invalid: bool,
}

pub fn input<'a, Message: Clone + 'a>(placeholder: &str, value: &str) -> NaniteInput<'a, Message> {
    NaniteInput {
        inner: iced_input(placeholder, value)
            .size(theme::DARK.font.size_base)
            .padding(iced::Padding {
                top: theme::DARK.space.sm + 1.0,
                bottom: theme::DARK.space.sm + 1.0,
                left: theme::DARK.space.md,
                right: theme::DARK.space.md,
            }),
        invalid: false,
    }
}

impl<'a, Message: Clone + 'a> NaniteInput<'a, Message> {
    pub fn on_input(mut self, on_change: impl Fn(String) -> Message + 'a) -> Self {
        self.inner = self.inner.on_input(on_change);
        self
    }

    pub fn on_submit(mut self, message: Message) -> Self {
        self.inner = self.inner.on_submit(message);
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.inner = self.inner.width(width);
        self
    }

    pub fn secure(mut self, is_secure: bool) -> Self {
        self.inner = self.inner.secure(is_secure);
        self
    }

    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    pub fn build(self) -> TextInput<'a, Message> {
        let invalid = self.invalid;
        self.inner
            .style(move |theme, status| style_for(theme, status, invalid))
    }
}

impl<'a, Message: Clone + 'a> From<NaniteInput<'a, Message>> for Element<'a, Message> {
    fn from(input: NaniteInput<'a, Message>) -> Self {
        input.build().into()
    }
}

fn style_for(theme: &iced::Theme, status: iced_input::Status, invalid: bool) -> iced_input::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    let radius = tokens.radius.md;

    let border_color = if invalid { c.destructive } else { c.border };
    let ring_color = if invalid { c.destructive } else { c.primary };

    let base = iced_input::Style {
        background: Background::Color(c.input),
        border: border(border_color, 1.0, radius),
        icon: c.muted_foreground,
        placeholder: c.muted_foreground,
        value: c.input_foreground,
        selection: with_alpha(c.primary, 0.35),
    };

    match status {
        iced_input::Status::Active => base,
        iced_input::Status::Hovered => iced_input::Style {
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
        iced_input::Status::Focused { .. } => iced_input::Style {
            border: border(ring_color, 1.5, radius),
            ..base
        },
        iced_input::Status::Disabled => iced_input::Style {
            background: Background::Color(c.muted),
            value: c.muted_foreground,
            placeholder: with_alpha(c.muted_foreground, 0.6),
            ..base
        },
    }
}
