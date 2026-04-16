use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

#[cfg(target_os = "windows")]
use crate::capture::obs::ObsBackend;
use crate::capture::{
    AudioSourceError, CaptureBackend, CaptureCapabilities, CaptureError, CaptureRequest,
    CaptureSession, DiscoveredAudioSource, RecoveryHint, ResolvedAudioSource,
};
use crate::config::{CaptureConfig, RecorderConfig};
use crate::process::CaptureSourcePlan;
use crate::rules::ClipLength;
#[cfg(target_os = "linux")]
use crate::{capture::gsr::GsrBackend, capture::obs::ObsBackend};

pub struct Recorder {
    backend: Arc<dyn CaptureBackend>,
    capture: CaptureConfig,
    config: RecorderConfig,
    session: Option<Box<dyn CaptureSession>>,
    active_capture: Option<CaptureSourcePlan>,
    #[allow(dead_code)]
    ffmpeg_available: bool,
}

pub use crate::capture::SavePollResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoResolution {
    pub width: u32,
    pub height: u32,
}

impl Recorder {
    pub fn new(capture: CaptureConfig, config: RecorderConfig) -> Self {
        Self {
            backend: create_backend(&capture, &config),
            capture,
            config,
            session: None,
            active_capture: None,
            ffmpeg_available: ffmpeg_available(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_backend(
        capture: CaptureConfig,
        config: RecorderConfig,
        backend: Arc<dyn CaptureBackend>,
    ) -> Self {
        Self {
            backend,
            capture,
            config,
            session: None,
            active_capture: None,
            ffmpeg_available: false,
        }
    }

    pub fn update_config(&mut self, capture: CaptureConfig, config: RecorderConfig) {
        self.capture = capture;
        self.config = config;
        if self.session.is_none() {
            self.backend = create_backend(&self.capture, &self.config);
        }
    }

    pub fn backend_id(&self) -> &str {
        self.backend.id()
    }

    #[allow(dead_code)]
    pub fn backend_display_name(&self) -> &str {
        self.backend.display_name()
    }

    #[allow(dead_code)]
    pub fn capabilities(&self) -> CaptureCapabilities {
        self.backend.capabilities()
    }

    #[allow(dead_code)]
    pub async fn discover_audio_sources(
        &self,
    ) -> Result<Vec<DiscoveredAudioSource>, AudioSourceError> {
        self.backend.discover_audio_sources().await
    }

    pub fn backend_handle(&self) -> Arc<dyn CaptureBackend> {
        self.backend.clone()
    }

    #[allow(dead_code)]
    pub fn translate_audio_source(
        &self,
        kind: &crate::config::AudioSourceKind,
    ) -> Result<(), AudioSourceError> {
        self.backend.validate_audio_source(kind)
    }

    #[allow(dead_code)]
    pub fn has_ffmpeg(&self) -> bool {
        self.ffmpeg_available
    }

    pub fn capture_request(&self, capture: &CaptureSourcePlan) -> CaptureRequest {
        CaptureRequest {
            capture: capture.clone(),
            recorder: self.config.clone(),
        }
    }

    #[allow(dead_code)]
    pub fn spawn_replay_session(
        &self,
        capture: &CaptureSourcePlan,
    ) -> Result<Box<dyn CaptureSession>, CaptureError> {
        self.backend.spawn_replay(self.capture_request(capture))
    }

    pub fn attach_session(
        &mut self,
        capture: CaptureSourcePlan,
        session: Box<dyn CaptureSession>,
    ) -> Result<(), CaptureError> {
        if self.is_running() {
            return Err(CaptureError::AlreadyRunning);
        }

        self.session = Some(session);
        self.active_capture = Some(capture);
        Ok(())
    }

    #[allow(dead_code)]
    pub fn start_replay(&mut self, capture: &CaptureSourcePlan) -> Result<(), CaptureError> {
        let session = self.spawn_replay_session(capture)?;
        self.attach_session(capture.clone(), session)
    }

    pub fn save_clip(&mut self, length: ClipLength) -> Result<(), CaptureError> {
        self.session
            .as_mut()
            .ok_or(CaptureError::NotRunning)?
            .save_clip(length)
    }

    pub fn poll_save_results(&mut self) -> Vec<SavePollResult> {
        let Some(session) = self.session.as_mut() else {
            return Vec::new();
        };
        let results = session.poll_results();
        if !session.is_running() && !session.save_in_progress() {
            self.session = None;
            self.active_capture = None;
        }
        results
    }

    pub fn stop(&mut self) -> Result<(), CaptureError> {
        let result = self
            .session
            .as_mut()
            .ok_or(CaptureError::NotRunning)?
            .stop();
        self.session = None;
        self.active_capture = None;
        result
    }

    pub fn is_running(&mut self) -> bool {
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        let running = session.is_running();
        if !running {
            self.session = None;
            self.active_capture = None;
        }
        running
    }

    pub fn save_in_progress(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.save_in_progress())
    }

    pub fn has_active_session(&self) -> bool {
        self.session.is_some()
    }

    #[allow(dead_code)]
    pub fn active_audio_layout(&self) -> &[ResolvedAudioSource] {
        self.session
            .as_ref()
            .map(|session| session.active_audio_layout())
            .unwrap_or(&[])
    }

    pub fn should_probe_saved_clip_resolution(&self) -> bool {
        self.active_capture
            .as_ref()
            .is_some_and(|capture| self.backend.should_probe_saved_clip_resolution(capture))
    }

    pub fn post_save_recovery_hint(
        &self,
        video_resolution: Option<VideoResolution>,
    ) -> RecoveryHint {
        self.active_capture
            .as_ref()
            .map(|capture| {
                self.backend
                    .post_save_recovery_hint(capture, video_resolution)
            })
            .unwrap_or(RecoveryHint::None)
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        if self.is_running() {
            let _ = self.stop();
        }
    }
}

pub async fn probe_video_resolution(path: PathBuf) -> Result<Option<VideoResolution>, String> {
    tokio::task::spawn_blocking(move || probe_video_resolution_blocking(&path))
        .await
        .map_err(|error| format!("failed to join ffprobe worker: {error}"))?
}

pub fn clear_portal_session_token() -> Result<bool, String> {
    let Some(path) = portal_session_token_path() else {
        return Ok(false);
    };

    match std::fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!(
            "failed to clear portal session token {}: {error}",
            path.display()
        )),
    }
}

