use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
#[cfg(test)]
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::config::{AudioSourceConfig, AudioSourceKind, RecorderConfig};
use crate::process::CaptureSourcePlan;
use crate::recorder::VideoResolution;
use crate::rules::ClipLength;

#[cfg(target_os = "linux")]
pub mod gsr;
pub mod obs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveredAudioKind {
    Device,
    Application,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredAudioSource {
    pub kind_hint: AudioSourceKind,
    pub display_label: String,
    pub kind: DiscoveredAudioKind,
    pub available: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CaptureCapabilities {
    pub per_app_audio: bool,
    pub application_inverse: bool,
    pub merged_tracks: bool,
    pub portal_session_restore: bool,
    pub replay_buffer: bool,
    pub hdr: bool,
    pub cursor_capture: bool,
}

#[cfg_attr(target_os = "windows", allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryHint {
    None,
    ReacquireCaptureTarget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedAudioSource {
    pub config: AudioSourceConfig,
    pub resolved_display: String,
}

#[derive(Debug, Clone)]
pub struct CaptureRequest {
    pub capture: CaptureSourcePlan,
    pub recorder: RecorderConfig,
}

#[derive(Debug, Clone)]
pub enum SavePollResult {
    Saved {
        path: PathBuf,
        duration: ClipLength,
        audio_layout: Vec<ResolvedAudioSource>,
    },
    SaveFailed(String),
    BackendEvent(BackendRuntimeEvent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendRuntimeEvent {
    ObsConnection(ObsConnectionStatus),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObsConnectionStatus {
    Connected,
    Reconnecting {
        attempt: u32,
        next_retry_in_secs: u64,
    },
    Failed {
        reason: String,
    },
}

#[allow(dead_code)]
pub trait CaptureBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn capabilities(&self) -> CaptureCapabilities;
    fn discover_audio_sources(
        &self,
    ) -> Pin<
        Box<dyn Future<Output = Result<Vec<DiscoveredAudioSource>, AudioSourceError>> + Send + '_>,
    >;
    fn validate_audio_source(&self, kind: &AudioSourceKind) -> Result<(), AudioSourceError>;
    fn spawn_replay(
        &self,
        request: CaptureRequest,
    ) -> Result<Box<dyn CaptureSession>, CaptureError>;
    fn should_probe_saved_clip_resolution(&self, _capture: &CaptureSourcePlan) -> bool {
        false
    }
    fn post_save_recovery_hint(
        &self,
        _capture: &CaptureSourcePlan,
        _video_resolution: Option<VideoResolution>,
    ) -> RecoveryHint {
        RecoveryHint::None
    }
}

pub trait CaptureSession: Send {
    fn save_clip(&mut self, length: ClipLength) -> Result<(), CaptureError>;
    fn poll_results(&mut self) -> Vec<SavePollResult>;
    fn stop(&mut self) -> Result<(), CaptureError>;
    #[allow(dead_code)]
    fn active_audio_layout(&self) -> &[ResolvedAudioSource];
    fn is_running(&mut self) -> bool;
    fn save_in_progress(&self) -> bool;
}

#[cfg_attr(target_os = "windows", allow(dead_code))]
#[derive(Debug, thiserror::Error)]
pub enum AudioSourceError {
    #[error("gpu-screen-recorder is not installed or not available in PATH")]
    RecorderNotFound,
    #[error("audio source discovery failed: {0}")]
    DiscoveryFailed(String),
    #[error("audio source discovery task failed: {0}")]
    TaskFailed(String),
    #[error("per-application audio discovery is unavailable: {reason}")]
    PerAppUnavailable {
        reason: String,
        partial: Vec<DiscoveredAudioSource>,
    },
    #[error("audio source is not supported by this backend: {capability}")]
    Unsupported { capability: String },
    #[error("raw audio source targets backend `{actual}` but active backend is `{expected}`")]
    WrongBackend { expected: String, actual: String },
}

#[cfg_attr(target_os = "windows", allow(dead_code))]
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("recorder is already running")]
    AlreadyRunning,
    #[error("recorder is not running")]
    NotRunning,
    #[error("a replay save is already in flight")]
    SaveInFlight,
    #[error("failed to spawn capture backend: {0}")]
    SpawnFailed(String),
    #[error("failed to send signal: {0}")]
    SignalFailed(String),
    #[error("failed to configure audio source: {0}")]
    AudioSource(#[from] AudioSourceError),
    #[error("capture backend does not support: {capability}")]
    Unsupported { capability: String },
    #[error("I/O error: {0}")]
    Io(String),
}

#[cfg(test)]
#[derive(Debug)]
pub struct MockBackend {
    pub capabilities: CaptureCapabilities,
    pub spawn_calls: Mutex<Vec<CaptureRequest>>,
    pub queued_results: Mutex<Vec<SavePollResult>>,
    pub force_spawn_error: Mutex<Option<CaptureError>>,
    pub shared_state: Arc<Mutex<MockSessionState>>,
}

#[cfg(test)]
impl Default for MockBackend {
    fn default() -> Self {
        Self {
            capabilities: CaptureCapabilities::default(),
            spawn_calls: Mutex::new(Vec::new()),
            queued_results: Mutex::new(Vec::new()),
            force_spawn_error: Mutex::new(None),
            shared_state: Arc::new(Mutex::new(MockSessionState::default())),
        }
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
pub struct MockSessionState {
    pub is_running: bool,
    pub save_in_flight: bool,
    pub save_calls: Vec<ClipLength>,
}

#[cfg(test)]
#[derive(Debug)]
pub struct MockSession {
    shared_state: Arc<Mutex<MockSessionState>>,
    pending_results: Vec<SavePollResult>,
    active_audio_layout: Vec<ResolvedAudioSource>,
}

#[cfg(test)]
impl MockBackend {
    pub fn new() -> Self {
        Self {
            shared_state: Arc::new(Mutex::new(MockSessionState {
                is_running: true,
                ..MockSessionState::default()
            })),
            ..Self::default()
        }
    }
}

#[cfg(test)]
impl CaptureBackend for MockBackend {
    fn id(&self) -> &'static str {
        "mock"
    }

    fn display_name(&self) -> &'static str {
        "Mock Backend"
    }

    fn capabilities(&self) -> CaptureCapabilities {
        self.capabilities
    }

    fn discover_audio_sources(
        &self,
    ) -> Pin<
        Box<dyn Future<Output = Result<Vec<DiscoveredAudioSource>, AudioSourceError>> + Send + '_>,
    > {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn validate_audio_source(&self, _kind: &AudioSourceKind) -> Result<(), AudioSourceError> {
        Ok(())
    }

    fn spawn_replay(
        &self,
        request: CaptureRequest,
    ) -> Result<Box<dyn CaptureSession>, CaptureError> {
        self.spawn_calls
            .lock()
            .expect("mock backend spawn_calls mutex poisoned")
            .push(request.clone());

        if let Some(error) = self
            .force_spawn_error
            .lock()
            .expect("mock backend force_spawn_error mutex poisoned")
            .take()
        {
            return Err(error);
        }

        let state = self.shared_state.clone();
        *state
            .lock()
            .expect("mock backend shared_state mutex poisoned") = MockSessionState {
            is_running: true,
            save_in_flight: false,
            save_calls: Vec::new(),
        };

        Ok(Box::new(MockSession {
            shared_state: state,
            pending_results: self
                .queued_results
                .lock()
                .expect("mock backend queued_results mutex poisoned")
                .drain(..)
                .collect(),
            active_audio_layout: Vec::new(),
        }))
    }
}

#[cfg(test)]
impl CaptureSession for MockSession {
    fn save_clip(&mut self, length: ClipLength) -> Result<(), CaptureError> {
        let mut state = self
            .shared_state
            .lock()
            .expect("mock session shared_state mutex poisoned");
        if state.save_in_flight {
            return Err(CaptureError::SaveInFlight);
        }
        state.save_in_flight = true;
        state.save_calls.push(length);
        Ok(())
    }

    fn poll_results(&mut self) -> Vec<SavePollResult> {
        let mut state = self
            .shared_state
            .lock()
            .expect("mock session shared_state mutex poisoned");
        if self.pending_results.is_empty() {
            return Vec::new();
        }

        state.save_in_flight = false;
        std::mem::take(&mut self.pending_results)
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        self.shared_state
            .lock()
            .expect("mock session shared_state mutex poisoned")
            .is_running = false;
        Ok(())
    }

    fn active_audio_layout(&self) -> &[ResolvedAudioSource] {
        &self.active_audio_layout
    }

    fn is_running(&mut self) -> bool {
        self.shared_state
            .lock()
            .expect("mock session shared_state mutex poisoned")
            .is_running
    }

    fn save_in_progress(&self) -> bool {
        self.shared_state
            .lock()
            .expect("mock session shared_state mutex poisoned")
            .save_in_flight
    }
}
