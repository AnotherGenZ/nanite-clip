use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::time::{Duration, Instant};

use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use pipewire as pw;

use crate::capture::{
    AudioSourceError, CaptureBackend, CaptureCapabilities, CaptureError, CaptureRequest,
    CaptureSession, DiscoveredAudioKind, DiscoveredAudioSource, RecoveryHint, ResolvedAudioSource,
    SavePollResult,
};
use crate::config::AudioSourceKind;
use crate::process::{CaptureSourcePlan, CaptureTarget, DisplayServer};
use crate::recorder::VideoResolution;
use crate::rules::ClipLength;

const STOP_TIMEOUT: Duration = Duration::from_secs(5);
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Default)]
pub struct GsrBackend;

impl GsrBackend {
    pub fn new() -> Self {
        Self
    }
}

impl CaptureBackend for GsrBackend {
    fn id(&self) -> &'static str {
        "gsr"
    }

    fn display_name(&self) -> &'static str {
        "gpu-screen-recorder"
    }

    fn capabilities(&self) -> CaptureCapabilities {
        CaptureCapabilities {
            per_app_audio: true,
            application_inverse: true,
            merged_tracks: true,
            portal_session_restore: true,
            replay_buffer: true,
            hdr: true,
            cursor_capture: true,
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
            let device_task = tokio::task::spawn_blocking(list_audio_devices);
            let app_task = tokio::task::spawn_blocking(list_application_audio);

            let devices = device_task
                .await
                .map_err(|error| AudioSourceError::TaskFailed(error.to_string()))??;
            let apps = app_task
                .await
                .map_err(|error| AudioSourceError::TaskFailed(error.to_string()))?;

            match apps {
                Ok(apps) => Ok(merge_discovered_audio(devices, apps)),
                Err(AudioSourceError::PerAppUnavailable { reason, .. }) => {
                    let partial = merge_discovered_audio(devices.clone(), Vec::new());
                    Err(AudioSourceError::PerAppUnavailable { reason, partial })
                }
                Err(error) => Err(error),
            }
        })
    }

    fn validate_audio_source(&self, kind: &AudioSourceKind) -> Result<(), AudioSourceError> {
        translate_kind_to_gsr(kind, self.id()).map(|_| ())
    }

    fn spawn_replay(
        &self,
        request: CaptureRequest,
    ) -> Result<Box<dyn CaptureSession>, CaptureError> {
        std::fs::create_dir_all(&request.recorder.save_directory)
            .map_err(|error| CaptureError::Io(error.to_string()))?;
        let gsr = request.recorder.gsr();
        let capture_target = gsr_capture_target_arg(&request.capture)?;

        let mut cmd = Command::new("gpu-screen-recorder");
        cmd.arg("-w")
            .arg(&capture_target)
            .arg("-f")
            .arg(gsr.framerate.to_string())
            .arg("-c")
            .arg(&gsr.container)
            .arg("-bm")
            .arg("cbr")
            .arg("-q")
            .arg(&gsr.quality)
            .arg("-k")
            .arg(&gsr.codec)
            .arg("-r")
            .arg(request.recorder.replay_buffer_secs.to_string())
            .arg("-o")
            .arg(&request.recorder.save_directory);

        if request.capture.backend_hints.restore_portal_session {
            cmd.arg("-restore-portal-session").arg("yes");
            if let Some(token_path) = portal_session_token_path() {
                cmd.arg("-portal-session-token-filepath").arg(token_path);
            }
        }

        let mut active_audio_layout = Vec::new();
        for config in request.recorder.audio_sources.iter().cloned() {
            let arg = translate_kind_to_gsr(&config.kind, self.id())?;
            cmd.arg("-a").arg(&arg);
            active_audio_layout.push(ResolvedAudioSource {
                config,
                resolved_display: arg,
            });
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let child = cmd
            .spawn()
            .map_err(|error| CaptureError::SpawnFailed(error.to_string()))?;

        if let Some(stdout) = child.stdout.as_ref() {
            set_nonblocking(stdout.as_raw_fd())?;
        }
        if let Some(stderr) = child.stderr.as_ref() {
            set_nonblocking(stderr.as_raw_fd())?;
        }

        write_layout_snapshot(&active_audio_layout);

        tracing::info!(
            "Started gpu-screen-recorder (pid {}) for capture source {}",
            child.id(),
            capture_target
        );

        Ok(Box::new(GsrCaptureSession {
            child: Some(child),
            stdout_buffer: String::new(),
            stderr_buffer: String::new(),
            save_in_flight: false,
            pending_save_length: None,
            save_directory: request.recorder.save_directory,
            active_audio_layout,
        }))
    }

    fn should_probe_saved_clip_resolution(&self, capture: &CaptureSourcePlan) -> bool {
        matches!(capture.target, CaptureTarget::WaylandPortal)
            && capture.backend_hints.display_server == Some(DisplayServer::Wayland)
    }

    fn post_save_recovery_hint(
        &self,
        capture: &CaptureSourcePlan,
        video_resolution: Option<VideoResolution>,
    ) -> RecoveryHint {
        let Some(video_resolution) = video_resolution else {
            return RecoveryHint::None;
        };

        if self.should_probe_saved_clip_resolution(capture)
            && resolution_looks_like_launcher_sized_portal_capture(video_resolution)
        {
            RecoveryHint::ReacquireCaptureTarget
        } else {
            RecoveryHint::None
        }
    }
}

