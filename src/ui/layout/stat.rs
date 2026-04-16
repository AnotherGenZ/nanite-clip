//! Stat composite — a small KPI card with label, value, and optional
//! delta indicator (e.g. "+12.4%"). Delta tone controls color.

use iced::widget::{Column, Container, column, container, row, text};
use iced::{Background, Element, Length, Padding};

use crate::ui::theme::{self, Tokens, border};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeltaTone {
    #[default]
    Neutral,
    Positive,
    Negative,
}

pub fn stat<'a, Message: 'a>(
    label: impl Into<String>,
    value: impl Into<String>,
) -> StatBuilder<'a, Message> {
    StatBuilder {
        label: label.into(),
        value: value.into(),
        delta: None,
        delta_tone: DeltaTone::default(),
        width: Length::Fill,
        _marker: std::marker::PhantomData,
    }
}

pub struct StatBuilder<'a, Message> {
    label: String,
    value: String,
    delta: Option<String>,
    delta_tone: DeltaTone,
    width: Length,
    _marker: std::marker::PhantomData<&'a Message>,
}

impl<'a, Message: 'a> StatBuilder<'a, Message> {
    pub fn delta(mut self, delta: impl Into<String>, tone: DeltaTone) -> Self {
        self.delta = Some(delta.into());
        self.delta_tone = tone;
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn build(self) -> Container<'a, Message> {
        let space = &theme::SPACE;
        let font = &theme::FONT;
        let delta_tone = self.delta_tone;

        let label_el = text(self.label)
            .size(font.size_xs)
            .style(|theme: &iced::Theme| iced::widget::text::Style {
                color: Some(theme::tokens_for(theme).color.muted_foreground),
            });

        let value_el = text(self.value)
            .size(font.size_2xl)
            .style(|theme: &iced::Theme| iced::widget::text::Style {
                color: Some(theme::tokens_for(theme).color.foreground),
            });

        let mut col: Column<'a, Message> = column![label_el, value_el].spacing(space.xxs);

        if let Some(delta) = self.delta {
            let delta_text = text(delta)
                .size(font.size_sm)
                .style(move |theme: &iced::Theme| iced::widget::text::Style {
                    color: Some(delta_color(theme, delta_tone)),
                });
            let wrapped = container(row![delta_text]).padding(Padding {
                top: 0.0,
                bottom: 0.0,
                left: 0.0,
                right: 0.0,
            });
            col = col.push(wrapped);
        }

        container(col)
            .padding(Padding {
                top: space.lg,
                bottom: space.lg,
                left: space.lg,
                right: space.lg,
            })
            .width(self.width)
            .style(stat_style)
    }
}

impl<'a, Message: 'a> From<StatBuilder<'a, Message>> for Element<'a, Message> {
    fn from(b: StatBuilder<'a, Message>) -> Self {
        b.build().into()
    }
}

fn delta_color(theme: &iced::Theme, tone: DeltaTone) -> iced::Color {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    match tone {
        DeltaTone::Neutral => c.muted_foreground,
        DeltaTone::Positive => c.success,
        DeltaTone::Negative => c.destructive,
    }
}

fn stat_style(theme: &iced::Theme) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    iced::widget::container::Style {
        text_color: Some(c.card_foreground),
        background: Some(Background::Color(c.card)),
        border: border(c.border, 1.0, tokens.radius.lg),
        shadow: tokens.shadow.sm,
        snap: false,
    }
}
