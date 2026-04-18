use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use std::{borrow::Cow, path::Path};

use iced::futures::StreamExt;
use obws::Client;
use obws::client::{ConnectConfig, DangerousConnectConfig};
use obws::error::Error as ObwsError;
use obws::events::{Event, EventStream};
use obws::requests::profiles::SetParameter;
use obws::responses::StatusCode;
use obws::responses::general::Version as ObsVersionInfo;
use semver::Version;
use tokio::runtime::Builder;
use tokio::sync::mpsc as tokio_mpsc;
use url::Url;

use crate::capture::{
    AudioSourceError, BackendRuntimeEvent, CaptureBackend, CaptureCapabilities, CaptureError,
    CaptureRequest, CaptureSession, DiscoveredAudioSource, ObsConnectionStatus, RecoveryHint,
    ResolvedAudioSource, SavePollResult,
};
use crate::config::{AudioSourceKind, ObsBackendConfig, ObsManagementMode};
use crate::process::{CaptureSourcePlan, CaptureTarget};
use crate::recorder::VideoResolution;
use crate::rules::ClipLength;

const OBS_MIN_STUDIO_VERSION_MAJOR: u64 = 28;
const OBS_SUPPORTED_WEBSOCKET_VERSION_MAJOR: u64 = 5;
const OBS_SUPPORTED_RPC_VERSION: u32 = 1;
/// Floor for the computed replay buffer size in MB. Below this OBS can truncate
/// recent seconds even if our math says the buffer fits.
const OBS_MIN_REPLAY_BUFFER_SIZE_MB: u64 = 256;
/// Ceiling for the computed replay buffer size in MB. Big enough for 15 minutes
/// at ~60 Mbps; anything higher risks blowing through user disks.
const OBS_MAX_REPLAY_BUFFER_SIZE_MB: u64 = 8192;
/// Safety headroom multiplier (percent) applied on top of raw
/// bitrate × duration to tolerate keyframe spikes and OBS bookkeeping.
const OBS_REPLAY_BUFFER_SIZE_HEADROOM_PCT: u64 = 125;
/// Fallback video bitrate (kbps) used when OBS does not expose the current
/// setting. Matches the OBS Simple Output default for 1080p60.
const OBS_FALLBACK_VIDEO_BITRATE_KBPS: u64 = 6_000;
/// Fallback audio bitrate (kbps) matching OBS Simple Output default.
const OBS_FALLBACK_AUDIO_BITRATE_KBPS: u64 = 160;
/// Suppress `ReplayBufferStateChanged { active: false }` notifications for a
/// brief window after connecting. Managed recording stops the replay buffer
/// while applying profile params, and OBS broadcasts that deactivation event
/// asynchronously — it can race past our own subscription and look like an
/// external disable.
const STARTUP_EVENT_GRACE: Duration = Duration::from_secs(3);
/// How long to wait for a `ReplayBufferSaved` event before giving up on an
/// in-flight save. Matches the worst-case latency observed on slow disks with
/// a full 5-minute buffer; beyond this, something has gone wrong inside OBS
/// and we need to surface a failure instead of hanging the UI forever.
const OBS_SAVE_EVENT_TIMEOUT: Duration = Duration::from_secs(60);
/// Max time to wait for the OBS `SaveReplayBuffer` request itself to return.
/// The request is usually sub-second; a longer wait implies a stuck websocket.
const OBS_SAVE_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Managed OBS sessions should eventually stop retrying and hand control back
/// to the user instead of retrying forever in the background.
const OBS_MAX_RECONNECT_ATTEMPTS: u32 = 10;
const OBS_OUTPUT_MODE_CATEGORY: &str = "Output";
const OBS_OUTPUT_MODE_NAME: &str = "Mode";
const OBS_OUTPUT_MODE_SIMPLE: &str = "Simple";
const OBS_SIMPLE_OUTPUT_CATEGORY: &str = "SimpleOutput";
const OBS_SIMPLE_OUTPUT_FILE_PATH: &str = "FilePath";
const OBS_SIMPLE_OUTPUT_VBITRATE: &str = "VBitrate";
const OBS_SIMPLE_OUTPUT_ABITRATE: &str = "ABitrate";
const OBS_REPLAY_BUFFER_ENABLE_GUIDANCE: &str = "OBS replay buffer is still disabled after NaniteClip tried to configure it. Open OBS Settings -> Output -> Replay Buffer, enable it for the active profile once, apply the change, then restart monitoring.";

#[derive(Debug, Clone)]
pub struct ObsBackend {
    config: ObsBackendConfig,
}

impl ObsBackend {
    pub fn new(config: ObsBackendConfig) -> Self {
        Self { config }
    }
}

pub async fn test_connection(config: ObsBackendConfig) -> Result<String, String> {
    let client = connect_client(&config)
        .await
        .map_err(|error| error.to_string())?;
    let version = client
        .general()
        .version()
        .await
        .map_err(|error| format!("OBS GetVersion failed: {error}"))?;
    ensure_supported_obs_version(&version).map_err(|error| error.to_string())?;

    let replay_status = match client.replay_buffer().status().await {
        Ok(true) => "running",
        Ok(false) => match config.management_mode {
            ObsManagementMode::ManagedRecording | ObsManagementMode::FullManagement => {
                "configured but not running (NaniteClip will try to start it when monitoring begins)"
            }
            ObsManagementMode::BringYourOwn => {
                "configured but not running (start it in OBS before monitoring)"
            }
        },
        Err(error) if is_replay_buffer_not_configured(&error) => match config.management_mode {
            ObsManagementMode::ManagedRecording | ObsManagementMode::FullManagement => {
                "not enabled yet (NaniteClip will try to configure it when monitoring begins)"
            }
            ObsManagementMode::BringYourOwn => {
                "not enabled in OBS (turn it on in Settings → Output → Replay Buffer, or switch to Managed Recording)"
            }
        },
        Err(error) => {
            return Err(format!("OBS GetReplayBufferStatus failed: {error}"));
        }
    };

    Ok(format!(
        "Connected to OBS {} at {}. Replay buffer: {}.",
        version.obs_studio_version, config.websocket_url, replay_status
    ))
}

