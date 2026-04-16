//! Sidebar composite — vertical navigation column.
//!
//! Generic over a `Value: Eq + Clone` key that identifies each nav entry.
//! Items support icons (any `Element`), a badge, active highlighting,
//! and disabled state. Groups render as a small muted header above a
//! cluster of items.

use iced::widget::{Column, Container, Row, button, column, container, row, text};
use iced::{Background, Element, Length, Padding};

use crate::ui::theme::{self, Tokens, border};

pub struct SidebarItem<'a, Value, Message> {
    pub value: Value,
    pub label: String,
    pub icon: Option<Element<'a, Message>>,
    pub badge: Option<String>,
    pub disabled: bool,
}

impl<'a, Value, Message> SidebarItem<'a, Value, Message> {
    pub fn new(value: Value, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
            icon: None,
            badge: None,
            disabled: false,
        }
    }

    pub fn icon(mut self, icon: impl Into<Element<'a, Message>>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn badge(mut self, badge: impl Into<String>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

enum Entry<'a, Value, Message> {
    Item(SidebarItem<'a, Value, Message>),
    Group(String),
    Spacer,
}

pub struct Sidebar<'a, Value, Message, F> {
    active: Value,
    on_select: F,
    entries: Vec<Entry<'a, Value, Message>>,
    width: f32,
    header: Option<Element<'a, Message>>,
    footer: Option<Element<'a, Message>>,
}

pub fn sidebar<'a, Value, Message, F>(active: Value, on_select: F) -> Sidebar<'a, Value, Message, F>
where
    Value: Eq + Clone + 'a,
    Message: Clone + 'a,
    F: Fn(Value) -> Message + Copy + 'a,
{
    Sidebar {
        active,
        on_select,
        entries: Vec::new(),
        width: 240.0,
        header: None,
        footer: None,
    }
}

impl<'a, Value, Message, F> Sidebar<'a, Value, Message, F>
where
    Value: Eq + Clone + 'a,
    Message: Clone + 'a,
    F: Fn(Value) -> Message + Copy + 'a,
{
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn header(mut self, header: impl Into<Element<'a, Message>>) -> Self {
        self.header = Some(header.into());
        self
    }

    pub fn footer(mut self, footer: impl Into<Element<'a, Message>>) -> Self {
        self.footer = Some(footer.into());
        self
    }

    pub fn push(mut self, item: SidebarItem<'a, Value, Message>) -> Self {
        self.entries.push(Entry::Item(item));
        self
    }

    pub fn group(mut self, label: impl Into<String>) -> Self {
        self.entries.push(Entry::Group(label.into()));
        self
    }

    pub fn spacer(mut self) -> Self {
        self.entries.push(Entry::Spacer);
        self
    }

    pub fn build(self) -> Container<'a, Message> {
        let space = &theme::SPACE;
        let font = &theme::FONT;

        let mut col: Column<'a, Message> = column![].spacing(space.xxs);

        if let Some(header) = self.header {
            col = col.push(container(header).padding(Padding {
                top: space.md,
                bottom: space.md,
                left: space.md,
                right: space.md,
            }));
        }

        for entry in self.entries {
            match entry {
                Entry::Item(item) => {
                    col = col.push(item_row::<'a, Value, Message, F>(
                        item,
                        self.active.clone(),
                        self.on_select,
                        space,
                        font,
                    ));
                }
                Entry::Group(label) => {
                    col = col.push(
                        container(text(label).size(font.size_xs).style(|theme: &iced::Theme| {
                            iced::widget::text::Style {
                                color: Some(theme::tokens_for(theme).color.muted_foreground),
                            }
                        }))
                        .padding(Padding {
                            top: space.md,
                            bottom: space.xxs,
                            left: space.md,
                            right: space.md,
                        }),
                    );
                }
                Entry::Spacer => {
                    col = col.push(
                        iced::widget::Space::new()
                            .width(Length::Fill)
                            .height(Length::Fixed(space.md)),
                    );
                }
            }
        }

        if let Some(footer) = self.footer {
            col = col
                .push(iced::widget::Space::new().height(Length::Fill))
                .push(container(footer).padding(Padding {
                    top: space.md,
                    bottom: space.md,
                    left: space.md,
                    right: space.md,
                }));
        }

        container(col)
            .padding(Padding {
                top: space.sm,
                bottom: space.sm,
                left: space.sm,
                right: space.sm,
            })
            .width(Length::Fixed(self.width))
            .height(Length::Fill)
            .style(sidebar_style)
    }
}

