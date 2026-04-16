mod app;
mod app_icon;
mod autostart;
mod background_jobs;
mod capture;
mod census;
mod clip_naming;
mod config;
mod db;
mod discord;
mod event_log;
mod honu;
mod hotkey;
mod launcher;
mod montage;
mod notifications;
mod platform_service;
mod post_process;
mod process;
mod recorder;
mod rules;
mod secure_store;
mod session_report;
mod storage_tiering;
mod timeline_export;
mod tray;
mod ui;
mod uploads;

use app::App;
use iced::{Element, window};

fn daemon_view(app: &App, _window: window::Id) -> Element<'_, app::Message> {
    app.view()
}

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nanite_clip=info".parse().unwrap()),
        )
        .init();

    iced::daemon(App::new, App::update, daemon_view)
        .title(|app: &App, _window| app.title())
        .theme(|app: &App, _window| app.theme())
        .subscription(App::subscription)
        .run()
}
