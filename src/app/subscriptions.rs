use std::time::Duration;

use iced::{Subscription, event, window};

use super::{App, AppState, Message, RuntimeMessage, View, tabs};
use crate::census::{self, StreamEvent};

#[derive(Debug, Clone, Hash)]
struct CensusKey {
    service_id: String,
    character_ids: Vec<u64>,
}

pub(super) fn build(app: &App) -> Subscription<Message> {
    let tick =
        iced::time::every(Duration::from_secs(3)).map(|_| Message::runtime(RuntimeMessage::Tick));
    let runtime_poll = iced::time::every(Duration::from_millis(200))
        .map(|_| Message::runtime(RuntimeMessage::RuntimePoll));

    Subscription::batch([
        tick,
        runtime_poll,
        census_subscription(app),
        hotkey_capture_subscription(app),
        clips_key_navigation_subscription(app),
        window::close_events().map(|id| Message::runtime(RuntimeMessage::MainWindowClosed(id))),
        window::close_requests()
            .map(|id| Message::runtime(RuntimeMessage::WindowCloseRequested(id))),
    ])
}

fn census_subscription(app: &App) -> Subscription<Message> {
    match &app.runtime.lifecycle {
        AppState::WaitingForLogin | AppState::Monitoring { .. } => {
            let mut ids: Vec<u64> = app
                .config
                .characters
                .iter()
                .filter_map(|character| character.character_id)
                .collect();
            ids.sort_unstable();
            ids.dedup();
            if ids.is_empty() || app.config.service_id.is_empty() {
                Subscription::none()
            } else {
                let key = CensusKey {
                    service_id: app.config.service_id.clone(),
                    character_ids: ids,
                };
                Subscription::run_with(key, build_census_stream)
                    .map(|event| Message::runtime(RuntimeMessage::CensusStream(event)))
            }
        }
        _ => Subscription::none(),
    }
}

fn hotkey_capture_subscription(app: &App) -> Subscription<Message> {
    if matches!(app.view, View::Settings) && app.settings.hotkey_capture_active {
        event::listen_with(capture_hotkey_event)
    } else {
        Subscription::none()
    }
}

fn clips_key_navigation_subscription(app: &App) -> Subscription<Message> {
    if matches!(app.view, View::Clips) {
        event::listen_with(clips_key_event_router)
    } else {
        Subscription::none()
    }
}

fn build_census_stream(key: &CensusKey) -> iced::futures::stream::BoxStream<'static, StreamEvent> {
    use iced::futures::StreamExt;

    Box::pin(census::event_stream(
        key.service_id.clone(),
        key.character_ids.clone(),
    ))
    .boxed()
}

fn capture_hotkey_event(
    event: iced::Event,
    _status: event::Status,
    _window: window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Keyboard(event) => Some(Message::Settings(
            tabs::settings::Message::HotkeyCaptureEvent(event),
        )),
        _ => None,
    }
}

fn clips_key_event_router(
    event: iced::Event,
    status: event::Status,
    _window: window::Id,
) -> Option<Message> {
    tabs::clips::subscription_event_handler(event, status).map(Message::Clips)
}
