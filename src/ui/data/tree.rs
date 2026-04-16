//! Tree widget — recursive collapsible tree view.
//!
//! The widget is stateless: callers track the expansion set themselves
//! (typically a `HashSet<NodeId>`) and pass a `is_expanded` callback plus a
//! `on_toggle` message constructor. Selection is also caller-owned.
//!
//! No virtualization — suitable for trees up to a few hundred nodes. For
//! large trees, wrap the result in `iced::widget::scrollable`.

use iced::widget::{Column, Row, button, column, container, text};
use iced::{Background, Element, Length, Padding};

use crate::ui::theme::{self, Tokens, border};

pub struct TreeNode<'a, Id, Message> {
    pub id: Id,
    pub label: String,
    pub icon: Option<Element<'a, Message>>,
    pub children: Vec<TreeNode<'a, Id, Message>>,
}

impl<'a, Id, Message> TreeNode<'a, Id, Message> {
    pub fn new(id: Id, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            icon: None,
            children: Vec::new(),
        }
    }

    pub fn icon(mut self, icon: impl Into<Element<'a, Message>>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn child(mut self, node: TreeNode<'a, Id, Message>) -> Self {
        self.children.push(node);
        self
    }

    pub fn children(mut self, nodes: Vec<TreeNode<'a, Id, Message>>) -> Self {
        self.children = nodes;
        self
    }
}

pub fn tree<'a, Id, Message, IsExpanded, OnToggle>(
    nodes: Vec<TreeNode<'a, Id, Message>>,
    is_expanded: IsExpanded,
    on_toggle: OnToggle,
) -> Element<'a, Message>
where
    Id: Clone + 'a,
    Message: Clone + 'a,
    IsExpanded: Fn(&Id) -> bool + Copy + 'a,
    OnToggle: Fn(Id) -> Message + Copy + 'a,
{
    let space = &theme::SPACE;
    let font = &theme::FONT;

    let mut col: Column<'a, Message> = column![].spacing(0.0);
    for node in nodes {
        col = col.push(render_node::<Id, Message, IsExpanded, OnToggle>(
            node,
            0,
            is_expanded,
            on_toggle,
            space,
            font,
        ));
    }

    container(col)
        .padding(Padding {
            top: space.xs,
            bottom: space.xs,
            left: space.xs,
            right: space.xs,
        })
        .width(Length::Fill)
        .into()
}

fn render_node<'a, Id, Message, IsExpanded, OnToggle>(
    node: TreeNode<'a, Id, Message>,
    depth: usize,
    is_expanded: IsExpanded,
    on_toggle: OnToggle,
    space: &crate::ui::theme::SpaceTokens,
    font: &crate::ui::theme::FontTokens,
) -> Element<'a, Message>
where
    Id: Clone + 'a,
    Message: Clone + 'a,
    IsExpanded: Fn(&Id) -> bool + Copy + 'a,
    OnToggle: Fn(Id) -> Message + Copy + 'a,
{
    let has_children = !node.children.is_empty();
    let expanded = has_children && is_expanded(&node.id);
    let indent = space.md * depth as f32;

    let chevron = if has_children {
        if expanded { "\u{25BE}" } else { "\u{25B8}" }
    } else {
        "  "
    };

    let chevron_text = text(chevron)
        .size(font.size_sm)
        .style(|theme: &iced::Theme| iced::widget::text::Style {
            color: Some(theme::tokens_for(theme).color.muted_foreground),
        });

    let label_text = text(node.label.clone())
        .size(font.size_sm)
        .style(|theme: &iced::Theme| iced::widget::text::Style {
            color: Some(theme::tokens_for(theme).color.foreground),
        });

    let mut row_inner: Row<'a, Message> = Row::new()
        .spacing(space.xs)
        .align_y(iced::Alignment::Center);
    row_inner = row_inner
        .push(iced::widget::Space::new().width(Length::Fixed(indent)))
        .push(chevron_text);
    if let Some(icon) = node.icon {
        row_inner = row_inner.push(icon);
    }
    row_inner = row_inner.push(label_text);

    let content = container(row_inner)
        .padding(Padding {
            top: space.xxs + 2.0,
            bottom: space.xxs + 2.0,
            left: space.sm,
            right: space.sm,
        })
        .width(Length::Fill);

    let header_el: Element<'a, Message> = if has_children {
        let id = node.id.clone();
        button(content)
            .padding(0)
            .width(Length::Fill)
            .style(toggle_style)
            .on_press(on_toggle(id))
            .into()
    } else {
        content.into()
    };

    let mut col: Column<'a, Message> = column![header_el].spacing(0.0);

    if expanded {
        for child in node.children {
            col = col.push(render_node::<Id, Message, IsExpanded, OnToggle>(
                child,
                depth + 1,
                is_expanded,
                on_toggle,
                space,
                font,
            ));
        }
    }

    col.into()
}

fn toggle_style(
    theme: &iced::Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let tokens: &Tokens = theme::tokens_for(theme);
    let c = &tokens.color;
    let bg = match status {
        iced::widget::button::Status::Hovered => Some(Background::Color(c.muted)),
        iced::widget::button::Status::Pressed => Some(Background::Color(c.accent)),
        _ => None,
    };
    iced::widget::button::Style {
        background: bg,
        text_color: c.foreground,
        border: border(iced::Color::TRANSPARENT, 0.0, tokens.radius.sm),
        shadow: Default::default(),
        snap: false,
    }
}