fn is_replay_buffer_not_configured(error: &ObwsError) -> bool {
    matches!(
        error,
        ObwsError::Api {
            code: StatusCode::InvalidResourceState,
            ..
        }
    )
}

fn is_output_running(error: &ObwsError) -> bool {
    matches!(
        error,
        ObwsError::Api {
            code: StatusCode::OutputRunning,
            ..
        }
    )
}

impl CaptureBackend for ObsBackend {
    fn id(&self) -> &'static str {
        "obs"
    }

    fn display_name(&self) -> &'static str {
        "OBS Studio (obs-websocket)"
    }

    fn capabilities(&self) -> CaptureCapabilities {
        match self.config.management_mode {
            ObsManagementMode::BringYourOwn | ObsManagementMode::ManagedRecording => {
                CaptureCapabilities {
                    replay_buffer: true,
                    ..CaptureCapabilities::default()
                }
            }
            ObsManagementMode::FullManagement => CaptureCapabilities {
                replay_buffer: true,
                cursor_capture: true,
                ..CaptureCapabilities::default()
            },
        }
    }

    fn discover_audio_sources(
        &self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Vec<DiscoveredAudioSource>, AudioSourceError>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async move {
            Err(AudioSourceError::Unsupported {
                capability: "OBS owns audio routing for this backend mode".into(),
            })
        })
    }

    fn validate_audio_source(&self, _kind: &AudioSourceKind) -> Result<(), AudioSourceError> {
        match self.config.management_mode {
            ObsManagementMode::BringYourOwn | ObsManagementMode::ManagedRecording => {
                Err(AudioSourceError::Unsupported {
                    capability: "OBS owns audio routing".into(),
                })
            }
            ObsManagementMode::FullManagement => Err(AudioSourceError::Unsupported {
                capability: "Full OBS audio management is not yet implemented".into(),
            }),
        }
    }

    fn spawn_replay(
        &self,
        request: CaptureRequest,
    ) -> Result<Box<dyn CaptureSession>, CaptureError> {
        if request.capture.target != CaptureTarget::BackendOwned {
            return Err(CaptureError::Unsupported {
                capability: "OBS capture backend requires a backend-owned capture target".into(),
            });
        }

        Ok(Box::new(ObsCaptureSession::spawn(
            self.config.clone(),
            request,
        )?))
    }

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

#[derive(Debug)]
pub struct ObsCaptureSession {
    command_tx: tokio_mpsc::UnboundedSender<ObsCommand>,
    result_rx: tokio_mpsc::UnboundedReceiver<SavePollResult>,
    worker: Option<thread::JoinHandle<()>>,
    save_in_flight: bool,
    pending_save_length: Option<ClipLength>,
    active_audio_layout: Vec<ResolvedAudioSource>,
}

#[derive(Debug)]
enum ObsCommand {
    Save,
    Stop,
}

struct ConnectedState {
    client: Client,
    events: EventStream,
    startup_at: Instant,
    parameter_keys: Option<ObsParameterKeys>,
    managed_profile_snapshot: Option<ManagedProfileSnapshot>,
}

#[derive(Debug, Clone)]
struct ManagedProfileSnapshot {
    file_path: Option<String>,
    rec_format: Option<String>,
    rec_rb: Option<String>,
    rec_rb_time: Option<String>,
    rec_rb_size: Option<String>,
    was_active: bool,
}

#[derive(Debug, Clone, Copy)]
struct ObsParameterKeys {
    simple_output_rec_format: &'static str,
    simple_output_rec_rb: &'static str,
    simple_output_rec_rb_time: &'static str,
    simple_output_rec_rb_size: &'static str,
}

impl ObsParameterKeys {
    fn for_version(version: &Version) -> Self {
        let format_key = if version.major >= 30 {
            "RecFormat2"
        } else {
            "RecFormat"
        };

        Self {
            simple_output_rec_format: format_key,
            simple_output_rec_rb: "RecRB",
            simple_output_rec_rb_time: "RecRBTime",
            simple_output_rec_rb_size: "RecRBSize",
        }
    }
}

impl ObsCaptureSession {
    pub fn spawn(config: ObsBackendConfig, request: CaptureRequest) -> Result<Self, CaptureError> {
        std::fs::create_dir_all(&request.recorder.save_directory)
            .map_err(|error| CaptureError::Io(error.to_string()))?;

        let (command_tx, command_rx) = tokio_mpsc::unbounded_channel();
        let (result_tx, result_rx) = tokio_mpsc::unbounded_channel();
        let (startup_tx, startup_rx) = mpsc::channel();

        let worker = thread::spawn(move || {
            let runtime = match Builder::new_current_thread().enable_all().build() {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = startup_tx
                        .send(Err(format!("failed to start OBS worker runtime: {error}")));
                    return;
                }
            };

            runtime.block_on(run_obs_session(
                config, request, command_rx, result_tx, startup_tx,
            ));
        });

        match startup_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                command_tx,
                result_rx,
                worker: Some(worker),
                save_in_flight: false,
                pending_save_length: None,
                active_audio_layout: Vec::new(),
            }),
            Ok(Err(error)) => {
                join_obs_worker(worker);
                Err(CaptureError::SpawnFailed(error))
            }
            Err(error) => {
                join_obs_worker(worker);
                Err(CaptureError::SpawnFailed(format!(
                    "failed to receive OBS startup status: {error}"
                )))
            }
        }
    }
}

