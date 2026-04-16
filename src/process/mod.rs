use std::sync::Arc;

mod env;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

pub use env::{
    DesktopEnvironment, DisplayServer, detect_desktop_environment, detect_display_server,
};
#[cfg(target_os = "linux")]
pub use linux::LinuxProcfsWatcher;
#[cfg(target_os = "windows")]
pub use windows::WindowsToolhelpWatcher;

pub trait GameProcessWatcher: Send + Sync {
    fn find_running_pid(&self) -> Option<u32>;
    fn is_running(&self, pid: u32) -> bool;
    fn resolve_capture_target(
        &self,
        pid: u32,
        configured_source: &str,
    ) -> Result<CaptureSourcePlan, CaptureTargetError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureSourcePlan {
    pub target: CaptureTarget,
    pub backend_hints: BackendHints,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureTarget {
    X11Window(u32),
    WaylandPortal,
    Monitor(String),
    BackendOwned,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BackendHints {
    pub display_server: Option<DisplayServer>,
    pub restore_portal_session: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureTargetError {
    #[error("PlanetSide 2 window is not available on X11 yet")]
    WindowNotFound,
    #[error("failed to connect to X11: {0}")]
    X11Connect(String),
    #[error("failed to query X11 window metadata: {0}")]
    X11Query(String),
    #[error("X11 connection did not expose a screen")]
    NoX11Screen,
    #[error("{0}")]
    Unsupported(String),
}

#[derive(Debug, Default)]
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub struct UnsupportedGameProcessWatcher;

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
impl UnsupportedGameProcessWatcher {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
impl GameProcessWatcher for UnsupportedGameProcessWatcher {
    fn find_running_pid(&self) -> Option<u32> {
        None
    }

    fn is_running(&self, _pid: u32) -> bool {
        false
    }

    fn resolve_capture_target(
        &self,
        _pid: u32,
        _configured_source: &str,
    ) -> Result<CaptureSourcePlan, CaptureTargetError> {
        Err(CaptureTargetError::Unsupported(
            "capture target resolution is not implemented for this platform yet".into(),
        ))
    }
}

#[cfg(target_os = "linux")]
pub fn default_game_process_watcher() -> Arc<dyn GameProcessWatcher> {
    Arc::new(LinuxProcfsWatcher::new())
}

#[cfg(target_os = "windows")]
pub fn default_game_process_watcher() -> Arc<dyn GameProcessWatcher> {
    Arc::new(WindowsToolhelpWatcher::new())
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn default_game_process_watcher() -> Arc<dyn GameProcessWatcher> {
    Arc::new(UnsupportedGameProcessWatcher::new())
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn resolve_capture_source(
    _configured_source: &str,
    _ps2_pid: u32,
) -> Result<CaptureSourcePlan, CaptureTargetError> {
    Err(CaptureTargetError::Unsupported(
        "capture target resolution is not implemented for this platform yet".into(),
    ))
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn find_ps2_pid() -> Option<u32> {
    None
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn is_process_running(_pid: u32) -> bool {
    false
}
