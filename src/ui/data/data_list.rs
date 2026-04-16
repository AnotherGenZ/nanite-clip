//! Data list — vertical list of structured card rows.
//!
//! Each row has an optional leading element (icon/avatar), a title, an
//! optional subtitle, optional trailing meta text, and an optional trailing
//! custom element. Rows can be made clickable.

use iced::widget::{Column, Row, button, column, container, text};
use iced::{Background, Element, Length, Padding};

use crate::ui::theme::{self, Tokens, border};

pub struct DataRow<'a, Message> {
    leading: Option<Element<'a, Message>>,
    title: String,
    subtitle: Option<String>,
    meta: Option<String>,
    trailing: Option<Element<'a, Message>>,
    on_click: Option<Message>,
}

impl<'a, Message: Clone + 'a> DataRow<'a, Message> {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            leading: None,
            title: title.into(),
            subtitle: None,
            meta: None,
            trailing: None,
            on_click: None,
        }
    }

    pub fn leading(mut self, leading: impl Into<Element<'a, Message>>) -> Self {
        self.leading = Some(leading.into());
        self
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn meta(mut self, meta: impl Into<String>) -> Self {
        self.meta = Some(meta.into());
        self
    }

    pub fn trailing(mut self, trailing: impl Into<Element<'a, Message>>) -> Self {
        self.trailing = Some(trailing.into());
        self
    }

    pub fn on_click(mut self, message: Message) -> Self {
        self.on_click = Some(message);
        self
    }
}

pub fn data_list<'a, Message: Clone + 'a>() -> DataListBuilder<'a, Message> {
    DataListBuilder { rows: Vec::new() }
}

pub struct DataListBuilder<'a, Message> {
    rows: Vec<DataRow<'a, Message>>,
}

impl<'a, Message: Clone + 'a> DataListBuilder<'a, Message> {
    pub fn push(mut self, row: DataRow<'a, Message>) -> Self {
        self.rows.push(row);
        self
    }

    pub fn build(self) -> Element<'a, Message> {
        let space = &theme::SPACE;
        let font = &theme::FONT;

        let mut col: Column<'a, Message> = column![].spacing(0.0);
        let last = self.rows.len().saturating_sub(1);

        for (idx, r) in self.rows.into_iter().enumerate() {
            let is_last = idx == last;
            col = col.push(row_view::<Message>(r, space, font, is_last));
        }

        container(col).width(Length::Fill).style(outer_style).into()
    }
}

fn row_view<'a, Message: Clone + 'a>(
    data: DataRow<'a, Message>,
    space: &crate::ui::theme::SpaceTokens,
    font: &crate::ui::theme::FontTokens,
    is_last: bool,
) -> Element<'a, Message> {
    let title = text(data.title)
        .size(font.size_base)
        .style(|theme: &iced::Theme| iced::widget::text::Style {
            color: Some(theme::tokens_for(theme).color.foreground),
        });

    let mut body: Column<'a, Message> = column![title].spacing(space.xxs);

    if let Some(sub) = data.subtitle {
        body = body.push(text(sub).size(font.size_sm).style(|theme: &iced::Theme| {
            iced::widget::text::Style {
                color: Some(theme::tokens_for(theme).color.muted_foreground),
            }
        }));
    }

    let mut inner: Row<'a, Message> = Row::new()
        .spacing(space.md)
        .align_y(iced::Alignment::Center);

    if let Some(leading) = data.leading {
        inner = inner.push(leading);
    }

    inner = inner.push(body.width(Length::Fill));

    if let Some(meta) = data.meta {
        inner = inner.push(text(meta).size(font.size_sm).style(|theme: &iced::Theme| {
            iced::widget::text::Style {
                color: Some(theme::tokens_for(theme).color.muted_foreground),
            }
        }));
    }

    if let Some(trailing) = data.trailing {
        inner = inner.push(trailing);
    }

    let content = container(inner)
        .padding(Padding {
            top: space.md,
            bottom: space.md,
            left: space.lg,
            right: space.lg,
        })
        .width(Length::Fill)
        .style(move |theme| row_style(theme, is_last));

    if let Some(msg) = data.on_click {
        button(content)
            .padding(0)
            .width(Length::Fill)
            .style(clickable_style)
            .on_press(msg)
            .into()
    } else {
        content.into()
    }
}

fn outer_style(theme: &iced::Theme) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    iced::widget::container::Style {
        text_color: Some(c.foreground),
        background: Some(Background::Color(c.card)),
        border: border(c.border, 1.0, tokens.radius.lg),
        shadow: Default::default(),
        snap: false,
    }
}

fn row_style(theme: &iced::Theme, _is_last: bool) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    iced::widget::container::Style {
        text_color: Some(c.foreground),
        background: None,
        border: iced::border::Border {
            color: c.border,
            width: 0.0,
            radius: 0.0.into(),
        },
        shadow: Default::default(),
        snap: false,
    }
}

fn clickable_style(
    theme: &iced::Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    let bg = match status {
        iced::widget::button::Status::Hovered => Some(Background::Color(c.accent)),
        iced::widget::button::Status::Pressed => Some(Background::Color(c.muted)),
        _ => None,
    };
    iced::widget::button::Style {
        background: bg,
        text_color: c.foreground,
        border: border(iced::Color::TRANSPARENT, 0.0, 0.0),
        shadow: Default::default(),
        snap: false,
    }
}
