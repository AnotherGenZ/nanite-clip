//! Progress bar primitive.

use iced::widget::{ProgressBar, progress_bar};
use iced::{Background, border};

use crate::ui::theme::{self, Tokens};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Primary,
    Success,
    Warning,
    Destructive,
}

pub fn progress<'a>(range: std::ops::RangeInclusive<f32>, value: f32) -> ProgressBar<'a> {
    progress_with_tone(range, value, Tone::default())
}

pub fn progress_with_tone<'a>(
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    tone: Tone,
) -> ProgressBar<'a> {
    progress_bar(range, value)
        .girth(8.0)
        .style(move |theme| style_for(theme, tone))
}

fn style_for(theme: &iced::Theme, tone: Tone) -> progress_bar::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    let fill = match tone {
        Tone::Primary => c.primary,
        Tone::Success => c.success,
        Tone::Warning => c.warning,
        Tone::Destructive => c.destructive,
    };
    progress_bar::Style {
        background: Background::Color(c.muted),
        bar: Background::Color(fill),
        border: border::Border {
            radius: tokens.radius.full.into(),
            width: 0.0,
            color: iced::Color::TRANSPARENT,
        },
    }
}
