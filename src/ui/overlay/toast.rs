//! Toast stack — transient floating notifications.
//!
//! The app owns a [`ToastStack`] and pushes new toasts via [`ToastStack::push`]
//! or [`ToastStack::push_sticky`]. Each frame (or on an `iced::time::every`
//! subscription tick), call [`ToastStack::tick`] to garbage-collect expired
//! toasts, then compose the stack into the view by calling [`view`] and
//! layering the result over the main content with [`iced::widget::Stack`].
//!
//! Phase 2 keeps the widget self-contained: no animations, no bespoke
//! overlay trait impls, just a column of cards positioned in a corner.
//!
//! # Example
//!
//! ```ignore
//! // in App state:
//! toasts: ToastStack,
//!
//! // in update:
//! Message::SomethingHappened => {
//!     self.toasts.push(Tone::Success, "Saved", Some("Changes committed"));
//! }
//! Message::ToastTick => self.toasts.tick(),
//! Message::ToastDismiss(id) => self.toasts.dismiss(id),
//! Message::ToastToggleExpand(id) => self.toasts.toggle_expand(id),
//!
//! // in view:
//! stack![
//!     main,
//!     toast::view(
//!         &self.toasts,
//!         Corner::BottomRight,
//!         Message::ToastDismiss,
//!         Message::ToastToggleExpand,
//!     )
//! ]
//! ```

use std::time::{Duration, Instant};

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{Column, Container, Row, button, column, container, row, text};
use iced::{Background, Element, Length, Padding};

use crate::ui::theme::{self, Tokens, border, with_alpha};

/// Messages longer than this get a "Show more" toggle. Roughly 2–3 lines in
/// the collapsed 340px card.
const COLLAPSED_MESSAGE_CHAR_LIMIT: usize = 140;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ToastId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub id: ToastId,
    pub tone: Tone,
    pub title: String,
    pub message: Option<String>,
    pub created_at: Instant,
    pub duration: Option<Duration>,
    pub expanded: bool,
}

impl Toast {
    fn is_expired(&self, now: Instant) -> bool {
        match self.duration {
            Some(d) => now.duration_since(self.created_at) >= d,
            None => false,
        }
    }
}

#[derive(Debug, Default)]
pub struct ToastStack {
    toasts: Vec<Toast>,
    next_id: u64,
}

pub const DEFAULT_DURATION: Duration = Duration::from_secs(4);

impl ToastStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.toasts.is_empty()
    }

    pub fn len(&self) -> usize {
        self.toasts.len()
    }

    pub fn push(
        &mut self,
        tone: Tone,
        title: impl Into<String>,
        message: Option<String>,
    ) -> ToastId {
        self.push_with(tone, title, message, Some(DEFAULT_DURATION))
    }

    pub fn push_sticky(
        &mut self,
        tone: Tone,
        title: impl Into<String>,
        message: Option<String>,
    ) -> ToastId {
        self.push_with(tone, title, message, None)
    }

    pub fn push_with(
        &mut self,
        tone: Tone,
        title: impl Into<String>,
        message: Option<String>,
        duration: Option<Duration>,
    ) -> ToastId {
        let id = ToastId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.toasts.push(Toast {
            id,
            tone,
            title: title.into(),
            message,
            created_at: Instant::now(),
            duration,
            expanded: false,
        });
        id
    }

    pub fn dismiss(&mut self, id: ToastId) {
        self.toasts.retain(|t| t.id != id);
    }

    /// Toggle the expanded state of a toast. Expanding a toast also cancels
    /// its auto-dismiss timer so the user has time to finish reading.
    pub fn toggle_expand(&mut self, id: ToastId) {
        if let Some(toast) = self.toasts.iter_mut().find(|t| t.id == id) {
            toast.expanded = !toast.expanded;
            if toast.expanded {
                toast.duration = None;
            }
        }
    }

    pub fn clear(&mut self) {
        self.toasts.clear();
    }

    /// Drop any toast whose duration has elapsed. Returns true if any were
    /// removed so the caller can request a redraw if needed.
    pub fn tick(&mut self) -> bool {
        let now = Instant::now();
        let before = self.toasts.len();
        self.toasts.retain(|t| !t.is_expired(now));
        self.toasts.len() != before
    }

    pub fn toasts(&self) -> &[Toast] {
        &self.toasts
    }
}

/// Build a floating column of toast cards positioned in the given corner.
/// Return `None` when the stack is empty so callers can skip the overlay
/// layer entirely.
pub fn view<'a, Message, D, E>(
    stack: &'a ToastStack,
    corner: Corner,
    on_dismiss: D,
    on_toggle_expand: E,
) -> Option<Element<'a, Message>>
where
    Message: Clone + 'a,
    D: Fn(ToastId) -> Message + Copy + 'a,
    E: Fn(ToastId) -> Message + Copy + 'a,
{
    if stack.is_empty() {
        return None;
    }

    let space = &theme::SPACE;

    let mut items: Column<'a, Message> = column![].spacing(space.sm);
    for toast in stack.toasts() {
        items = items.push(toast_card(toast, on_dismiss, on_toggle_expand));
    }

    let (h_align, v_align) = match corner {
        Corner::TopLeft => (Horizontal::Left, Vertical::Top),
        Corner::TopRight => (Horizontal::Right, Vertical::Top),
        Corner::BottomLeft => (Horizontal::Left, Vertical::Bottom),
        Corner::BottomRight => (Horizontal::Right, Vertical::Bottom),
    };

    let anchored: Container<'a, Message> = container(items)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(h_align)
        .align_y(v_align)
        .padding(space.xl);

    Some(anchored.into())
}