fn join_obs_worker(worker: thread::JoinHandle<()>) {
    if let Err(panic) = worker.join() {
        // Surface panic payloads so we are not debugging silent worker
        // deaths. `Any::downcast_ref` covers the two common panic payload
        // shapes (`&'static str` and `String`); anything else falls through
        // to a generic marker.
        let message = panic
            .downcast_ref::<&'static str>()
            .map(|s| (*s).to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".into());
        tracing::error!("OBS worker thread panicked: {message}");
    }
}

impl CaptureSession for ObsCaptureSession {
    fn save_clip(&mut self, length: ClipLength) -> Result<(), CaptureError> {
        if self.save_in_flight {
            return Err(CaptureError::SaveInFlight);
        }

        if !matches!(length, ClipLength::FullBuffer) {
            tracing::warn!(
                "OBS replay buffer currently saves the full buffer; custom clip lengths will be trimmed in a follow-up"
            );
        }

        self.command_tx
            .send(ObsCommand::Save)
            .map_err(|_| CaptureError::NotRunning)?;
        self.save_in_flight = true;
        self.pending_save_length = Some(length);
        Ok(())
    }

    fn poll_results(&mut self) -> Vec<SavePollResult> {
        let mut results = Vec::new();
        while let Ok(result) = self.result_rx.try_recv() {
            let result = match result {
                SavePollResult::Saved {
                    path,
                    duration: _,
                    audio_layout,
                } => SavePollResult::Saved {
                    path,
                    duration: self
                        .pending_save_length
                        .take()
                        .unwrap_or(ClipLength::FullBuffer),
                    audio_layout,
                },
                SavePollResult::SaveFailed(error) => {
                    self.pending_save_length = None;
                    SavePollResult::SaveFailed(error)
                }
                SavePollResult::BackendEvent(event) => SavePollResult::BackendEvent(event),
            };

            if !matches!(result, SavePollResult::BackendEvent(_)) {
                self.save_in_flight = false;
            }
            results.push(result);
        }

        if self
            .worker
            .as_ref()
            .is_some_and(thread::JoinHandle::is_finished)
            && self.save_in_flight
        {
            self.save_in_flight = false;
            self.pending_save_length = None;
            results.push(SavePollResult::SaveFailed(
                "OBS capture session stopped unexpectedly".into(),
            ));
        }

        results
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        // Send the stop command best-effort. Even when the channel is already
        // closed (the worker has exited on its own, e.g. during a reconnect
        // loop cancellation), we still need to join() the thread so the
        // handle is not leaked.
        let _ = self.command_tx.send(ObsCommand::Stop);
        if let Some(worker) = self.worker.take() {
            join_obs_worker(worker);
        }
        self.save_in_flight = false;
        self.pending_save_length = None;
        Ok(())
    }

    fn active_audio_layout(&self) -> &[ResolvedAudioSource] {
        &self.active_audio_layout
    }

    fn is_running(&mut self) -> bool {
        if self
            .worker
            .as_ref()
            .is_some_and(thread::JoinHandle::is_finished)
        {
            if let Some(worker) = self.worker.take() {
                join_obs_worker(worker);
            }
            return false;
        }

        self.worker.is_some()
    }

    fn save_in_progress(&self) -> bool {
        self.save_in_flight
    }
}

impl Drop for ObsCaptureSession {
    fn drop(&mut self) {
        if self.worker.is_some() {
            let _ = self.stop();
        }
    }
}

async fn run_obs_session(
    config: ObsBackendConfig,
    request: CaptureRequest,
    mut command_rx: tokio_mpsc::UnboundedReceiver<ObsCommand>,
    result_tx: tokio_mpsc::UnboundedSender<SavePollResult>,
    startup_tx: mpsc::Sender<Result<(), String>>,
) {
    let mut connected = match connect_and_initialize(&config, &request, None).await {
        Ok(state) => {
            let _ = startup_tx.send(Ok(()));
            state
        }
        Err(error) => {
            let _ = startup_tx.send(Err(error.to_string()));
            return;
        }
    };

    let mut replay_disabled_notified = false;
    let mut save_in_flight = false;
    let mut save_deadline: Option<tokio::time::Instant> = None;

    loop {
        tokio::select! {
            () = async {
                match save_deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending::<()>().await,
                }
            } => {
                save_deadline = None;
                if save_in_flight {
                    save_in_flight = false;
                    let _ = result_tx.send(SavePollResult::SaveFailed(format!(
                        "OBS did not report a saved replay within {} seconds",
                        OBS_SAVE_EVENT_TIMEOUT.as_secs()
                    )));
                }
            }
            command = command_rx.recv() => {
                match command {
                    Some(ObsCommand::Save) => {
                        match tokio::time::timeout(
                            OBS_SAVE_REQUEST_TIMEOUT,
                            connected.client.replay_buffer().save(),
                        )
                        .await
                        {
                            Ok(Ok(())) => {
                                save_in_flight = true;
                                save_deadline = Some(tokio::time::Instant::now() + OBS_SAVE_EVENT_TIMEOUT);
                            }
                            Ok(Err(error)) => {
                                let _ = result_tx.send(SavePollResult::SaveFailed(format!(
                                    "OBS SaveReplayBuffer failed: {error}"
                                )));
                            }
                            Err(_) => {
                                let _ = result_tx.send(SavePollResult::SaveFailed(format!(
                                    "OBS SaveReplayBuffer request timed out after {} seconds",
                                    OBS_SAVE_REQUEST_TIMEOUT.as_secs()
                                )));
                            }
                        }
                    }
                    Some(ObsCommand::Stop) | None => {
                        if let Err(error) = shutdown_connected_state(&connected).await {
                            tracing::warn!("Failed to restore OBS profile on shutdown: {error}");
                        }
                        return;
                    }
                }
            }
            event = connected.events.next() => {
                match event {
                    Some(Event::ReplayBufferSaved { path }) => {
                        // OBS emits ReplayBufferSaved for user-initiated saves
                        // as well (hotkey, menu click). Only forward events
                        // that match a save NaniteClip has in flight —
                        // otherwise the app sees a phantom clip without a
                        // pending sequence and logs a spurious warning.
                        if save_in_flight {
                            save_in_flight = false;
                            save_deadline = None;
                            let _ = result_tx.send(SavePollResult::Saved {
                                path,
                                duration: ClipLength::FullBuffer,
                                audio_layout: Vec::new(),
                            });
                        } else {
                            tracing::debug!(
                                "Ignoring ReplayBufferSaved from OBS — no NaniteClip save in flight (path: {path:?})"
                            );
                        }
                    }
                    Some(Event::ReplayBufferStateChanged { active, .. }) => {
                        if active {
                            replay_disabled_notified = false;
                        } else if config.management_mode == ObsManagementMode::ManagedRecording
                            && !replay_disabled_notified
                            && connected.startup_at.elapsed() >= STARTUP_EVENT_GRACE
                        {
                            replay_disabled_notified = true;
                            let _ = result_tx.send(SavePollResult::SaveFailed(
                                "OBS replay buffer was disabled externally. Re-enable it in OBS before the next clip can save.".into(),
                            ));
                        }
                    }
                    Some(_) => {}
                    None => {
                        if save_in_flight {
                            save_in_flight = false;
                            save_deadline = None;
                            let _ = result_tx.send(SavePollResult::SaveFailed(
                                "OBS disconnected before the replay buffer reported the saved clip.".into(),
                            ));
                        }

                        match reconnect_until_connected(
                            &config,
                            &request,
                            connected.managed_profile_snapshot.clone(),
                            &mut command_rx,
                            &result_tx,
                        )
                        .await
                        {
                            Some(state) => {
                                connected = state;
                                replay_disabled_notified = false;
                            }
                            None => return,
                        }
                    }
                }
            }
        }
    }
}

