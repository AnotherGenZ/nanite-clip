//! Radio button primitive.

use iced::Background;
use iced::widget::{Radio, radio as iced_radio};

use crate::ui::theme::{self, Tokens};

pub fn radio<'a, Message, V>(
    label: impl Into<String>,
    value: V,
    selected: Option<V>,
    on_click: impl FnOnce(V) -> Message + 'a,
) -> Radio<'a, Message>
where
    Message: Clone + 'a,
    V: Eq + Copy,
{
    iced_radio(label.into(), value, selected, on_click)
        .size(16)
        .spacing(theme::DARK.space.sm)
        .text_size(theme::DARK.font.size_base)
        .style(style_for)
}

fn style_for(theme: &iced::Theme, status: iced_radio::Status) -> iced_radio::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;

    let (background, dot_color, border_color) = match status {
        iced_radio::Status::Active { is_selected } => {
            if is_selected {
                (c.input, c.primary, c.primary)
            } else {
                (c.input, c.input, c.border_strong)
            }
        }
        iced_radio::Status::Hovered { is_selected } => {
            if is_selected {
                (c.input, c.primary_hover, c.primary_hover)
            } else {
                (c.input, c.input, c.primary)
            }
        }
    };

    iced_radio::Style {
        background: Background::Color(background),
        dot_color,
        border_width: 1.5,
        border_color,
        text_color: Some(c.foreground),
    }
}
