//! Button primitive.
//!
//! Wraps `iced::widget::Button` with a set of semantic variants and sizes that
//! read from [`crate::ui::theme`]. Use [`button`] for a label-only button,
//! [`button_with`] when you need a custom child element (icon + text, etc.).

use iced::widget::{Button, button as iced_button, text};
use iced::{Background, Element, Length, Padding};

use crate::ui::theme::{self, Tokens, border, mix, with_alpha};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Variant {
    #[default]
    Primary,
    Secondary,
    Destructive,
    Outline,
    Ghost,
    Link,
    Success,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Size {
    Sm,
    #[default]
    Md,
    Lg,
    Icon,
}

impl Size {
    fn padding(self, space: &theme::SpaceTokens) -> Padding {
        match self {
            Size::Sm => Padding {
                top: space.xs,
                bottom: space.xs,
                left: space.sm + 2.0,
                right: space.sm + 2.0,
            },
            Size::Md => Padding {
                top: space.sm,
                bottom: space.sm,
                left: space.md + 2.0,
                right: space.md + 2.0,
            },
            Size::Lg => Padding {
                top: space.md,
                bottom: space.md,
                left: space.lg,
                right: space.lg,
            },
            Size::Icon => Padding {
                top: space.sm,
                bottom: space.sm,
                left: space.sm,
                right: space.sm,
            },
        }
    }

    fn font_size(self, font: &theme::FontTokens) -> f32 {
        match self {
            Size::Sm => font.size_sm,
            Size::Md => font.size_base,
            Size::Lg => font.size_lg,
            Size::Icon => font.size_base,
        }
    }
}

/// Create a button with a text label.
pub fn button<'a, Message>(label: impl Into<String>) -> NaniteButton<'a, Message> {
    let size = Size::default();
    let variant = Variant::default();
    let tokens = theme::DARK;
    NaniteButton {
        inner: iced_button(text(label.into()).size(size.font_size(&tokens.font))),
        variant,
        size,
    }
}

/// Create a button that wraps an arbitrary child element (icon + text, etc.).
pub fn button_with<'a, Message>(
    content: impl Into<Element<'a, Message>>,
) -> NaniteButton<'a, Message> {
    NaniteButton {
        inner: iced_button(content),
        variant: Variant::default(),
        size: Size::default(),
    }
}

/// Builder wrapper that keeps the chosen variant/size alive until conversion
/// into an `iced::Element`, so style and padding are applied at the right time.
pub struct NaniteButton<'a, Message> {
    inner: Button<'a, Message>,
    variant: Variant,
    size: Size,
}

impl<'a, Message: Clone + 'a> NaniteButton<'a, Message> {
    pub fn variant(mut self, variant: Variant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    pub fn primary(self) -> Self {
        self.variant(Variant::Primary)
    }

    pub fn secondary(self) -> Self {
        self.variant(Variant::Secondary)
    }

    pub fn destructive(self) -> Self {
        self.variant(Variant::Destructive)
    }

    pub fn outline(self) -> Self {
        self.variant(Variant::Outline)
    }

    pub fn ghost(self) -> Self {
        self.variant(Variant::Ghost)
    }

    pub fn link(self) -> Self {
        self.variant(Variant::Link)
    }

    pub fn success(self) -> Self {
        self.variant(Variant::Success)
    }

    pub fn warning(self) -> Self {
        self.variant(Variant::Warning)
    }

    pub fn on_press(mut self, message: Message) -> Self {
        self.inner = self.inner.on_press(message);
        self
    }

    pub fn on_press_maybe(mut self, message: Option<Message>) -> Self {
        self.inner = self.inner.on_press_maybe(message);
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.inner = self.inner.width(width);
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.inner = self.inner.height(height);
        self
    }

    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.inner = self.inner.padding(padding);
        self
    }

    pub fn build(self) -> Button<'a, Message> {
        let NaniteButton {
            inner,
            variant,
            size,
        } = self;
        let tokens = theme::DARK;
        let padding = size.padding(&tokens.space);
        inner
            .padding(padding)
            .style(move |theme, status| style_for(theme, status, variant))
    }
}

