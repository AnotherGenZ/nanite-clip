//! Table widget — header row + body rows with column alignment.
//!
//! The widget is stateless: callers pass a vector of [`Column`] definitions
//! and row data rendered into cells. Cells are opaque [`Element`]s so any
//! primitive or composite can be embedded. Row click wiring is optional —
//! if set, the row is wrapped in a transparent button.
//!
//! ```ignore
//! let rows = clips.iter().map(|c| vec![
//!     text(c.name.clone()).into(),
//!     text(c.duration.to_string()).into(),
//!     text(c.status.clone()).into(),
//! ]).collect();
//!
//! let table = table::table(columns, rows)
//!     .striped(true)
//!     .on_row_click(Message::SelectClip)
//!     .build();
//! ```

use iced::widget::{Column, Row, button, column, container, row, text};
use iced::{Alignment, Background, Element, Length, Padding};

use crate::ui::theme::{self, Tokens, border};

#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub label: String,
    pub width: Length,
    pub align: Alignment,
}

impl ColumnDef {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            width: Length::Fill,
            align: Alignment::Start,
        }
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn align(mut self, align: Alignment) -> Self {
        self.align = align;
        self
    }
}

pub fn table<'a, Message>(
    columns: Vec<ColumnDef>,
    rows: Vec<Vec<Element<'a, Message>>>,
) -> TableBuilder<'a, Message>
where
    Message: Clone + 'a,
{
    TableBuilder {
        columns,
        rows,
        striped: false,
        on_row_click: None,
    }
}

type RowClickFn<'a, Message> = Box<dyn Fn(usize) -> Message + 'a>;

pub struct TableBuilder<'a, Message> {
    columns: Vec<ColumnDef>,
    rows: Vec<Vec<Element<'a, Message>>>,
    striped: bool,
    on_row_click: Option<RowClickFn<'a, Message>>,
}

impl<'a, Message: Clone + 'a> TableBuilder<'a, Message> {
    pub fn striped(mut self, striped: bool) -> Self {
        self.striped = striped;
        self
    }

    pub fn on_row_click(mut self, f: impl Fn(usize) -> Message + 'a) -> Self {
        self.on_row_click = Some(Box::new(f));
        self
    }

    pub fn build(self) -> Element<'a, Message> {
        let space = &theme::SPACE;
        let font = &theme::FONT;
        let striped = self.striped;
        let columns = self.columns;
        let on_click = self.on_row_click;

        let header = header_row(&columns, space, font);

        let mut body: Column<'a, Message> = column![header].spacing(0.0);

        for (idx, cells) in self.rows.into_iter().enumerate() {
            let row_el = row_view(&columns, cells, space, idx, striped, on_click.as_deref());
            body = body.push(row_el);
        }

        container(body)
            .style(table_style)
            .width(Length::Fill)
            .into()
    }
}

fn header_row<'a, Message: 'a>(
    columns: &[ColumnDef],
    space: &crate::ui::theme::SpaceTokens,
    font: &crate::ui::theme::FontTokens,
) -> Element<'a, Message> {
    let mut r: Row<'a, Message> = row![]
        .spacing(0.0)
        .align_y(Alignment::Center)
        .width(Length::Fill);

    for col in columns {
        let label = text(col.label.clone())
            .size(font.size_xs)
            .style(|theme: &iced::Theme| iced::widget::text::Style {
                color: Some(theme::tokens_for(theme).color.muted_foreground),
            });
        let cell = container(label)
            .padding(Padding {
                top: space.sm,
                bottom: space.sm,
                left: space.md,
                right: space.md,
            })
            .width(col.width)
            .align_x(match col.align {
                Alignment::Start => iced::alignment::Horizontal::Left,
                Alignment::Center => iced::alignment::Horizontal::Center,
                Alignment::End => iced::alignment::Horizontal::Right,
            });
        r = r.push(cell);
    }

    container(r).style(header_style).width(Length::Fill).into()
}

fn row_view<'a, Message: Clone + 'a>(
    columns: &[ColumnDef],
    cells: Vec<Element<'a, Message>>,
    space: &crate::ui::theme::SpaceTokens,
    index: usize,
    striped: bool,
    on_click: Option<&(dyn Fn(usize) -> Message + 'a)>,
) -> Element<'a, Message> {
    let mut r: Row<'a, Message> = row![]
        .spacing(0.0)
        .align_y(Alignment::Center)
        .width(Length::Fill);

    for (col, cell) in columns.iter().zip(cells.into_iter()) {
        let wrapped = container(cell)
            .padding(Padding {
                top: space.sm,
                bottom: space.sm,
                left: space.md,
                right: space.md,
            })
            .width(col.width)
            .align_x(match col.align {
                Alignment::Start => iced::alignment::Horizontal::Left,
                Alignment::Center => iced::alignment::Horizontal::Center,
                Alignment::End => iced::alignment::Horizontal::Right,
            });
        r = r.push(wrapped);
    }

    let row_style = move |theme: &iced::Theme| row_container_style(theme, index, striped);

    let row_container = container(r).width(Length::Fill).style(row_style);

    if let Some(f) = on_click {
        let msg = f(index);
        button(row_container)
            .padding(0)
            .style(clickable_row_style)
            .on_press(msg)
            .into()
    } else {
        row_container.into()
    }
}

fn table_style(theme: &iced::Theme) -> iced::widget::container::Style {
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

fn header_style(theme: &iced::Theme) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    iced::widget::container::Style {
        text_color: Some(c.muted_foreground),
        background: Some(Background::Color(c.muted)),
        border: iced::border::Border {
            color: c.border,
            width: 0.0,
            radius: iced::border::top(tokens.radius.lg),
        },
        shadow: Default::default(),
        snap: false,
    }
}

fn row_container_style(
    theme: &iced::Theme,
    index: usize,
    striped: bool,
) -> iced::widget::container::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;

    let background = if striped && index % 2 == 1 {
        Some(Background::Color(c.muted))
    } else {
        None
    };

    iced::widget::container::Style {
        text_color: Some(c.foreground),
        background,
        border: iced::border::Border {
            color: c.border,
            width: 0.0,
            radius: 0.0.into(),
        },
        shadow: Default::default(),
        snap: false,
    }
}

fn clickable_row_style(
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
