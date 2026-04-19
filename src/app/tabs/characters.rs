use std::sync::LazyLock;

use iced::widget::{Space, image};
use iced::{ContentFit, Element, Length};

use crate::census::{self, ResolvedCharacter};
use crate::config::CharacterConfig;
use crate::ui::app::{column, container, row, scrollable, text, text_input};

use super::super::shared::{ButtonTone, styled_button, with_tooltip};
use super::super::{App, AppState, Message as AppMessage};

// nanite-ui layout components
use crate::ui::layout::card::card;
use crate::ui::layout::empty_state::empty_state;
use crate::ui::layout::page_header::page_header;
use crate::ui::layout::panel::panel;
use crate::ui::layout::section::section;
use crate::ui::layout::toolbar::toolbar;
use crate::ui::primitives::badge::{Tone as BadgeTone, badge};

static TR_LOGO: LazyLock<image::Handle> = LazyLock::new(|| {
    image::Handle::from_bytes(include_bytes!("../../../assets/factions/logo_tr.png").as_slice())
});
static VS_LOGO: LazyLock<image::Handle> = LazyLock::new(|| {
    image::Handle::from_bytes(include_bytes!("../../../assets/factions/logo_vs.png").as_slice())
});
static NC_LOGO: LazyLock<image::Handle> = LazyLock::new(|| {
    image::Handle::from_bytes(include_bytes!("../../../assets/factions/logo_nc.png").as_slice())
});
static NS_LOGO: LazyLock<image::Handle> = LazyLock::new(|| {
    image::Handle::from_bytes(include_bytes!("../../../assets/factions/logo_ns.png").as_slice())
});

#[derive(Debug, Clone)]
pub enum Message {
    NewNameChanged(String),
    Add,
    Remove(usize),
    Resolved(String, ResolvedCharacter),
    ResolveFailed(String, String),
}

pub(in crate::app) fn update(app: &mut App, message: Message) -> iced::Task<AppMessage> {
    match message {
        Message::NewNameChanged(name) => {
            app.new_character_name = name;
            iced::Task::none()
        }
        Message::Add => {
            let name = app.new_character_name.trim().to_string();
            if !name.is_empty() {
                app.config.characters.push(CharacterConfig {
                    name: name.clone(),
                    character_id: None,
                    world_id: None,
                    faction_id: None,
                });
                app.new_character_name.clear();
                let _ = app.config.save();
                return app.queue_character_resolution(name);
            }
            iced::Task::none()
        }
        Message::Remove(idx) => {
            if idx < app.config.characters.len() {
                let removed = app.config.characters.remove(idx);
                app.rules.resolving_characters.remove(&removed.name);
                let _ = app.config.save();
            }
            iced::Task::none()
        }
        Message::Resolved(name, resolved) => {
            app.rules.resolving_characters.remove(&name);

            let mut changed = false;
            for character in app.config.characters.iter_mut().filter(|c| c.name == name) {
                if character.character_id != Some(resolved.id)
                    || character.world_id != resolved.world_id
                    || character.faction_id != resolved.faction_id
                {
                    character.character_id = Some(resolved.id);
                    character.world_id = resolved.world_id;
                    character.faction_id = resolved.faction_id;
                    changed = true;
                }
            }

            if changed {
                tracing::info!(
                    "Resolved {name} -> {} (world={:?}, faction={:?})",
                    resolved.id,
                    resolved.world_id,
                    resolved.faction_id
                );
                let _ = app.config.save();
            }

            if changed && matches!(app.runtime.lifecycle, AppState::WaitingForLogin) {
                app.check_online_status()
            } else {
                iced::Task::none()
            }
        }
        Message::ResolveFailed(name, err) => {
            app.rules.resolving_characters.remove(&name);
            tracing::error!("Failed to resolve {name}: {err}");
            iced::Task::none()
        }
    }
}