struct GsrCaptureSession {
    child: Option<Child>,
    stdout_buffer: String,
    stderr_buffer: String,
    save_in_flight: bool,
    pending_save_length: Option<ClipLength>,
    save_directory: PathBuf,
    active_audio_layout: Vec<ResolvedAudioSource>,
}

impl CaptureSession for GsrCaptureSession {
    fn save_clip(&mut self, length: ClipLength) -> Result<(), CaptureError> {
        if self.save_in_flight {
            return Err(CaptureError::SaveInFlight);
        }

        let pid = self
            .child
            .as_ref()
            .map(|child| child.id() as i32)
            .ok_or(CaptureError::NotRunning)?;
        match length {
            ClipLength::FullBuffer => {
                signal::kill(Pid::from_raw(pid), Signal::SIGUSR1)
                    .map_err(|error| CaptureError::SignalFailed(error.to_string()))?;
            }
            ClipLength::Seconds(wanted_secs) => {
                let (signum, _) = nearest_ceiling_signal(wanted_secs);
                let ret = unsafe { libc::kill(pid, signum) };
                if ret != 0 {
                    return Err(CaptureError::SignalFailed(
                        std::io::Error::last_os_error().to_string(),
                    ));
                }
            }
        }

        self.save_in_flight = true;
        self.pending_save_length = Some(length);
        Ok(())
    }

    fn poll_results(&mut self) -> Vec<SavePollResult> {
        let mut results = Vec::new();
        self.drain_stderr();
        let lines = self.read_stdout_lines();

        for line in lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if trimmed.starts_with("gsr error:") {
                self.save_in_flight = false;
                self.pending_save_length = None;
                results.push(SavePollResult::SaveFailed(trimmed.to_string()));
                continue;
            }

            if !self.save_in_flight {
                tracing::info!("gpu-screen-recorder: {trimmed}");
                continue;
            }

            self.save_in_flight = false;
            let duration = self
                .pending_save_length
                .take()
                .unwrap_or(ClipLength::FullBuffer);
            let path = resolve_saved_path(trimmed, &self.save_directory);
            results.push(SavePollResult::Saved {
                path,
                duration,
                audio_layout: self.active_audio_layout.clone(),
            });
        }

        results
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        let Some(pid) = self.child.as_ref().map(|child| child.id() as i32) else {
            return Err(CaptureError::NotRunning);
        };