impl<'a, Value, Message, F> From<Sidebar<'a, Value, Message, F>> for Element<'a, Message>
where
    Value: Eq + Clone + 'a,
    Message: Clone + 'a,
    F: Fn(Value) -> Message + Copy + 'a,
{
    fn from(s: Sidebar<'a, Value, Message, F>) -> Self {
        s.build().into()
    }
}

fn item_row<'a, Value, Message, F>(
    item: SidebarItem<'a, Value, Message>,
    active: Value,
    on_select: F,
    space: &'a crate::ui::theme::SpaceTokens,
    font: &'a crate::ui::theme::FontTokens,
) -> Element<'a, Message>
where
    Value: Eq + Clone + 'a,
    Message: Clone + 'a,
    F: Fn(Value) -> Message + Copy + 'a,
{
    let is_active = item.value == active;
    let disabled = item.disabled;

    let label = text(item.label.clone())
        .size(font.size_sm)
        .style(move |theme: &iced::Theme| iced::widget::text::Style {
            color: Some(label_color(theme, is_active, disabled)),
        });

    let mut inner: Row<'a, Message> = row![].spacing(space.sm).align_y(iced::Alignment::Center);
    if let Some(icon) = item.icon {
        inner = inner.push(icon);
    }
    inner = inner.push(label);

    if let Some(badge) = item.badge {
        let spacer = iced::widget::Space::new().width(Length::Fill);
        let badge_text =
            text(badge)
                .size(font.size_xs)
                .style(|theme: &iced::Theme| iced::widget::text::Style {
                    color: Some(theme::tokens_for(theme).color.muted_foreground),
                });
        let wrapped = container(badge_text).padding(Padding {
            top: 0.0,
            bottom: 0.0,
            left: space.xs + 2.0,
            right: space.xs + 2.0,
        });
        inner = inner.push(spacer).push(wrapped);
    }

    let content = container(inner)
        .padding(Padding {
            top: space.xs + 2.0,
            bottom: space.xs + 2.0,
            left: space.md,
            right: space.md,
        })
        .width(Length::Fill);

    let mut btn = button(content)
        .padding(0)
        .width(Length::Fill)
        .style(move |theme, status| item_style(theme, status, is_active, disabled));

    if !disabled {
        let value = item.value.clone();
        btn = btn.on_press(on_select(value));
    }

    btn.into()
}

fn label_color(theme: &iced::Theme, is_active: bool, disabled: bool) -> iced::Color {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    if disabled {
        c.muted_foreground
    } else if is_active {
        c.foreground
    } else {
        c.muted_foreground
    }
}

fn item_style(
    theme: &iced::Theme,
    status: iced::widget::button::Status,
    is_active: bool,
    disabled: bool,
) -> iced::widget::button::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;

    let bg = if disabled {
        None
    } else if is_active {
        Some(Background::Color(c.accent))
    } else {
        match status {
            iced::widget::button::Status::Hovered => Some(Background::Color(c.muted)),
            iced::widget::button::Status::Pressed => Some(Background::Color(c.accent)),
            _ => None,
        }
    };

    iced::widget::button::Style {
        background: bg,
        text_color: if is_active {
            c.accent_foreground
        } else {
            c.muted_foreground
        },
        border: border(iced::Color::TRANSPARENT, 0.0, tokens.radius.md),
        shadow: Default::default(),
        snap: false,
    }
}

fn sidebar_style(theme: &iced::Theme) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    iced::widget::container::Style {
        text_color: Some(c.foreground),
        background: Some(Background::Color(c.card)),
        border: iced::border::Border {
            color: c.border,
            width: 1.0,
            radius: 0.0.into(),
        },
        shadow: Default::default(),
        snap: false,
    }
}