async fn reconnect_until_connected(
    config: &ObsBackendConfig,
    request: &CaptureRequest,
    managed_profile_snapshot: Option<ManagedProfileSnapshot>,
    command_rx: &mut tokio_mpsc::UnboundedReceiver<ObsCommand>,
    result_tx: &tokio_mpsc::UnboundedSender<SavePollResult>,
) -> Option<ConnectedState> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let backoff = backoff_for_attempt(attempt);
        let _ = result_tx.send(SavePollResult::BackendEvent(
            BackendRuntimeEvent::ObsConnection(ObsConnectionStatus::Reconnecting {
                attempt,
                next_retry_in_secs: backoff.as_secs(),
            }),
        ));

        tokio::select! {
            _ = tokio::time::sleep(backoff) => {
                match connect_and_initialize(config, request, managed_profile_snapshot.clone()).await {
                    Ok(state) => {
                        let _ = result_tx.send(SavePollResult::BackendEvent(
                            BackendRuntimeEvent::ObsConnection(ObsConnectionStatus::Connected),
                        ));
                        return Some(state);
                    }
                    Err(error) => {
                        tracing::warn!("OBS reconnect attempt failed: {error}");
                        if attempt >= OBS_MAX_RECONNECT_ATTEMPTS {
                            let _ = result_tx.send(SavePollResult::BackendEvent(
                                BackendRuntimeEvent::ObsConnection(ObsConnectionStatus::Failed {
                                    reason: format!(
                                        "OBS did not reconnect after {attempt} attempts: {error}"
                                    ),
                                }),
                            ));
                            return None;
                        }
                    }
                }
            }
            command = command_rx.recv() => {
                match command {
                    Some(ObsCommand::Stop) | None => return None,
                    Some(ObsCommand::Save) => {
                        let _ = result_tx.send(SavePollResult::SaveFailed(
                            "OBS is reconnecting. The replay buffer cannot be saved until the connection comes back.".into(),
                        ));
                    }
                }
            }
        }
    }
}

fn backoff_for_attempt(attempt: u32) -> Duration {
    match attempt {
        1 => Duration::from_secs(1),
        2 => Duration::from_secs(2),
        3 => Duration::from_secs(5),
        4 => Duration::from_secs(10),
        _ => Duration::from_secs(30),
    }
}

async fn connect_and_initialize(
    config: &ObsBackendConfig,
    request: &CaptureRequest,
    managed_profile_snapshot: Option<ManagedProfileSnapshot>,
) -> Result<ConnectedState, CaptureError> {
    let client = connect_client(config).await?;
    let version =
        client.general().version().await.map_err(|error| {
            CaptureError::SpawnFailed(format!("OBS GetVersion failed: {error}"))
        })?;
    ensure_supported_obs_version(&version)?;
    let keys = ObsParameterKeys::for_version(&version.obs_studio_version);

    let managed_profile_snapshot = match config.management_mode {
        ObsManagementMode::BringYourOwn => {
            initialize_bring_your_own(&client).await?;
            None
        }
        ObsManagementMode::ManagedRecording => Some(
            initialize_managed_recording(&client, &keys, request, managed_profile_snapshot).await?,
        ),
        ObsManagementMode::FullManagement => {
            return Err(CaptureError::Unsupported {
                capability: "Full OBS management mode is not yet implemented".into(),
            });
        }
    };

    let events = client.events().map_err(|error| {
        CaptureError::SpawnFailed(format!("Could not subscribe to OBS events: {error}"))
    })?;

    Ok(ConnectedState {
        client,
        events,
        startup_at: Instant::now(),
        parameter_keys: managed_profile_snapshot.as_ref().map(|_| keys),
        managed_profile_snapshot,
    })
}

async fn initialize_bring_your_own(client: &Client) -> Result<(), CaptureError> {
    match client.replay_buffer().status().await {
        Ok(true) => Ok(()),
        Ok(false) => Err(CaptureError::SpawnFailed(
            "OBS replay buffer is not running. Start it in OBS before monitoring, or switch to Managed Recording mode to let NaniteClip start it for you.".into(),
        )),
        Err(error) if is_replay_buffer_not_configured(&error) => Err(CaptureError::SpawnFailed(
            "OBS replay buffer is not enabled. Turn it on in OBS Settings → Output → Replay Buffer, or switch to Managed Recording mode to let NaniteClip configure it for you.".into(),
        )),
        Err(error) => Err(CaptureError::SpawnFailed(format!(
            "OBS GetReplayBufferStatus failed: {error}"
        ))),
    }
}

