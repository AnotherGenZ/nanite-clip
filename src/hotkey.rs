mod binding;

use std::str::FromStr;
#[cfg(target_os = "windows")]
use std::sync::{Mutex, OnceLock, mpsc};
#[cfg(target_os = "windows")]
use std::thread::JoinHandle;

#[cfg(target_os = "linux")]
use ashpd::desktop::global_shortcuts::{
    Activated, BindShortcutsOptions, ConfigureShortcutsOptions, GlobalShortcuts,
    ListShortcutsOptions, NewShortcut,
};
#[cfg(target_os = "linux")]
use ashpd::desktop::{CreateSessionOptions, Session};
#[cfg(target_os = "linux")]
use ashpd::zbus::{Connection as DBusConnection, Proxy as DBusProxy};
use global_hotkey::hotkey::HotKey;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use global_hotkey::hotkey::{Code as HotKeyCode, Modifiers as HotKeyModifiers};
#[cfg(not(target_os = "windows"))]
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
#[cfg(target_os = "linux")]
use iced::futures::StreamExt;
use iced::keyboard;
use iced::keyboard::key::{Code as IcedKeyCode, Named, Physical};
#[cfg(target_os = "linux")]
use tokio::sync::{mpsc, oneshot};
#[cfg(target_os = "linux")]
use tokio::task::JoinHandle;
#[cfg(target_os = "linux")]
use tokio::time::{Duration, sleep};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{
    ERROR_HOTKEY_ALREADY_REGISTERED, GetLastError, LPARAM, LRESULT, WPARAM,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::GetCurrentThreadId;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    RegisterHotKey, UnregisterHotKey, VIRTUAL_KEY, VK_0, VK_1, VK_2, VK_3, VK_4, VK_5, VK_6, VK_7,
    VK_8, VK_9, VK_A, VK_ADD, VK_B, VK_BACK, VK_C, VK_CAPITAL, VK_CONTROL, VK_D, VK_DECIMAL,
    VK_DELETE, VK_DIVIDE, VK_DOWN, VK_E, VK_END, VK_ESCAPE, VK_F, VK_F1, VK_F2, VK_F3, VK_F4,
    VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_F10, VK_F11, VK_F12, VK_F13, VK_F14, VK_F15, VK_F16,
    VK_F17, VK_F18, VK_F19, VK_F20, VK_F21, VK_F22, VK_F23, VK_F24, VK_G, VK_H, VK_HOME, VK_I,
    VK_INSERT, VK_J, VK_K, VK_L, VK_LEFT, VK_LWIN, VK_M, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE,
    VK_MEDIA_PREV_TRACK, VK_MEDIA_STOP, VK_MENU, VK_MULTIPLY, VK_N, VK_NEXT, VK_NUMLOCK,
    VK_NUMPAD0, VK_NUMPAD1, VK_NUMPAD2, VK_NUMPAD3, VK_NUMPAD4, VK_NUMPAD5, VK_NUMPAD6, VK_NUMPAD7,
    VK_NUMPAD8, VK_NUMPAD9, VK_O, VK_OEM_1, VK_OEM_2, VK_OEM_3, VK_OEM_4, VK_OEM_5, VK_OEM_6,
    VK_OEM_7, VK_OEM_COMMA, VK_OEM_MINUS, VK_OEM_PERIOD, VK_OEM_PLUS, VK_P, VK_PAUSE, VK_PLAY,
    VK_PRIOR, VK_Q, VK_R, VK_RETURN, VK_RIGHT, VK_RWIN, VK_S, VK_SCROLL, VK_SHIFT, VK_SNAPSHOT,
    VK_SPACE, VK_SUBTRACT, VK_T, VK_TAB, VK_U, VK_UP, VK_V, VK_VOLUME_DOWN, VK_VOLUME_MUTE,
    VK_VOLUME_UP, VK_W, VK_X, VK_Y, VK_Z,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, MSG,
    PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WM_APP, WM_HOTKEY, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::config::ManualClipConfig;
#[cfg(target_os = "linux")]
use crate::platform::{
    PlatformHotkeyEvent, PlatformHotkeyServiceHandle, start_plasma_manual_clip_hotkey,
};
use crate::process::{DesktopEnvironment, DisplayServer};
pub(crate) use binding::*;

pub use binding::capture_binding;

#[cfg(target_os = "linux")]
const MANUAL_CLIP_SHORTCUT_ID: &str = "manual_clip_save";
#[cfg(target_os = "linux")]
const KDE_KGLOBALACCEL_SERVICE: &str = "org.kde.kglobalaccel";
#[cfg(target_os = "linux")]
const KDE_KGLOBALACCEL_PATH: &str = "/kglobalaccel";
#[cfg(target_os = "linux")]
const KDE_KGLOBALACCEL_INTERFACE: &str = "org.kde.KGlobalAccel";
#[cfg(target_os = "linux")]
const KDE_PORTAL_COMPONENT_UNIQUE: &str = "surface-transient";
#[cfg(target_os = "linux")]
type KdeShortcutSequence = (Vec<i32>,);
#[cfg(target_os = "linux")]
const QT_SHIFT_MODIFIER: i32 = 0x0200_0000;
#[cfg(target_os = "linux")]
const QT_CONTROL_MODIFIER: i32 = 0x0400_0000;
#[cfg(target_os = "linux")]
const QT_ALT_MODIFIER: i32 = 0x0800_0000;
#[cfg(target_os = "linux")]
const QT_META_MODIFIER: i32 = 0x1000_0000;
#[cfg(target_os = "linux")]
const QT_KEYPAD_MODIFIER: i32 = 0x2000_0000;
#[cfg(target_os = "linux")]
const QT_KEY_ESCAPE: i32 = 0x0100_0000;
#[cfg(target_os = "linux")]
const QT_KEY_TAB: i32 = 0x0100_0001;
#[cfg(target_os = "linux")]
const QT_KEY_BACKSPACE: i32 = 0x0100_0003;
#[cfg(target_os = "linux")]
const QT_KEY_RETURN: i32 = 0x0100_0004;
#[cfg(target_os = "linux")]
const QT_KEY_ENTER: i32 = 0x0100_0005;
#[cfg(target_os = "linux")]
const QT_KEY_INSERT: i32 = 0x0100_0006;
#[cfg(target_os = "linux")]
const QT_KEY_DELETE: i32 = 0x0100_0007;
#[cfg(target_os = "linux")]
const QT_KEY_PAUSE: i32 = 0x0100_0008;
#[cfg(target_os = "linux")]
const QT_KEY_PRINT: i32 = 0x0100_0009;
#[cfg(target_os = "linux")]
const QT_KEY_HOME: i32 = 0x0100_0010;
#[cfg(target_os = "linux")]
const QT_KEY_END: i32 = 0x0100_0011;
#[cfg(target_os = "linux")]
const QT_KEY_LEFT: i32 = 0x0100_0012;
#[cfg(target_os = "linux")]
const QT_KEY_UP: i32 = 0x0100_0013;
#[cfg(target_os = "linux")]
const QT_KEY_RIGHT: i32 = 0x0100_0014;
#[cfg(target_os = "linux")]
const QT_KEY_DOWN: i32 = 0x0100_0015;
#[cfg(target_os = "linux")]
const QT_KEY_PAGE_UP: i32 = 0x0100_0016;
#[cfg(target_os = "linux")]
const QT_KEY_PAGE_DOWN: i32 = 0x0100_0017;
#[cfg(target_os = "linux")]
const QT_KEY_CAPS_LOCK: i32 = 0x0100_0024;
#[cfg(target_os = "linux")]
const QT_KEY_NUM_LOCK: i32 = 0x0100_0025;
#[cfg(target_os = "linux")]
const QT_KEY_SCROLL_LOCK: i32 = 0x0100_0026;
#[cfg(target_os = "linux")]
const QT_KEY_F1: i32 = 0x0100_0030;
#[cfg(target_os = "linux")]
const QT_KEY_VOLUME_DOWN: i32 = 0x0100_0070;
#[cfg(target_os = "linux")]
const QT_KEY_VOLUME_MUTE: i32 = 0x0100_0071;
#[cfg(target_os = "linux")]
const QT_KEY_VOLUME_UP: i32 = 0x0100_0072;
#[cfg(target_os = "linux")]
const QT_KEY_MEDIA_PLAY: i32 = 0x0100_0080;
#[cfg(target_os = "linux")]
const QT_KEY_MEDIA_STOP: i32 = 0x0100_0081;
#[cfg(target_os = "linux")]
const QT_KEY_MEDIA_PREVIOUS: i32 = 0x0100_0082;
#[cfg(target_os = "linux")]
const QT_KEY_MEDIA_NEXT: i32 = 0x0100_0083;
#[cfg(target_os = "linux")]
const QT_KEY_MEDIA_PAUSE: i32 = 0x0100_0085;
#[cfg(target_os = "linux")]
const QT_KEY_MEDIA_TOGGLE_PLAY_PAUSE: i32 = 0x0100_0086;
pub struct HotkeyManager {
    backend: HotkeyBackend,
    binding_label: Option<String>,
    configuration_note: Option<String>,
}

impl std::fmt::Debug for HotkeyManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HotkeyManager")
            .field("binding_label", &self.binding_label)
            .field("configuration_note", &self.configuration_note)
            .finish_non_exhaustive()
    }
}

