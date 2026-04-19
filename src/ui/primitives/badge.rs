//! Badge primitive — a small pill showing status or counts.
//!
//! Uses a `container` around a `text` so it inherits placement from its parent
//! `row`/`column`.

use iced::widget::{Container, container};
use iced::{Background, Element, Padding};

use crate::ui::primitives::label::text;
use crate::ui::theme::{self, Tokens, border};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Neutral,
    Primary,
    Success,
    Warning,
    Destructive,
    Info,
    Outline,
}

pub fn badge<'a, Message: 'a>(label: impl Into<String>) -> BadgeBuilder<'a, Message> {
    BadgeBuilder {
        label: label.into(),
        tone: Tone::default(),
        _marker: std::marker::PhantomData,
    }
}

pub struct BadgeBuilder<'a, Message> {
    label: String,
    tone: Tone,
    _marker: std::marker::PhantomData<&'a Message>,
}

impl<'a, Message: 'a> BadgeBuilder<'a, Message> {
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    pub fn neutral(self) -> Self {
        self.tone(Tone::Neutral)
    }
    pub fn primary(self) -> Self {
        self.tone(Tone::Primary)
    }
    pub fn success(self) -> Self {
        self.tone(Tone::Success)
    }
    pub fn warning(self) -> Self {
        self.tone(Tone::Warning)
    }
    pub fn destructive(self) -> Self {
        self.tone(Tone::Destructive)
    }
    pub fn info(self) -> Self {
        self.tone(Tone::Info)
    }
    pub fn outline(self) -> Self {
        self.tone(Tone::Outline)
    }

    pub fn build(self) -> Container<'a, Message> {
        let tone = self.tone;
        let space = &theme::DARK.space;
        let label = text(self.label).size(theme::DARK.font.size_xs);
        container(label)
            .padding(Padding {
                top: 2.0,
                bottom: 2.0,
                left: space.sm,
                right: space.sm,
            })
            .style(move |theme| style_for(theme, tone))
    }
}

impl<'a, Message: 'a> From<BadgeBuilder<'a, Message>> for Element<'a, Message> {
    fn from(builder: BadgeBuilder<'a, Message>) -> Self {
        builder.build().into()
    }
}

fn style_for(theme: &iced::Theme, tone: Tone) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;

    let (bg, fg, border_color, border_w) = match tone {
        Tone::Neutral => (Some(c.secondary), c.secondary_foreground, c.border, 1.0),
        Tone::Primary => (Some(c.primary), c.primary_foreground, c.primary, 0.0),
        Tone::Success => (Some(c.success), c.success_foreground, c.success, 0.0),
        Tone::Warning => (Some(c.warning), c.warning_foreground, c.warning, 0.0),
        Tone::Destructive => (
            Some(c.destructive),
            c.destructive_foreground,
            c.destructive,
            0.0,
        ),
        Tone::Info => (Some(c.info), c.info_foreground, c.info, 0.0),
        Tone::Outline => (None, c.foreground, c.border_strong, 1.0),
    };

    iced::widget::container::Style {
        text_color: Some(fg),
        background: bg.map(Background::Color),
        border: border(border_color, border_w, tokens.radius.full),
        shadow: Default::default(),
        snap: false,
    }
}