pub(in crate::app) fn view(app: &App) -> Element<'_, Message> {
    let resolved_count = app
        .config
        .characters
        .iter()
        .filter(|c| c.character_id.is_some() && c.world_id.is_some() && c.faction_id.is_some())
        .count();
    let resolving_count = app.rules.resolving_characters.len();
    let total = app.config.characters.len();

    let header = page_header("Characters")
        .subtitle("Tracked PlanetSide 2 characters.")
        .build();

    let status_bar = toolbar()
        .push(char_badge(
            format!("{total} character{}", if total == 1 { "" } else { "s" }),
            BadgeTone::Outline,
        ))
        .push(char_badge(
            format!("{resolved_count} resolved"),
            if resolved_count == total && total > 0 {
                BadgeTone::Success
            } else if resolved_count > 0 {
                BadgeTone::Info
            } else {
                BadgeTone::Neutral
            },
        ))
        .push(if resolving_count > 0 {
            char_badge(
                format!("{resolving_count} resolving..."),
                BadgeTone::Warning,
            )
        } else {
            char_badge("All idle", BadgeTone::Neutral)
        })
        .build();

    let add_section = section("Add Character")
        .push(
            row![
                text_input("Character name", &app.new_character_name)
                    .on_input(Message::NewNameChanged)
                    .on_submit(Message::Add)
                    .width(280),
                styled_button("Add Character", ButtonTone::Success).on_press(Message::Add),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        )
        .build();

    let tracked_section: Element<'_, Message> = if app.config.characters.is_empty() {
        empty_state("No characters tracked")
            .description("Add a character above to start monitoring PlanetSide 2 events.")
            .build()
            .into()
    } else {
        let mut chars_col = column![].spacing(8);
        for (i, c) in app.config.characters.iter().enumerate() {
            chars_col = chars_col.push(character_card(app, i, c));
        }
        section("Tracked Characters").push(chars_col).build().into()
    };

    let body = panel("Character Management")
        .push(add_section)
        .push(tracked_section)
        .build();

    column![
        header,
        status_bar,
        scrollable(container(body).width(Length::Fill)).height(Length::Fill)
    ]
    .spacing(12)
    .into()
}

fn char_badge<'a>(label: impl Into<String>, tone: BadgeTone) -> Element<'a, Message> {
    badge(label).tone(tone).build().into()
}

fn character_card<'a>(
    app: &'a App,
    index: usize,
    character: &'a CharacterConfig,
) -> Element<'a, Message> {
    let (status_label, status_tone) = match character.character_id {
        Some(id) => (format!("ID: {id}"), BadgeTone::Success),
        None if app.rules.resolving_characters.contains(&character.name) => {
            ("Resolving...".into(), BadgeTone::Warning)
        }
        None => ("Unresolved".into(), BadgeTone::Destructive),
    };
    let mut content = row![
        text(&character.name).size(14).width(180),
        char_badge(status_label, status_tone),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    if let Some(world_id) = character.world_id {
        content = content.push(char_badge(
            format!("World {world_id}: {}", census::world_name(world_id)),
            BadgeTone::Info,
        ));
    }

    if let Some(faction_id) = character.faction_id
        && let Some(logo) = faction_logo(faction_id)
    {
        content = content.push(with_tooltip(
            image(logo)
                .width(24)
                .height(24)
                .content_fit(ContentFit::Contain)
                .into(),
            format!("Faction {faction_id}: {}", census::faction_name(faction_id)),
        ));
    }

    content = content.push(Space::new().width(Length::Fill));
    content = content.push(with_tooltip(
        styled_button("Remove", ButtonTone::Danger)
            .on_press(Message::Remove(index))
            .into(),
        format!("Stop tracking {}.", character.name),
    ));

    card().body(content).width(Length::Fill).into()
}

fn faction_logo(faction_id: u32) -> Option<image::Handle> {
    let handle = match faction_id {
        1 => VS_LOGO.clone(),
        2 => NC_LOGO.clone(),
        3 => TR_LOGO.clone(),
        4 => NS_LOGO.clone(),
        _ => return None,
    };

    Some(handle)
}