        signal::kill(Pid::from_raw(pid), Signal::SIGINT)
            .map_err(|error| CaptureError::SignalFailed(error.to_string()))?;
        let deadline = Instant::now() + STOP_TIMEOUT;
        let mut killed = false;
        loop {
            self.drain_stderr();
            let _ = self.read_stdout_lines();

            let exited = match self.child.as_mut().unwrap().try_wait() {
                Ok(Some(_)) => true,
                Ok(None) => false,
                Err(error) => {
                    tracing::warn!("try_wait on recorder failed: {error}");
                    true
                }
            };
            if exited {
                break;
            }

            if !killed && Instant::now() >= deadline {
                let _ = signal::kill(Pid::from_raw(pid), Signal::SIGKILL);
                killed = true;
            }

            std::thread::sleep(STOP_POLL_INTERVAL);
        }

        self.child = None;
        self.stdout_buffer.clear();
        self.stderr_buffer.clear();
        self.pending_save_length = None;
        self.save_in_flight = false;
        clear_layout_snapshot();
        Ok(())
    }

    fn active_audio_layout(&self) -> &[ResolvedAudioSource] {
        &self.active_audio_layout
    }

    fn is_running(&mut self) -> bool {
        if let Some(ref mut child) = self.child {
            match child.try_wait() {
                Ok(Some(_)) | Err(_) => {
                    self.child = None;
                    self.save_in_flight = false;
                    false
                }
                Ok(None) => true,
            }
        } else {
            false
        }
    }

    fn save_in_progress(&self) -> bool {
        self.save_in_flight
    }
}

impl Drop for GsrCaptureSession {
    fn drop(&mut self) {
        if self.is_running() {
            let _ = self.stop();
        }
    }
}

