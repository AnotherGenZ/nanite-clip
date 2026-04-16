//! Inline banner — persistent notice rendered directly in layout flow.
//!
//! Banners live in the overlay module only because they share the tone
//! vocabulary with toasts; they are *not* actually overlaid — they sit in
//! normal column flow and take up their own space. Use them for blocking
//! errors that must stay visible until the user acknowledges them.

use iced::widget::{Column, Container, button, column, container, row, text};
use iced::{Background, Element, Length, Padding};

use crate::ui::theme::{self, Tokens, border};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Info,
    Success,
    Warning,
    Error,
    Neutral,
}

pub fn banner<'a, Message: 'a + Clone>(title: impl Into<String>) -> BannerBuilder<'a, Message> {
    BannerBuilder {
        title: title.into(),
        description: None,
        tone: Tone::default(),
        on_dismiss: None,
        _marker: std::marker::PhantomData,
    }
}

pub struct BannerBuilder<'a, Message> {
    title: String,
    description: Option<String>,
    tone: Tone,
    on_dismiss: Option<Message>,
    _marker: std::marker::PhantomData<&'a Message>,
}

impl<'a, Message: 'a + Clone> BannerBuilder<'a, Message> {
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    pub fn info(self) -> Self {
        self.tone(Tone::Info)
    }
    pub fn success(self) -> Self {
        self.tone(Tone::Success)
    }
    pub fn warning(self) -> Self {
        self.tone(Tone::Warning)
    }
    pub fn error(self) -> Self {
        self.tone(Tone::Error)
    }
    pub fn neutral(self) -> Self {
        self.tone(Tone::Neutral)
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn on_dismiss(mut self, message: Message) -> Self {
        self.on_dismiss = Some(message);
        self
    }

    pub fn build(self) -> Container<'a, Message> {
        let space = &theme::SPACE;
        let font = &theme::FONT;
        let tone = self.tone;

        let title_text = text(self.title)
            .size(font.size_base)
            .style(|theme: &iced::Theme| iced::widget::text::Style {
                color: Some(theme::tokens_for(theme).color.foreground),
            });

        let mut body: Column<'a, Message> = column![title_text].spacing(space.xxs);

        if let Some(desc) = self.description {
            let desc_text = text(desc).size(font.size_sm).style(|theme: &iced::Theme| {
                iced::widget::text::Style {
                    color: Some(theme::tokens_for(theme).color.muted_foreground),
                }
            });
            body = body.push(desc_text);
        }

        let mut inner = row![body.width(Length::Fill)]
            .spacing(space.md)
            .align_y(iced::Alignment::Start);

        if let Some(message) = self.on_dismiss {
            let close = button(text("\u{00D7}").size(font.size_lg))
                .padding(Padding {
                    top: 0.0,
                    bottom: 2.0,
                    left: space.xs,
                    right: space.xs,
                })
                .on_press(message)
                .style(dismiss_style);
            inner = inner.push(close);
        }

        container(inner)
            .padding(Padding {
                top: space.md,
                bottom: space.md,
                left: space.lg,
                right: space.md,
            })
            .width(Length::Fill)
            .style(move |theme| style_for(theme, tone))
    }
}

impl<'a, Message: 'a + Clone> From<BannerBuilder<'a, Message>> for Element<'a, Message> {
    fn from(builder: BannerBuilder<'a, Message>) -> Self {
        builder.build().into()
    }
}

fn style_for(theme: &iced::Theme, tone: Tone) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;

    let (accent, text_color, border_w) = match tone {
        Tone::Info => (c.info, c.foreground, 1.0),
        Tone::Success => (c.success, c.foreground, 1.0),
        Tone::Warning => (c.warning, c.foreground, 1.0),
        Tone::Error => (c.destructive, c.foreground, 1.0),
        Tone::Neutral => (c.border_strong, c.foreground, 1.0),
    };

    iced::widget::container::Style {
        text_color: Some(text_color),
        background: Some(Background::Color(c.card)),
        border: iced::border::Border {
            color: accent,
            width: border_w,
            radius: tokens.radius.md.into(),
        },
        shadow: tokens.shadow.sm,
        snap: false,
    }
}

fn dismiss_style(
    theme: &iced::Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;

    let (bg, fg) = match status {
        iced::widget::button::Status::Hovered => (Some(Background::Color(c.muted)), c.foreground),
        iced::widget::button::Status::Pressed => (Some(Background::Color(c.accent)), c.foreground),
        _ => (None, c.muted_foreground),
    };

    iced::widget::button::Style {
        background: bg,
        text_color: fg,
        border: border(iced::Color::TRANSPARENT, 0.0, tokens.radius.sm),
        shadow: Default::default(),
        snap: false,
    }
}