fn toast_card<'a, Message, D, E>(
    toast: &Toast,
    on_dismiss: D,
    on_toggle_expand: E,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
    D: Fn(ToastId) -> Message + Copy + 'a,
    E: Fn(ToastId) -> Message + Copy + 'a,
{
    let space = &theme::SPACE;
    let font = &theme::FONT;
    let tone = toast.tone;
    let id = toast.id;

    let title_text = text(toast.title.clone())
        .size(font.size_base)
        .width(Length::Fill)
        .style(move |theme: &iced::Theme| iced::widget::text::Style {
            color: Some(theme::tokens_for(theme).color.foreground),
        });

    let mut body: Column<'a, Message> = column![title_text].spacing(space.xxs).width(Length::Fill);
    if let Some(msg) = &toast.message {
        let is_long = msg.chars().count() > COLLAPSED_MESSAGE_CHAR_LIMIT;
        let display_text = if is_long && !toast.expanded {
            let head: String = msg.chars().take(COLLAPSED_MESSAGE_CHAR_LIMIT).collect();
            format!("{}…", head.trim_end())
        } else {
            msg.clone()
        };

        let muted = text(display_text)
            .size(font.size_sm)
            .width(Length::Fill)
            .style(|theme: &iced::Theme| iced::widget::text::Style {
                color: Some(theme::tokens_for(theme).color.muted_foreground),
            });
        body = body.push(muted);

        if is_long {
            let label = if toast.expanded {
                "Show less"
            } else {
                "Show more"
            };
            let toggle = button(text(label).size(font.size_sm).style(|theme: &iced::Theme| {
                iced::widget::text::Style {
                    color: Some(theme::tokens_for(theme).color.foreground),
                }
            }))
            .padding(Padding {
                top: 2.0,
                bottom: 2.0,
                left: space.xs,
                right: space.xs,
            })
            .on_press(on_toggle_expand(id))
            .style(|theme, status| expand_button_style(theme, status));
            body = body.push(toggle);
        }
    }

    let close = button(text("\u{00D7}").size(font.size_lg))
        .padding(Padding {
            top: 0.0,
            bottom: 2.0,
            left: space.xs,
            right: space.xs,
        })
        .on_press(on_dismiss(id))
        .style(|theme, status| dismiss_button_style(theme, status));

    let content: Row<'a, Message> = row![body, close]
        .spacing(space.md)
        .align_y(iced::Alignment::Start)
        .width(Length::Fill);

    container(content)
        .padding(Padding {
            top: space.md,
            bottom: space.md,
            left: space.lg,
            right: space.md,
        })
        .width(Length::Fixed(340.0))
        .style(move |theme| card_style(theme, tone))
        .into()
}

fn card_style(theme: &iced::Theme, tone: Tone) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    let accent = match tone {
        Tone::Info => c.info,
        Tone::Success => c.success,
        Tone::Warning => c.warning,
        Tone::Error => c.destructive,
    };

    iced::widget::container::Style {
        text_color: Some(c.popover_foreground),
        background: Some(Background::Color(c.popover)),
        border: iced::border::Border {
            color: accent,
            width: 1.0,
            radius: tokens.radius.lg.into(),
        },
        shadow: tokens.shadow.lg,
        snap: false,
    }
}

fn dismiss_button_style(
    theme: &iced::Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;

    let (bg, fg) = match status {
        iced::widget::button::Status::Hovered => (
            Some(Background::Color(with_alpha(c.muted, 0.8))),
            c.foreground,
        ),
        iced::widget::button::Status::Pressed => (Some(Background::Color(c.muted)), c.foreground),
        _ => (None, c.muted_foreground),
    };

    iced::widget::button::Style {
        background: bg,
        text_color: fg,
        border: border(iced::Color::TRANSPARENT, 0.0, tokens.radius.sm),
        shadow: Default::default(),
        snap: false,
    }
}

fn expand_button_style(
    theme: &iced::Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;

    let (bg, fg) = match status {
        iced::widget::button::Status::Hovered => (
            Some(Background::Color(with_alpha(c.muted, 0.6))),
            c.foreground,
        ),
        iced::widget::button::Status::Pressed => (Some(Background::Color(c.muted)), c.foreground),
        _ => (None, c.muted_foreground),
    };

    iced::widget::button::Style {
        background: bg,
        text_color: fg,
        border: border(iced::Color::TRANSPARENT, 0.0, tokens.radius.sm),
        shadow: Default::default(),
        snap: false,
    }
}