impl GsrCaptureSession {
    fn read_stdout_lines(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        let Some(child) = self.child.as_mut() else {
            return lines;
        };
        let Some(stdout) = child.stdout.as_mut() else {
            return lines;
        };

        let mut chunk = [0_u8; 4096];
        loop {
            match stdout.read(&mut chunk) {
                Ok(0) => break,
                Ok(size) => {
                    self.stdout_buffer
                        .push_str(&String::from_utf8_lossy(&chunk[..size]));
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => {
                    tracing::warn!("Failed reading recorder stdout: {error}");
                    break;
                }
            }
        }

        while let Some(index) = self.stdout_buffer.find('\n') {
            let line = self.stdout_buffer[..index]
                .trim_end_matches('\r')
                .to_string();
            self.stdout_buffer.drain(..=index);
            lines.push(line);
        }

        lines
    }

    fn drain_stderr(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        let Some(stderr) = child.stderr.as_mut() else {
            return;
        };

        let mut chunk = [0_u8; 4096];
        loop {
            match stderr.read(&mut chunk) {
                Ok(0) => break,
                Ok(size) => {
                    self.stderr_buffer
                        .push_str(&String::from_utf8_lossy(&chunk[..size]));
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => {
                    tracing::warn!("Failed reading recorder stderr: {error}");
                    break;
                }
            }
        }

        while let Some(index) = self.stderr_buffer.find('\n') {
            let line = self.stderr_buffer[..index]
                .trim_end_matches('\r')
                .to_string();
            self.stderr_buffer.drain(..=index);
            if !line.is_empty() {
                tracing::debug!("gpu-screen-recorder[stderr]: {line}");
            }
        }
    }
}

fn list_audio_devices() -> Result<Vec<DiscoveredAudioSource>, AudioSourceError> {
    let output = Command::new("gpu-screen-recorder")
        .arg("--list-audio-devices")
        .output()
        .map_err(map_spawn_error)?;
    parse_audio_discovery_output(output, DiscoveredAudioKind::Device)
}

fn list_application_audio() -> Result<Vec<DiscoveredAudioSource>, AudioSourceError> {
    let native_apps = list_application_audio_via_pipewire()
        .map_err(|error| {
            tracing::debug!("native PipeWire application discovery failed: {error}");
            error
        })
        .ok();

    let gsr_apps = list_application_audio_via_gsr()?;

    Ok(match native_apps {
        Some(native_apps) => merge_gsr_and_native_application_audio(gsr_apps, native_apps),
        None => gsr_apps,
    })
}

fn list_application_audio_via_gsr() -> Result<Vec<DiscoveredAudioSource>, AudioSourceError> {
    let output = Command::new("gpu-screen-recorder")
        .arg("--list-application-audio")
        .output()
        .map_err(map_spawn_error)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let reason = if stderr.is_empty() {
            format!("exit status {}", output.status)
        } else {
            stderr
        };
        return Err(AudioSourceError::PerAppUnavailable {
            reason,
            partial: Vec::new(),
        });
    }

    parse_audio_discovery_output(output, DiscoveredAudioKind::Application)
}

fn list_application_audio_via_pipewire() -> Result<Vec<DiscoveredAudioSource>, String> {
    pw::init();
    (|| {
        let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(|error| error.to_string())?;
        let context =
            pw::context::ContextRc::new(&mainloop, None).map_err(|error| error.to_string())?;
        let core = context
            .connect_rc(None)
            .map_err(|error| error.to_string())?;
        let registry = core
            .get_registry()
            .map_err(|error| format!("failed to get PipeWire registry: {error}"))?;

        let done = Rc::new(Cell::new(false));
        let app_nodes = Rc::new(RefCell::new(BTreeMap::<String, String>::new()));

        let done_clone = done.clone();
        let loop_clone = mainloop.clone();
        let pending = core.sync(0).map_err(|error| error.to_string())?;

        let _core_listener = core
            .add_listener_local()
            .done(move |id, seq| {
                if id == pw::core::PW_ID_CORE && seq == pending {
                    done_clone.set(true);
                    loop_clone.quit();
                }
            })
            .error({
                let done = done.clone();
                let loop_clone = mainloop.clone();
                move |id, seq, res, message| {
                    tracing::debug!(
                        "PipeWire discovery core error id={id} seq={seq} res={res}: {message}"
                    );
                    done.set(true);
                    loop_clone.quit();
                }
            })
            .register();

        let _registry_listener = registry
            .add_listener_local()
            .global({
                let app_nodes = app_nodes.clone();
                move |global| {
                    if global.type_ != pw::types::ObjectType::Node {
                        return;
                    }

                    let Some(props) = global.props.as_ref().map(|props| props.as_ref()) else {
                        return;
                    };

                    if props.get(*pw::keys::MEDIA_CLASS) != Some("Stream/Output/Audio") {
                        return;
                    }

                    let Some(node_name) = props.get(*pw::keys::NODE_NAME).map(str::trim) else {
                        return;
                    };
                    if node_name.is_empty() {
                        return;
                    }

                    let display_label = preferred_pipewire_application_label(props, node_name);
                    app_nodes
                        .borrow_mut()
                        .entry(node_name.to_string())
                        .or_insert(display_label);
                }
            })
            .register();

        while !done.get() {
            mainloop.run();
        }

        Ok(finalize_pipewire_application_sources(
            app_nodes.borrow().clone(),
        ))
    })()
}

fn preferred_pipewire_application_label(
    props: &pw::spa::utils::dict::DictRef,
    node_name: &str,
) -> String {
    [
        props.get(*pw::keys::APP_NAME),
        props.get(*pw::keys::NODE_DESCRIPTION),
        props.get(*pw::keys::MEDIA_NAME),
        props.get(*pw::keys::NODE_NICK),
        props.get(*pw::keys::CLIENT_NAME),
        props.get(*pw::keys::APP_PROCESS_BINARY),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|value| !value.is_empty())
    .unwrap_or(node_name)
    .to_string()
}

fn finalize_pipewire_application_sources(
    app_nodes: BTreeMap<String, String>,
) -> Vec<DiscoveredAudioSource> {
    let duplicate_counts =
        app_nodes
            .values()
            .fold(BTreeMap::<String, usize>::new(), |mut counts, label| {
                *counts.entry(label.clone()).or_default() += 1;
                counts
            });

    app_nodes
        .into_iter()
        .map(|(node_name, display_label)| {
            let display_label = if duplicate_counts.get(&display_label).copied().unwrap_or(0) > 1 {
                format!("{display_label} ({node_name})")
            } else {
                display_label
            };
            DiscoveredAudioSource {
                kind_hint: AudioSourceKind::Application { name: node_name },
                display_label,
                kind: DiscoveredAudioKind::Application,
                available: true,
            }
        })
        .collect()
}

fn merge_gsr_and_native_application_audio(
    mut gsr_apps: Vec<DiscoveredAudioSource>,
    native_apps: Vec<DiscoveredAudioSource>,
) -> Vec<DiscoveredAudioSource> {
    let native_labels = native_apps
        .into_iter()
        .map(|source| {
            (
                source.kind_hint.config_display_value(),
                source.display_label,
            )
        })
        .collect::<BTreeMap<_, _>>();

    for gsr_app in &mut gsr_apps {
        if let Some(display_label) =
            native_labels.get(gsr_app.kind_hint.config_display_value().as_str())
        {
            gsr_app.display_label = display_label.clone();
        }
    }

    gsr_apps
}

fn parse_audio_discovery_output(
    output: std::process::Output,
    kind: DiscoveredAudioKind,
) -> Result<Vec<DiscoveredAudioSource>, AudioSourceError> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            format!("exit status {}", output.status)
        } else {
            stderr
        };
        return Err(AudioSourceError::DiscoveryFailed(detail));
    }

    let mut discovered = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let (source, description) = match kind {
            DiscoveredAudioKind::Device => {
                let Some((source, description)) = line.split_once('|') else {
                    continue;
                };
                (source.trim(), description.trim())
            }
            // `gpu-screen-recorder --list-application-audio` emits plain app names,
            // one per line, not the `source|label` form used by device discovery.
            DiscoveredAudioKind::Application => {
                let source = line.trim();
                (source, source)
            }
        };
        let source = source.trim();
        if source.is_empty() {
            continue;
        }

        let kind_hint = match kind {
            DiscoveredAudioKind::Device => AudioSourceKind::Raw {
                backend_id: "gsr".into(),
                value: source.to_string(),
            },
            DiscoveredAudioKind::Application => AudioSourceKind::Application {
                name: source.to_string(),
            },
        };
        discovered.push(DiscoveredAudioSource {
            kind_hint,
            display_label: if description.is_empty() {
                source.to_string()
            } else {
                description.to_string()
            },
            kind,
            available: true,
        });
    }

    Ok(discovered)
}

