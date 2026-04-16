#[cfg(not(target_os = "windows"))]
use std::sync::mpsc;
#[cfg(not(target_os = "windows"))]
use std::thread;
#[cfg(not(target_os = "windows"))]
use std::time::Duration;

#[cfg(target_os = "linux")]
use ksni::blocking::TrayMethods;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

#[cfg(target_os = "linux")]
use crate::process::{DesktopEnvironment, detect_desktop_environment};

const MENU_ID_SHOW_WINDOW: &str = "tray.show_window";
const MENU_ID_START_MONITORING: &str = "tray.start_monitoring";
const MENU_ID_STOP_MONITORING: &str = "tray.stop_monitoring";
const MENU_ID_QUIT: &str = "tray.quit";
const MENU_ID_PROFILE_PREFIX: &str = "tray.profile.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraySnapshot {
    pub title: String,
    pub status_label: String,
    pub can_start_monitoring: bool,
    pub can_stop_monitoring: bool,
    pub profile_options: Vec<TrayProfileOption>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEvent {
    StartMonitoring,
    StopMonitoring,
    ShowWindow,
    SwitchProfile(String),
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayProfileOption {
    pub id: String,
    pub name: String,
    pub selected: bool,
}

pub struct TrayController {
    backend: TrayBackend,
    #[cfg(not(target_os = "windows"))]
    event_rx: mpsc::Receiver<TrayEvent>,
}

impl std::fmt::Debug for TrayController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrayController").finish_non_exhaustive()
    }
}

enum TrayBackend {
    #[cfg(target_os = "linux")]
    Ksni {
        handle: ksni::blocking::Handle<AppTray>,
    },
    #[cfg(target_os = "windows")]
    CrossPlatform { tray: tray_icon::TrayIcon },
    #[cfg(not(target_os = "windows"))]
    CrossPlatform {
        command_tx: mpsc::Sender<CrossPlatformTrayCommand>,
        worker: Option<thread::JoinHandle<()>>,
    },
}

#[cfg(not(target_os = "windows"))]
enum CrossPlatformTrayCommand {
    Update(TraySnapshot),
    Shutdown,
}

#[derive(Debug)]
#[cfg(target_os = "linux")]
struct AppTray {
    snapshot: TraySnapshot,
    event_tx: mpsc::Sender<TrayEvent>,
}

impl TrayController {
    pub fn spawn(snapshot: TraySnapshot) -> Result<Self, String> {
        #[cfg(not(target_os = "windows"))]
        let (event_tx, event_rx) = mpsc::channel();

        #[cfg(target_os = "linux")]
        if prefers_ksni(detect_desktop_environment()) {
            let tray = AppTray { snapshot, event_tx };
            let handle = tray
                .spawn()
                .map_err(|error| format!("failed to start tray integration: {error}"))?;

            return Ok(Self {
                backend: TrayBackend::Ksni { handle },
                event_rx,
            });
        }

        #[cfg(target_os = "windows")]
        {
            let tray = build_cross_platform_tray(&snapshot)?;
            Ok(Self {
                backend: TrayBackend::CrossPlatform { tray },
            })
        }

        #[cfg(not(target_os = "windows"))]
        {
            let (command_tx, command_rx) = mpsc::channel();
            let (startup_tx, startup_rx) = mpsc::channel();
            let worker = thread::spawn(move || {
                run_cross_platform_tray(snapshot, command_rx, event_tx, startup_tx);
            });

            match startup_rx.recv() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    let _ = worker.join();
                    return Err(error);
                }
                Err(error) => {
                    let _ = worker.join();
                    return Err(format!("failed to receive tray startup status: {error}"));
                }
            }

            Ok(Self {
                backend: TrayBackend::CrossPlatform {
                    command_tx,
                    worker: Some(worker),
                },
                event_rx,
            })
        }
    }

    pub fn update_snapshot(&self, snapshot: TraySnapshot) {
        match &self.backend {
            #[cfg(target_os = "linux")]
            TrayBackend::Ksni { handle } => {
                let _ = handle.update(move |tray| tray.snapshot = snapshot);
            }
            #[cfg(target_os = "windows")]
            TrayBackend::CrossPlatform { tray } => {
                if let Err(error) = update_cross_platform_tray(tray, &snapshot) {
                    tracing::warn!("Failed to update tray snapshot: {error}");
                }
            }
            #[cfg(not(target_os = "windows"))]
            TrayBackend::CrossPlatform { command_tx, .. } => {
                let _ = command_tx.send(CrossPlatformTrayCommand::Update(snapshot));
            }
        }
    }

    pub fn drain_events(&self) -> Vec<TrayEvent> {
        #[cfg(target_os = "windows")]
        {
            let mut events = Vec::new();

            for event in TrayIconEvent::receiver().try_iter() {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    events.push(TrayEvent::ShowWindow);
                }
            }

            for event in MenuEvent::receiver().try_iter() {
                let id = event.id().as_ref();
                let translated = match id {
                    MENU_ID_SHOW_WINDOW => Some(TrayEvent::ShowWindow),
                    MENU_ID_START_MONITORING => Some(TrayEvent::StartMonitoring),
                    MENU_ID_STOP_MONITORING => Some(TrayEvent::StopMonitoring),
                    MENU_ID_QUIT => Some(TrayEvent::Quit),
                    _ => id
                        .strip_prefix(MENU_ID_PROFILE_PREFIX)
                        .map(|profile_id| TrayEvent::SwitchProfile(profile_id.to_string())),
                };

                if let Some(event) = translated {
                    events.push(event);
                }
            }

            events
        }

        #[cfg(not(target_os = "windows"))]
        self.event_rx.try_iter().collect()
    }
}

