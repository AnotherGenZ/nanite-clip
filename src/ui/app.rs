//! App-facing UI facade.
//!
//! The application modules should import widgets from here instead of
//! depending on `iced::widget` directly. Primitive inputs/buttons route
//! through nanite-ui, while structural helpers re-export or wrap the small
//! set of generic layout widgets the current screens still need.

use iced::Element;
use iced::widget::{
    Container, MouseArea, Scrollable, Space, container as iced_container,
    mouse_area as iced_mouse_area, scrollable as iced_scrollable,
};

pub use iced::widget::{Column, center, column, row, stack};
pub use iced_font_awesome::fa_icon_solid;

pub use crate::ui::primitives::button::{
    NaniteButton, Variant as ButtonVariant, button, button_with,
};
pub use crate::ui::primitives::pick_list::pick_list;
pub use crate::ui::primitives::tooltip::{Position as TooltipPosition, tooltip};

pub type ContainerStyle = iced::widget::container::Style;
pub type TextStyle = iced::widget::text::Style;

pub fn text<'a>(label: impl Into<String>) -> crate::ui::primitives::label::Text<'a> {
    crate::ui::primitives::label::text(label)
}

pub fn text_non_selectable<'a>(label: impl Into<String>) -> crate::ui::primitives::label::Text<'a> {
    crate::ui::primitives::label::text_non_selectable(label)
}

pub fn checkbox<'a, Message: 'a>(is_checked: bool) -> iced::widget::Checkbox<'a, Message> {
    crate::ui::primitives::checkbox::checkbox("", is_checked)
}

pub fn text_input<'a, Message: Clone + 'a>(
    placeholder: &str,
    value: &str,
) -> crate::ui::primitives::input::NaniteInput<'a, Message> {
    crate::ui::primitives::input::input(placeholder, value)
}

pub fn container<'a, Message>(child: impl Into<Element<'a, Message>>) -> Container<'a, Message> {
    iced_container(child)
}

pub fn scrollable<'a, Message>(child: impl Into<Element<'a, Message>>) -> Scrollable<'a, Message> {
    iced_scrollable(child).spacing(4)
}

pub fn mouse_area<'a, Message>(child: impl Into<Element<'a, Message>>) -> MouseArea<'a, Message> {
    iced_mouse_area(child)
}

pub fn horizontal_space() -> Space {
    Space::new().width(iced::Length::Fill)
}

pub fn vertical_space() -> Space {
    Space::new().height(iced::Length::Fill)
}

pub fn rounded_box(theme: &iced::Theme) -> ContainerStyle {
    iced::widget::container::rounded_box(theme)
}

pub fn transparent_box(theme: &iced::Theme) -> ContainerStyle {
    iced::widget::container::transparent(theme)
}

pub mod rule {
    use iced::widget::Rule;

    pub fn horizontal<'a>(_thickness: u16) -> Rule<'a> {
        crate::ui::primitives::separator::horizontal()
    }

    pub fn vertical<'a>(_thickness: u16) -> Rule<'a> {
        crate::ui::primitives::separator::vertical()
    }
}