fn merge_discovered_audio(
    devices: Vec<DiscoveredAudioSource>,
    apps: Vec<DiscoveredAudioSource>,
) -> Vec<DiscoveredAudioSource> {
    let mut combined = devices;
    for app in apps {
        if combined.iter().any(|existing| existing == &app) {
            continue;
        }
        combined.push(app);
    }
    combined
}

fn gsr_capture_target_arg(capture: &CaptureSourcePlan) -> Result<String, CaptureError> {
    match &capture.target {
        CaptureTarget::X11Window(window) => Ok(window.to_string()),
        CaptureTarget::WaylandPortal => Ok("portal".into()),
        CaptureTarget::Monitor(name) => Ok(name.clone()),
        CaptureTarget::BackendOwned => Err(CaptureError::Unsupported {
            capability: "backend-owned capture target".into(),
        }),
    }
}

fn translate_kind_to_gsr(
    kind: &AudioSourceKind,
    backend_id: &str,
) -> Result<String, AudioSourceError> {
    match kind {
        AudioSourceKind::DefaultOutput => Ok("default_output".into()),
        AudioSourceKind::DefaultInput => Ok("default_input".into()),
        AudioSourceKind::Device { name } => Ok(format!("device:{}", name.trim())),
        AudioSourceKind::Application { name } => Ok(format!("app:{}", name.trim())),
        AudioSourceKind::ApplicationInverse { names } => {
            if names.len() != 1 {
                return Err(AudioSourceError::Unsupported {
                    capability: "multi-name application inverse".into(),
                });
            }
            Ok(format!("app-inverse:{}", names[0].trim()))
        }
        AudioSourceKind::Merged { entries } => {
            let contains_app = entries
                .iter()
                .any(|entry| matches!(entry, AudioSourceKind::Application { .. }));
            let contains_inverse = entries
                .iter()
                .any(|entry| matches!(entry, AudioSourceKind::ApplicationInverse { .. }));
            if contains_app && contains_inverse {
                return Err(AudioSourceError::Unsupported {
                    capability: "merged tracks cannot combine app and app-inverse sources".into(),
                });
            }
            let resolved = entries
                .iter()
                .map(|entry| translate_kind_to_gsr(entry, backend_id))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(resolved.join("|"))
        }
        AudioSourceKind::Raw {
            backend_id: raw_backend,
            value,
        } => {
            if raw_backend != backend_id {
                return Err(AudioSourceError::WrongBackend {
                    expected: backend_id.into(),
                    actual: raw_backend.clone(),
                });
            }
            Ok(value.trim().to_string())
        }
    }
}

