//! Tabs composite — horizontal tab bar.
//!
//! The widget is stateless: callers own the active tab id and map the
//! selected id back to content in their view. Items are generic over a
//! `Value: Eq + Clone` so ids can be enum variants, strings, or indices.
//!
//! ```ignore
//! let bar = tabs(self.active_tab, Message::TabSelected)
//!     .push(Tab::new(PageTab::Clips, "Clips"))
//!     .push(Tab::new(PageTab::Settings, "Settings").badge("3"));
//! ```

use iced::widget::{Row, button, container, row, text};
use iced::{Background, Element, Length, Padding};

use crate::ui::theme::{self, Tokens, border};

pub struct Tab<Value> {
    pub value: Value,
    pub label: String,
    pub badge: Option<String>,
    pub disabled: bool,
}

impl<Value> Tab<Value> {
    pub fn new(value: Value, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
            badge: None,
            disabled: false,
        }
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

pub fn tabs<'a, Value, Message, F>(
    active: Value,
    on_select: F,
) -> TabBarBuilder<'a, Value, Message, F>
where
    Value: Eq + Clone + 'a,
    Message: Clone + 'a,
    F: Fn(Value) -> Message + Copy + 'a,
{
    TabBarBuilder {
        active,
        on_select,
        items: Vec::new(),
        _marker: std::marker::PhantomData,
    }
}

pub struct TabBarBuilder<'a, Value, Message, F> {
    active: Value,
    on_select: F,
    items: Vec<Tab<Value>>,
    _marker: std::marker::PhantomData<&'a Message>,
}

impl<'a, Value, Message, F> TabBarBuilder<'a, Value, Message, F>
where
    Value: Eq + Clone + 'a,
    Message: Clone + 'a,
    F: Fn(Value) -> Message + Copy + 'a,
{
    pub fn push(mut self, tab: Tab<Value>) -> Self {
        self.items.push(tab);
        self
    }

    pub fn build(self) -> Element<'a, Message> {
        let space = &theme::SPACE;
        let font = &theme::FONT;

        let mut bar: Row<'a, Message> = row![].spacing(space.xs).align_y(iced::Alignment::Center);

        for tab in self.items {
            let is_active = tab.value == self.active;
            let disabled = tab.disabled;

            let label = text(tab.label)
                .size(font.size_sm)
                .style(move |theme: &iced::Theme| iced::widget::text::Style {
                    color: Some(label_color(theme, is_active, disabled)),
                });

            let mut inner: Row<'a, Message> = row![label]
                .spacing(space.xs)
                .align_y(iced::Alignment::Center);

            if let Some(badge) = tab.badge {
                let badge_text =
                    text(badge)
                        .size(font.size_xs)
                        .style(move |theme: &iced::Theme| iced::widget::text::Style {
                            color: Some(label_color(theme, is_active, disabled)),
                        });
                let wrapped = container(badge_text)
                    .padding(Padding {
                        top: 0.0,
                        bottom: 0.0,
                        left: space.xs + 2.0,
                        right: space.xs + 2.0,
                    })
                    .style(move |theme| badge_style(theme, is_active));
                inner = inner.push(wrapped);
            }

            let content = container(inner).padding(Padding {
                top: space.xs + 2.0,
                bottom: space.xs + 2.0,
                left: space.md,
                right: space.md,
            });

            let mut btn = button(content)
                .padding(0)
                .style(move |theme, status| tab_style(theme, status, is_active, disabled));

            if !disabled {
                let value = tab.value.clone();
                btn = btn.on_press((self.on_select)(value));
            }

            bar = bar.push(btn);
        }

        container(bar)
            .padding(Padding {
                top: space.xs,
                bottom: space.xs,
                left: space.xs,
                right: space.xs,
            })
            .width(Length::Shrink)
            .style(track_style)
            .into()
    }
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

fn tab_style(
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
        Some(Background::Color(c.card))
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
            c.foreground
        } else {
            c.muted_foreground
        },
        border: border(iced::Color::TRANSPARENT, 0.0, tokens.radius.md),
        shadow: if is_active {
            tokens.shadow.sm
        } else {
            Default::default()
        },
        snap: false,
    }
}

fn badge_style(theme: &iced::Theme, is_active: bool) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    let bg = if is_active { c.accent } else { c.muted };
    iced::widget::container::Style {
        text_color: None,
        background: Some(Background::Color(bg)),
        border: border(iced::Color::TRANSPARENT, 0.0, tokens.radius.full),
        shadow: Default::default(),
        snap: false,
    }
}

fn track_style(theme: &iced::Theme) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    iced::widget::container::Style {
        text_color: Some(c.muted_foreground),
        background: Some(Background::Color(c.muted)),
        border: border(c.border, 1.0, tokens.radius.md),
        shadow: Default::default(),
        snap: false,
    }
}