fn create_backend(capture: &CaptureConfig, config: &RecorderConfig) -> Arc<dyn CaptureBackend> {
    match capture.backend.as_str() {
        #[cfg(target_os = "linux")]
        "gsr" | "" => Arc::new(GsrBackend::new()),
        "obs" => Arc::new(ObsBackend::new(config.obs().clone())),
        other => {
            tracing::warn!(
                "Unknown capture backend '{other}', falling back to the platform default"
            );
            default_backend_for_platform()
        }
    }
}

#[cfg(target_os = "linux")]
fn default_backend_for_platform() -> Arc<dyn CaptureBackend> {
    Arc::new(GsrBackend::new())
}

#[cfg(target_os = "windows")]
fn default_backend_for_platform() -> Arc<dyn CaptureBackend> {
    Arc::new(ObsBackend::new(Default::default()))
}

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn portal_session_token_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "nanite-clip")
        .map(|dirs| dirs.config_dir().join("gpu-screen-recorder-portal-token"))
}

fn probe_video_resolution_blocking(path: &Path) -> Result<Option<VideoResolution>, String> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=width,height")
        .arg("-of")
        .arg("csv=s=x:p=0")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("failed to run ffprobe for {}: {error}", path.display()))?;

    if !output.status.success() {
        return Err(format!(
            "ffprobe rejected {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let dimensions = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if dimensions.is_empty() {
        return Ok(None);
    }

    let Some((width, height)) = dimensions.split_once('x') else {
        return Err(format!(
            "ffprobe returned an unexpected resolution format for {}: {dimensions}",
            path.display()
        ));
    };

    Ok(Some(VideoResolution {
        width: width
            .parse()
            .map_err(|error| format!("invalid width for {}: {error}", path.display()))?,
        height: height
            .parse()
            .map_err(|error| format!("invalid height for {}: {error}", path.display()))?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::{MockBackend, SavePollResult};
    use crate::config::CaptureConfig;
    use crate::process::{BackendHints, CaptureSourcePlan, CaptureTarget};
    use std::sync::Arc;

    #[test]
    fn parses_ffprobe_resolution_output() {
        let resolution = {
            let dimensions = "1920x1080";
            let (width, height) = dimensions.split_once('x').unwrap();
            VideoResolution {
                width: width.parse().unwrap(),
                height: height.parse().unwrap(),
            }
        };
        assert_eq!(resolution.width, 1920);
        assert_eq!(resolution.height, 1080);
    }

    #[test]
    fn unknown_backend_falls_back_without_panicking() {
        let backend = create_backend(
            &CaptureConfig {
                backend: "mystery".into(),
            },
            &RecorderConfig::default(),
        );

        #[cfg(target_os = "linux")]
        assert_eq!(backend.id(), "gsr");
        #[cfg(target_os = "windows")]
        assert_eq!(backend.id(), "obs");
    }

    #[test]
    fn recorder_happy_path_uses_backend_results() {
        let backend = MockBackend::new();
        backend
            .queued_results
            .lock()
            .unwrap()
            .push(SavePollResult::Saved {
                path: PathBuf::from("/tmp/clip.mkv"),
                duration: ClipLength::FullBuffer,
                audio_layout: Vec::new(),
            });

        let mut recorder = Recorder::with_backend(
            CaptureConfig {
                backend: "mock".into(),
            },
            RecorderConfig::default(),
            Arc::new(backend),
        );
        let capture = CaptureSourcePlan {
            target: CaptureTarget::Monitor("DP-1".into()),
            backend_hints: BackendHints::default(),
        };

        recorder.start_replay(&capture).unwrap();
        recorder.save_clip(ClipLength::FullBuffer).unwrap();
        let results = recorder.poll_save_results();

        assert!(matches!(
            results.as_slice(),
            [SavePollResult::Saved {
                duration: ClipLength::FullBuffer,
                ..
            }]
        ));
    }

    #[test]
    fn recorder_rejects_save_while_in_flight() {
        let backend = Arc::new(MockBackend::new());
        let mut recorder = Recorder::with_backend(
            CaptureConfig {
                backend: "mock".into(),
            },
            RecorderConfig::default(),
            backend.clone(),
        );
        let capture = CaptureSourcePlan {
            target: CaptureTarget::Monitor("DP-1".into()),
            backend_hints: BackendHints::default(),
        };

        recorder.start_replay(&capture).unwrap();
        recorder.save_clip(ClipLength::FullBuffer).unwrap();
        let error = recorder.save_clip(ClipLength::FullBuffer).unwrap_err();

        assert!(matches!(error, CaptureError::SaveInFlight));
    }

    #[test]
    fn recorder_clears_session_after_backend_exit() {
        let backend = Arc::new(MockBackend::new());
        let mut recorder = Recorder::with_backend(
            CaptureConfig {
                backend: "mock".into(),
            },
            RecorderConfig::default(),
            backend.clone(),
        );
        let capture = CaptureSourcePlan {
            target: CaptureTarget::Monitor("DP-1".into()),
            backend_hints: BackendHints::default(),
        };

        recorder.start_replay(&capture).unwrap();
        backend.shared_state.lock().unwrap().is_running = false;

        assert!(!recorder.is_running());
        assert!(!recorder.has_active_session());
    }

    #[test]
    fn recorder_does_not_swap_backend_mid_session() {
        let backend = Arc::new(MockBackend::new());
        let mut recorder = Recorder::with_backend(
            CaptureConfig {
                backend: "mock".into(),
            },
            RecorderConfig::default(),
            backend.clone(),
        );
        let capture = CaptureSourcePlan {
            target: CaptureTarget::Monitor("DP-1".into()),
            backend_hints: BackendHints::default(),
        };

        recorder.start_replay(&capture).unwrap();
        recorder.update_config(
            CaptureConfig {
                backend: "gsr".into(),
            },
            RecorderConfig::default(),
        );

        assert_eq!(recorder.backend_id(), "mock");
    }

    #[test]
    fn recorder_propagates_spawn_failures_without_leaking_session() {
        let backend = Arc::new(MockBackend::new());
        *backend.force_spawn_error.lock().unwrap() = Some(CaptureError::SpawnFailed("boom".into()));
        let mut recorder = Recorder::with_backend(
            CaptureConfig {
                backend: "mock".into(),
            },
            RecorderConfig::default(),
            backend,
        );
        let capture = CaptureSourcePlan {
            target: CaptureTarget::Monitor("DP-1".into()),
            backend_hints: BackendHints::default(),
        };

        let error = recorder.start_replay(&capture).unwrap_err();

        assert!(matches!(error, CaptureError::SpawnFailed(_)));
        assert!(!recorder.has_active_session());
    }
}