fn map_spawn_error(error: std::io::Error) -> AudioSourceError {
    if error.kind() == std::io::ErrorKind::NotFound {
        AudioSourceError::RecorderNotFound
    } else {
        AudioSourceError::DiscoveryFailed(error.to_string())
    }
}

fn resolution_looks_like_launcher_sized_portal_capture(resolution: VideoResolution) -> bool {
    resolution.width < 1280 || resolution.height < 720
}

fn resolve_saved_path(raw_path: &str, save_directory: &Path) -> PathBuf {
    let trimmed = raw_path.trim();
    if let Some(stripped) = trimmed.strip_prefix("~/") {
        if let Some(home) = directories::UserDirs::new().map(|dirs| dirs.home_dir().to_path_buf()) {
            return home.join(stripped);
        }
    }

    let path = PathBuf::from(trimmed);
    if path.is_absolute() {
        path
    } else {
        save_directory.join(path)
    }
}

fn nearest_ceiling_signal(wanted_secs: u32) -> (i32, u32) {
    const PRESETS: &[(i32, u32)] = &[(1, 10), (2, 30), (3, 60), (4, 300), (5, 600), (6, 1800)];

    let (offset, preset_secs) = PRESETS
        .iter()
        .copied()
        .find(|(_, preset_secs)| *preset_secs >= wanted_secs)
        .unwrap_or((libc::SIGUSR1, 1800));

    if offset == libc::SIGUSR1 {
        (libc::SIGUSR1, preset_secs)
    } else {
        (libc::SIGRTMIN() + offset, preset_secs)
    }
}

fn set_nonblocking(fd: i32) -> Result<(), CaptureError> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(CaptureError::Io(
            std::io::Error::last_os_error().to_string(),
        ));
    }

    let ret = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
    if ret < 0 {
        return Err(CaptureError::Io(
            std::io::Error::last_os_error().to_string(),
        ));
    }

    Ok(())
}

fn portal_session_token_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "nanite-clip")
        .map(|dirs| dirs.config_dir().join("gpu-screen-recorder-portal-token"))
}

/// This file belongs to the GSR backend only. Other backends should not read it
/// and should not assume it exists.
fn gsr_audio_layout_snapshot_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "nanite-clip")
        .map(|dirs| dirs.config_dir().join("gsr-active-audio-layout.json"))
}

fn write_layout_snapshot(layout: &[ResolvedAudioSource]) {
    let Some(path) = gsr_audio_layout_snapshot_path() else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let _ = std::fs::write(
        path,
        serde_json::to_vec_pretty(layout).unwrap_or_else(|_| b"[]".to_vec()),
    );
}