impl Drop for TrayController {
    fn drop(&mut self) {
        match &mut self.backend {
            #[cfg(target_os = "linux")]
            TrayBackend::Ksni { handle } => {
                let shutdown = handle.shutdown();
                let _ = thread::spawn(move || {
                    shutdown.wait();
                });
            }
            #[cfg(target_os = "windows")]
            TrayBackend::CrossPlatform { .. } => {}
            #[cfg(not(target_os = "windows"))]
            TrayBackend::CrossPlatform { command_tx, worker } => {
                let _ = command_tx.send(CrossPlatformTrayCommand::Shutdown);
                if let Some(worker) = worker.take() {
                    let _ = worker.join();
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
impl ksni::Tray for AppTray {
    fn id(&self) -> String {
        "NaniteClip".into()
    }

    fn icon_name(&self) -> String {
        String::new()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        crate::app_icon::tray_icon()
    }

    fn title(&self) -> String {
        self.snapshot.title.clone()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: self.snapshot.title.clone(),
            description: self.snapshot.status_label.clone(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.event_tx.send(TrayEvent::ShowWindow);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{MenuItem, RadioGroup, RadioItem, StandardItem, SubMenu};

        let mut items = vec![
            StandardItem {
                label: self.snapshot.status_label.clone(),
                enabled: false,
                ..Default::default()
            }
            .into(),
        ];

        items.push(
            StandardItem {
                label: "Show Window".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.event_tx.send(TrayEvent::ShowWindow);
                }),
                ..Default::default()
            }
            .into(),
        );

        if self.snapshot.can_start_monitoring {
            items.push(
                StandardItem {
                    label: "Start Monitoring".into(),
                    activate: Box::new(|tray: &mut Self| {
                        let _ = tray.event_tx.send(TrayEvent::StartMonitoring);
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }

        if self.snapshot.can_stop_monitoring {
            items.push(
                StandardItem {
                    label: "Stop Monitoring".into(),
                    activate: Box::new(|tray: &mut Self| {
                        let _ = tray.event_tx.send(TrayEvent::StopMonitoring);
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }

        items.push(if self.snapshot.profile_options.is_empty() {
            SubMenu {
                label: "Profiles".into(),
                enabled: false,
                submenu: vec![
                    StandardItem {
                        label: "No profiles configured".into(),
                        enabled: false,
                        ..Default::default()
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into()
        } else {
            let selected = self
                .snapshot
                .profile_options
                .iter()
                .position(|profile| profile.selected)
                .unwrap_or(0);
            let options = self
                .snapshot
                .profile_options
                .iter()
                .map(|profile| RadioItem {
                    label: profile.name.clone(),
                    ..Default::default()
                })
                .collect();

            SubMenu {
                label: "Profiles".into(),
                submenu: vec![
                    RadioGroup {
                        selected,
                        select: Box::new(|tray: &mut Self, index| {
                            if let Some(profile) = tray.snapshot.profile_options.get(index) {
                                let _ = tray
                                    .event_tx
                                    .send(TrayEvent::SwitchProfile(profile.id.clone()));
                            }
                        }),
                        options,
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into()
        });

        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.event_tx.send(TrayEvent::Quit);
                }),
                ..Default::default()
            }
            .into(),
        );

        items
    }
}

#[cfg(not(target_os = "windows"))]
fn run_cross_platform_tray(
    snapshot: TraySnapshot,
    command_rx: mpsc::Receiver<CrossPlatformTrayCommand>,
    event_tx: mpsc::Sender<TrayEvent>,
    startup_tx: mpsc::Sender<Result<(), String>>,
) {
    let tray = match build_cross_platform_tray(&snapshot) {
        Ok(tray) => tray,
        Err(error) => {
            let _ = startup_tx.send(Err(format!("failed to start tray integration: {error}")));
            return;
        }
    };

    let _ = startup_tx.send(Ok(()));

    loop {
        while let Ok(command) = command_rx.try_recv() {
            match command {
                CrossPlatformTrayCommand::Update(snapshot) => {
                    if let Err(error) = update_cross_platform_tray(&tray, &snapshot) {
                        tracing::warn!("Failed to update tray snapshot: {error}");
                    }
                }
                CrossPlatformTrayCommand::Shutdown => return,
            }
        }

        for event in TrayIconEvent::receiver().try_iter() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = event_tx.send(TrayEvent::ShowWindow);
            }
        }

        for event in MenuEvent::receiver().try_iter() {
            let id = event.id().as_ref();
            let translated = match id {
                MENU_ID_SHOW_WINDOW => Some(TrayEvent::ShowWindow),
                MENU_ID_START_MONITORING => Some(TrayEvent::StartMonitoring),
                MENU_ID_STOP_MONITORING => Some(TrayEvent::StopMonitoring),
                MENU_ID_QUIT => Some(TrayEvent::Quit),
                _ => id
                    .strip_prefix(MENU_ID_PROFILE_PREFIX)
                    .map(|profile_id| TrayEvent::SwitchProfile(profile_id.to_string())),
            };

            if let Some(event) = translated {
                let _ = event_tx.send(event);
            }
        }

        thread::sleep(Duration::from_millis(50));
    }
}

fn build_cross_platform_tray(snapshot: &TraySnapshot) -> Result<tray_icon::TrayIcon, String> {
    let menu = build_cross_platform_menu(snapshot)?;
    let mut builder = TrayIconBuilder::new().with_menu(Box::new(menu));
    if let Some(icon) = crate::app_icon::tray_icon_cross_platform() {
        builder = builder.with_icon(icon);
    }
    if !snapshot.title.trim().is_empty() {
        builder = builder.with_title(snapshot.title.clone());
    }
    if !snapshot.status_label.trim().is_empty() {
        builder = builder.with_tooltip(snapshot.status_label.clone());
    }
    #[cfg(target_os = "windows")]
    {
        builder = builder
            .with_menu_on_left_click(false)
            .with_menu_on_right_click(true);
    }

    let tray = builder.build().map_err(|error| error.to_string())?;
    #[cfg(target_os = "windows")]
    {
        tray.set_show_menu_on_left_click(false);
        tray.set_show_menu_on_right_click(true);
    }
    Ok(tray)
}

fn build_cross_platform_menu(snapshot: &TraySnapshot) -> Result<Menu, String> {
    let menu = Menu::new();

    let status = MenuItem::with_id(
        MenuId::new("tray.status"),
        &snapshot.status_label,
        false,
        None,
    );
    menu.append(&status).map_err(|error| error.to_string())?;

    let show_window =
        MenuItem::with_id(MenuId::new(MENU_ID_SHOW_WINDOW), "Show Window", true, None);
    menu.append(&show_window)
        .map_err(|error| error.to_string())?;

    if snapshot.can_start_monitoring {
        let start = MenuItem::with_id(
            MenuId::new(MENU_ID_START_MONITORING),
            "Start Monitoring",
            true,
            None,
        );
        menu.append(&start).map_err(|error| error.to_string())?;
    }

    if snapshot.can_stop_monitoring {
        let stop = MenuItem::with_id(
            MenuId::new(MENU_ID_STOP_MONITORING),
            "Stop Monitoring",
            true,
            None,
        );
        menu.append(&stop).map_err(|error| error.to_string())?;
    }

    let profiles = Submenu::with_id(MenuId::new("tray.profiles"), "Profiles", true);
    if snapshot.profile_options.is_empty() {
        let empty = MenuItem::new("No profiles configured", false, None);
        profiles.append(&empty).map_err(|error| error.to_string())?;
    } else {
        for profile in &snapshot.profile_options {
            let label = if profile.selected {
                format!("* {}", profile.name)
            } else {
                profile.name.clone()
            };
            let item = MenuItem::with_id(
                MenuId::new(format!("{MENU_ID_PROFILE_PREFIX}{}", profile.id)),
                label,
                true,
                None,
            );
            profiles.append(&item).map_err(|error| error.to_string())?;
        }
    }
    menu.append(&profiles).map_err(|error| error.to_string())?;

    let separator = PredefinedMenuItem::separator();
    menu.append(&separator).map_err(|error| error.to_string())?;

    let quit = MenuItem::with_id(MenuId::new(MENU_ID_QUIT), "Quit", true, None);
    menu.append(&quit).map_err(|error| error.to_string())?;

    Ok(menu)
}

fn update_cross_platform_tray(
    tray: &tray_icon::TrayIcon,
    snapshot: &TraySnapshot,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        tray.set_show_menu_on_left_click(false);
        tray.set_show_menu_on_right_click(true);
    }
    tray.set_tooltip(Some(snapshot.status_label.clone()))
        .map_err(|error| error.to_string())?;
    tray.set_title(Some(snapshot.title.clone()));
    let menu = build_cross_platform_menu(snapshot)?;
    tray.set_menu(Some(Box::new(menu)));
    Ok(())
}

#[cfg(target_os = "linux")]
fn prefers_ksni(desktop_environment: DesktopEnvironment) -> bool {
    desktop_environment == DesktopEnvironment::KdePlasma
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn prefers_ksni_only_on_kde_plasma() {
        assert!(prefers_ksni(DesktopEnvironment::KdePlasma));
        assert!(!prefers_ksni(DesktopEnvironment::Unknown));
    }
}
