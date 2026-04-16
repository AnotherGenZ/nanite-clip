//! Shared UI widgets and styling helpers used across every tab.

use iced::Element;

use crate::ui::app::{
    ButtonVariant, NaniteButton, TooltipPosition, button, button_with, fa_icon_solid, pick_list,
    row, text, text_input, tooltip,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ButtonTone {
    Primary,
    Secondary,
    Success,
    Warning,
    Danger,
}

pub(super) fn styled_button<'a, M: Clone + 'a>(
    label: impl Into<String>,
    tone: ButtonTone,
) -> NaniteButton<'a, M> {
    let button = button(label.into()).size(crate::ui::primitives::button::Size::Sm);

    match tone {
        ButtonTone::Primary => button.variant(ButtonVariant::Primary),
        ButtonTone::Secondary => button.variant(ButtonVariant::Secondary),
        ButtonTone::Success => button.variant(ButtonVariant::Success),
        ButtonTone::Warning => button.variant(ButtonVariant::Warning),
        ButtonTone::Danger => button.variant(ButtonVariant::Destructive),
    }
}

pub(super) fn styled_button_row<'a, M>(
    content: impl Into<Element<'a, M>>,
    tone: ButtonTone,
) -> NaniteButton<'a, M>
where
    M: Clone + 'a,
{
    let button = button_with(content).size(crate::ui::primitives::button::Size::Sm);

    match tone {
        ButtonTone::Primary => button.variant(ButtonVariant::Primary),
        ButtonTone::Secondary => button.variant(ButtonVariant::Secondary),
        ButtonTone::Success => button.variant(ButtonVariant::Success),
        ButtonTone::Warning => button.variant(ButtonVariant::Warning),
        ButtonTone::Danger => button.variant(ButtonVariant::Destructive),
    }
}

pub(super) fn solid_icon<'a, M: 'a>(name: &'static str, size: f32) -> Element<'a, M> {
    fa_icon_solid(name).size(size).into()
}

pub(super) fn icon_label<'a, M: 'a>(
    icon: &'static str,
    label: impl Into<String>,
) -> Element<'a, M> {
    row![solid_icon(icon, 14.0), text(label.into()).size(14)]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .into()
}

pub(super) fn with_tooltip<'a, M: 'a>(
    content: Element<'a, M>,
    description: impl Into<String>,
) -> Element<'a, M> {
    tooltip(content, description, TooltipPosition::Bottom).into()
}

pub(super) fn field_label<'a, M: 'a>(
    label: &'a str,
    description: &'a str,
    width: f32,
) -> Element<'a, M> {
    with_tooltip(text(label).size(14).width(width).into(), description)
}

pub(super) fn settings_text_field<'a, M: Clone + 'a>(
    label: &'a str,
    description: &'a str,
    value: &str,
    on_change: impl Fn(String) -> M + 'a,
) -> Element<'a, M> {
    row![
        field_label(label, description, 200.0),
        text_input("", value).on_input(on_change).width(300),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into()
}

pub(super) fn settings_text_field_with_button<'a, M: Clone + 'a>(
    label: &'a str,
    description: &'a str,
    value: &str,
    on_change: impl Fn(String) -> M + 'a,
    control: Element<'a, M>,
) -> Element<'a, M> {
    row![
        field_label(label, description, 200.0),
        text_input("", value).on_input(on_change).width(300),
        control,
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into()
}

pub(super) fn settings_pick_list_field<'a, T, M>(
    label: &'a str,
    description: &'a str,
    options: &'a [T],
    selected: Option<T>,
    on_change: impl Fn(T) -> M + 'a,
) -> Element<'a, M>
where
    T: ToString + PartialEq + Clone + 'a,
    M: Clone + 'a,
{
    row![
        field_label(label, description, 200.0),
        pick_list(options, selected, on_change).width(300),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into()
}

pub(super) fn settings_stepper_field<'a, M: Clone + 'a>(
    label: &'a str,
    description: &'a str,
    value: u32,
    unit: &'a str,
    on_step: impl Fn(i32) -> M + Copy + 'a,
) -> Element<'a, M> {
    row![
        field_label(label, description, 200.0),
        row![
            styled_button("-", ButtonTone::Secondary).on_press(on_step(-1)),
            text(format!("{value} {unit}")).width(64).center(),
            styled_button("+", ButtonTone::Secondary).on_press(on_step(1)),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into()
}