async fn initialize_managed_recording(
    client: &Client,
    keys: &ObsParameterKeys,
    request: &CaptureRequest,
    existing_snapshot: Option<ManagedProfileSnapshot>,
) -> Result<ManagedProfileSnapshot, CaptureError> {
    let output_mode = client
        .profiles()
        .parameter(OBS_OUTPUT_MODE_CATEGORY, OBS_OUTPUT_MODE_NAME)
        .await
        .map_err(|error| {
            CaptureError::SpawnFailed(format!(
                "OBS GetProfileParameter(Output.Mode) failed: {error}"
            ))
        })?;

    if output_mode
        .value
        .as_deref()
        .is_none_or(|value| !value.eq_ignore_ascii_case(OBS_OUTPUT_MODE_SIMPLE))
    {
        return Err(CaptureError::SpawnFailed(
            "OBS managed recording requires Simple Output mode. Switch OBS to Simple output mode or use Bring Your Own mode.".into(),
        ));
    }

    let snapshot = match existing_snapshot {
        Some(snapshot) => snapshot,
        None => capture_managed_profile_snapshot(client, keys).await?,
    };
    let desired_file_path = request
        .recorder
        .save_directory
        .to_string_lossy()
        .to_string();
    let desired_rec_format = map_container(request.recorder.gsr().container.as_str())?;
    let desired_rec_rb = "true".to_string();
    let desired_rec_rb_time = request.recorder.replay_buffer_secs.to_string();
    let desired_rec_rb_size =
        compute_replay_buffer_size_mb(client, request.recorder.replay_buffer_secs)
            .await
            .to_string();

    if managed_profile_matches_request(
        &snapshot,
        &desired_file_path,
        &desired_rec_format,
        &desired_rec_rb,
        &desired_rec_rb_time,
        &desired_rec_rb_size,
    ) {
        tracing::info!(
            save_directory = %desired_file_path,
            rec_format = %desired_rec_format,
            replay_buffer_secs = request.recorder.replay_buffer_secs,
            replay_buffer_size_mb = %desired_rec_rb_size,
            "OBS replay buffer is already active with the desired managed settings; reusing existing replay buffer"
        );
        return Ok(snapshot);
    }

    let initialize_result = async {
        let was_active = match client.replay_buffer().status().await {
            Ok(active) => active,
            Err(error) if is_replay_buffer_not_configured(&error) => false,
            Err(error) => {
                return Err(CaptureError::SpawnFailed(format!(
                    "OBS GetReplayBufferStatus failed: {error}"
                )));
            }
        };
        if was_active {
            client.replay_buffer().stop().await.map_err(|error| {
                CaptureError::SpawnFailed(format!("OBS StopReplayBuffer failed: {error}"))
            })?;
            wait_for_replay_buffer_inactive(client).await?;
        }

        write_profile_parameter(
            client,
            OBS_SIMPLE_OUTPUT_CATEGORY,
            OBS_SIMPLE_OUTPUT_FILE_PATH,
            &desired_file_path,
        )
        .await?;
        write_profile_parameter(
            client,
            OBS_SIMPLE_OUTPUT_CATEGORY,
            keys.simple_output_rec_format,
            &desired_rec_format,
        )
        .await?;
        write_profile_parameter(
            client,
            OBS_SIMPLE_OUTPUT_CATEGORY,
            keys.simple_output_rec_rb,
            &desired_rec_rb,
        )
        .await?;
        write_profile_parameter(
            client,
            OBS_SIMPLE_OUTPUT_CATEGORY,
            keys.simple_output_rec_rb_time,
            &desired_rec_rb_time,
        )
        .await?;
        write_profile_parameter(
            client,
            OBS_SIMPLE_OUTPUT_CATEGORY,
            keys.simple_output_rec_rb_size,
            &desired_rec_rb_size,
        )
        .await?;

        start_replay_buffer_with_verification(client).await
    }
    .await;

    match initialize_result {
        Ok(()) => Ok(snapshot),
        Err(error) => {
            if let Err(restore_error) =
                restore_managed_profile_snapshot(client, keys, &snapshot).await
            {
                tracing::warn!(
                    "Failed to restore OBS managed profile after initialization error: {restore_error}"
                );
            }
            Err(error)
        }
    }
}

fn managed_profile_matches_request(
    snapshot: &ManagedProfileSnapshot,
    desired_file_path: &str,
    desired_rec_format: &str,
    desired_rec_rb: &str,
    desired_rec_rb_time: &str,
    desired_rec_rb_size: &str,
) -> bool {
    snapshot.was_active
        && snapshot.file_path.as_deref().is_some_and(|value| {
            normalized_profile_path(value) == normalized_profile_path(desired_file_path)
        })
        && snapshot.rec_format.as_deref().is_some_and(|value| {
            normalized_profile_text(value) == normalized_profile_text(desired_rec_format)
        })
        && snapshot
            .rec_rb
            .as_deref()
            .and_then(parse_obs_bool)
            .zip(parse_obs_bool(desired_rec_rb))
            .is_some_and(|(current, desired)| current == desired)
        && snapshot
            .rec_rb_time
            .as_deref()
            .and_then(parse_obs_u64)
            .zip(parse_obs_u64(desired_rec_rb_time))
            .is_some_and(|(current, desired)| current == desired)
        && snapshot
            .rec_rb_size
            .as_deref()
            .and_then(parse_obs_u64)
            .zip(parse_obs_u64(desired_rec_rb_size))
            .is_some_and(|(current, desired)| current == desired)
}

fn normalized_profile_text(value: &str) -> Cow<'_, str> {
    Cow::Owned(value.trim().to_ascii_lowercase())
}

fn parse_obs_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_obs_u64(value: &str) -> Option<u64> {
    value.trim().parse().ok()
}

fn normalized_profile_path(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let normalized = Path::new(trimmed)
        .components()
        .as_path()
        .to_string_lossy()
        .replace('/', "\\");
    let normalized = normalized.trim_end_matches(['\\', '/']).to_string();

    #[cfg(target_os = "windows")]
    {
        normalized.to_ascii_lowercase()
    }

    #[cfg(not(target_os = "windows"))]
    {
        normalized
    }
}

async fn capture_managed_profile_snapshot(
    client: &Client,
    keys: &ObsParameterKeys,
) -> Result<ManagedProfileSnapshot, CaptureError> {
    let was_active = match client.replay_buffer().status().await {
        Ok(active) => active,
        Err(error) if is_replay_buffer_not_configured(&error) => false,
        Err(error) => {
            return Err(CaptureError::SpawnFailed(format!(
                "OBS GetReplayBufferStatus failed: {error}"
            )));
        }
    };

    Ok(ManagedProfileSnapshot {
        file_path: read_profile_parameter(
            client,
            OBS_SIMPLE_OUTPUT_CATEGORY,
            OBS_SIMPLE_OUTPUT_FILE_PATH,
        )
        .await?,
        rec_format: read_profile_parameter(
            client,
            OBS_SIMPLE_OUTPUT_CATEGORY,
            keys.simple_output_rec_format,
        )
        .await?,
        rec_rb: read_profile_parameter(
            client,
            OBS_SIMPLE_OUTPUT_CATEGORY,
            keys.simple_output_rec_rb,
        )
        .await?,
        rec_rb_time: read_profile_parameter(
            client,
            OBS_SIMPLE_OUTPUT_CATEGORY,
            keys.simple_output_rec_rb_time,
        )
        .await?,
        rec_rb_size: read_profile_parameter(
            client,
            OBS_SIMPLE_OUTPUT_CATEGORY,
            keys.simple_output_rec_rb_size,
        )
        .await?,
        was_active,
    })
}

