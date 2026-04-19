//! Color picker — preset swatch grid with an active indicator.
//!
//! Phase-6 scope is intentionally small: this is a preset-only picker
//! suitable for theme accents, tag colors, and category assignments.
//! Arbitrary HSV/RGB manipulation would need a proper canvas-backed
//! widget, which is out of scope here.

use iced::widget::{Row, button, column, container, row};
use iced::{Background, Color, Element, Length, Padding};

use crate::ui::primitives::label::text;
use crate::ui::theme::{self, Tokens, border};

/// A named preset swatch. Labels appear in the tooltip-less hex preview
/// below the grid when the swatch is active.
#[derive(Debug, Clone, Copy)]
pub struct Swatch {
    pub name: &'static str,
    pub color: Color,
}

impl Swatch {
    pub const fn new(name: &'static str, color: Color) -> Self {
        Self { name, color }
    }
}

/// Tailwind-ish default palette. 18 colors in a 6-wide grid.
pub const DEFAULT_PALETTE: &[Swatch] = &[
    Swatch::new("Slate", Color::from_rgb(0.40, 0.44, 0.52)),
    Swatch::new("Gray", Color::from_rgb(0.42, 0.45, 0.50)),
    Swatch::new("Zinc", Color::from_rgb(0.44, 0.44, 0.48)),
    Swatch::new("Red", Color::from_rgb(0.94, 0.27, 0.27)),
    Swatch::new("Orange", Color::from_rgb(0.98, 0.45, 0.09)),
    Swatch::new("Amber", Color::from_rgb(0.96, 0.62, 0.04)),
    Swatch::new("Yellow", Color::from_rgb(0.92, 0.70, 0.03)),
    Swatch::new("Lime", Color::from_rgb(0.51, 0.80, 0.09)),
    Swatch::new("Green", Color::from_rgb(0.13, 0.77, 0.37)),
    Swatch::new("Emerald", Color::from_rgb(0.06, 0.73, 0.51)),
    Swatch::new("Teal", Color::from_rgb(0.08, 0.72, 0.65)),
    Swatch::new("Cyan", Color::from_rgb(0.02, 0.71, 0.83)),
    Swatch::new("Sky", Color::from_rgb(0.02, 0.66, 0.96)),
    Swatch::new("Blue", Color::from_rgb(0.23, 0.51, 0.96)),
    Swatch::new("Indigo", Color::from_rgb(0.38, 0.40, 0.96)),
    Swatch::new("Violet", Color::from_rgb(0.55, 0.36, 0.97)),
    Swatch::new("Fuchsia", Color::from_rgb(0.85, 0.27, 0.94)),
    Swatch::new("Pink", Color::from_rgb(0.93, 0.28, 0.60)),
];

pub fn color_picker<'a, Message, F>(selected: Option<Color>, on_select: F) -> Element<'a, Message>
where
    Message: Clone + 'a,
    F: Fn(Color) -> Message + Copy + 'a,
{
    color_picker_with(DEFAULT_PALETTE, selected, on_select)
}

pub fn color_picker_with<'a, Message, F>(
    palette: &'static [Swatch],
    selected: Option<Color>,
    on_select: F,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
    F: Fn(Color) -> Message + Copy + 'a,
{
    let space = &theme::SPACE;
    let font = &theme::FONT;

    let mut grid_col = iced::widget::Column::new().spacing(space.xs);
    let mut iter = palette.iter();

    loop {
        let mut row_items: Row<'a, Message> = Row::new().spacing(space.xs);
        let mut any = false;
        for _ in 0..6 {
            if let Some(sw) = iter.next() {
                any = true;
                let is_active = selected.map(|c| colors_equal(c, sw.color)).unwrap_or(false);
                row_items = row_items.push(swatch_button::<Message, F>(*sw, is_active, on_select));
            }
        }
        if !any {
            break;
        }
        grid_col = grid_col.push(row_items);
    }

    let preview =
        if let Some(color) = selected {
            let hex = format_hex(color);
            row![
                container(iced::widget::Space::new())
                    .width(Length::Fixed(20.0))
                    .height(Length::Fixed(20.0))
                    .style(move |theme| hex_preview_style(theme, color)),
                text(hex).size(font.size_sm).style(|theme: &iced::Theme| {
                    iced::widget::text::Style {
                        color: Some(theme::tokens_for(theme).color.muted_foreground),
                    }
                }),
            ]
            .spacing(space.sm)
            .align_y(iced::Alignment::Center)
            .into()
        } else {
            Element::from(text("No color selected").size(font.size_sm).style(
                |theme: &iced::Theme| iced::widget::text::Style {
                    color: Some(theme::tokens_for(theme).color.muted_foreground),
                },
            ))
        };

    container(column![grid_col, preview].spacing(space.md))
        .padding(Padding {
            top: space.md,
            bottom: space.md,
            left: space.md,
            right: space.md,
        })
        .style(card_style)
        .into()
}

fn swatch_button<'a, Message, F>(
    swatch: Swatch,
    is_active: bool,
    on_select: F,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
    F: Fn(Color) -> Message + Copy + 'a,
{
    let color = swatch.color;
    let cell = container(iced::widget::Space::new())
        .width(Length::Fixed(28.0))
        .height(Length::Fixed(28.0));

    button(cell)
        .padding(0)
        .style(move |theme, status| swatch_style(theme, status, color, is_active))
        .on_press(on_select(color))
        .into()
}

fn swatch_style(
    theme: &iced::Theme,
    status: iced::widget::button::Status,
    color: Color,
    is_active: bool,
) -> iced::widget::button::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;

    let (border_color, border_w) = if is_active {
        (c.foreground, 2.0)
    } else {
        match status {
            iced::widget::button::Status::Hovered => (c.border_strong, 1.5),
            _ => (c.border, 1.0),
        }
    };

    iced::widget::button::Style {
        background: Some(Background::Color(color)),
        text_color: iced::Color::TRANSPARENT,
        border: border(border_color, border_w, tokens.radius.sm),
        shadow: Default::default(),
        snap: false,
    }
}

fn card_style(theme: &iced::Theme) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    iced::widget::container::Style {
        text_color: Some(c.popover_foreground),
        background: Some(Background::Color(c.popover)),
        border: border(c.border_strong, 1.0, tokens.radius.lg),
        shadow: tokens.shadow.lg,
        snap: false,
    }
}

fn hex_preview_style(theme: &iced::Theme, color: Color) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    iced::widget::container::Style {
        text_color: None,
        background: Some(Background::Color(color)),
        border: border(tokens.color.border_strong, 1.0, tokens.radius.sm),
        shadow: Default::default(),
        snap: false,
    }
}

fn colors_equal(a: Color, b: Color) -> bool {
    let eps = 0.002;
    (a.r - b.r).abs() < eps
        && (a.g - b.g).abs() < eps
        && (a.b - b.b).abs() < eps
        && (a.a - b.a).abs() < eps
}

fn format_hex(color: Color) -> String {
    let r = (color.r.clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (color.g.clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (color.b.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}