fn clear_layout_snapshot() {
    if let Some(path) = gsr_audio_layout_snapshot_path() {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    fn output_with_stdout(stdout: &str) -> std::process::Output {
        std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn translates_typed_audio_sources_to_gsr_args() {
        assert_eq!(
            translate_kind_to_gsr(&AudioSourceKind::DefaultOutput, "gsr").unwrap(),
            "default_output"
        );
        assert_eq!(
            translate_kind_to_gsr(
                &AudioSourceKind::Device {
                    name: "alsa_output.monitor".into(),
                },
                "gsr",
            )
            .unwrap(),
            "device:alsa_output.monitor"
        );
        assert_eq!(
            translate_kind_to_gsr(
                &AudioSourceKind::Application {
                    name: "PlanetSide2".into(),
                },
                "gsr",
            )
            .unwrap(),
            "app:PlanetSide2"
        );
    }

    #[test]
    fn rejects_multi_name_application_inverse_for_gsr() {
        let error = translate_kind_to_gsr(
            &AudioSourceKind::ApplicationInverse {
                names: vec!["A".into(), "B".into()],
            },
            "gsr",
        )
        .unwrap_err();

        assert!(matches!(error, AudioSourceError::Unsupported { .. }));
    }

    #[test]
    fn rejects_wrong_backend_raw_audio_source() {
        let error = translate_kind_to_gsr(
            &AudioSourceKind::Raw {
                backend_id: "wasapi".into(),
                value: "session:123".into(),
            },
            "gsr",
        )
        .unwrap_err();

        assert!(matches!(error, AudioSourceError::WrongBackend { .. }));
    }

    #[test]
    fn parses_application_audio_plain_line_output() {
        let discovered = parse_audio_discovery_output(
            output_with_stdout("Discord\nFirefox\n"),
            DiscoveredAudioKind::Application,
        )
        .unwrap();

        assert_eq!(discovered.len(), 2);
        assert_eq!(
            discovered[0].kind_hint,
            AudioSourceKind::Application {
                name: "Discord".into()
            }
        );
        assert_eq!(discovered[0].display_label, "Discord");
        assert_eq!(
            discovered[1].kind_hint,
            AudioSourceKind::Application {
                name: "Firefox".into()
            }
        );
    }

    #[test]
    fn ignores_malformed_device_audio_output_lines() {
        let discovered = parse_audio_discovery_output(
            output_with_stdout("default_output|Default output\nbad-line\n"),
            DiscoveredAudioKind::Device,
        )
        .unwrap();

        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].display_label, "Default output");
    }

    #[test]
    fn merge_prefers_native_pipewire_labels_for_matching_gsr_apps() {
        let merged = merge_gsr_and_native_application_audio(
            vec![DiscoveredAudioSource {
                kind_hint: AudioSourceKind::Application {
                    name: "audio-src".into(),
                },
                display_label: "audio-src".into(),
                kind: DiscoveredAudioKind::Application,
                available: true,
            }],
            vec![DiscoveredAudioSource {
                kind_hint: AudioSourceKind::Application {
                    name: "audio-src".into(),
                },
                display_label: "Spotify".into(),
                kind: DiscoveredAudioKind::Application,
                available: true,
            }],
        );

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].display_label, "Spotify");
        assert_eq!(
            merged[0].kind_hint,
            AudioSourceKind::Application {
                name: "audio-src".into()
            }
        );
    }

    #[test]
    fn finalize_pipewire_application_sources_disambiguates_duplicate_labels() {
        let discovered = finalize_pipewire_application_sources(BTreeMap::from([
            ("audio-src".into(), "Spotify".into()),
            ("audio-src-2".into(), "Spotify".into()),
        ]));

        assert_eq!(discovered.len(), 2);
        assert_eq!(discovered[0].display_label, "Spotify (audio-src)");
        assert_eq!(discovered[1].display_label, "Spotify (audio-src-2)");
    }
}