async fn restore_managed_profile_snapshot(
    client: &Client,
    keys: &ObsParameterKeys,
    snapshot: &ManagedProfileSnapshot,
) -> Result<(), CaptureError> {
    let is_active = match client.replay_buffer().status().await {
        Ok(active) => active,
        Err(error) if is_replay_buffer_not_configured(&error) => false,
        Err(error) => {
            return Err(CaptureError::SpawnFailed(format!(
                "OBS GetReplayBufferStatus failed while restoring the previous profile: {error}"
            )));
        }
    };
    if is_active {
        client.replay_buffer().stop().await.map_err(|error| {
            CaptureError::SpawnFailed(format!(
                "OBS StopReplayBuffer failed while restoring the previous profile: {error}"
            ))
        })?;
        wait_for_replay_buffer_inactive(client).await?;
    }

    write_profile_parameter_value(
        client,
        OBS_SIMPLE_OUTPUT_CATEGORY,
        OBS_SIMPLE_OUTPUT_FILE_PATH,
        snapshot.file_path.as_deref(),
    )
    .await?;
    write_profile_parameter_value(
        client,
        OBS_SIMPLE_OUTPUT_CATEGORY,
        keys.simple_output_rec_format,
        snapshot.rec_format.as_deref(),
    )
    .await?;
    write_profile_parameter_value(
        client,
        OBS_SIMPLE_OUTPUT_CATEGORY,
        keys.simple_output_rec_rb,
        snapshot.rec_rb.as_deref(),
    )
    .await?;
    write_profile_parameter_value(
        client,
        OBS_SIMPLE_OUTPUT_CATEGORY,
        keys.simple_output_rec_rb_time,
        snapshot.rec_rb_time.as_deref(),
    )
    .await?;
    write_profile_parameter_value(
        client,
        OBS_SIMPLE_OUTPUT_CATEGORY,
        keys.simple_output_rec_rb_size,
        snapshot.rec_rb_size.as_deref(),
    )
    .await?;

    if snapshot.was_active {
        start_replay_buffer_with_verification(client).await?;
    }

    Ok(())
}

async fn start_replay_buffer_with_verification(client: &Client) -> Result<(), CaptureError> {
    match client.replay_buffer().start().await {
        Ok(()) => wait_for_replay_buffer_active(client).await,
        Err(error) if is_replay_buffer_not_configured(&error) => Err(CaptureError::SpawnFailed(
            OBS_REPLAY_BUFFER_ENABLE_GUIDANCE.into(),
        )),
        Err(error) if is_output_running(&error) => {
            let status = client.replay_buffer().status().await.map_err(|status_err| {
                CaptureError::SpawnFailed(format!(
                    "OBS StartReplayBuffer returned OutputRunning but GetReplayBufferStatus also failed: {status_err}"
                ))
            })?;
            if !status {
                return Err(CaptureError::SpawnFailed(
                    "OBS reported OutputRunning for StartReplayBuffer but the buffer is not actually active. Restart OBS and try again.".into(),
                ));
            }
            tracing::info!(
                "OBS StartReplayBuffer reported OutputRunning; accepting because buffer is already active"
            );
            Ok(())
        }
        Err(error) => Err(CaptureError::SpawnFailed(format!(
            "OBS StartReplayBuffer failed: {error}"
        ))),
    }
}

async fn compute_replay_buffer_size_mb(client: &Client, replay_buffer_secs: u32) -> u64 {
    let video_kbps = read_profile_parameter_u64(
        client,
        OBS_SIMPLE_OUTPUT_CATEGORY,
        OBS_SIMPLE_OUTPUT_VBITRATE,
    )
    .await
    .unwrap_or(OBS_FALLBACK_VIDEO_BITRATE_KBPS);
    let audio_kbps = read_profile_parameter_u64(
        client,
        OBS_SIMPLE_OUTPUT_CATEGORY,
        OBS_SIMPLE_OUTPUT_ABITRATE,
    )
    .await
    .unwrap_or(OBS_FALLBACK_AUDIO_BITRATE_KBPS);

    let total_kbps = video_kbps.saturating_add(audio_kbps);
    // bytes = kbps * 1000 / 8 * seconds = kbps * 125 * seconds
    let raw_bytes = total_kbps
        .saturating_mul(125)
        .saturating_mul(replay_buffer_secs as u64);
    let padded_bytes = raw_bytes.saturating_mul(OBS_REPLAY_BUFFER_SIZE_HEADROOM_PCT) / 100;
    let raw_mb = padded_bytes / (1024 * 1024);

    raw_mb.clamp(OBS_MIN_REPLAY_BUFFER_SIZE_MB, OBS_MAX_REPLAY_BUFFER_SIZE_MB)
}

async fn read_profile_parameter_u64(
    client: &Client,
    category: &'static str,
    name: &'static str,
) -> Option<u64> {
    match client.profiles().parameter(category, name).await {
        Ok(param) => param
            .value
            .as_deref()
            .and_then(|value| value.trim().parse::<u64>().ok()),
        Err(error) => {
            tracing::debug!(
                "OBS GetProfileParameter({category}.{name}) failed while sizing replay buffer: {error}"
            );
            None
        }
    }
}

async fn read_profile_parameter(
    client: &Client,
    category: &'static str,
    name: &'static str,
) -> Result<Option<String>, CaptureError> {
    client
        .profiles()
        .parameter(category, name)
        .await
        .map(|param| param.value)
        .map_err(|error| {
            CaptureError::SpawnFailed(format!(
                "OBS GetProfileParameter({category}.{name}) failed: {error}"
            ))
        })
}

async fn wait_for_replay_buffer_active(client: &Client) -> Result<(), CaptureError> {
    wait_for_replay_buffer_state(client, true)
        .await
        .map_err(|error| {
            CaptureError::SpawnFailed(format!(
                "OBS replay buffer did not become active after start: {error}"
            ))
        })
}

