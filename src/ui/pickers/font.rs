//! Font picker — thin wrapper over the themed [`pick_list`] primitive.
//!
//! Opinionated: it takes a slice of display names and emits a message
//! containing the selected `String`. The caller is responsible for mapping
//! that name back to an `iced::Font` (usually via a static map).
//! This keeps the picker independent of whatever font registry the app
//! ends up using.

use iced::widget::PickList;

use crate::ui::primitives::pick_list;

pub fn font_picker<'a, Message>(
    options: &'a [String],
    selected: Option<String>,
    on_select: impl Fn(String) -> Message + 'a,
) -> PickList<'a, String, &'a [String], String, Message>
where
    Message: Clone + 'a,
{
    pick_list::pick_list(options, selected, on_select)
}
