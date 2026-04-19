//! Typography primitives.
//!
//! `text()` is selectable by default. Use `text_non_selectable()` when text
//! lives inside another interactive control and should not capture pointer or
//! keyboard selection.

use iced::widget::{Text as IcedText, text as iced_text};
use iced::{Color, Element, Font, Length, Pixels, Theme, alignment};
use iced_selection::Text as SelectableText;

use crate::ui::theme as ui_theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    H1,
    H2,
    H3,
    Body,
    BodySmall,
    Caption,
}

impl Level {
    pub fn size(self) -> f32 {
        let f = &ui_theme::DARK.font;
        match self {
            Level::H1 => f.size_3xl,
            Level::H2 => f.size_2xl,
            Level::H3 => f.size_xl,
            Level::Body => f.size_base,
            Level::BodySmall => f.size_sm,
            Level::Caption => f.size_xs,
        }
    }
}

enum Kind<'a> {
    Selectable(SelectableText<'a>),
    Static(IcedText<'a>),
}

pub struct Text<'a> {
    kind: Kind<'a>,
}

impl<'a> Text<'a> {
    fn selectable(label: impl Into<String>) -> Self {
        Self {
            kind: Kind::Selectable(SelectableText::new(label.into())),
        }
    }

    fn non_selectable(label: impl Into<String>) -> Self {
        Self {
            kind: Kind::Static(iced_text(label.into())),
        }
    }

    pub fn size(self, size: impl Into<Pixels>) -> Self {
        let size = size.into();
        Self {
            kind: match self.kind {
                Kind::Selectable(text) => Kind::Selectable(text.size(size)),
                Kind::Static(text) => Kind::Static(text.size(size)),
            },
        }
    }

    pub fn line_height(self, line_height: impl Into<iced::widget::text::LineHeight>) -> Self {
        let line_height = line_height.into();
        Self {
            kind: match self.kind {
                Kind::Selectable(text) => Kind::Selectable(text.line_height(line_height)),
                Kind::Static(text) => Kind::Static(text.line_height(line_height)),
            },
        }
    }

    pub fn font(self, font: impl Into<Font>) -> Self {
        let font = font.into();
        Self {
            kind: match self.kind {
                Kind::Selectable(text) => Kind::Selectable(text.font(font)),
                Kind::Static(text) => Kind::Static(text.font(font)),
            },
        }
    }

    pub fn width(self, width: impl Into<Length>) -> Self {
        let width = width.into();
        Self {
            kind: match self.kind {
                Kind::Selectable(text) => Kind::Selectable(text.width(width)),
                Kind::Static(text) => Kind::Static(text.width(width)),
            },
        }
    }

    pub fn height(self, height: impl Into<Length>) -> Self {
        let height = height.into();
        Self {
            kind: match self.kind {
                Kind::Selectable(text) => Kind::Selectable(text.height(height)),
                Kind::Static(text) => Kind::Static(text.height(height)),
            },
        }
    }

    pub fn center(self) -> Self {
        Self {
            kind: match self.kind {
                Kind::Selectable(text) => Kind::Selectable(text.center()),
                Kind::Static(text) => Kind::Static(text.center()),
            },
        }
    }

    pub fn align_x(self, alignment: impl Into<iced::widget::text::Alignment>) -> Self {
        let alignment = alignment.into();
        Self {
            kind: match self.kind {
                Kind::Selectable(text) => Kind::Selectable(text.align_x(alignment)),
                Kind::Static(text) => Kind::Static(text.align_x(alignment)),
            },
        }
    }

    pub fn align_y(self, alignment: impl Into<alignment::Vertical>) -> Self {
        let alignment = alignment.into();
        Self {
            kind: match self.kind {
                Kind::Selectable(text) => Kind::Selectable(text.align_y(alignment)),
                Kind::Static(text) => Kind::Static(text.align_y(alignment)),
            },
        }
    }

    pub fn shaping(self, shaping: iced::widget::text::Shaping) -> Self {
        Self {
            kind: match self.kind {
                Kind::Selectable(text) => Kind::Selectable(text.shaping(shaping)),
                Kind::Static(text) => Kind::Static(text.shaping(shaping)),
            },
        }
    }

    pub fn wrapping(self, wrapping: iced::widget::text::Wrapping) -> Self {
        Self {
            kind: match self.kind {
                Kind::Selectable(text) => Kind::Selectable(text.wrapping(wrapping)),
                Kind::Static(text) => Kind::Static(text.wrapping(wrapping)),
            },
        }
    }

    pub fn style(self, style: impl Fn(&Theme) -> iced::widget::text::Style + 'a) -> Self {
        Self {
            kind: match self.kind {
                Kind::Selectable(text) => Kind::Selectable(text.style(move |theme| {
                    let text_style = style(theme);
                    iced_selection::text::Style {
                        color: text_style.color,
                        selection: iced_selection::text::default(theme).selection,
                    }
                })),
                Kind::Static(text) => Kind::Static(text.style(style)),
            },
        }
    }

    pub fn color(self, color: impl Into<Color>) -> Self {
        let color = color.into();
        self.style(move |_theme| iced::widget::text::Style { color: Some(color) })
    }
}

impl<'a, Message: 'a> From<Text<'a>> for Element<'a, Message> {
    fn from(text: Text<'a>) -> Self {
        match text.kind {
            Kind::Selectable(text) => text.into(),
            Kind::Static(text) => text.into(),
        }
    }
}

pub fn text<'a>(label: impl Into<String>) -> Text<'a> {
    Text::selectable(label).size(Level::Body.size())
}

pub fn text_non_selectable<'a>(label: impl Into<String>) -> Text<'a> {
    Text::non_selectable(label).size(Level::Body.size())
}

pub fn heading<'a>(level: Level, label: impl Into<String>) -> Text<'a> {
    Text::selectable(label)
        .size(level.size())
        .style(|theme: &Theme| iced::widget::text::Style {
            color: Some(ui_theme::tokens_for(theme).color.foreground),
        })
}

pub fn muted<'a>(label: impl Into<String>) -> Text<'a> {
    Text::selectable(label)
        .size(Level::BodySmall.size())
        .style(|theme: &Theme| iced::widget::text::Style {
            color: Some(ui_theme::tokens_for(theme).color.muted_foreground),
        })
}

pub fn caption<'a>(label: impl Into<String>) -> Text<'a> {
    Text::selectable(label)
        .size(Level::Caption.size())
        .style(|theme: &Theme| iced::widget::text::Style {
            color: Some(ui_theme::tokens_for(theme).color.muted_foreground),
        })
}