enum HotkeyBackend {
    Disabled,
    #[cfg(not(target_os = "windows"))]
    GlobalHotkey {
        _manager: GlobalHotKeyManager,
        hotkey: HotKey,
    },
    #[cfg(target_os = "windows")]
    Windows {
        receiver: mpsc::Receiver<HotkeyEvent>,
        shutdown_thread_id: u32,
        worker: Option<JoinHandle<()>>,
    },
    #[cfg(target_os = "linux")]
    KdePlatformService {
        service: PlatformHotkeyServiceHandle,
    },
    #[cfg(target_os = "linux")]
    Wayland {
        receiver: mpsc::UnboundedReceiver<HotkeyEvent>,
        shutdown: Option<oneshot::Sender<()>>,
        task: JoinHandle<()>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Activated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingCapture {
    Captured(String),
    Unsupported,
    Ignored,
}

#[cfg(target_os = "windows")]
const WINDOWS_HOTKEY_SHUTDOWN_MESSAGE: u32 = WM_APP + 1;
#[cfg(target_os = "windows")]
static WINDOWS_LOW_LEVEL_HOOK_STATE: OnceLock<Mutex<Option<WindowsLowLevelHookState>>> =
    OnceLock::new();

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WindowsHookSpec {
    scan_code: u32,
    extended: bool,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowsHotkeyRegistration {
    RegisterHotKey { virtual_key: VIRTUAL_KEY },
    LowLevelKeyboardHook { spec: WindowsHookSpec },
}

#[cfg(target_os = "windows")]
enum WindowsWorkerRegistration {
    RegisterHotKey,
    LowLevelKeyboardHook { hook: HHOOK },
}

#[cfg(target_os = "windows")]
struct WindowsLowLevelHookState {
    sender: mpsc::Sender<HotkeyEvent>,
    hotkey_id: u32,
    binding: String,
    modifiers: HotKeyModifiers,
    spec: WindowsHookSpec,
    active: bool,
}

impl HotkeyManager {
    pub fn disabled() -> Self {
        Self {
            backend: HotkeyBackend::Disabled,
            binding_label: None,
            configuration_note: None,
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub async fn configure(
        config: &ManualClipConfig,
        display_server: DisplayServer,
        desktop_environment: DesktopEnvironment,
    ) -> Result<Self, String> {
        if !config.enabled {
            return Ok(Self::disabled());
        }

        #[cfg(target_os = "linux")]
        match display_server {
            DisplayServer::Wayland => {
                if desktop_environment == DesktopEnvironment::KdePlasma {
                    match Self::configure_plasma_platform_service(config).await {
                        Ok(manager) => return Ok(manager),
                        Err(error) => {
                            tracing::warn!(
                                "failed to configure the KDE platform service hotkey backend, falling back: {error}"
                            );
                        }
                    }
                }
                Self::configure_wayland(config).await
            }
            DisplayServer::X11 | DisplayServer::Unknown => Self::configure_global_hotkey(config),
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = display_server;
            let _ = desktop_environment;
            Self::configure_global_hotkey(config)
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn configure_sync(
        config: &ManualClipConfig,
        display_server: DisplayServer,
        desktop_environment: DesktopEnvironment,
    ) -> Result<Self, String> {
        if !config.enabled {
            return Ok(Self::disabled());
        }

        let _ = display_server;
        let _ = desktop_environment;
        Self::configure_global_hotkey(config)
    }

    pub fn binding_label(&self) -> Option<&str> {
        self.binding_label.as_deref()
    }

    pub fn configuration_note(&self) -> Option<&str> {
        self.configuration_note.as_deref()
    }

    pub fn drain_events(&mut self) -> Vec<HotkeyEvent> {
        let events = match &mut self.backend {
            HotkeyBackend::Disabled => Vec::new(),
            #[cfg(not(target_os = "windows"))]
            HotkeyBackend::GlobalHotkey { hotkey, .. } => {
                let receiver = GlobalHotKeyEvent::receiver();
                receiver
                    .try_iter()
                    .filter_map(|event| {
                        (event.id == hotkey.id() && event.state == HotKeyState::Pressed)
                            .then_some(HotkeyEvent::Activated)
                    })
                    .collect()
            }
            #[cfg(target_os = "windows")]
            HotkeyBackend::Windows { receiver, .. } => receiver.try_iter().collect(),
            #[cfg(target_os = "linux")]
            HotkeyBackend::KdePlatformService { service } => service
                .drain_events()
                .into_iter()
                .map(|event| match event {
                    PlatformHotkeyEvent::Activated => HotkeyEvent::Activated,
                })
                .collect(),
            #[cfg(target_os = "linux")]
            HotkeyBackend::Wayland { receiver, .. } => receiver.try_recv().into_iter().collect(),
        };

        if !events.is_empty() {
            tracing::debug!(
                event_count = events.len(),
                ?events,
                "drained manual clip hotkey events"
            );
        }

        events
    }

    fn configure_global_hotkey(config: &ManualClipConfig) -> Result<Self, String> {
        tracing::debug!(
            enabled = config.enabled,
            hotkey = %config.hotkey,
            duration_secs = config.duration_secs,
            "configuring manual clip hotkey"
        );
        let hotkey = parse_hotkey(config.hotkey.as_str())?;

        #[cfg(target_os = "windows")]
        {
            Self::configure_windows_hotkey(hotkey)
        }

        #[cfg(not(target_os = "windows"))]
        {
            let manager = GlobalHotKeyManager::new().map_err(|error| error.to_string())?;
            manager
                .register(hotkey)
                .map_err(|error| format!("failed to register manual clip hotkey: {error}"))?;

            Ok(Self {
                backend: HotkeyBackend::GlobalHotkey {
                    _manager: manager,
                    hotkey,
                },
                binding_label: Some(hotkey.to_string()),
                configuration_note: None,
            })
        }
    }

    #[cfg(target_os = "windows")]
    fn configure_windows_hotkey(hotkey: HotKey) -> Result<Self, String> {
        let registration = windows_hotkey_registration(hotkey)?;
        let backend = match registration {
            WindowsHotkeyRegistration::RegisterHotKey { .. } => "register_hotkey",
            WindowsHotkeyRegistration::LowLevelKeyboardHook { .. } => "low_level_keyboard_hook",
        };
        tracing::debug!(
            binding = %hotkey,
            hotkey_id = hotkey.id(),
            backend,
            "starting Windows manual clip hotkey worker"
        );
        let (event_tx, event_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("manual-clip-hotkey".into())
            .spawn(move || {
                let mut message = MSG::default();
                // SAFETY: Touching the queue once ensures PostThreadMessage can address this
                // worker thread before it blocks inside GetMessageW.
                let _ = unsafe { PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE) };
                let thread_id = unsafe { GetCurrentThreadId() };
                tracing::debug!(
                    binding = %hotkey,
                    hotkey_id = hotkey.id(),
                    backend,
                    thread_id,
                    "manual clip hotkey worker ready"
                );

                let result = match registration {
                    WindowsHotkeyRegistration::RegisterHotKey { virtual_key } => {
                        register_windows_hotkey(hotkey, virtual_key)
                            .map(|_| WindowsWorkerRegistration::RegisterHotKey)
                    }
                    WindowsHotkeyRegistration::LowLevelKeyboardHook { spec } => {
                        register_windows_low_level_keyboard_hook(
                            hotkey,
                            spec,
                            event_tx.clone(),
                        )
                        .map(|hook| WindowsWorkerRegistration::LowLevelKeyboardHook { hook })
                    }
                };
                let ready_result = result
                    .as_ref()
                    .map(|_| thread_id)
                    .map_err(|error| error.clone());
                if ready_tx.send(ready_result).is_err() {
                    if let Ok(registration) = result {
                        let _ = unregister_windows_registration(hotkey, registration);
                    }
                    return;
                }
                let Ok(worker_registration) = result else {
                    return;
                };

                loop {
                    let status = unsafe { GetMessageW(&mut message, None, 0, 0) };
                    if status.0 == -1 {
                        tracing::warn!(
                            binding = %hotkey,
                            hotkey_id = hotkey.id(),
                            thread_id,
                            "manual clip hotkey worker failed to receive a Windows message"
                        );
                        break;
                    }
                    if !status.as_bool() {
                        tracing::debug!(binding = %hotkey, hotkey_id = hotkey.id(), thread_id, "manual clip hotkey worker received quit message");
                        break;
                    }

                    match message.message {
                        WM_HOTKEY if message.wParam == WPARAM(hotkey.id() as usize) => {
                            tracing::debug!(
                                binding = %hotkey,
                                hotkey_id = hotkey.id(),
                                backend,
                                thread_id,
                                "manual clip hotkey received WM_HOTKEY"
                            );
                            let _ = event_tx.send(HotkeyEvent::Activated);
                        }
                        WINDOWS_HOTKEY_SHUTDOWN_MESSAGE => {
                            tracing::debug!(
                                binding = %hotkey,
                                hotkey_id = hotkey.id(),
                                backend,
                                thread_id,
                                "manual clip hotkey worker received shutdown message"
                            );
                            break;
                        }
                        _ => {}
                    }
                }

                if let Err(error) = unregister_windows_registration(hotkey, worker_registration) {
                    tracing::warn!("failed to unregister manual clip hotkey: {error}");
                }
            })
            .map_err(|error| format!("failed to spawn manual clip hotkey worker: {error}"))?;

        match ready_rx.recv() {
            Ok(Ok(thread_id)) => {
                tracing::debug!(
                    binding = %hotkey,
                    hotkey_id = hotkey.id(),
                    backend,
                    thread_id,
                    "manual clip hotkey worker registered successfully"
                );
                Ok(Self {
                    backend: HotkeyBackend::Windows {
                        receiver: event_rx,
                        shutdown_thread_id: thread_id,
                        worker: Some(worker),
                    },
                    binding_label: Some(hotkey.to_string()),
                    configuration_note: None,
                })
            }
            Ok(Err(error)) => {
                let _ = worker.join();
                tracing::warn!(
                    binding = %hotkey,
                    hotkey_id = hotkey.id(),
                    backend,
                    %error,
                    "manual clip hotkey worker failed to register"
                );
                Err(error)
            }
            Err(error) => {
                let _ = worker.join();
                let error = format!("failed to initialize the manual clip hotkey worker: {error}");
                tracing::warn!(
                    binding = %hotkey,
                    hotkey_id = hotkey.id(),
                    backend,
                    %error,
                    "manual clip hotkey worker failed to initialize"
                );
                Err(error)
            }
        }
    }

    #[cfg(target_os = "linux")]
    async fn configure_plasma_platform_service(config: &ManualClipConfig) -> Result<Self, String> {
        let description = format!("Save a {} second manual clip", config.duration_secs);
        let requested_sequence = kde_shortcut_sequence(config.hotkey.as_str())?;
        let combined_key =
            requested_sequence.0.first().copied().ok_or_else(|| {
                "requested KDE shortcut sequence was unexpectedly empty".to_string()
            })?;

        clear_plasma_portal_shortcut_conflict().await?;
        let service =
            start_plasma_manual_clip_hotkey(combined_key, MANUAL_CLIP_SHORTCUT_ID, &description)
                .await?;

        Ok(Self {
            binding_label: Some(service.binding_label().to_string()),
            configuration_note: service.configuration_note().map(str::to_string),
            backend: HotkeyBackend::KdePlatformService { service },
        })
    }

    #[cfg(target_os = "linux")]
    async fn configure_wayland(config: &ManualClipConfig) -> Result<Self, String> {
        let description = format!("Save a {} second manual clip", config.duration_secs);
        let preferred_trigger = portal_preferred_trigger(config.hotkey.as_str());
        let proxy = GlobalShortcuts::new()
            .await
            .map_err(|error| format!("failed to connect to the GlobalShortcuts portal: {error}"))?;
        let session = proxy
            .create_session(CreateSessionOptions::default())
            .await
            .map_err(|error| {
                format!("failed to create a GlobalShortcuts portal session: {error}")
            })?;

        let binding = Self::bind_wayland_shortcut(
            &proxy,
            &session,
            preferred_trigger.as_deref(),
            &description,
        )
        .await?;
        let binding_label =
            Self::resolve_wayland_binding_label(&proxy, &session, binding.shortcuts()).await?;

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let task = tokio::spawn(run_wayland_hotkey_session(
            proxy,
            session,
            event_tx,
            shutdown_rx,
        ));

        let configuration_note = binding_label.is_none().then(|| {
            let requested = config.hotkey.trim();
            if requested.is_empty() {
                format!(
                    "Manual clip action `{description}` was registered with the desktop shortcut portal, but no active shortcut was assigned. Assign it in your desktop shortcut settings."
                )
            } else {
                format!(
                    "Manual clip action `{description}` was registered with the desktop shortcut portal, but no active shortcut was assigned. Assign `{requested}` to it in your desktop shortcut settings."
                )
            }
        });

        Ok(Self {
            backend: HotkeyBackend::Wayland {
                receiver: event_rx,
                shutdown: Some(shutdown_tx),
                task,
            },
            binding_label,
            configuration_note,
        })
    }

    #[cfg(target_os = "linux")]
    async fn bind_wayland_shortcut(
        proxy: &GlobalShortcuts,
        session: &Session<GlobalShortcuts>,
        preferred_trigger: Option<&str>,
        description: &str,
    ) -> Result<ashpd::desktop::global_shortcuts::BindShortcuts, String> {
        let request = proxy
            .bind_shortcuts(
                session,
                &[NewShortcut::new(MANUAL_CLIP_SHORTCUT_ID, description)
                    .preferred_trigger(preferred_trigger)],
                None,
                BindShortcutsOptions::default(),
            )
            .await
            .map_err(|error| format!("failed to request portal shortcut binding: {error}"))?;

        request
            .response()
            .map_err(|error| format!("failed to bind portal shortcut: {error}"))
    }

    #[cfg(target_os = "linux")]
    async fn resolve_wayland_binding_label(
        proxy: &GlobalShortcuts,
        session: &Session<GlobalShortcuts>,
        initially_bound: &[ashpd::desktop::global_shortcuts::Shortcut],
    ) -> Result<Option<String>, String> {
        if let Some(binding_label) = first_wayland_binding_label(initially_bound) {
            return Ok(Some(binding_label));
        }

        let listed = Self::list_wayland_shortcuts(proxy, session).await?;
        if let Some(binding_label) = first_wayland_binding_label(&listed) {
            return Ok(Some(binding_label));
        }

        if proxy.version() < 2 {
            tracing::warn!(
                "GlobalShortcuts portal v{} did not report an active shortcut; continuing without a confirmed binding label",
                proxy.version()
            );
            return Ok(None);
        }

        proxy
            .configure_shortcuts(session, None, ConfigureShortcutsOptions::default())
            .await
            .map_err(|error| {
                format!(
                    "failed to configure manual clip hotkey through the GlobalShortcuts portal: {error}"
                )
            })?;

        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            let listed = Self::list_wayland_shortcuts(proxy, session).await?;
            if let Some(binding_label) = first_wayland_binding_label(&listed) {
                return Ok(Some(binding_label));
            }

            if std::time::Instant::now() >= deadline {
                break;
            }

            sleep(Duration::from_millis(500)).await;
        }

        Err("manual clip hotkey was not activated by the GlobalShortcuts portal. Configure it in the portal or desktop shortcut UI, then save settings again.".into())
    }

    #[cfg(target_os = "linux")]
    async fn list_wayland_shortcuts(
        proxy: &GlobalShortcuts,
        session: &Session<GlobalShortcuts>,
    ) -> Result<Vec<ashpd::desktop::global_shortcuts::Shortcut>, String> {
        let request = proxy
            .list_shortcuts(session, ListShortcutsOptions::default())
            .await
            .map_err(|error| format!("failed to query portal shortcuts: {error}"))?;
        let response = request
            .response()
            .map_err(|error| format!("failed to read portal shortcuts: {error}"))?;

        Ok(response.shortcuts().to_vec())
    }
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        match &mut self.backend {
            #[cfg(target_os = "windows")]
            HotkeyBackend::Windows {
                shutdown_thread_id,
                worker,
                ..
            } => {
                tracing::debug!(
                    thread_id = *shutdown_thread_id,
                    "shutting down manual clip hotkey worker"
                );
                let _ = unsafe {
                    PostThreadMessageW(
                        *shutdown_thread_id,
                        WINDOWS_HOTKEY_SHUTDOWN_MESSAGE,
                        WPARAM(0),
                        LPARAM(0),
                    )
                };
                if let Some(worker) = worker.take() {
                    let _ = worker.join();
                }
            }
            #[cfg(target_os = "linux")]
            HotkeyBackend::KdePlatformService { .. } => {}
            #[cfg(target_os = "linux")]
            HotkeyBackend::Wayland { shutdown, task, .. } => {
                if let Some(shutdown) = shutdown.take() {
                    let _ = shutdown.send(());
                }
                task.abort();
            }
            #[cfg(not(target_os = "windows"))]
            HotkeyBackend::Disabled | HotkeyBackend::GlobalHotkey { .. } => {}
            #[cfg(target_os = "windows")]
            HotkeyBackend::Disabled => {}
        }
    }
}
