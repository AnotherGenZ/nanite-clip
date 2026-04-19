//! Tag primitive — similar to `badge` but rectangular with slightly larger
//! type, intended for filter chips and keywords.

use iced::widget::{Container, container};
use iced::{Background, Element, Padding};

use crate::ui::primitives::label::text;
use crate::ui::theme::{self, Tokens, border};

pub use super::badge::Tone;

pub fn tag<'a, Message: 'a>(label: impl Into<String>) -> TagBuilder<'a, Message> {
    TagBuilder {
        label: label.into(),
        tone: Tone::default(),
        _marker: std::marker::PhantomData,
    }
}

pub struct TagBuilder<'a, Message> {
    label: String,
    tone: Tone,
    _marker: std::marker::PhantomData<&'a Message>,
}

impl<'a, Message: 'a> TagBuilder<'a, Message> {
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    pub fn build(self) -> Container<'a, Message> {
        let tone = self.tone;
        let space = &theme::DARK.space;
        let label = text(self.label).size(theme::DARK.font.size_sm);
        container(label)
            .padding(Padding {
                top: space.xs,
                bottom: space.xs,
                left: space.sm + 2.0,
                right: space.sm + 2.0,
            })
            .style(move |theme| style_for(theme, tone))
    }
}

impl<'a, Message: 'a> From<TagBuilder<'a, Message>> for Element<'a, Message> {
    fn from(builder: TagBuilder<'a, Message>) -> Self {
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
        border: border(border_color, border_w, tokens.radius.md),
        shadow: Default::default(),
        snap: false,
    }
}
