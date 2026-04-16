//! Dropdown menu card — a vertical list of clickable rows.
//!
//! The menu card itself is just a container column; it does not manage
//! open/closed state or positioning. Callers open it by hosting it inside a
//! [`popover`](super::popover) (or any layout) and track open state in their
//! own app model.
//!
//! ```ignore
//! let menu = Menu::new()
//!     .item(MenuItem::new("Open", Message::OpenFile))
//!     .item(MenuItem::new("Save", Message::SaveFile).shortcut("Ctrl+S"))
//!     .separator()
//!     .item(MenuItem::new("Delete", Message::Delete).destructive());
//!
//! let view = popover(base, menu.build(), Anchor::TopRight, Some(Message::CloseMenu));
//! ```

use iced::widget::{Column, button, column, container, row, text};
use iced::{Background, Element, Length, Padding};

use crate::ui::theme::{self, Tokens, border};

#[derive(Debug, Clone)]
pub struct MenuItem<Message> {
    label: String,
    shortcut: Option<String>,
    on_select: Option<Message>,
    destructive: bool,
    disabled: bool,
}

impl<Message> MenuItem<Message> {
    pub fn new(label: impl Into<String>, on_select: Message) -> Self {
        Self {
            label: label.into(),
            shortcut: None,
            on_select: Some(on_select),
            destructive: false,
            disabled: false,
        }
    }

    pub fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        if disabled {
            self.on_select = None;
        }
        self
    }
}

enum Entry<Message> {
    Item(MenuItem<Message>),
    Separator,
    Header(String),
}

pub struct Menu<Message> {
    entries: Vec<Entry<Message>>,
    width: f32,
}

impl<Message> Default for Menu<Message> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            width: 220.0,
        }
    }
}

impl<Message> Menu<Message> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn item(mut self, item: MenuItem<Message>) -> Self {
        self.entries.push(Entry::Item(item));
        self
    }

    pub fn separator(mut self) -> Self {
        self.entries.push(Entry::Separator);
        self
    }

    pub fn header(mut self, label: impl Into<String>) -> Self {
        self.entries.push(Entry::Header(label.into()));
        self
    }
}

impl<Message> Menu<Message>
where
    Message: Clone + 'static,
{
    pub fn build<'a>(self) -> Element<'a, Message>
    where
        Message: 'a,
    {
        let space = &theme::SPACE;
        let font = &theme::FONT;

        let mut col: Column<'a, Message> = column![].spacing(0.0).width(Length::Fixed(self.width));

        for entry in self.entries {
            match entry {
                Entry::Item(item) => col = col.push(item_row::<'a, Message>(item, font, space)),
                Entry::Separator => {
                    col = col.push(
                        container(iced::widget::Space::new())
                            .height(Length::Fixed(1.0))
                            .width(Length::Fill)
                            .padding(Padding {
                                top: space.xxs,
                                bottom: space.xxs,
                                left: 0.0,
                                right: 0.0,
                            })
                            .style(|theme: &iced::Theme| iced::widget::container::Style {
                                text_color: None,
                                background: Some(Background::Color(
                                    theme::tokens_for(theme).color.border,
                                )),
                                border: iced::border::Border::default(),
                                shadow: Default::default(),
                                snap: false,
                            }),
                    );
                }
                Entry::Header(label) => {
                    col = col.push(
                        container(text(label).size(font.size_xs).style(|theme: &iced::Theme| {
                            iced::widget::text::Style {
                                color: Some(theme::tokens_for(theme).color.muted_foreground),
                            }
                        }))
                        .padding(Padding {
                            top: space.sm,
                            bottom: space.xs,
                            left: space.md,
                            right: space.md,
                        }),
                    );
                }
            }
        }

        container(col)
            .padding(Padding {
                top: space.xs,
                bottom: space.xs,
                left: space.xs,
                right: space.xs,
            })
            .width(Length::Fixed(self.width))
            .style(card_style)
            .into()
    }
}

fn item_row<'a, Message>(
    item: MenuItem<Message>,
    font: &'a crate::ui::theme::FontTokens,
    space: &'a crate::ui::theme::SpaceTokens,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let destructive = item.destructive;
    let disabled = item.disabled;

    let label = text(item.label.clone())
        .size(font.size_sm)
        .style(move |theme: &iced::Theme| iced::widget::text::Style {
            color: Some(label_color(theme, destructive, disabled)),
        });

    let mut inner = row![label]
        .spacing(space.md)
        .align_y(iced::Alignment::Center);

    if let Some(shortcut) = item.shortcut.clone() {
        let spacer = iced::widget::Space::new().width(Length::Fill);
        let shortcut_text = text(shortcut)
            .size(font.size_xs)
            .style(|theme: &iced::Theme| iced::widget::text::Style {
                color: Some(theme::tokens_for(theme).color.muted_foreground),
            });
        inner = inner.push(spacer).push(shortcut_text);
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
        .style(move |theme: &iced::Theme, status| item_style(theme, status, destructive, disabled))
        .padding(0);

    if let Some(msg) = item.on_select {
        btn = btn.on_press(msg);
    }

    btn.into()
}

fn label_color(theme: &iced::Theme, destructive: bool, disabled: bool) -> iced::Color {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    if disabled {
        c.muted_foreground
    } else if destructive {
        c.destructive
    } else {
        c.popover_foreground
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

fn item_style(
    theme: &iced::Theme,
    status: iced::widget::button::Status,
    destructive: bool,
    disabled: bool,
) -> iced::widget::button::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;

    let (bg, fg) = if disabled {
        (None, c.muted_foreground)
    } else {
        match status {
            iced::widget::button::Status::Hovered => {
                if destructive {
                    (
                        Some(Background::Color(c.destructive)),
                        c.destructive_foreground,
                    )
                } else {
                    (Some(Background::Color(c.accent)), c.accent_foreground)
                }
            }
            iced::widget::button::Status::Pressed => {
                if destructive {
                    (
                        Some(Background::Color(c.destructive_active)),
                        c.destructive_foreground,
                    )
                } else {
                    (Some(Background::Color(c.accent)), c.accent_foreground)
                }
            }
            _ => (
                None,
                if destructive {
                    c.destructive
                } else {
                    c.popover_foreground
                },
            ),
        }
    };

    iced::widget::button::Style {
        background: bg,
        text_color: fg,
        border: border(iced::Color::TRANSPARENT, 0.0, tokens.radius.md),
        shadow: Default::default(),
        snap: false,
    }
}