impl<'a, Message: Clone + 'a> From<NaniteButton<'a, Message>> for Element<'a, Message> {
    fn from(btn: NaniteButton<'a, Message>) -> Self {
        btn.build().into()
    }
}

fn style_for(
    theme: &iced::Theme,
    status: iced_button::Status,
    variant: Variant,
) -> iced_button::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    let radius = tokens.radius.md;

    let (bg, fg, border_color, border_w) = match variant {
        Variant::Primary => (Some(c.primary), c.primary_foreground, c.primary, 0.0),
        Variant::Secondary => (Some(c.secondary), c.secondary_foreground, c.border, 1.0),
        Variant::Destructive => (
            Some(c.destructive),
            c.destructive_foreground,
            c.destructive,
            0.0,
        ),
        Variant::Outline => (None, c.foreground, c.border_strong, 1.0),
        Variant::Ghost => (None, c.foreground, with_alpha(c.border, 0.0), 0.0),
        Variant::Link => (None, c.primary, with_alpha(c.border, 0.0), 0.0),
        Variant::Success => (Some(c.success), c.success_foreground, c.success, 0.0),
        Variant::Warning => (Some(c.warning), c.warning_foreground, c.warning, 0.0),
    };

    let base = iced_button::Style {
        background: bg.map(Background::Color),
        text_color: fg,
        border: border(border_color, border_w, radius),
        shadow: Default::default(),
        snap: false,
    };

    match status {
        iced_button::Status::Active => base,
        iced_button::Status::Hovered => hover(base, variant, tokens),
        iced_button::Status::Pressed => press(base, variant, tokens),
        iced_button::Status::Disabled => disabled(base),
    }
}

fn hover(base: iced_button::Style, variant: Variant, tokens: &Tokens) -> iced_button::Style {
    let c = &tokens.color;
    match variant {
        Variant::Primary => iced_button::Style {
            background: Some(Background::Color(c.primary_hover)),
            ..base
        },
        Variant::Secondary => iced_button::Style {
            background: Some(Background::Color(c.secondary_hover)),
            ..base
        },
        Variant::Destructive => iced_button::Style {
            background: Some(Background::Color(c.destructive_hover)),
            ..base
        },
        Variant::Outline => iced_button::Style {
            background: Some(Background::Color(c.accent)),
            ..base
        },
        Variant::Ghost => iced_button::Style {
            background: Some(Background::Color(c.accent)),
            ..base
        },
        Variant::Link => iced_button::Style {
            text_color: mix(c.primary, c.foreground, 0.15),
            ..base
        },
        Variant::Success => iced_button::Style {
            background: Some(Background::Color(mix(c.success, c.foreground, 0.1))),
            ..base
        },
        Variant::Warning => iced_button::Style {
            background: Some(Background::Color(mix(c.warning, c.foreground, 0.1))),
            ..base
        },
    }
}

fn press(base: iced_button::Style, variant: Variant, tokens: &Tokens) -> iced_button::Style {
    let c = &tokens.color;
    match variant {
        Variant::Primary => iced_button::Style {
            background: Some(Background::Color(c.primary_active)),
            ..base
        },
        Variant::Secondary => iced_button::Style {
            background: Some(Background::Color(c.secondary_active)),
            ..base
        },
        Variant::Destructive => iced_button::Style {
            background: Some(Background::Color(c.destructive_active)),
            ..base
        },
        _ => hover(base, variant, tokens),
    }
}

fn disabled(base: iced_button::Style) -> iced_button::Style {
    iced_button::Style {
        background: base.background.map(|bg| match bg {
            Background::Color(color) => Background::Color(with_alpha(color, 0.5)),
            other => other,
        }),
        text_color: with_alpha(base.text_color, 0.5),
        border: iced::border::Border {
            color: with_alpha(base.border.color, 0.5),
            ..base.border
        },
        ..base
    }
}
