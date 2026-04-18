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
use crate::platform_service::{
    PlatformHotkeyEvent, PlatformHotkeyServiceHandle, start_plasma_manual_clip_hotkey,
};
use crate::process::{DesktopEnvironment, DisplayServer};

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

pub fn capture_binding(event: &keyboard::Event) -> BindingCapture {
    let keyboard::Event::KeyPressed {
        key,
        physical_key,
        modifiers,
        repeat,
        ..
    } = event
    else {
        return BindingCapture::Ignored;
    };

    if *repeat || is_modifier_key(key) {
        return BindingCapture::Ignored;
    }

    match binding_key_name(*physical_key, key) {
        Some(key) => BindingCapture::Captured(format_binding(*modifiers, key)),
        None => BindingCapture::Unsupported,
    }
}

#[cfg(target_os = "linux")]
async fn run_wayland_hotkey_session(
    proxy: GlobalShortcuts,
    session: Session<GlobalShortcuts>,
    event_tx: mpsc::UnboundedSender<HotkeyEvent>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let mut activated_stream = match proxy.receive_activated().await {
        Ok(stream) => stream,
        Err(error) => {
            tracing::warn!("failed to listen for portal hotkey activations: {error}");
            return;
        }
    };
    let mut closed_stream = match session.receive_closed().await {
        Ok(stream) => stream,
        Err(error) => {
            tracing::warn!("failed to listen for portal session closure: {error}");
            return;
        }
    };

    loop {
        tokio::select! {
            _ = &mut shutdown_rx => {
                let _ = session.close().await;
                return;
            }
            event = activated_stream.next() => {
                match event {
                    Some(activated) => handle_wayland_activation(&event_tx, activated),
                    None => return,
                }
            }
            event = closed_stream.next() => {
                match event {
                    Some(_) | None => return,
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn handle_wayland_activation(event_tx: &mpsc::UnboundedSender<HotkeyEvent>, event: Activated) {
    if event.shortcut_id() == MANUAL_CLIP_SHORTCUT_ID {
        let _ = event_tx.send(HotkeyEvent::Activated);
    }
}

#[cfg(target_os = "linux")]
fn kde_shortcut_sequence(binding: &str) -> Result<KdeShortcutSequence, String> {
    let hotkey = parse_hotkey(binding)?;
    Ok((vec![kde_shortcut_to_combined(hotkey)?],))
}

#[cfg(target_os = "linux")]
fn kde_shortcut_to_combined(hotkey: HotKey) -> Result<i32, String> {
    let mut combined = kde_key_code(hotkey.key)?;

    if hotkey.mods.contains(HotKeyModifiers::SHIFT) {
        combined |= QT_SHIFT_MODIFIER;
    }
    if hotkey.mods.contains(HotKeyModifiers::CONTROL) {
        combined |= QT_CONTROL_MODIFIER;
    }
    if hotkey.mods.contains(HotKeyModifiers::ALT) {
        combined |= QT_ALT_MODIFIER;
    }
    if hotkey.mods.contains(HotKeyModifiers::SUPER) || hotkey.mods.contains(HotKeyModifiers::META) {
        combined |= QT_META_MODIFIER;
    }

    Ok(combined)
}

#[cfg(target_os = "linux")]
fn kde_key_code(key: HotKeyCode) -> Result<i32, String> {
    use HotKeyCode::*;

    let code = match key {
        Backquote => '`' as i32,
        Backslash => '\\' as i32,
        BracketLeft => '[' as i32,
        BracketRight => ']' as i32,
        Comma => ',' as i32,
        Digit0 => '0' as i32,
        Digit1 => '1' as i32,
        Digit2 => '2' as i32,
        Digit3 => '3' as i32,
        Digit4 => '4' as i32,
        Digit5 => '5' as i32,
        Digit6 => '6' as i32,
        Digit7 => '7' as i32,
        Digit8 => '8' as i32,
        Digit9 => '9' as i32,
        Equal => '=' as i32,
        KeyA => 'A' as i32,
        KeyB => 'B' as i32,
        KeyC => 'C' as i32,
        KeyD => 'D' as i32,
        KeyE => 'E' as i32,
        KeyF => 'F' as i32,
        KeyG => 'G' as i32,
        KeyH => 'H' as i32,
        KeyI => 'I' as i32,
        KeyJ => 'J' as i32,
        KeyK => 'K' as i32,
        KeyL => 'L' as i32,
        KeyM => 'M' as i32,
        KeyN => 'N' as i32,
        KeyO => 'O' as i32,
        KeyP => 'P' as i32,
        KeyQ => 'Q' as i32,
        KeyR => 'R' as i32,
        KeyS => 'S' as i32,
        KeyT => 'T' as i32,
        KeyU => 'U' as i32,
        KeyV => 'V' as i32,
        KeyW => 'W' as i32,
        KeyX => 'X' as i32,
        KeyY => 'Y' as i32,
        KeyZ => 'Z' as i32,
        Minus => '-' as i32,
        Period => '.' as i32,
        Quote => '\'' as i32,
        Semicolon => ';' as i32,
        Slash => '/' as i32,
        Backspace => QT_KEY_BACKSPACE,
        CapsLock => QT_KEY_CAPS_LOCK,
        Enter => QT_KEY_RETURN,
        Space => ' ' as i32,
        Tab => QT_KEY_TAB,
        Delete => QT_KEY_DELETE,
        End => QT_KEY_END,
        Home => QT_KEY_HOME,
        Insert => QT_KEY_INSERT,
        PageDown => QT_KEY_PAGE_DOWN,
        PageUp => QT_KEY_PAGE_UP,
        PrintScreen => QT_KEY_PRINT,
        ScrollLock => QT_KEY_SCROLL_LOCK,
        ArrowDown => QT_KEY_DOWN,
        ArrowLeft => QT_KEY_LEFT,
        ArrowRight => QT_KEY_RIGHT,
        ArrowUp => QT_KEY_UP,
        NumLock => QT_KEY_NUM_LOCK,
        Numpad0 => QT_KEYPAD_MODIFIER | ('0' as i32),
        Numpad1 => QT_KEYPAD_MODIFIER | ('1' as i32),
        Numpad2 => QT_KEYPAD_MODIFIER | ('2' as i32),
        Numpad3 => QT_KEYPAD_MODIFIER | ('3' as i32),
        Numpad4 => QT_KEYPAD_MODIFIER | ('4' as i32),
        Numpad5 => QT_KEYPAD_MODIFIER | ('5' as i32),
        Numpad6 => QT_KEYPAD_MODIFIER | ('6' as i32),
        Numpad7 => QT_KEYPAD_MODIFIER | ('7' as i32),
        Numpad8 => QT_KEYPAD_MODIFIER | ('8' as i32),
        Numpad9 => QT_KEYPAD_MODIFIER | ('9' as i32),
        NumpadAdd => QT_KEYPAD_MODIFIER | ('+' as i32),
        NumpadDecimal => QT_KEYPAD_MODIFIER | ('.' as i32),
        NumpadDivide => QT_KEYPAD_MODIFIER | ('/' as i32),
        NumpadEnter => QT_KEYPAD_MODIFIER | QT_KEY_ENTER,
        NumpadEqual => QT_KEYPAD_MODIFIER | ('=' as i32),
        NumpadMultiply => QT_KEYPAD_MODIFIER | ('*' as i32),
        NumpadSubtract => QT_KEYPAD_MODIFIER | ('-' as i32),
        Escape => QT_KEY_ESCAPE,
        F1 => QT_KEY_F1,
        F2 => QT_KEY_F1 + 1,
        F3 => QT_KEY_F1 + 2,
        F4 => QT_KEY_F1 + 3,
        F5 => QT_KEY_F1 + 4,
        F6 => QT_KEY_F1 + 5,
        F7 => QT_KEY_F1 + 6,
        F8 => QT_KEY_F1 + 7,
        F9 => QT_KEY_F1 + 8,
        F10 => QT_KEY_F1 + 9,
        F11 => QT_KEY_F1 + 10,
        F12 => QT_KEY_F1 + 11,
        F13 => QT_KEY_F1 + 12,
        F14 => QT_KEY_F1 + 13,
        F15 => QT_KEY_F1 + 14,
        F16 => QT_KEY_F1 + 15,
        F17 => QT_KEY_F1 + 16,
        F18 => QT_KEY_F1 + 17,
        F19 => QT_KEY_F1 + 18,
        F20 => QT_KEY_F1 + 19,
        F21 => QT_KEY_F1 + 20,
        F22 => QT_KEY_F1 + 21,
        F23 => QT_KEY_F1 + 22,
        F24 => QT_KEY_F1 + 23,
        AudioVolumeDown => QT_KEY_VOLUME_DOWN,
        AudioVolumeUp => QT_KEY_VOLUME_UP,
        AudioVolumeMute => QT_KEY_VOLUME_MUTE,
        MediaPlay => QT_KEY_MEDIA_PLAY,
        MediaPause => QT_KEY_MEDIA_PAUSE,
        MediaPlayPause => QT_KEY_MEDIA_TOGGLE_PLAY_PAUSE,
        MediaStop => QT_KEY_MEDIA_STOP,
        MediaTrackNext => QT_KEY_MEDIA_NEXT,
        MediaTrackPrevious => QT_KEY_MEDIA_PREVIOUS,
        Pause => QT_KEY_PAUSE,
        other => {
            return Err(format!(
                "the KDE Plasma backend does not support the key `{other:?}` yet"
            ));
        }
    };

    Ok(code)
}

#[cfg(target_os = "linux")]
#[allow(dead_code)]
fn kde_shortcut_from_sequence(sequence: &[i32]) -> Option<String> {
    if sequence.len() != 1 {
        return None;
    }

    let combined = *sequence.first()?;
    let modifiers = combined & 0xfe00_0000u32 as i32;
    let key = combined & !0xfe00_0000u32 as i32;
    let keypad = modifiers & QT_KEYPAD_MODIFIER != 0;
    let mut parts = Vec::new();

    if modifiers & QT_CONTROL_MODIFIER != 0 {
        parts.push("Ctrl".to_string());
    }
    if modifiers & QT_ALT_MODIFIER != 0 {
        parts.push("Alt".to_string());
    }
    if modifiers & QT_SHIFT_MODIFIER != 0 {
        parts.push("Shift".to_string());
    }
    if modifiers & QT_META_MODIFIER != 0 {
        parts.push("Super".to_string());
    }

    let key = match key {
        value if keypad && value == QT_KEY_ENTER => "NumEnter".into(),
        value if keypad && value == ('0' as i32) => "Num0".into(),
        value if keypad && value == ('1' as i32) => "Num1".into(),
        value if keypad && value == ('2' as i32) => "Num2".into(),
        value if keypad && value == ('3' as i32) => "Num3".into(),
        value if keypad && value == ('4' as i32) => "Num4".into(),
        value if keypad && value == ('5' as i32) => "Num5".into(),
        value if keypad && value == ('6' as i32) => "Num6".into(),
        value if keypad && value == ('7' as i32) => "Num7".into(),
        value if keypad && value == ('8' as i32) => "Num8".into(),
        value if keypad && value == ('9' as i32) => "Num9".into(),
        value if keypad && value == ('+' as i32) => "NumAdd".into(),
        value if keypad && value == ('.' as i32) => "NumDecimal".into(),
        value if keypad && value == ('/' as i32) => "NumDivide".into(),
        value if keypad && value == ('=' as i32) => "NumEqual".into(),
        value if keypad && value == ('*' as i32) => "NumMultiply".into(),
        value if keypad && value == ('-' as i32) => "NumSubtract".into(),
        value if value == QT_KEY_ESCAPE => "Escape".into(),
        value if value == QT_KEY_TAB => "Tab".into(),
        value if value == QT_KEY_BACKSPACE => "Backspace".into(),
        value if value == QT_KEY_RETURN => "Enter".into(),
        value if value == QT_KEY_INSERT => "Insert".into(),
        value if value == QT_KEY_DELETE => "Delete".into(),
        value if value == QT_KEY_PAUSE => "Pause".into(),
        value if value == QT_KEY_PRINT => "PrintScreen".into(),
        value if value == QT_KEY_HOME => "Home".into(),
        value if value == QT_KEY_END => "End".into(),
        value if value == QT_KEY_LEFT => "ArrowLeft".into(),
        value if value == QT_KEY_UP => "ArrowUp".into(),
        value if value == QT_KEY_RIGHT => "ArrowRight".into(),
        value if value == QT_KEY_DOWN => "ArrowDown".into(),
        value if value == QT_KEY_PAGE_UP => "PageUp".into(),
        value if value == QT_KEY_PAGE_DOWN => "PageDown".into(),
        value if value == QT_KEY_CAPS_LOCK => "CapsLock".into(),
        value if value == QT_KEY_NUM_LOCK => "NumLock".into(),
        value if value == QT_KEY_SCROLL_LOCK => "ScrollLock".into(),
        value if (QT_KEY_F1..=QT_KEY_F1 + 23).contains(&value) => {
            format!("F{}", value - QT_KEY_F1 + 1)
        }
        value if value == QT_KEY_VOLUME_DOWN => "VolumeDown".into(),
        value if value == QT_KEY_VOLUME_MUTE => "VolumeMute".into(),
        value if value == QT_KEY_VOLUME_UP => "VolumeUp".into(),
        value if value == QT_KEY_MEDIA_PLAY => "MediaPlay".into(),
        value if value == QT_KEY_MEDIA_STOP => "MediaStop".into(),
        value if value == QT_KEY_MEDIA_PREVIOUS => "MediaTrackPrevious".into(),
        value if value == QT_KEY_MEDIA_NEXT => "MediaTrackNext".into(),
        value if value == QT_KEY_MEDIA_PAUSE => "MediaPause".into(),
        value if value == QT_KEY_MEDIA_TOGGLE_PLAY_PAUSE => "MediaPlayPause".into(),
        value if value == (' ' as i32) => "Space".into(),
        value if (0x21..=0x7e).contains(&value) => char::from_u32(value as u32)?.to_string(),
        _ => return None,
    };

    parts.push(key);
    Some(parts.join("+"))
}

#[cfg(target_os = "linux")]
fn first_wayland_binding_label(
    shortcuts: &[ashpd::desktop::global_shortcuts::Shortcut],
) -> Option<String> {
    shortcuts
        .first()
        .map(|shortcut| shortcut.trigger_description().trim().to_string())
        .filter(|value| !value.is_empty())
}

fn is_modifier_key(key: &keyboard::Key) -> bool {
    matches!(
        key.as_ref(),
        keyboard::Key::Named(
            Named::Alt
                | Named::AltGraph
                | Named::Control
                | Named::Shift
                | Named::Super
                | Named::Meta
                | Named::Hyper
                | Named::Fn
                | Named::FnLock
                | Named::Symbol
                | Named::SymbolLock
        )
    )
}

fn format_binding(modifiers: keyboard::Modifiers, key: String) -> String {
    let mut parts = Vec::new();

    if modifiers.control() {
        parts.push("Ctrl".to_string());
    }
    if modifiers.alt() {
        parts.push("Alt".to_string());
    }
    if modifiers.shift() {
        parts.push("Shift".to_string());
    }
    if modifiers.logo() {
        parts.push("Super".to_string());
    }

    parts.push(key);
    parts.join("+")
}

fn binding_key_name(physical_key: Physical, key: &keyboard::Key) -> Option<String> {
    match physical_key {
        Physical::Code(code) => binding_key_name_from_code(code),
        Physical::Unidentified(_) => binding_key_name_from_named_key(key),
    }
}

fn binding_key_name_from_named_key(key: &keyboard::Key) -> Option<String> {
    match key.as_ref() {
        keyboard::Key::Character(value) => {
            let mut characters = value.chars();
            let character = characters.next()?;
            if characters.next().is_none() && character.is_ascii_alphanumeric() {
                Some(character.to_ascii_uppercase().to_string())
            } else {
                None
            }
        }
        keyboard::Key::Named(Named::ArrowDown) => Some("ArrowDown".into()),
        keyboard::Key::Named(Named::ArrowLeft) => Some("ArrowLeft".into()),
        keyboard::Key::Named(Named::ArrowRight) => Some("ArrowRight".into()),
        keyboard::Key::Named(Named::ArrowUp) => Some("ArrowUp".into()),
        keyboard::Key::Named(Named::AudioVolumeDown) => Some("AudioVolumeDown".into()),
        keyboard::Key::Named(Named::AudioVolumeMute) => Some("AudioVolumeMute".into()),
        keyboard::Key::Named(Named::AudioVolumeUp) => Some("AudioVolumeUp".into()),
        keyboard::Key::Named(Named::Backspace) => Some("Backspace".into()),
        keyboard::Key::Named(Named::CapsLock) => Some("CapsLock".into()),
        keyboard::Key::Named(Named::Delete) => Some("Delete".into()),
        keyboard::Key::Named(Named::End) => Some("End".into()),
        keyboard::Key::Named(Named::Enter) => Some("Enter".into()),
        keyboard::Key::Named(Named::Escape) => Some("Escape".into()),
        keyboard::Key::Named(Named::F1) => Some("F1".into()),
        keyboard::Key::Named(Named::F2) => Some("F2".into()),
        keyboard::Key::Named(Named::F3) => Some("F3".into()),
        keyboard::Key::Named(Named::F4) => Some("F4".into()),
        keyboard::Key::Named(Named::F5) => Some("F5".into()),
        keyboard::Key::Named(Named::F6) => Some("F6".into()),
        keyboard::Key::Named(Named::F7) => Some("F7".into()),
        keyboard::Key::Named(Named::F8) => Some("F8".into()),
        keyboard::Key::Named(Named::F9) => Some("F9".into()),
        keyboard::Key::Named(Named::F10) => Some("F10".into()),
        keyboard::Key::Named(Named::F11) => Some("F11".into()),
        keyboard::Key::Named(Named::F12) => Some("F12".into()),
        keyboard::Key::Named(Named::F13) => Some("F13".into()),
        keyboard::Key::Named(Named::F14) => Some("F14".into()),
        keyboard::Key::Named(Named::F15) => Some("F15".into()),
        keyboard::Key::Named(Named::F16) => Some("F16".into()),
        keyboard::Key::Named(Named::F17) => Some("F17".into()),
        keyboard::Key::Named(Named::F18) => Some("F18".into()),
        keyboard::Key::Named(Named::F19) => Some("F19".into()),
        keyboard::Key::Named(Named::F20) => Some("F20".into()),
        keyboard::Key::Named(Named::F21) => Some("F21".into()),
        keyboard::Key::Named(Named::F22) => Some("F22".into()),
        keyboard::Key::Named(Named::F23) => Some("F23".into()),
        keyboard::Key::Named(Named::F24) => Some("F24".into()),
        keyboard::Key::Named(Named::Home) => Some("Home".into()),
        keyboard::Key::Named(Named::Insert) => Some("Insert".into()),
        keyboard::Key::Named(Named::MediaPlayPause) => Some("MediaPlayPause".into()),
        keyboard::Key::Named(Named::MediaStop) => Some("MediaStop".into()),
        keyboard::Key::Named(Named::MediaTrackNext) => Some("MediaTrackNext".into()),
        keyboard::Key::Named(Named::MediaTrackPrevious) => Some("MediaTrackPrevious".into()),
        keyboard::Key::Named(Named::NumLock) => Some("NumLock".into()),
        keyboard::Key::Named(Named::PageDown) => Some("PageDown".into()),
        keyboard::Key::Named(Named::PageUp) => Some("PageUp".into()),
        keyboard::Key::Named(Named::Pause) => Some("Pause".into()),
        keyboard::Key::Named(Named::PrintScreen) => Some("PrintScreen".into()),
        keyboard::Key::Named(Named::ScrollLock) => Some("ScrollLock".into()),
        keyboard::Key::Named(Named::Space) => Some("Space".into()),
        keyboard::Key::Named(Named::Tab) => Some("Tab".into()),
        _ => None,
    }
}

fn binding_key_name_from_code(code: IcedKeyCode) -> Option<String> {
    match code {
        IcedKeyCode::Backquote => Some("Backquote".into()),
        IcedKeyCode::Backslash => Some("Backslash".into()),
        IcedKeyCode::Backspace => Some("Backspace".into()),
        IcedKeyCode::BracketLeft => Some("BracketLeft".into()),
        IcedKeyCode::BracketRight => Some("BracketRight".into()),
        IcedKeyCode::CapsLock => Some("CapsLock".into()),
        IcedKeyCode::Comma => Some("Comma".into()),
        IcedKeyCode::Delete => Some("Delete".into()),
        IcedKeyCode::Digit0 => Some("0".into()),
        IcedKeyCode::Digit1 => Some("1".into()),
        IcedKeyCode::Digit2 => Some("2".into()),
        IcedKeyCode::Digit3 => Some("3".into()),
        IcedKeyCode::Digit4 => Some("4".into()),
        IcedKeyCode::Digit5 => Some("5".into()),
        IcedKeyCode::Digit6 => Some("6".into()),
        IcedKeyCode::Digit7 => Some("7".into()),
        IcedKeyCode::Digit8 => Some("8".into()),
        IcedKeyCode::Digit9 => Some("9".into()),
        IcedKeyCode::End => Some("End".into()),
        IcedKeyCode::Enter => Some("Enter".into()),
        IcedKeyCode::Equal => Some("Equal".into()),
        IcedKeyCode::Escape => Some("Escape".into()),
        IcedKeyCode::F1 => Some("F1".into()),
        IcedKeyCode::F2 => Some("F2".into()),
        IcedKeyCode::F3 => Some("F3".into()),
        IcedKeyCode::F4 => Some("F4".into()),
        IcedKeyCode::F5 => Some("F5".into()),
        IcedKeyCode::F6 => Some("F6".into()),
        IcedKeyCode::F7 => Some("F7".into()),
        IcedKeyCode::F8 => Some("F8".into()),
        IcedKeyCode::F9 => Some("F9".into()),
        IcedKeyCode::F10 => Some("F10".into()),
        IcedKeyCode::F11 => Some("F11".into()),
        IcedKeyCode::F12 => Some("F12".into()),
        IcedKeyCode::F13 => Some("F13".into()),
        IcedKeyCode::F14 => Some("F14".into()),
        IcedKeyCode::F15 => Some("F15".into()),
        IcedKeyCode::F16 => Some("F16".into()),
        IcedKeyCode::F17 => Some("F17".into()),
        IcedKeyCode::F18 => Some("F18".into()),
        IcedKeyCode::F19 => Some("F19".into()),
        IcedKeyCode::F20 => Some("F20".into()),
        IcedKeyCode::F21 => Some("F21".into()),
        IcedKeyCode::F22 => Some("F22".into()),
        IcedKeyCode::F23 => Some("F23".into()),
        IcedKeyCode::F24 => Some("F24".into()),
        IcedKeyCode::Home => Some("Home".into()),
        IcedKeyCode::Insert => Some("Insert".into()),
        IcedKeyCode::KeyA => Some("A".into()),
        IcedKeyCode::KeyB => Some("B".into()),
        IcedKeyCode::KeyC => Some("C".into()),
        IcedKeyCode::KeyD => Some("D".into()),
        IcedKeyCode::KeyE => Some("E".into()),
        IcedKeyCode::KeyF => Some("F".into()),
        IcedKeyCode::KeyG => Some("G".into()),
        IcedKeyCode::KeyH => Some("H".into()),
        IcedKeyCode::KeyI => Some("I".into()),
        IcedKeyCode::KeyJ => Some("J".into()),
        IcedKeyCode::KeyK => Some("K".into()),
        IcedKeyCode::KeyL => Some("L".into()),
        IcedKeyCode::KeyM => Some("M".into()),
        IcedKeyCode::KeyN => Some("N".into()),
        IcedKeyCode::KeyO => Some("O".into()),
        IcedKeyCode::KeyP => Some("P".into()),
        IcedKeyCode::KeyQ => Some("Q".into()),
        IcedKeyCode::KeyR => Some("R".into()),
        IcedKeyCode::KeyS => Some("S".into()),
        IcedKeyCode::KeyT => Some("T".into()),
        IcedKeyCode::KeyU => Some("U".into()),
        IcedKeyCode::KeyV => Some("V".into()),
        IcedKeyCode::KeyW => Some("W".into()),
        IcedKeyCode::KeyX => Some("X".into()),
        IcedKeyCode::KeyY => Some("Y".into()),
        IcedKeyCode::KeyZ => Some("Z".into()),
        IcedKeyCode::MediaPlayPause => Some("MediaPlayPause".into()),
        IcedKeyCode::MediaStop => Some("MediaStop".into()),
        IcedKeyCode::MediaTrackNext => Some("MediaTrackNext".into()),
        IcedKeyCode::MediaTrackPrevious => Some("MediaTrackPrevious".into()),
        IcedKeyCode::Minus => Some("Minus".into()),
        IcedKeyCode::NumLock => Some("NumLock".into()),
        IcedKeyCode::Numpad0 => Some("Num0".into()),
        IcedKeyCode::Numpad1 => Some("Num1".into()),
        IcedKeyCode::Numpad2 => Some("Num2".into()),
        IcedKeyCode::Numpad3 => Some("Num3".into()),
        IcedKeyCode::Numpad4 => Some("Num4".into()),
        IcedKeyCode::Numpad5 => Some("Num5".into()),
        IcedKeyCode::Numpad6 => Some("Num6".into()),
        IcedKeyCode::Numpad7 => Some("Num7".into()),
        IcedKeyCode::Numpad8 => Some("Num8".into()),
        IcedKeyCode::Numpad9 => Some("Num9".into()),
        IcedKeyCode::NumpadAdd => Some("NumAdd".into()),
        IcedKeyCode::NumpadDecimal => Some("NumDecimal".into()),
        IcedKeyCode::NumpadDivide => Some("NumDivide".into()),
        IcedKeyCode::NumpadEnter => Some("NumEnter".into()),
        IcedKeyCode::NumpadEqual => Some("NumEqual".into()),
        IcedKeyCode::NumpadMultiply => Some("NumMultiply".into()),
        IcedKeyCode::NumpadSubtract => Some("NumSubtract".into()),
        IcedKeyCode::PageDown => Some("PageDown".into()),
        IcedKeyCode::PageUp => Some("PageUp".into()),
        IcedKeyCode::Pause => Some("Pause".into()),
        IcedKeyCode::Period => Some("Period".into()),
        IcedKeyCode::PrintScreen => Some("PrintScreen".into()),
        IcedKeyCode::Quote => Some("Quote".into()),
        IcedKeyCode::ScrollLock => Some("ScrollLock".into()),
        IcedKeyCode::Semicolon => Some("Semicolon".into()),
        IcedKeyCode::Slash => Some("Slash".into()),
        IcedKeyCode::Space => Some("Space".into()),
        IcedKeyCode::Tab => Some("Tab".into()),
        IcedKeyCode::ArrowDown => Some("ArrowDown".into()),
        IcedKeyCode::ArrowLeft => Some("ArrowLeft".into()),
        IcedKeyCode::ArrowRight => Some("ArrowRight".into()),
        IcedKeyCode::ArrowUp => Some("ArrowUp".into()),
        IcedKeyCode::AudioVolumeDown => Some("AudioVolumeDown".into()),
        IcedKeyCode::AudioVolumeMute => Some("AudioVolumeMute".into()),
        IcedKeyCode::AudioVolumeUp => Some("AudioVolumeUp".into()),
        _ => None,
    }
}

fn parse_hotkey(binding: &str) -> Result<HotKey, String> {
    HotKey::from_str(binding.trim()).map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
fn windows_hotkey_registration(hotkey: HotKey) -> Result<WindowsHotkeyRegistration, String> {
    if let Some(virtual_key) = windows_virtual_key(hotkey.key) {
        return Ok(WindowsHotkeyRegistration::RegisterHotKey { virtual_key });
    }

    if let Some(spec) = windows_hook_spec(hotkey.key) {
        return Ok(WindowsHotkeyRegistration::LowLevelKeyboardHook { spec });
    }

    Err(format!(
        "failed to register manual clip hotkey: unsupported key {}",
        hotkey.key
    ))
}

#[cfg(target_os = "windows")]
fn register_windows_hotkey(hotkey: HotKey, virtual_key: VIRTUAL_KEY) -> Result<(), String> {
    let modifiers = windows_hotkey_modifiers(hotkey);

    // SAFETY: This is called on the dedicated hotkey worker thread after its message queue
    // has been created, and uses a thread-associated registration with a stable hotkey id.
    tracing::debug!(
        binding = %hotkey,
        hotkey_id = hotkey.id(),
        modifiers = modifiers.0,
        virtual_key = virtual_key.0,
        "registering Windows manual clip hotkey"
    );
    match unsafe {
        RegisterHotKey(
            None,
            hotkey.id() as i32,
            modifiers,
            u32::from(virtual_key.0),
        )
    } {
        Ok(()) => Ok(()),
        Err(error) => {
            let message = if unsafe { GetLastError() } == ERROR_HOTKEY_ALREADY_REGISTERED {
                format!(
                    "failed to register manual clip hotkey: `{hotkey}` is already registered by another application"
                )
            } else {
                format!("failed to register manual clip hotkey: {error}")
            };
            tracing::warn!(
                binding = %hotkey,
                hotkey_id = hotkey.id(),
                modifiers = modifiers.0,
                virtual_key = virtual_key.0,
                error = %message,
                "registering Windows manual clip hotkey failed"
            );
            Err(message)
        }
    }
}

#[cfg(target_os = "windows")]
fn unregister_windows_hotkey(hotkey: HotKey) -> Result<(), String> {
    // SAFETY: The hotkey was registered by the current worker thread using this id.
    unsafe { UnregisterHotKey(None, hotkey.id() as i32) }
        .map_err(|error| format!("failed to unregister manual clip hotkey: {error}"))
}

#[cfg(target_os = "windows")]
fn register_windows_low_level_keyboard_hook(
    hotkey: HotKey,
    spec: WindowsHookSpec,
    sender: mpsc::Sender<HotkeyEvent>,
) -> Result<HHOOK, String> {
    tracing::debug!(
        binding = %hotkey,
        hotkey_id = hotkey.id(),
        scan_code = spec.scan_code,
        extended = spec.extended,
        "installing Windows manual clip low-level keyboard hook"
    );

    {
        let mut state = windows_low_level_hook_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *state = Some(WindowsLowLevelHookState {
            sender,
            hotkey_id: hotkey.id(),
            binding: hotkey.to_string(),
            modifiers: hotkey.mods,
            spec,
            active: false,
        });
    }

    let module = unsafe { GetModuleHandleW(None) }
        .map_err(|error| format!("failed to resolve the NaniteClip module handle: {error}"))?;
    match unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(windows_low_level_keyboard_proc),
            Some(module.into()),
            0,
        )
    } {
        Ok(hook) => Ok(hook),
        Err(error) => {
            clear_windows_low_level_hook_state(hotkey.id());
            Err(format!(
                "failed to install the manual clip low-level keyboard hook: {error}"
            ))
        }
    }
}

#[cfg(target_os = "windows")]
fn unregister_windows_low_level_keyboard_hook(hotkey: HotKey, hook: HHOOK) -> Result<(), String> {
    clear_windows_low_level_hook_state(hotkey.id());
    unsafe { UnhookWindowsHookEx(hook) }.map_err(|error| {
        format!("failed to uninstall the manual clip low-level keyboard hook: {error}")
    })
}

#[cfg(target_os = "windows")]
fn unregister_windows_registration(
    hotkey: HotKey,
    registration: WindowsWorkerRegistration,
) -> Result<(), String> {
    match registration {
        WindowsWorkerRegistration::RegisterHotKey => unregister_windows_hotkey(hotkey),
        WindowsWorkerRegistration::LowLevelKeyboardHook { hook } => {
            unregister_windows_low_level_keyboard_hook(hotkey, hook)
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_hotkey_modifiers(hotkey: HotKey) -> HOT_KEY_MODIFIERS {
    let mut modifiers = MOD_NOREPEAT;
    if hotkey.mods.shift() {
        modifiers |= MOD_SHIFT;
    }
    if hotkey.mods.contains(HotKeyModifiers::SUPER) || hotkey.mods.meta() {
        modifiers |= MOD_WIN;
    }
    if hotkey.mods.alt() {
        modifiers |= MOD_ALT;
    }
    if hotkey.mods.ctrl() {
        modifiers |= MOD_CONTROL;
    }
    modifiers
}

#[cfg(target_os = "windows")]
fn windows_hook_spec(key: HotKeyCode) -> Option<WindowsHookSpec> {
    Some(match key {
        HotKeyCode::NumpadEnter => WindowsHookSpec {
            scan_code: 0x1c,
            extended: true,
        },
        HotKeyCode::NumpadEqual => WindowsHookSpec {
            scan_code: 0x59,
            extended: false,
        },
        _ => return None,
    })
}

#[cfg(target_os = "windows")]
fn windows_low_level_hook_state() -> &'static Mutex<Option<WindowsLowLevelHookState>> {
    WINDOWS_LOW_LEVEL_HOOK_STATE.get_or_init(|| Mutex::new(None))
}

#[cfg(target_os = "windows")]
fn clear_windows_low_level_hook_state(hotkey_id: u32) {
    let mut state = windows_low_level_hook_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if state
        .as_ref()
        .is_some_and(|state| state.hotkey_id == hotkey_id)
    {
        *state = None;
    }
}

#[cfg(target_os = "windows")]
fn windows_hotkey_requires_win(modifiers: HotKeyModifiers) -> bool {
    modifiers.contains(HotKeyModifiers::SUPER) || modifiers.meta()
}

#[cfg(target_os = "windows")]
fn windows_key_is_down(key: VIRTUAL_KEY) -> bool {
    (unsafe { GetAsyncKeyState(i32::from(key.0)) }) < 0
}

#[cfg(target_os = "windows")]
fn windows_hotkey_modifiers_match(modifiers: HotKeyModifiers) -> bool {
    let shift_down = windows_key_is_down(VK_SHIFT);
    let control_down = windows_key_is_down(VK_CONTROL);
    let alt_down = windows_key_is_down(VK_MENU);
    let win_down = windows_key_is_down(VK_LWIN) || windows_key_is_down(VK_RWIN);

    shift_down == modifiers.shift()
        && control_down == modifiers.ctrl()
        && alt_down == modifiers.alt()
        && win_down == windows_hotkey_requires_win(modifiers)
}

#[cfg(target_os = "windows")]
fn windows_low_level_event_matches_spec(event: &KBDLLHOOKSTRUCT, spec: WindowsHookSpec) -> bool {
    event.scanCode == spec.scan_code && event.flags.contains(LLKHF_EXTENDED) == spec.extended
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn windows_low_level_keyboard_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code != HC_ACTION as i32 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let message = wparam.0 as u32;
    let is_keydown = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
    let is_keyup = matches!(message, WM_KEYUP | WM_SYSKEYUP);
    if !is_keydown && !is_keyup {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let event = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
    let mut state = windows_low_level_hook_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(state) = state.as_mut() else {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    };
    if !windows_low_level_event_matches_spec(event, state.spec) {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    if is_keyup {
        state.active = false;
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    if windows_hotkey_modifiers_match(state.modifiers) {
        if !state.active {
            state.active = true;
            tracing::debug!(
                binding = %state.binding,
                hotkey_id = state.hotkey_id,
                scan_code = event.scanCode,
                extended = event.flags.contains(LLKHF_EXTENDED),
                "manual clip hotkey received low-level keyboard activation"
            );
            let _ = state.sender.send(HotkeyEvent::Activated);
        }
    } else {
        state.active = false;
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

#[cfg(target_os = "windows")]
fn windows_virtual_key(key: HotKeyCode) -> Option<VIRTUAL_KEY> {
    Some(match key {
        HotKeyCode::KeyA => VK_A,
        HotKeyCode::KeyB => VK_B,
        HotKeyCode::KeyC => VK_C,
        HotKeyCode::KeyD => VK_D,
        HotKeyCode::KeyE => VK_E,
        HotKeyCode::KeyF => VK_F,
        HotKeyCode::KeyG => VK_G,
        HotKeyCode::KeyH => VK_H,
        HotKeyCode::KeyI => VK_I,
        HotKeyCode::KeyJ => VK_J,
        HotKeyCode::KeyK => VK_K,
        HotKeyCode::KeyL => VK_L,
        HotKeyCode::KeyM => VK_M,
        HotKeyCode::KeyN => VK_N,
        HotKeyCode::KeyO => VK_O,
        HotKeyCode::KeyP => VK_P,
        HotKeyCode::KeyQ => VK_Q,
        HotKeyCode::KeyR => VK_R,
        HotKeyCode::KeyS => VK_S,
        HotKeyCode::KeyT => VK_T,
        HotKeyCode::KeyU => VK_U,
        HotKeyCode::KeyV => VK_V,
        HotKeyCode::KeyW => VK_W,
        HotKeyCode::KeyX => VK_X,
        HotKeyCode::KeyY => VK_Y,
        HotKeyCode::KeyZ => VK_Z,
        HotKeyCode::Digit0 => VK_0,
        HotKeyCode::Digit1 => VK_1,
        HotKeyCode::Digit2 => VK_2,
        HotKeyCode::Digit3 => VK_3,
        HotKeyCode::Digit4 => VK_4,
        HotKeyCode::Digit5 => VK_5,
        HotKeyCode::Digit6 => VK_6,
        HotKeyCode::Digit7 => VK_7,
        HotKeyCode::Digit8 => VK_8,
        HotKeyCode::Digit9 => VK_9,
        HotKeyCode::Equal => VK_OEM_PLUS,
        HotKeyCode::Comma => VK_OEM_COMMA,
        HotKeyCode::Minus => VK_OEM_MINUS,
        HotKeyCode::Period => VK_OEM_PERIOD,
        HotKeyCode::Semicolon => VK_OEM_1,
        HotKeyCode::Slash => VK_OEM_2,
        HotKeyCode::Backquote => VK_OEM_3,
        HotKeyCode::BracketLeft => VK_OEM_4,
        HotKeyCode::Backslash => VK_OEM_5,
        HotKeyCode::BracketRight => VK_OEM_6,
        HotKeyCode::Quote => VK_OEM_7,
        HotKeyCode::Backspace => VK_BACK,
        HotKeyCode::Tab => VK_TAB,
        HotKeyCode::Space => VK_SPACE,
        HotKeyCode::Enter => VK_RETURN,
        HotKeyCode::CapsLock => VK_CAPITAL,
        HotKeyCode::Escape => VK_ESCAPE,
        HotKeyCode::PageUp => VK_PRIOR,
        HotKeyCode::PageDown => VK_NEXT,
        HotKeyCode::End => VK_END,
        HotKeyCode::Home => VK_HOME,
        HotKeyCode::ArrowLeft => VK_LEFT,
        HotKeyCode::ArrowUp => VK_UP,
        HotKeyCode::ArrowRight => VK_RIGHT,
        HotKeyCode::ArrowDown => VK_DOWN,
        HotKeyCode::PrintScreen => VK_SNAPSHOT,
        HotKeyCode::Insert => VK_INSERT,
        HotKeyCode::Delete => VK_DELETE,
        HotKeyCode::F1 => VK_F1,
        HotKeyCode::F2 => VK_F2,
        HotKeyCode::F3 => VK_F3,
        HotKeyCode::F4 => VK_F4,
        HotKeyCode::F5 => VK_F5,
        HotKeyCode::F6 => VK_F6,
        HotKeyCode::F7 => VK_F7,
        HotKeyCode::F8 => VK_F8,
        HotKeyCode::F9 => VK_F9,
        HotKeyCode::F10 => VK_F10,
        HotKeyCode::F11 => VK_F11,
        HotKeyCode::F12 => VK_F12,
        HotKeyCode::F13 => VK_F13,
        HotKeyCode::F14 => VK_F14,
        HotKeyCode::F15 => VK_F15,
        HotKeyCode::F16 => VK_F16,
        HotKeyCode::F17 => VK_F17,
        HotKeyCode::F18 => VK_F18,
        HotKeyCode::F19 => VK_F19,
        HotKeyCode::F20 => VK_F20,
        HotKeyCode::F21 => VK_F21,
        HotKeyCode::F22 => VK_F22,
        HotKeyCode::F23 => VK_F23,
        HotKeyCode::F24 => VK_F24,
        HotKeyCode::NumLock => VK_NUMLOCK,
        HotKeyCode::Numpad0 => VK_NUMPAD0,
        HotKeyCode::Numpad1 => VK_NUMPAD1,
        HotKeyCode::Numpad2 => VK_NUMPAD2,
        HotKeyCode::Numpad3 => VK_NUMPAD3,
        HotKeyCode::Numpad4 => VK_NUMPAD4,
        HotKeyCode::Numpad5 => VK_NUMPAD5,
        HotKeyCode::Numpad6 => VK_NUMPAD6,
        HotKeyCode::Numpad7 => VK_NUMPAD7,
        HotKeyCode::Numpad8 => VK_NUMPAD8,
        HotKeyCode::Numpad9 => VK_NUMPAD9,
        HotKeyCode::NumpadAdd => VK_ADD,
        HotKeyCode::NumpadDecimal => VK_DECIMAL,
        HotKeyCode::NumpadDivide => VK_DIVIDE,
        HotKeyCode::NumpadEnter => return None,
        HotKeyCode::NumpadEqual => return None,
        HotKeyCode::NumpadMultiply => VK_MULTIPLY,
        HotKeyCode::NumpadSubtract => VK_SUBTRACT,
        HotKeyCode::ScrollLock => VK_SCROLL,
        HotKeyCode::AudioVolumeDown => VK_VOLUME_DOWN,
        HotKeyCode::AudioVolumeUp => VK_VOLUME_UP,
        HotKeyCode::AudioVolumeMute => VK_VOLUME_MUTE,
        HotKeyCode::MediaPlay => VK_PLAY,
        HotKeyCode::MediaPause => VK_PAUSE,
        HotKeyCode::MediaPlayPause => VK_MEDIA_PLAY_PAUSE,
        HotKeyCode::MediaStop => VK_MEDIA_STOP,
        HotKeyCode::MediaTrackNext => VK_MEDIA_NEXT_TRACK,
        HotKeyCode::MediaTrackPrevious => VK_MEDIA_PREV_TRACK,
        HotKeyCode::Pause => VK_PAUSE,
        _ => return None,
    })
}

#[cfg(target_os = "linux")]
fn portal_preferred_trigger(binding: &str) -> Option<String> {
    let mut output = Vec::new();
    let mut key = None;

    for token in binding
        .split('+')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        match token.to_ascii_uppercase().as_str() {
            "CTRL" | "CONTROL" => output.push("CTRL".to_string()),
            "ALT" | "OPTION" => output.push("ALT".to_string()),
            "SHIFT" => output.push("SHIFT".to_string()),
            "SUPER" | "CMD" | "COMMAND" | "LOGO" => output.push("LOGO".to_string()),
            _ => key = normalize_portal_key(token),
        }
    }

    key.map(|key| {
        if output.is_empty() {
            key
        } else {
            format!("{}+{key}", output.join("+"))
        }
    })
}

#[cfg(target_os = "linux")]
fn normalize_portal_key(token: &str) -> Option<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }

    let normalized = match trimmed.to_ascii_uppercase().as_str() {
        "UP" | "ARROWUP" => "Up".into(),
        "DOWN" | "ARROWDOWN" => "Down".into(),
        "LEFT" | "ARROWLEFT" => "Left".into(),
        "RIGHT" | "ARROWRIGHT" => "Right".into(),
        "ENTER" | "RETURN" => "Return".into(),
        "NUMPADENTER" | "NUMENTER" => "KP_Enter".into(),
        "ESC" | "ESCAPE" => "Escape".into(),
        "SPACE" | "SPACEBAR" => "space".into(),
        "TAB" => "Tab".into(),
        "BACKSPACE" => "BackSpace".into(),
        "DELETE" | "DEL" => "Delete".into(),
        "HOME" => "Home".into(),
        "END" => "End".into(),
        "PAGEUP" => "Page_Up".into(),
        "PAGEDOWN" => "Page_Down".into(),
        "INSERT" => "Insert".into(),
        "NUMPAD0" | "NUM0" => "KP_0".into(),
        "NUMPAD1" | "NUM1" => "KP_1".into(),
        "NUMPAD2" | "NUM2" => "KP_2".into(),
        "NUMPAD3" | "NUM3" => "KP_3".into(),
        "NUMPAD4" | "NUM4" => "KP_4".into(),
        "NUMPAD5" | "NUM5" => "KP_5".into(),
        "NUMPAD6" | "NUM6" => "KP_6".into(),
        "NUMPAD7" | "NUM7" => "KP_7".into(),
        "NUMPAD8" | "NUM8" => "KP_8".into(),
        "NUMPAD9" | "NUM9" => "KP_9".into(),
        "NUMPADADD" | "NUMADD" | "NUMPADPLUS" | "NUMPLUS" => "KP_Add".into(),
        "NUMPADDECIMAL" | "NUMDECIMAL" => "KP_Decimal".into(),
        "NUMPADDIVIDE" | "NUMDIVIDE" => "KP_Divide".into(),
        "NUMPADEQUAL" | "NUMEQUAL" => "KP_Equal".into(),
        "NUMPADMULTIPLY" | "NUMMULTIPLY" => "KP_Multiply".into(),
        "NUMPADSUBTRACT" | "NUMSUBTRACT" => "KP_Subtract".into(),
        other
            if other.starts_with('F')
                && other[1..]
                    .chars()
                    .all(|character| character.is_ascii_digit()) =>
        {
            other.into()
        }
        other if other.len() == 1 => other.into(),
        _ => trimmed.into(),
    };

    Some(normalized)
}

#[cfg(target_os = "linux")]
async fn clear_plasma_portal_shortcut_conflict() -> Result<(), String> {
    let connection = DBusConnection::session()
        .await
        .map_err(|error| format!("failed to connect to the KDE session bus: {error}"))?;
    let root_proxy = DBusProxy::new(
        &connection,
        KDE_KGLOBALACCEL_SERVICE,
        KDE_KGLOBALACCEL_PATH,
        KDE_KGLOBALACCEL_INTERFACE,
    )
    .await
    .map_err(|error| format!("failed to open the KDE global shortcut interface: {error}"))?;

    let removed: bool = root_proxy
        .call(
            "unregister",
            &(KDE_PORTAL_COMPONENT_UNIQUE, MANUAL_CLIP_SHORTCUT_ID),
        )
        .await
        .map_err(|error| {
            format!("failed to clear the previous Plasma portal shortcut entry: {error}")
        })?;
    if removed {
        tracing::info!(
            "removed the previous Plasma portal shortcut entry for `{MANUAL_CLIP_SHORTCUT_ID}`"
        );
    }

    Ok(())
}

#[allow(dead_code)]
#[cfg(test)]
fn env_flag_enabled(value: Option<String>) -> bool {
    matches!(
        value.as_deref().map(str::trim),
        Some("1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn portal_preferred_trigger_normalizes_common_bindings() {
        assert_eq!(
            portal_preferred_trigger("Ctrl+Shift+F8"),
            Some("CTRL+SHIFT+F8".into())
        );
        assert_eq!(
            portal_preferred_trigger("Alt+ArrowRight"),
            Some("ALT+Right".into())
        );
        assert_eq!(
            portal_preferred_trigger("NumEnter"),
            Some("KP_Enter".into())
        );
        assert_eq!(
            portal_preferred_trigger("Ctrl+Num5"),
            Some("CTRL+KP_5".into())
        );
    }

    #[test]
    fn parse_hotkey_accepts_manual_clip_default() {
        let hotkey = parse_hotkey("Ctrl+Shift+F8").unwrap();
        assert_eq!(hotkey.to_string(), "shift+control+F8");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_virtual_key_accepts_manual_clip_default() {
        let hotkey = parse_hotkey("Ctrl+Shift+F8").unwrap();
        assert_eq!(windows_virtual_key(hotkey.key), Some(VK_F8));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_hotkey_modifiers_include_control_shift_and_norepeat() {
        let hotkey = parse_hotkey("Ctrl+Shift+F8").unwrap();
        let modifiers = windows_hotkey_modifiers(hotkey);
        assert!(modifiers.contains(MOD_CONTROL));
        assert!(modifiers.contains(MOD_SHIFT));
        assert!(modifiers.contains(MOD_NOREPEAT));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_virtual_key_rejects_numpad_enter() {
        let hotkey = parse_hotkey("NumEnter").unwrap();
        assert_eq!(windows_virtual_key(hotkey.key), None);
        assert_eq!(
            windows_hook_spec(hotkey.key),
            Some(WindowsHookSpec {
                scan_code: 0x1c,
                extended: true
            })
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_hook_spec_supports_numpad_equal() {
        let hotkey = parse_hotkey("NumEqual").unwrap();
        assert_eq!(windows_virtual_key(hotkey.key), None);
        assert_eq!(
            windows_hook_spec(hotkey.key),
            Some(WindowsHookSpec {
                scan_code: 0x59,
                extended: false
            })
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn capture_binding_accepts_numpad_enter_on_windows() {
        let event = keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(Named::Enter),
            modified_key: keyboard::Key::Named(Named::Enter),
            physical_key: Physical::Code(IcedKeyCode::NumpadEnter),
            location: keyboard::Location::Numpad,
            modifiers: keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        };

        assert_eq!(
            capture_binding(&event),
            BindingCapture::Captured("NumEnter".into())
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn kde_shortcut_sequence_encodes_num_enter() {
        assert_eq!(
            kde_shortcut_sequence("NumEnter").unwrap(),
            (vec![0x2100_0005],)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn kde_shortcut_round_trips_num_enter() {
        let encoded = kde_shortcut_sequence("NumEnter").unwrap();
        assert_eq!(
            kde_shortcut_from_sequence(&encoded.0),
            Some("NumEnter".into())
        );
    }

    #[test]
    fn env_flag_enabled_accepts_common_truthy_values() {
        assert!(env_flag_enabled(Some("1".into())));
        assert!(env_flag_enabled(Some("true".into())));
        assert!(env_flag_enabled(Some("YES".into())));
        assert!(env_flag_enabled(Some("on".into())));
    }

    #[test]
    fn env_flag_enabled_rejects_unset_and_falsey_values() {
        assert!(!env_flag_enabled(None));
        assert!(!env_flag_enabled(Some(String::new())));
        assert!(!env_flag_enabled(Some("0".into())));
        assert!(!env_flag_enabled(Some("false".into())));
        assert!(!env_flag_enabled(Some("off".into())));
    }

    #[test]
    fn capture_binding_formats_modifier_combination() {
        let event = keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(Named::F8),
            modified_key: keyboard::Key::Named(Named::F8),
            physical_key: Physical::Code(IcedKeyCode::F8),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::CTRL | keyboard::Modifiers::SHIFT,
            text: None,
            repeat: false,
        };

        assert_eq!(
            capture_binding(&event),
            BindingCapture::Captured("Ctrl+Shift+F8".into())
        );
    }

    #[test]
    fn capture_binding_ignores_modifier_only_press() {
        let event = keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(Named::Control),
            modified_key: keyboard::Key::Named(Named::Control),
            physical_key: Physical::Code(IcedKeyCode::ControlLeft),
            location: keyboard::Location::Left,
            modifiers: keyboard::Modifiers::CTRL,
            text: None,
            repeat: false,
        };

        assert_eq!(capture_binding(&event), BindingCapture::Ignored);
    }
}
