//! App-facing platform integration boundary.
//!
//! Supported product targets today are Linux and Windows. macOS/open-ended
//! `cfg` fallbacks are compile-safe only and should be treated as unsupported
//! until the corresponding tray, hotkey, notification, and autostart services
//! report real capabilities here.

use std::path::Path;

use crate::autostart;
use crate::command_runner;
use crate::config::{LaunchAtLoginConfig, ManualClipConfig};
use crate::hotkey::HotkeyManager;
use crate::launcher;
use crate::notifications::NotificationCenter;
use crate::process::{DesktopEnvironment, DisplayServer};
use crate::secure_store::SecureStore;
use crate::tray::{TrayController, TraySnapshot};

#[cfg(target_os = "linux")]
#[path = "../platform_service.rs"]
mod hotkey_sidecar;
#[cfg(target_os = "linux")]
pub(crate) use hotkey_sidecar::{
    PlatformHotkeyEvent, PlatformHotkeyServiceHandle, start_plasma_manual_clip_hotkey,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformCapabilities {
    pub notifications: bool,
    pub tray: bool,
    pub global_hotkeys: bool,
    pub launch_at_login: bool,
    pub opener: bool,
    pub secret_store: bool,
}

impl PlatformCapabilities {
    pub fn current() -> Self {
        let notifications = if cfg!(target_os = "linux") {
            command_runner::command_available("gsr-notify")
        } else {
            cfg!(target_os = "windows")
        };
        let tray = cfg!(any(target_os = "linux", target_os = "windows"));
        let global_hotkeys = cfg!(any(target_os = "linux", target_os = "windows"));
        let launch_at_login = cfg!(any(target_os = "linux", target_os = "windows"));
        let opener = cfg!(any(
            target_os = "linux",
            target_os = "macos",
            target_os = "windows"
        ));
        let secret_store = cfg!(target_os = "windows")
            || command_runner::command_available("secret-tool")
            || SecureStore::new().backend() == crate::secure_store::SecureStoreBackend::LocalFile;

        Self {
            notifications,
            tray,
            global_hotkeys,
            launch_at_login,
            opener,
            secret_store,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlatformServices {
    capabilities: PlatformCapabilities,
    secure_store: SecureStore,
}

impl PlatformServices {
    pub fn new() -> Self {
        Self {
            capabilities: PlatformCapabilities::current(),
            secure_store: SecureStore::new(),
        }
    }

    pub fn capabilities(&self) -> PlatformCapabilities {
        self.capabilities
    }

    pub fn secure_store(&self) -> &SecureStore {
        &self.secure_store
    }

    pub fn create_notification_center(&self) -> NotificationCenter {
        NotificationCenter::new()
    }

    pub fn spawn_tray(&self, snapshot: TraySnapshot) -> Result<TrayController, String> {
        TrayController::spawn(snapshot)
    }

    pub fn disabled_hotkeys(&self) -> HotkeyManager {
        HotkeyManager::disabled()
    }

    #[cfg(not(target_os = "windows"))]
    pub async fn configure_hotkeys(
        &self,
        config: &ManualClipConfig,
        display_server: DisplayServer,
        desktop_environment: DesktopEnvironment,
    ) -> Result<HotkeyManager, String> {
        HotkeyManager::configure(config, display_server, desktop_environment).await
    }

    #[cfg(target_os = "windows")]
    pub fn configure_hotkeys_sync(
        &self,
        config: &ManualClipConfig,
        display_server: DisplayServer,
        desktop_environment: DesktopEnvironment,
    ) -> Result<HotkeyManager, String> {
        HotkeyManager::configure_sync(config, display_server, desktop_environment)
    }

    pub fn sync_launch_at_login(&self, config: &LaunchAtLoginConfig) -> Result<(), String> {
        autostart::sync_launch_at_login(config)
    }

    pub fn open_path(&self, path: &Path) -> Result<(), String> {
        launcher::open_path(path)
    }

    pub fn open_url(&self, url: &str) -> Result<(), String> {
        launcher::open_url(url)
    }

    pub fn launch_command(
        &self,
        program: &str,
        args: &[String],
        display: &str,
    ) -> Result<(), String> {
        launcher::launch_command(program, args, display)
    }
}

#[cfg(test)]
mod tests {
    use super::PlatformCapabilities;

    #[test]
    fn current_capabilities_expose_supported_integrations() {
        let capabilities = PlatformCapabilities::current();
        assert_eq!(
            capabilities.tray,
            cfg!(any(target_os = "linux", target_os = "windows"))
        );
        assert_eq!(
            capabilities.global_hotkeys,
            cfg!(any(target_os = "linux", target_os = "windows"))
        );
        assert_eq!(
            capabilities.launch_at_login,
            cfg!(any(target_os = "linux", target_os = "windows"))
        );
    }
}
