//! PickList primitive — themed single-select dropdown.
//!
//! Wraps `iced::widget::pick_list` with the token-driven style closures so
//! callers can reach for `nanite_ui::primitives::pick_list::pick_list` the
//! same way they would `iced::widget::pick_list` and get themed visuals.

use std::borrow::Borrow;

use iced::widget::overlay::menu;
use iced::widget::{PickList, pick_list as iced_pick_list};
use iced::{Background, Padding};

use crate::ui::theme::{self, Tokens, border};

pub fn pick_list<'a, T, L, V, Message>(
    options: L,
    selected: Option<V>,
    on_select: impl Fn(T) -> Message + 'a,
) -> PickList<'a, T, L, V, Message>
where
    T: ToString + PartialEq + Clone + 'a,
    L: Borrow<[T]> + 'a,
    V: Borrow<T> + 'a,
    Message: Clone,
{
    iced_pick_list(options, selected, on_select)
        .padding(Padding {
            top: theme::SPACE.xs + 2.0,
            bottom: theme::SPACE.xs + 2.0,
            left: theme::SPACE.md,
            right: theme::SPACE.md,
        })
        .text_size(theme::FONT.size_base)
        .style(field_style)
        .menu_style(menu_style)
}

fn field_style(
    theme: &iced::Theme,
    status: iced::widget::pick_list::Status,
) -> iced::widget::pick_list::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;

    let border_color = match status {
        iced::widget::pick_list::Status::Active => c.border_strong,
        iced::widget::pick_list::Status::Hovered
        | iced::widget::pick_list::Status::Opened { .. } => c.primary,
    };

    iced::widget::pick_list::Style {
        text_color: c.input_foreground,
        placeholder_color: c.muted_foreground,
        handle_color: c.muted_foreground,
        background: Background::Color(c.input),
        border: border(border_color, 1.0, tokens.radius.md),
    }
}

fn menu_style(theme: &iced::Theme) -> menu::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;

    menu::Style {
        background: Background::Color(c.popover),
        border: border(c.border_strong, 1.0, tokens.radius.md),
        text_color: c.popover_foreground,
        selected_text_color: c.primary_foreground,
        selected_background: Background::Color(c.primary),
        shadow: tokens.shadow.md,
    }
}
