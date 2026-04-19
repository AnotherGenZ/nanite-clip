#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod app_icon;
mod autostart;
mod background_jobs;
mod capture;
mod census;
mod clip_naming;
mod command_runner;
mod config;
mod db;
mod discord;
mod event_log;
mod honu;
mod hotkey;
mod launcher;
mod montage;
mod notifications;
mod platform;
mod post_process;
mod process;
mod profile_transfer;
mod recorder;
mod rules;
mod secure_store;
mod session_report;
mod storage_tiering;
mod timeline_export;
mod tray;
mod ui;
mod update;
mod uploads;

use app::App;
use iced::{Element, window};

fn daemon_view(app: &App, _window: window::Id) -> Element<'_, app::Message> {
    app.view()
}

fn main() -> iced::Result {
    let mut args = std::env::args_os();
    let _ = args.next();
    if let (Some(flag), Some(plan_path)) = (args.next(), args.next())
        && flag == "--apply-plan"
    {
        if let Err(error) = update::helper_runner::run_apply_plan(std::path::Path::new(&plan_path))
        {
            eprintln!("NaniteClip updater error: {error}");
            std::process::exit(1);
        }
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    if let Err(error) = notifications::configure_windows_notifications() {
        eprintln!("NaniteClip warning: {error}");
    }

    #[cfg(debug_assertions)]
    let default_log_filter = "nanite_clip=debug";
    #[cfg(not(debug_assertions))]
    let default_log_filter = "nanite_clip=info";

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_log_filter.parse().unwrap()),
        )
        .init();

    tracing::info!(
        debug_build = cfg!(debug_assertions),
        default_log_filter,
        "starting NaniteClip"
    );

    iced::daemon(App::new, App::update, daemon_view)
        .title(|app: &App, _window| app.title())
        .theme(|app: &App, _window| app.theme())
        .subscription(App::subscription)
        .run()
}