async fn wait_for_replay_buffer_inactive(client: &Client) -> Result<(), CaptureError> {
    wait_for_replay_buffer_state(client, false)
        .await
        .map_err(|error| {
            CaptureError::SpawnFailed(format!(
                "OBS replay buffer did not stop within 5 seconds: {error}"
            ))
        })
}

async fn wait_for_replay_buffer_state(
    client: &Client,
    expected_active: bool,
) -> Result<(), String> {
    const MAX_WAIT: Duration = Duration::from_secs(5);
    const POLL_INTERVAL: Duration = Duration::from_millis(100);

    let deadline = Instant::now() + MAX_WAIT;
    loop {
        match client.replay_buffer().status().await {
            Ok(active) if active == expected_active => return Ok(()),
            Ok(_) => {}
            Err(error) if is_replay_buffer_not_configured(&error) && !expected_active => {
                return Ok(());
            }
            Err(error) if is_replay_buffer_not_configured(&error) => {
                return Err(OBS_REPLAY_BUFFER_ENABLE_GUIDANCE.into());
            }
            Err(error) => {
                return Err(format!("OBS GetReplayBufferStatus failed: {error}"));
            }
        }

        if Instant::now() >= deadline {
            return Err("timed out".into());
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn write_profile_parameter(
    client: &Client,
    category: &'static str,
    name: &'static str,
    value: &str,
) -> Result<(), CaptureError> {
    write_profile_parameter_value(client, category, name, Some(value)).await
}

async fn write_profile_parameter_value(
    client: &Client,
    category: &'static str,
    name: &'static str,
    value: Option<&str>,
) -> Result<(), CaptureError> {
    client
        .profiles()
        .set_parameter(SetParameter {
            category,
            name,
            value,
        })
        .await
        .map_err(|error| {
            CaptureError::SpawnFailed(format!(
                "OBS SetProfileParameter({category}.{name}) failed: {error}"
            ))
        })
}

async fn connect_client(config: &ObsBackendConfig) -> Result<Client, CaptureError> {
    let target = parse_websocket_url(config.websocket_url.as_str())?;

    Client::connect_with_config(ConnectConfig {
        host: target.host,
        port: target.port,
        dangerous: Some(DangerousConnectConfig {
            skip_studio_version_check: true,
            skip_websocket_version_check: false,
        }),
        password: config.websocket_password.as_deref(),
        event_subscriptions: None,
        tls: target.tls,
        broadcast_capacity: obws::client::DEFAULT_BROADCAST_CAPACITY,
        connect_timeout: obws::client::DEFAULT_CONNECT_TIMEOUT,
    })
    .await
    .map_err(|error| {
        CaptureError::SpawnFailed(format!(
            "Could not connect to OBS at {}. Make sure OBS is running and obs-websocket is enabled: {error}",
            config.websocket_url
        ))
    })
}

async fn shutdown_connected_state(connected: &ConnectedState) -> Result<(), CaptureError> {
    if let (Some(keys), Some(snapshot)) = (
        connected.parameter_keys.as_ref(),
        connected.managed_profile_snapshot.as_ref(),
    ) {
        restore_managed_profile_snapshot(&connected.client, keys, snapshot).await?;
    }
    Ok(())
}

fn ensure_supported_obs_version(version: &ObsVersionInfo) -> Result<(), CaptureError> {
    if version.obs_studio_version.major < OBS_MIN_STUDIO_VERSION_MAJOR {
        return Err(CaptureError::SpawnFailed(format!(
            "OBS Studio {} is too old. NaniteClip requires OBS Studio 28.0 or newer.",
            version.obs_studio_version
        )));
    }

    if !version.obs_studio_version.pre.is_empty() {
        return Err(CaptureError::SpawnFailed(format!(
            "OBS Studio {} is a pre-release build. NaniteClip only supports stable OBS releases.",
            version.obs_studio_version
        )));
    }

    if version.obs_web_socket_version.major != OBS_SUPPORTED_WEBSOCKET_VERSION_MAJOR {
        return Err(CaptureError::SpawnFailed(format!(
            "obs-websocket {} is unsupported. NaniteClip requires obs-websocket major version {}.",
            version.obs_web_socket_version, OBS_SUPPORTED_WEBSOCKET_VERSION_MAJOR
        )));
    }

    if version.rpc_version != OBS_SUPPORTED_RPC_VERSION {
        return Err(CaptureError::SpawnFailed(format!(
            "obs-websocket RPC version {} is unsupported. NaniteClip requires RPC version {}.",
            version.rpc_version, OBS_SUPPORTED_RPC_VERSION
        )));
    }

    Ok(())
}

fn map_container(container: &str) -> Result<String, CaptureError> {
    let normalized = container.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "mkv" | "mp4" | "mov" | "flv" | "ts" | "m3u8" | "hls" | "fragmented_mp4"
        | "fragmented_mov" | "mpegts" => Ok(normalized),
        other => Err(CaptureError::Unsupported {
            capability: format!(
                "Container `{other}` is not supported by OBS. Use mkv, mp4, mov, flv, ts, m3u8, hls, fragmented_mp4, fragmented_mov, or mpegts."
            ),
        }),
    }
}

#[derive(Debug, Clone)]
struct ObsConnectionTarget {
    host: String,
    port: u16,
    tls: bool,
}

fn parse_websocket_url(url: &str) -> Result<ObsConnectionTarget, CaptureError> {
    let trimmed = url.trim();
    let parsed = Url::parse(trimmed).map_err(|error| {
        CaptureError::SpawnFailed(format!("OBS websocket URL `{trimmed}` is invalid: {error}"))
    })?;

    let tls = match parsed.scheme() {
        "ws" => false,
        "wss" => true,
        other => {
            return Err(CaptureError::SpawnFailed(format!(
                "OBS websocket URL scheme `{other}` is unsupported. Use ws:// or wss://."
            )));
        }
    };

    let host = parsed.host_str().ok_or_else(|| {
        CaptureError::SpawnFailed(format!("OBS websocket URL `{trimmed}` is missing a host"))
    })?;
    let port = parsed.port_or_known_default().ok_or_else(|| {
        CaptureError::SpawnFailed(format!("OBS websocket URL `{trimmed}` is missing a port"))
    })?;

    if parsed.path() != "/" && !parsed.path().is_empty() {
        return Err(CaptureError::SpawnFailed(format!(
            "OBS websocket URL `{trimmed}` must not include a path"
        )));
    }

    if !is_loopback_host(host) {
        return Err(CaptureError::SpawnFailed(format!(
            "OBS websocket URL `{trimmed}` must point to a local OBS instance (host: {host}). \
             Remote OBS is not supported — the ReplayBufferSaved event returns a path on \
             the OBS machine, and NaniteClip cannot read files that live on another host."
        )));
    }

    Ok(ObsConnectionTarget {
        host: host.to_string(),
        port,
        tls,
    })
}

fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    // url::Url::host_str() wraps IPv6 literals in brackets (e.g. `[::1]`);
    // strip them before feeding to IpAddr::parse.
    let unbracketed = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(ip) = unbracketed.parse::<std::net::IpAddr>() {
        return ip.is_loopback();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{
        ManagedProfileSnapshot, ObsParameterKeys, backoff_for_attempt,
        ensure_supported_obs_version, managed_profile_matches_request, map_container,
        parse_websocket_url,
    };
    use obws::responses::general::Version as ObsVersionInfo;
    use semver::Version;
    use std::time::Duration;

    fn sample_obs_version() -> ObsVersionInfo {
        ObsVersionInfo {
            obs_studio_version: Version::new(30, 2, 0),
            obs_web_socket_version: Version::new(5, 5, 0),
            rpc_version: 1,
            available_requests: Vec::new(),
            supported_image_formats: Vec::new(),
            platform: "linux".into(),
            platform_description: "Linux".into(),
        }
    }

    #[test]
    fn obs_container_mapping_accepts_supported_formats() {
        assert_eq!(map_container("mkv").unwrap(), "mkv");
        assert_eq!(map_container("MP4").unwrap(), "mp4");
        assert_eq!(map_container("fragmented_mp4").unwrap(), "fragmented_mp4");
        assert_eq!(map_container("hls").unwrap(), "hls");
        assert_eq!(map_container("mpegts").unwrap(), "mpegts");
        assert!(map_container("webm").is_err());
    }

    #[test]
    fn obs_websocket_url_parses_localhost_ws_and_wss() {
        let ws = parse_websocket_url("ws://127.0.0.1:4455").unwrap();
        assert_eq!(ws.host, "127.0.0.1");
        assert_eq!(ws.port, 4455);
        assert!(!ws.tls);

        let localhost = parse_websocket_url("ws://localhost:4455").unwrap();
        assert_eq!(localhost.host, "localhost");

        let ipv6 = parse_websocket_url("wss://[::1]:8443").unwrap();
        assert_eq!(ipv6.host, "[::1]");
        assert!(ipv6.tls);
    }

    #[test]
    fn obs_websocket_url_rejects_remote_hosts() {
        assert!(parse_websocket_url("ws://obs.example.com:4455").is_err());
        assert!(parse_websocket_url("ws://10.0.0.5:4455").is_err());
        assert!(parse_websocket_url("wss://192.168.1.10:4455").is_err());
    }

    #[test]
    fn obs_parameter_keys_switch_rec_format_name_for_newer_obs() {
        let old = ObsParameterKeys::for_version(&Version::new(29, 1, 0));
        let new = ObsParameterKeys::for_version(&Version::new(30, 2, 0));

        assert_eq!(old.simple_output_rec_format, "RecFormat");
        assert_eq!(new.simple_output_rec_format, "RecFormat2");
    }

    #[test]
    fn reconnect_backoff_matches_documented_schedule() {
        assert_eq!(backoff_for_attempt(1), Duration::from_secs(1));
        assert_eq!(backoff_for_attempt(2), Duration::from_secs(2));
        assert_eq!(backoff_for_attempt(3), Duration::from_secs(5));
        assert_eq!(backoff_for_attempt(4), Duration::from_secs(10));
        assert_eq!(backoff_for_attempt(5), Duration::from_secs(30));
        assert_eq!(backoff_for_attempt(12), Duration::from_secs(30));
    }

    #[test]
    fn obs_version_guard_accepts_supported_stable_versions() {
        assert!(ensure_supported_obs_version(&sample_obs_version()).is_ok());
    }

    #[test]
    fn obs_version_guard_rejects_prerelease_studio_builds() {
        let mut version = sample_obs_version();
        version.obs_studio_version = Version::parse("30.2.0-rc1").unwrap();

        assert!(ensure_supported_obs_version(&version).is_err());
    }

    #[test]
    fn obs_version_guard_rejects_wrong_websocket_major() {
        let mut version = sample_obs_version();
        version.obs_web_socket_version = Version::new(6, 0, 0);

        assert!(ensure_supported_obs_version(&version).is_err());
    }

    #[test]
    fn managed_profile_match_requires_active_snapshot_with_expected_values() {
        let snapshot = ManagedProfileSnapshot {
            file_path: Some("C:\\Clips\\".into()),
            rec_format: Some("MKV".into()),
            rec_rb: Some("1".into()),
            rec_rb_time: Some("030".into()),
            rec_rb_size: Some("0512".into()),
            was_active: true,
        };

        assert!(managed_profile_matches_request(
            &snapshot, "C:/Clips", "mkv", "true", "30", "512",
        ));
    }

    #[test]
    fn managed_profile_match_rejects_mismatched_or_inactive_snapshots() {
        let inactive_snapshot = ManagedProfileSnapshot {
            file_path: Some("C:\\Clips".into()),
            rec_format: Some("mkv".into()),
            rec_rb: Some("true".into()),
            rec_rb_time: Some("30".into()),
            rec_rb_size: Some("512".into()),
            was_active: false,
        };
        assert!(!managed_profile_matches_request(
            &inactive_snapshot,
            "C:\\Clips",
            "mkv",
            "true",
            "30",
            "512",
        ));

        let mismatched_snapshot = ManagedProfileSnapshot {
            file_path: Some("C:\\Other".into()),
            rec_format: Some("mp4".into()),
            rec_rb: Some("false".into()),
            rec_rb_time: Some("15".into()),
            rec_rb_size: Some("256".into()),
            was_active: true,
        };
        assert!(!managed_profile_matches_request(
            &mismatched_snapshot,
            "C:\\Clips",
            "mkv",
            "true",
            "30",
            "512",
        ));
    }
}
