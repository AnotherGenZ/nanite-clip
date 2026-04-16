# Wave 6: Recorder Backend Abstraction & Cross-Platform Capture (INF-02 + PLAT-01)

## Scope & design goals

Wave 6 closes out two FUTURE.md items:

- **INF-02 — Recorder Backend Abstraction:** finish factoring `gpu-screen-recorder` out of the recorder layer so additional backends drop in cleanly.
- **PLAT-01 — OBS / Windows Platform Support:** ship a Windows build of NaniteClip that uses OBS Studio (via obs-websocket v5) as its capture backend, while preserving the existing Linux/GSR experience byte-for-byte.

The end state: a NaniteClip user on Windows installs the app, points it at a running OBS instance with a configured replay buffer, and gets the same monitoring → rule → save → post-process → distribute workflow as the Linux user. A NaniteClip user on Linux who happens to prefer OBS over GSR can also pick the OBS backend.

**Cross-platform readiness target.** After Wave 6, `src/app.rs` contains zero `#[cfg(target_os)]` arms and zero direct calls to `process::find_ps2_pid` or `process::detect_display_server` outside the hotkey configuration path. Every platform-specific concern lives behind one of three trait boundaries: `CaptureBackend`, `CaptureSession`, `GameProcessWatcher`.

**Non-goals for Wave 6.**

- Shipping macOS support. (The `#[cfg]` hooks should not actively block it, but no macOS backend is implemented.)
- Rewriting OBS or shipping an OBS plugin. Wave 6 communicates exclusively over obs-websocket v5.
- A working `ObsManagementMode::FullManagement` implementation. The mode lands as an enum option and trait-level branch, but the actual code path is a follow-up. Wave 6 ships `BringYourOwn` and `ManagedRecording`.
- Per-application audio capture on Windows. `wasapi_process_output_capture` is an OBS source kind that requires Windows 10 2004+, and routing it from NaniteClip's `audio_sources` list is a `FullManagement` concern.
- Trimming OBS replay-buffer saves to a requested `ClipLength::Seconds(n)`. v1 returns `Unsupported` for non-`FullBuffer` requests on the OBS backend; trim-via-ffmpeg is a follow-up.

---

## Locked decisions

These are settled. Reopen only if implementation proves one impossible.

- **D1: OBS transport is obs-websocket v5.** Use the `obws` crate (most-maintained Rust client). Minimum supported OBS Studio version is **28.0** (the version that bundles obs-websocket 5 by default). Older OBS gets a clear connection-refused error message.
- **D2: Audio ownership depends on management mode.** Under `BringYourOwn` and `ManagedRecording`, OBS owns audio and NaniteClip's audio panel is hidden. Under `FullManagement`, NaniteClip mirrors its `audio_sources` list onto OBS inputs (deferred out of Wave 6).
- **D3: Capture target ownership depends on management mode.** Same split. Under `BringYourOwn` and `ManagedRecording`, the user creates their scene and capture source in OBS themselves; NaniteClip's `resolve_capture_target` returns `CaptureTarget::BackendOwned`. Windows process detection only confirms PS2 is running — it does not enumerate windows.
- **D4: OBS credentials live in the secure store.** The websocket URL is in `BackendConfigs.obs`; the password is stored under a new secure-store entry `obs_websocket_password` and never serialized to `config.toml`.
- **D5: Restart semantics.** Most OBS recording-output settings only take effect after `StopReplayBuffer` → `SetProfileParameter` → `StartReplayBuffer`. `ObsCaptureSession` exposes a `restart_for_config_change()` helper used whenever a tracked profile parameter changes mid-session. There is no portal-style recovery probe for OBS.
- **D6: Windows build infrastructure.** `auraxis-rs` will be published to crates.io before or alongside Wave 6. Once published, `nanite-clip`'s `Cargo.toml` switches from `path = "../auraxis-rs/auraxis"` to a regular crates.io dependency. CI runs on `windows-latest` with a single checkout, installs the nightly toolchain from `rust-toolchain.toml`, and runs `cargo build --release --target x86_64-pc-windows-msvc`. Artifact upload is in scope for Wave 6. **Order-of-operations note:** if `auraxis-rs` has not yet shipped to crates.io when PR 1 lands, CI uses a sibling-checkout interim (see Phase 7.1 fallback) and the dep swap happens in a follow-up PR.
- **D7: Three OBS management modes.** `BringYourOwn`, `ManagedRecording` (default), `FullManagement`. Wave 6 ships v1 and v2; v3 is a documented follow-up.
- **D8: SimpleOutput only for parameter management.** OBS Advanced Output mode stores encoder settings in JSON sidecar files (`recordEncoder.json`) that obs-websocket does not expose. NaniteClip's `ManagedRecording` and `FullManagement` modes write only to `SimpleOutput.*` parameter keys. If the user has Advanced Output configured, NaniteClip warns and either declines to push parameters or asks the user to switch to SimpleOutput.
- **D9: Tray uses a per-environment dispatch.** `ksni` is the best-of-breed implementation on KDE Plasma (it speaks the freedesktop StatusNotifierItem protocol natively and avoids the libappindicator quirks that bite the cross-platform crates on KDE). Everywhere else — non-KDE Linux desktops, Windows, and future macOS — Wave 6 uses [`tray-icon`](https://crates.io/crates/tray-icon) (the Tauri-maintained cross-platform crate). Selection happens at runtime in `App::new` based on `target_os` and `detect_desktop_environment()`. Wave 6 ships Windows with full tray support — there is no "tray gap."
- **D10: Linux/GSR behavior is byte-identical.** Every refactor in INF-02 includes a regression test (or fixture) that asserts the recorder produces the same `gpu-screen-recorder` command line and the same poll-result shape as before the change. If a refactor would change a single argv flag, it's wrong.

---

## Current state assessment

The trait scaffolding for INF-02 already exists. Wave 6 does **not** start from zero.

**What's done:**

- `src/capture/mod.rs:67-93` defines `CaptureBackend` and `CaptureSession` traits with `id`, `display_name`, `capabilities`, `discover_audio_sources`, `translate_audio_source`, `spawn_replay`, plus session lifecycle (`save_clip`, `poll_results`, `stop`, `is_running`, `save_in_progress`, `active_audio_layout`).
- `src/capture/gsr.rs` implements `GsrBackend` and `GsrCaptureSession` behind the trait.
- `src/recorder.rs:14-20` holds `Arc<dyn CaptureBackend>` and `Box<dyn CaptureSession>`, and `app.rs` already talks to it through backend-neutral methods.
- `src/config.rs:77-80` has `CaptureConfig { backend: String }` with normalization, and `AudioSourceKind::Raw { backend_id, value }` carries backend tagging through audio configs.
- `src/app/tabs/settings.rs:770` already calls `recorder.backend_handle().discover_audio_sources()` instead of going around the recorder.

**Known leaks that Wave 6 will close:**

- `src/recorder.rs:180-185` `create_backend` ignores the configured `backend` value and always returns `GsrBackend`.
- `src/process.rs:392-396` `CaptureSourcePlan` has `restore_portal_session: bool` and `display_server: DisplayServer` fields that only the GSR backend understands.
- `src/capture/mod.rs:39-43` `BackendAudioArg::Opaque(Box<dyn Any>)` is dead — nothing reads it.
- `RecorderConfig` (`src/config.rs:58-74`) mixes general fields with GSR-specific knobs (`framerate`, `codec`, `quality`, `container`).
- `src/app.rs:960`, `src/app.rs:979`, `src/app.rs:1896` call `process::find_ps2_pid()` and `process::resolve_capture_source(...)` directly — these are Linux/`/proc`/X11 paths.
- `src/app.rs:3167-3172` reads `process::detect_display_server()` to decide whether to run the portal recovery probe.
- `src/process.rs` (555 lines) mixes X11 window search, /proc walking, dead audio discovery, and env detection in one file.
- The `Recorder::capabilities()` accessor exists but the Settings UI does not currently gate any of its widgets on it.
- No tests exercise the `CaptureBackend` trait through a fake — only the GSR backend's own unit tests.

INF-02 is roughly 60% structurally done. The remaining work is closing the leaks above, making backend selection actually take effect, and adding test seams.

---

# INF-02 — Recorder Backend Abstraction

## Phase 1 — Plug the leaks in the trait surface

### 1.1 Make backend selection real

`src/recorder.rs:180-185` becomes:

```rust
fn create_backend(capture: &CaptureConfig) -> Arc<dyn CaptureBackend> {
    match capture.backend.as_str() {
        "gsr" => Arc::new(GsrBackend::new()),
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        "obs" => Arc::new(ObsBackend::new(capture.backends.obs.clone())),
        other => {
            tracing::warn!(
                "Unknown capture backend '{other}', falling back to default for this platform"
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
    Arc::new(ObsBackend::new(ObsBackendConfig::default()))
}
```

The factory is the single seam every new backend hooks into. Add a unit test that asserts unknown backend ids fall back without panicking.

### 1.2 Decouple `CaptureSourcePlan` from Linux concepts

`src/process.rs:392-396` currently leaks GSR/Linux-specific concepts into a struct that crosses the trait boundary. Refactor:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureSourcePlan {
    pub target: CaptureTarget,
    pub backend_hints: BackendHints,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureTarget {
    /// A specific X11 window (for GSR's `-w <window_id>` mode).
    X11Window(u32),
    /// Wayland desktop portal capture (GSR's `-w portal` mode).
    WaylandPortal,
    /// A named monitor or output (e.g., DP-1).
    Monitor(String),
    /// The capture target is configured inside the backend itself (OBS scenes).
    BackendOwned,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BackendHints {
    pub display_server: Option<DisplayServer>,
    pub restore_portal_session: bool,
}
```

The GSR backend pattern-matches on `CaptureTarget` and rejects variants it doesn't understand with `CaptureError::Unsupported`. The OBS backend ignores everything except `BackendOwned`. `BackendHints` exists as a small typed extension struct so we don't introduce another `Box<dyn Any>`; if a future backend needs richer hints, they get a new field with a sensible default.

### 1.3 Replace `BackendAudioArg::Opaque`

`src/capture/mod.rs:39-43` declares `BackendAudioArg::Opaque(Box<dyn Any + Send + Sync>)` that nothing reads. Replace the trait method with:

```rust
pub trait CaptureBackend: Send + Sync {
    // ...
    fn validate_audio_source(&self, kind: &AudioSourceKind) -> Result<(), AudioSourceError>;
}
```

Delete `BackendAudioArg` entirely. The GSR backend implements `validate_audio_source` by calling its existing `translate_kind_to_gsr` and discarding the return. The OBS backend implements it as `Err(AudioSourceError::Unsupported { capability: "OBS owns audio routing".into() })` for the `BringYourOwn` / `ManagedRecording` modes.

### 1.4 Move backend-specific recorder fields out of root config

`src/config.rs:58-74` `RecorderConfig` mixes general fields with GSR-specific knobs. Restructure:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct RecorderConfig {
    pub replay_buffer_secs: u32,
    pub save_directory: PathBuf,
    pub save_delay_secs: u32,
    pub clip_saved_notifications: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audio_sources: Vec<AudioSourceConfig>,
    #[serde(default)]
    pub post_processing: PostProcessingConfig,
    pub backends: BackendConfigs,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BackendConfigs {
    #[serde(default)]
    pub gsr: GsrBackendConfig,
    #[serde(default)]
    pub obs: ObsBackendConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GsrBackendConfig {
    pub capture_source: String,
    pub framerate: u32,
    pub codec: String,
    pub container: String,
    pub quality: String,
}
```

The active backend (`CaptureConfig.backend`) decides which sub-struct is read at spawn time. **Migration is mandatory** — the existing custom `Deserialize` impl at `src/config.rs:1052` needs an additional pre-pass that reads flat `framerate`/`codec`/`quality`/`container`/`capture_source` keys and folds them into `backends.gsr`. Add a checked-in fixture `tests/fixtures/config_pre_inf02.toml` (a real serialized pre-Wave-6 config) and a test that loads it and asserts the post-migration shape.

### 1.5 Stop touching `process::detect_display_server` from app.rs for non-hotkey paths

`src/app.rs:3167-3172` uses display-server detection to decide "should we run the portal recovery probe?". This is GSR-portal-specific. Move it behind a backend method:

```rust
pub trait CaptureBackend: Send + Sync {
    // ...
    /// Hint to the recorder layer about what to do after a save completes.
    /// Backends that do not need post-save recovery (OBS, future Windows backends)
    /// should return `RecoveryHint::None`.
    fn post_save_recovery_hint(
        &self,
        video_resolution: Option<VideoResolution>,
    ) -> RecoveryHint {
        RecoveryHint::None
    }
}

pub enum RecoveryHint {
    None,
    /// The capture target needs to be torn down and re-acquired before the
    /// next save. Used by GSR's Wayland portal path when a launcher-sized
    /// resolution is detected.
    ReacquireCaptureTarget,
}
```

`app.rs` consumes the hint via `self.recorder.backend_handle().post_save_recovery_hint(resolution)` and runs the existing portal-reset flow only when `ReacquireCaptureTarget` is returned. The hotkey path at `app.rs:4442-4446` legitimately needs `DisplayServer` (it's about input, not capture) and stays as-is.

### Phase 1 done criteria

- `cargo test` passes including the new fixture-based config migration test.
- `app.rs` references `process::detect_display_server` only in hotkey configuration code paths.
- `RecorderConfig` no longer contains backend-specific fields directly.
- A diff of the `gpu-screen-recorder` argv produced by `Recorder::start_replay` against the pre-Wave-6 baseline shows zero changes (per D10).

---

## Phase 2 — Decouple process detection

### 2.1 Define a `GameProcessWatcher` trait

In `src/process/mod.rs`:

```rust
pub trait GameProcessWatcher: Send + Sync {
    /// Look for a running PlanetSide 2 process. Returns the PID if found.
    fn find_running_pid(&self) -> Option<u32>;

    /// Check whether a previously-known PID is still alive.
    fn is_running(&self, pid: u32) -> bool;

    /// Resolve a capture target for the running process. The returned target
    /// is consumed by the active capture backend; backends that own their
    /// own capture target should return [`CaptureTarget::BackendOwned`].
    fn resolve_capture_target(
        &self,
        pid: u32,
        configured_source: &str,
    ) -> Result<CaptureTarget, CaptureTargetError>;
}
```

Implementations:
- `LinuxProcfsWatcher` — wraps the existing `find_ps2_pid`, `is_process_running`, and X11 window resolution code. Gated `#[cfg(target_os = "linux")]`.
- `WindowsToolhelpWatcher` — implemented in PLAT-01 Phase 4. Gated `#[cfg(target_os = "windows")]`.

`App` holds an `Arc<dyn GameProcessWatcher>` constructed once at startup and stored in the existing `App` struct. The platform-conditional construction lives in `App::new` (or `main.rs` if cleaner).

### 2.2 Split `src/process.rs` into a folder

Restructure the 555-line `src/process.rs` into:

```
src/process/
├── mod.rs        # public traits, re-exports, shared types
├── linux.rs      # LinuxProcfsWatcher, find_ps2_pid, X11 window resolution,
│                 # detect_display_server, detect_desktop_environment.
│                 # Gated #[cfg(target_os = "linux")].
├── windows.rs    # WindowsToolhelpWatcher.
│                 # Gated #[cfg(target_os = "windows")].
└── env.rs        # Cross-platform helpers (any leftovers that don't fit
                  # linux.rs or windows.rs — likely empty in v1).
```

**Delete dead code while restructuring.** `src/process.rs:122-181` defines `discover_audio_sources_blocking` and `parse_audio_source_discovery` that duplicate the working logic in `src/capture/gsr.rs`. Confirm no caller uses `process::discover_audio_sources` (`grep` already shows none) and remove. The associated `AudioSourceError` and `DiscoveredAudioSource` definitions in `process.rs:399-424` should also go — `src/capture/mod.rs` is the canonical home for these.

### 2.3 Update `app.rs` call sites

Three call sites change:

- `src/app.rs:960` — `process::find_ps2_pid()` becomes `self.process_watcher.find_running_pid()`.
- `src/app.rs:979` — same.
- `src/app.rs:1896-1905` — `process::resolve_capture_source(&self.config.recorder.capture_source, ps2_pid)` becomes `self.process_watcher.resolve_capture_target(ps2_pid, &self.config.recorder.backends.gsr.capture_source)`.

`process::is_process_running` callers (if any beyond the watcher itself) move to `self.process_watcher.is_running(pid)`.

### 2.4 Move audio_layout snapshot helper

`src/capture/gsr.rs:789-814` writes `gsr-active-audio-layout.json` to the project config dir. It's only read from inside the same file, so it stays in `gsr.rs`. Add a doc comment that other backends should not read this file and should not assume it exists. Rename `audio_layout_snapshot_path()` to `gsr_audio_layout_snapshot_path()` to make ownership explicit.

### Phase 2 done criteria

- `src/process.rs` no longer exists; `src/process/` folder replaces it.
- `app.rs` contains zero direct references to `process::find_ps2_pid` or `process::resolve_capture_source`.
- `cargo build --target x86_64-pc-windows-msvc` (after PLAT-01 Phase 2 cfg-gating) compiles `src/process/windows.rs` and excludes `src/process/linux.rs`.
- Linux build still passes and recorder argv is unchanged.

---

## Phase 3 — Test seams

### 3.1 Add a `MockBackend`

Under `#[cfg(test)]` in `src/capture/mod.rs`, add:

```rust
#[cfg(test)]
pub struct MockBackend {
    pub capabilities: CaptureCapabilities,
    pub spawn_calls: std::sync::Mutex<Vec<CaptureRequest>>,
    pub queued_results: std::sync::Mutex<Vec<SavePollResult>>,
    pub force_spawn_error: std::sync::Mutex<Option<CaptureError>>,
}

#[cfg(test)]
pub struct MockSession {
    is_running: bool,
    save_in_flight: bool,
    pending_results: Vec<SavePollResult>,
    save_calls: Vec<ClipLength>,
}
```

`MockBackend` should:
- Track every `spawn_replay` call.
- Let tests inject `SavePollResult::Saved` or `SaveFailed` by pushing into a queue that `MockSession::poll_results` drains.
- Allow simulating `is_running()` transitions (process-exit, save-in-flight clear).
- Allow forcing `spawn_replay` to error for negative-path coverage.

### 3.2 Add backend-independent tests in `src/recorder.rs`

Cover at minimum:

- **Happy path:** `start_replay` → `save_clip(FullBuffer)` → `poll_save_results` returns the queued `Saved` result with the correct duration.
- **Save-in-flight rejection:** `save_clip` while `save_in_progress` returns `CaptureError::SaveInFlight`.
- **Backend exit handling:** when `MockSession::is_running` flips to `false`, `Recorder::is_running` clears the session and returns `false`.
- **Backend cannot be swapped mid-session:** call `Recorder::update_config` with a different backend id while a session is active, assert the active backend is unchanged. (The existing logic at `src/recorder.rs:44-46` enforces this; the test pins it.)
- **Spawn-failure path:** `start_replay` returns `CaptureError::SpawnFailed` propagates without leaving a stale session.

### 3.3 Config migration regression test

Create `tests/fixtures/config_pre_inf02.toml` with a real serialized pre-Wave-6 config (flat `framerate`, `codec`, `container`, `quality`, `capture_source` at the recorder level). Add a test in `src/config.rs` that loads the fixture and asserts:

- `RecorderConfig.backends.gsr.framerate == 60` (or whatever the fixture has).
- `RecorderConfig.backends.gsr.capture_source == "planetside2"`.
- The flat keys are no longer addressable directly.
- Re-serializing and re-loading is idempotent.

### Phase 3 done criteria

- `cargo test -p nanite-clip` covers the `MockBackend` paths.
- A new backend can be added by implementing `CaptureBackend`, registering it in `create_backend`, and adding a sub-struct to `BackendConfigs` — with no edits to `app.rs` required.
- The fixture-based migration test loads the pre-Wave-6 config without errors.

---

# PLAT-01 — OBS / Windows Platform Support

Hard dependency: INF-02 Phases 1 and 2 must land first.

## Phase 1 — Architecture decisions (already locked)

See the locked decisions table above (D1–D10). The most important one for implementers to internalize is **D7: three OBS management modes**, restated here for context:

- **`BringYourOwn`** — NaniteClip touches nothing in OBS. Connect, `GetReplayBufferStatus`, `SaveReplayBuffer`. The first-ship safety net for users with strong OBS opinions.
- **`ManagedRecording`** (default) — NaniteClip pushes recording-output knobs (output dir, container, replay length, replay enabled) via `SetProfileParameter` against the currently active OBS profile. The user still creates their own scene, capture source, and audio routing in OBS.
- **`FullManagement`** — NaniteClip creates and owns a dedicated `nanite-clip` profile and scene collection in OBS, pushes every setting including encoder, audio routing, and the capture source. **Deferred out of Wave 6.** The mode lands as an enum option and trait branch, but the actual code path is gated behind an "experimental" feature flag and returns `CaptureError::Unsupported` at runtime.

Wave 6 ships **`BringYourOwn` and `ManagedRecording` only**.

## Phase 2 — Cross-platform compile gates

### 2.1 Audit Linux-only deps in `Cargo.toml`

Current `[dependencies]` mixes cross-platform and Linux-only crates. Move the Linux-only ones:

```toml
[dependencies]
# ... existing cross-platform deps ...
tray-icon = "0.x"           # cross-platform tray; used everywhere except KDE Plasma
obws = "0.x"                # OBS WebSocket client

[target.'cfg(target_os = "linux")'.dependencies]
nix = { version = "0.31.2", features = ["signal", "process"] }
x11rb = "0.13"
ashpd = { version = "0.13", default-features = false, features = ["tokio", "global_shortcuts"] }
ksni = { version = "0.3", features = ["blocking"] }
pipewire = "0.9"

[target.'cfg(target_os = "windows")'.dependencies]
windows = { version = "0.59", features = [
    "Win32_Foundation",
    "Win32_System_Threading",
    "Win32_System_Diagnostics_ToolHelp",
    "Win32_Security",
] }
windows-credentials = "0.x"  # for secure_store DPAPI fallback if needed
```

`tray-icon` and `obws` are cross-platform and live in the shared `[dependencies]` block. `ksni` stays Linux-only because it's only used by the KDE-specific tray backend.

`libc` stays in shared `[dependencies]` because the GSR backend uses it for signal numbers and the cross-platform paths do too.

`global-hotkey` v0.7 already supports Windows natively; no change needed there.

### 2.2 Gate Linux-only modules

Audit and gate every module that pulls in Linux-only deps:

| Module | Linux-only? | Action |
|---|---|---|
| `src/capture/gsr.rs` | Yes | Gate the entire file `#[cfg(target_os = "linux")]`. Update `src/capture/mod.rs` to conditionally export it. |
| `src/process/linux.rs` | Yes | Already gated by 2.2 of INF-02 Phase 2. |
| `src/process/windows.rs` | Yes | Created in PLAT-01 Phase 4. |
| `src/hotkey.rs` | Mixed | The X11 / portal arms are already split internally. Wrap Linux helpers in `#[cfg(target_os = "linux")]` and add a `configure_windows` arm using `global-hotkey`'s native Windows backend. |
| `src/tray.rs` | Mixed | Per D9, refactor into a folder with two backends — see Phase 2.4 below. |
| `src/autostart.rs:21-102` | Mixed | Already has `target_os` arms for linux/macos/windows. Compile-test the Windows arm and fix any breakage. |
| `src/secure_store.rs:317` | Mixed | Has a `#[cfg(unix)]` branch. Add a `#[cfg(target_os = "windows")]` branch using DPAPI (`windows-credentials` or the `windows` crate's `DataProtection` APIs). |
| `src/launcher.rs` | Already split | Just verify it still compiles for Windows. |

### 2.4 Refactor `src/tray.rs` for the dual-backend dispatch

`src/tray.rs` becomes a folder:

```
src/tray/
├── mod.rs        # Tray trait, dispatcher, shared types
├── ksni.rs       # KsniTrayBackend. Gated #[cfg(target_os = "linux")].
└── tray_icon.rs  # TrayIconBackend (cross-platform, used everywhere except KDE Plasma).
```

The trait is small — most of the existing `src/tray.rs` API surface is already abstract enough:

```rust
pub trait TrayBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn update_state(&self, state: TrayState);
    fn shutdown(self: Box<Self>);
}

pub enum TrayState {
    Idle,
    WaitingForGame,
    WaitingForLogin,
    Monitoring { character_name: Option<String> },
    Error { message: String },
}

pub enum TrayMessage {
    Show,
    Hide,
    StartMonitoring,
    StopMonitoring,
    Quit,
}
```

Selection happens once in `App::new`:

```rust
fn build_tray_backend(message_tx: mpsc::UnboundedSender<TrayMessage>) -> Box<dyn TrayBackend> {
    #[cfg(target_os = "linux")]
    {
        use crate::process::linux::detect_desktop_environment;
        use crate::process::DesktopEnvironment;
        if matches!(detect_desktop_environment(), DesktopEnvironment::KdePlasma) {
            return Box::new(crate::tray::ksni::KsniTrayBackend::new(message_tx));
        }
    }
    Box::new(crate::tray::tray_icon::TrayIconBackend::new(message_tx))
}
```

`KsniTrayBackend` wraps the existing `ksni`-based code unchanged. `TrayIconBackend` is the new module — it constructs a `tray-icon::TrayIconBuilder`, builds a context menu mirroring the existing actions, and forwards `tray-icon` events into `TrayMessage`s on the same `mpsc::UnboundedSender`.

**Why dispatch by desktop environment, not just by `target_os`?** `tray-icon` on Linux uses the libappindicator/AppIndicator bridge, which has long-standing rendering and click-handling quirks under KDE Plasma's StatusNotifierItem host. `ksni` speaks the StatusNotifierItem protocol natively and produces the polished KDE experience users expect. Other Linux desktops (GNOME with the AppIndicator extension, XFCE, etc.) work fine with `tray-icon` and benefit from being on the same code path as Windows.

**Behavioral parity checklist** for `TrayIconBackend` vs `KsniTrayBackend`:

- Icon updates on state change (Idle → WaitingForGame → Monitoring).
- Tooltip text matches state.
- Context menu items: Show / Start / Stop / Quit (mirror current ksni behavior).
- Left-click on tray icon shows the main window.
- Quit action shuts down the app cleanly without leaking the tray icon.

Add an integration test (or scripted manual smoke test) that exercises each state on each backend.

### 2.5 Make `cargo check --target x86_64-pc-windows-msvc` pass

Don't try to make the app *run* on Windows in this phase — just make it compile. Track every cfg-gate gap that surfaces. Acceptable resolutions:

- Add a `#[cfg]` arm.
- Provide a no-op fallback that satisfies the same type signature.
- For genuinely impossible-on-Windows code (PipeWire), `compile_error!("...not supported on Windows")` inside the Linux module is fine because the module itself should be gated.

`unimplemented!()` is **not** an acceptable fallback for code that runs at startup on Windows — it must either be excluded by `#[cfg]` or have a real Windows implementation.

### Phase 2 done criteria

- `cargo check --target x86_64-pc-windows-msvc` passes from a clean checkout.
- `cargo build --target x86_64-pc-windows-msvc` succeeds (may be slow on first run).
- Linux build still passes.
- The tray dispatcher selects `KsniTrayBackend` on KDE Plasma and `TrayIconBackend` everywhere else, verified by a unit test that injects a fake `DesktopEnvironment` value.
- Tray parity checklist from §2.4 passes on at least one KDE Plasma machine, one GNOME machine, and one Windows 10/11 machine.

---

## Phase 3 — Implement `ObsBackend`

### 3.1 Module structure

Create `src/capture/obs.rs`:

```rust
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::capture::{
    AudioSourceError, CaptureBackend, CaptureCapabilities, CaptureError, CaptureRequest,
    CaptureSession, DiscoveredAudioSource, RecoveryHint, ResolvedAudioSource, SavePollResult,
};
use crate::config::{AudioSourceKind, ObsBackendConfig, ObsManagementMode};
use crate::rules::ClipLength;

mod keys;
mod session;

pub use keys::ObsParameterKeys;
pub use session::ObsCaptureSession;

pub struct ObsBackend {
    config: ObsBackendConfig,
}

impl ObsBackend {
    pub fn new(config: ObsBackendConfig) -> Self {
        Self { config }
    }
}

impl CaptureBackend for ObsBackend {
    fn id(&self) -> &'static str { "obs" }
    fn display_name(&self) -> &'static str { "OBS Studio (WebSocket)" }

    fn capabilities(&self) -> CaptureCapabilities {
        match self.config.mode {
            ObsManagementMode::BringYourOwn | ObsManagementMode::ManagedRecording => {
                CaptureCapabilities {
                    per_app_audio: false,
                    application_inverse: false,
                    merged_tracks: false,
                    portal_session_restore: false,
                    replay_buffer: true,
                    hdr: false,
                    cursor_capture: false,
                }
            }
            ObsManagementMode::FullManagement => CaptureCapabilities {
                per_app_audio: cfg!(target_os = "windows"),
                application_inverse: false,
                merged_tracks: false,
                portal_session_restore: false,
                replay_buffer: true,
                hdr: false,
                cursor_capture: true,
            },
        }
    }

    fn validate_audio_source(&self, _kind: &AudioSourceKind) -> Result<(), AudioSourceError> {
        match self.config.mode {
            ObsManagementMode::BringYourOwn | ObsManagementMode::ManagedRecording => {
                Err(AudioSourceError::Unsupported {
                    capability: "OBS owns audio routing".into(),
                })
            }
            ObsManagementMode::FullManagement => {
                // v3 will validate against the WASAPI input kinds.
                Err(AudioSourceError::Unsupported {
                    capability: "FullManagement audio routing not yet implemented".into(),
                })
            }
        }
    }

    fn discover_audio_sources(&self) -> /* pinned future */ {
        Box::pin(async move {
            Err(AudioSourceError::Unsupported {
                capability: "OBS does not expose audio device enumeration via websocket".into(),
            })
        })
    }

    fn spawn_replay(&self, request: CaptureRequest) -> Result<Box<dyn CaptureSession>, CaptureError> {
        ObsCaptureSession::spawn(self.config.clone(), request)
            .map(|session| Box::new(session) as Box<dyn CaptureSession>)
    }

    fn post_save_recovery_hint(&self, _: Option<VideoResolution>) -> RecoveryHint {
        RecoveryHint::None
    }
}
```

### 3.2 `ObsCaptureSession` lifecycle

Defined in `src/capture/obs/session.rs`:

```rust
pub struct ObsCaptureSession {
    client: Arc<obws::Client>,
    mode: ObsManagementMode,
    keys: ObsParameterKeys,
    save_in_flight: bool,
    pending_save_length: Option<ClipLength>,
    save_results_rx: mpsc::UnboundedReceiver<SavePollResult>,
    event_task: tokio::task::JoinHandle<()>,
    /// Set under FullManagement so we can clean up on stop.
    owned_resources: Option<OwnedObsResources>,
    /// Set under ManagedRecording so we can detect external drift.
    last_known_rb_secs: Option<u32>,
    /// Empty for OBS — kept for trait compatibility.
    active_audio_layout: Vec<ResolvedAudioSource>,
}

struct OwnedObsResources {
    profile_name: String,
    scene_collection_name: String,
    previous_profile: Option<String>,
    previous_scene_collection: Option<String>,
}
```

### 3.3 `spawn` dispatch

```rust
impl ObsCaptureSession {
    pub fn spawn(
        config: ObsBackendConfig,
        request: CaptureRequest,
    ) -> Result<Self, CaptureError> {
        let runtime = tokio::runtime::Handle::current();
        runtime.block_on(async move {
            let client = Self::connect(&config).await?;
            let version = client.general().version().await
                .map_err(|e| CaptureError::SpawnFailed(format!("OBS GetVersion failed: {e}")))?;
            let keys = ObsParameterKeys::for_version(parse_obs_version(&version)?);

            match config.mode {
                ObsManagementMode::BringYourOwn => Self::spawn_byoobs(client, keys, &config).await,
                ObsManagementMode::ManagedRecording => {
                    Self::spawn_managed_recording(client, keys, &config, &request).await
                }
                ObsManagementMode::FullManagement => Err(CaptureError::SpawnFailed(
                    "FullManagement mode is not yet implemented in this version".into(),
                )),
            }
        })
    }
}
```

`Self::connect` uses `obws::Client::connect(host, port, password_from_secure_store())?`. Connection failures map to `CaptureError::SpawnFailed` with a clear "Could not connect to OBS at <url>. Make sure OBS is running and obs-websocket is enabled" message.

### 3.4 `spawn_byoobs`

```rust
async fn spawn_byoobs(
    client: obws::Client,
    keys: ObsParameterKeys,
    config: &ObsBackendConfig,
) -> Result<Self, CaptureError> {
    let status = client.replay_buffer().status().await
        .map_err(|e| CaptureError::SpawnFailed(format!("OBS GetReplayBufferStatus failed: {e}")))?;
    if !status.active {
        return Err(CaptureError::SpawnFailed(
            "OBS replay buffer is not active. Enable it in OBS → Settings → Output → Replay Buffer.".into(),
        ));
    }

    let (tx, rx) = mpsc::unbounded_channel();
    let event_task = spawn_event_task(client.clone(), tx);

    Ok(Self {
        client: Arc::new(client),
        mode: ObsManagementMode::BringYourOwn,
        keys,
        save_in_flight: false,
        pending_save_length: None,
        save_results_rx: rx,
        event_task,
        owned_resources: None,
        last_known_rb_secs: Some(status.duration_secs as u32),
        active_audio_layout: Vec::new(),
    })
}
```

`spawn_byoobs` never writes any profile parameter — verifiable by an integration test against a stub obs-websocket server (Phase 6).

### 3.5 `spawn_managed_recording`

```rust
async fn spawn_managed_recording(
    client: obws::Client,
    keys: ObsParameterKeys,
    config: &ObsBackendConfig,
    request: &CaptureRequest,
) -> Result<Self, CaptureError> {
    let was_active = client.replay_buffer().status().await
        .map(|s| s.active)
        .unwrap_or(false);

    if was_active {
        client.replay_buffer().stop().await
            .map_err(|e| CaptureError::SpawnFailed(format!("OBS StopReplayBuffer failed: {e}")))?;
    }

    let active_profile = client.profiles().current().await
        .map_err(|e| CaptureError::SpawnFailed(format!("OBS GetCurrentProfile failed: {e}")))?;

    let mut writer = ObsParameterWriter::new(&client, &keys, "SimpleOutput");
    writer.set("FilePath", &request.recorder.save_directory.to_string_lossy()).await?;
    writer.set(keys.simple_output_rec_format, &map_container(&request.recorder.backends.gsr.container)?).await?;
    writer.set(keys.simple_output_rec_rb, "true").await?;
    writer.set(keys.simple_output_rec_rb_time, &request.recorder.replay_buffer_secs.to_string()).await?;
    writer.set(keys.simple_output_rec_rb_size, "512").await?;  // generous default

    client.replay_buffer().start().await
        .map_err(|e| CaptureError::SpawnFailed(format!("OBS StartReplayBuffer failed: {e}")))?;

    let (tx, rx) = mpsc::unbounded_channel();
    let event_task = spawn_event_task(client.clone(), tx);

    tracing::info!(
        "Started OBS managed recording (profile={active_profile}, rb_secs={})",
        request.recorder.replay_buffer_secs
    );

    Ok(Self { /* ... */ })
}
```

`map_container` translates NaniteClip's container strings to OBS's narrower set:
- `mkv` → `mkv`
- `mp4` → `mp4`
- `mov` → `mov`
- `flv` → `flv`
- `ts` → `ts`
- anything else → `Err(CaptureError::Unsupported { ... })` with a "Container 'foo' is not supported by OBS, use mkv or mp4" message.

### 3.6 `save_clip`

```rust
fn save_clip(&mut self, length: ClipLength) -> Result<(), CaptureError> {
    if self.save_in_flight {
        return Err(CaptureError::SaveInFlight);
    }

    if !matches!(length, ClipLength::FullBuffer) {
        // v1 limitation: OBS replay buffer length is profile-bound.
        // Trim-via-ffmpeg is a follow-up.
        tracing::warn!(
            "OBS backend does not support custom clip lengths in v1; saving full buffer instead"
        );
    }

    let client = self.client.clone();
    tokio::spawn(async move {
        if let Err(e) = client.replay_buffer().save().await {
            tracing::error!("OBS SaveReplayBuffer failed: {e}");
        }
    });

    self.save_in_flight = true;
    self.pending_save_length = Some(length);
    Ok(())
}
```

### 3.7 `poll_results`

```rust
fn poll_results(&mut self) -> Vec<SavePollResult> {
    let mut results = Vec::new();
    while let Ok(result) = self.save_results_rx.try_recv() {
        if matches!(result, SavePollResult::Saved { .. } | SavePollResult::SaveFailed(_)) {
            self.save_in_flight = false;
        }
        // Attach the requested length the caller asked for, even though OBS gave us
        // the full buffer (v1 limitation).
        let result = match (result, self.pending_save_length.take()) {
            (SavePollResult::Saved { path, audio_layout, .. }, Some(duration)) => {
                SavePollResult::Saved { path, duration, audio_layout }
            }
            (other, _) => other,
        };
        results.push(result);
    }
    results
}
```

### 3.8 `spawn_event_task`

```rust
fn spawn_event_task(
    client: obws::Client,
    tx: mpsc::UnboundedSender<SavePollResult>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut events = match client.events() {
            Ok(events) => events,
            Err(e) => {
                let _ = tx.send(SavePollResult::SaveFailed(
                    format!("Could not subscribe to OBS events: {e}")
                ));
                return;
            }
        };

        while let Some(event) = events.next().await {
            match event {
                obws::events::Event::ReplayBufferSaved { path } => {
                    let _ = tx.send(SavePollResult::Saved {
                        path: PathBuf::from(path),
                        duration: ClipLength::FullBuffer,
                        audio_layout: Vec::new(),
                    });
                }
                obws::events::Event::ReplayBufferStateChanged { active: false, .. } => {
                    tracing::warn!("OBS replay buffer was stopped externally");
                    // Do not push a SaveFailed here — the watchdog (3.10) handles
                    // surfacing this to the UI as a yellow status banner.
                }
                _ => {}
            }
        }

        tracing::warn!("OBS event stream ended");
        let _ = tx.send(SavePollResult::SaveFailed(
            "OBS websocket disconnected".into()
        ));
    })
}
```

### 3.9 `stop`

```rust
fn stop(&mut self) -> Result<(), CaptureError> {
    self.event_task.abort();

    match self.mode {
        ObsManagementMode::BringYourOwn | ObsManagementMode::ManagedRecording => {
            // Leave the replay buffer running. The user may want it for their
            // own use after NaniteClip stops monitoring.
        }
        ObsManagementMode::FullManagement => {
            let runtime = tokio::runtime::Handle::current();
            let client = self.client.clone();
            let owned = self.owned_resources.take();
            runtime.block_on(async move {
                let _ = client.replay_buffer().stop().await;
                if let Some(owned) = owned {
                    if let Some(prev) = owned.previous_profile {
                        let _ = client.profiles().set_current(&prev).await;
                    }
                    if let Some(prev) = owned.previous_scene_collection {
                        let _ = client.scene_collections().set_current(&prev).await;
                    }
                }
            });
        }
    }
    Ok(())
}
```

### 3.10 Version-gated parameter keys

`src/capture/obs/keys.rs`:

```rust
use semver::Version;

#[derive(Debug, Clone)]
pub struct ObsParameterKeys {
    pub obs_version: Version,
    pub simple_output_file_path: &'static str,
    pub simple_output_rec_format: &'static str,
    pub simple_output_rec_rb: &'static str,
    pub simple_output_rec_rb_time: &'static str,
    pub simple_output_rec_rb_size: &'static str,
    pub simple_output_rec_encoder: &'static str,
    pub simple_output_v_bitrate: &'static str,
}

impl ObsParameterKeys {
    pub fn for_version(version: Version) -> Self {
        let rec_format_key = if version >= Version::new(29, 0, 0) {
            "RecFormat2"
        } else {
            "RecFormat"
        };
        Self {
            obs_version: version,
            simple_output_file_path: "FilePath",
            simple_output_rec_format: rec_format_key,
            simple_output_rec_rb: "RecRB",
            simple_output_rec_rb_time: "RecRBTime",
            simple_output_rec_rb_size: "RecRBSize",
            simple_output_rec_encoder: "RecEncoder",
            simple_output_v_bitrate: "VBitrate",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_legacy_rec_format_for_obs_28() {
        let keys = ObsParameterKeys::for_version(Version::new(28, 1, 0));
        assert_eq!(keys.simple_output_rec_format, "RecFormat");
    }

    #[test]
    fn picks_modern_rec_format_for_obs_29() {
        let keys = ObsParameterKeys::for_version(Version::new(29, 0, 0));
        assert_eq!(keys.simple_output_rec_format, "RecFormat2");
    }

    #[test]
    fn picks_modern_rec_format_for_obs_30() {
        let keys = ObsParameterKeys::for_version(Version::new(30, 0, 1));
        assert_eq!(keys.simple_output_rec_format, "RecFormat2");
    }
}
```

**All parameter writes go through this struct.** No string literals like `"RecFormat"` or `"RecFormat2"` may appear outside `keys.rs`. Add a clippy-ish guard: a CI grep that fails the build if either literal appears anywhere else under `src/`.

### 3.11 Watchdog for parameter drift

`ObsCaptureSession` polls `GetReplayBufferStatus` once per `Tick` (already a 1-sec cadence in `app.rs`). If `status.active` is `false` while `mode == ManagedRecording` and we expected it to be running, emit a `SavePollResult::SaveFailed("OBS replay buffer was disabled externally — recordings will not save until it's re-enabled in OBS")`. The `app.rs` toast/notification pipeline already handles `SaveFailed`, so the user sees a yellow status banner without new UI work.

Add a debounce so we don't spam the banner: only emit the warning once per `active=false` → `active=true` transition.

### 3.12 Automatic reconnect supervisor

When the obs-websocket connection drops mid-session — OBS quit, network blip, OBS UI restart — NaniteClip automatically reconnects in the background until either the connection comes back or the user explicitly stops monitoring. The user does not need to click anything.

#### Lifecycle

```
[Connected] --(disconnect detected)--> [Reconnecting] --(success)--> [Connected]
                                            |
                                            +-(user stops monitoring)-> [Stopped]
```

#### Implementation shape

Extract the connection-and-initialization logic in §3.4 / §3.5 into a reusable helper:

```rust
impl ObsCaptureSession {
    /// Establish a fresh websocket connection and rehydrate session state
    /// according to the management mode. Used both for initial spawn and for
    /// reconnect attempts.
    async fn connect_and_initialize(
        config: &ObsBackendConfig,
        request: &CaptureRequest,
    ) -> Result<ConnectedState, CaptureError> {
        let client = Self::connect(config).await?;
        let version = client.general().version().await
            .map_err(|e| CaptureError::SpawnFailed(format!("OBS GetVersion failed: {e}")))?;
        let keys = ObsParameterKeys::for_version(parse_obs_version(&version)?);

        match config.mode {
            ObsManagementMode::BringYourOwn => {
                Self::initialize_byoobs(&client).await?;
            }
            ObsManagementMode::ManagedRecording => {
                Self::push_managed_parameters(&client, &keys, request).await?;
                client.replay_buffer().start().await
                    .map_err(|e| CaptureError::SpawnFailed(format!("StartReplayBuffer failed: {e}")))?;
            }
            ObsManagementMode::FullManagement => {
                return Err(CaptureError::SpawnFailed(
                    "FullManagement mode is not yet implemented in this version".into(),
                ));
            }
        }

        Ok(ConnectedState { client: Arc::new(client), keys })
    }
}
```

Add a supervisor task that runs alongside `event_task` and owns the reconnect state machine:

```rust
struct ObsCaptureSession {
    // ... existing fields ...
    supervisor: ReconnectSupervisor,
    config: ObsBackendConfig,
    request: CaptureRequest,           // kept so we can re-initialize on reconnect
    status_tx: mpsc::UnboundedSender<ObsConnectionStatus>,
}

#[derive(Debug, Clone)]
pub enum ObsConnectionStatus {
    Connected,
    Reconnecting { attempt: u32, next_retry_in: Duration },
    Failed { reason: String },
}

struct ReconnectSupervisor {
    handle: tokio::task::JoinHandle<()>,
    cancel: tokio_util::sync::CancellationToken,
}
```

The supervisor task:

```rust
async fn reconnect_loop(
    config: ObsBackendConfig,
    request: CaptureRequest,
    status_tx: mpsc::UnboundedSender<ObsConnectionStatus>,
    save_results_tx: mpsc::UnboundedSender<SavePollResult>,
    cancel: CancellationToken,
) {
    let mut attempt: u32 = 0;
    loop {
        if cancel.is_cancelled() { return; }

        attempt += 1;
        let backoff = backoff_for_attempt(attempt);
        let _ = status_tx.send(ObsConnectionStatus::Reconnecting {
            attempt,
            next_retry_in: backoff,
        });

        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tokio::time::sleep(backoff) => {}
        }

        match ObsCaptureSession::connect_and_initialize(&config, &request).await {
            Ok(connected) => {
                let _ = status_tx.send(ObsConnectionStatus::Connected);
                tracing::info!("OBS reconnected after {attempt} attempts");
                // Hand the new client back to the session via a oneshot; the
                // session swaps it in atomically and resubscribes events.
                // (Implementation detail — see ConnectedState dispatch below.)
                return;
            }
            Err(error) => {
                tracing::warn!("OBS reconnect attempt {attempt} failed: {error}");
                if attempt >= 3 {
                    let _ = status_tx.send(ObsConnectionStatus::Failed {
                        reason: error.to_string(),
                    });
                }
                // Keep trying — failure is just informational, not terminal.
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
```

#### Trigger conditions

The supervisor starts when **any** of the following happens:

1. `spawn_event_task` observes the obs-websocket event stream end (existing behavior in §3.8 — replace the `SaveFailed("OBS websocket disconnected")` push with a supervisor-start signal).
2. The watchdog (§3.11) sees `GetReplayBufferStatus` fail with a connection error rather than just `active=false`.
3. A `save_clip` call fails because the underlying client returns a transport error.

The supervisor stops when:

1. `ObsCaptureSession::stop` is called (user clicked Stop monitoring or quit). The cancellation token fires.
2. Reconnect succeeds. The supervisor task returns; the session is back in `Connected` state.

#### Save-in-flight handling during reconnect

If a save was in flight when the disconnect happened, the `ReplayBufferSaved` event is gone. Mark the in-flight save as failed:

```rust
// Inside spawn_event_task, when the event stream ends:
if save_in_flight {
    let _ = save_results_tx.send(SavePollResult::SaveFailed(
        "OBS disconnected mid-save; the clip may or may not have been written".into()
    ));
}
```

The rule engine treats this as a normal save failure (cooldown fires, etc.). On reconnect, we do **not** retry the save — the user can manually trigger another save if they want.

#### UI surface

Add `ObsConnectionStatus` to the App state and render it in the Status tab:

- `Connected` → no banner.
- `Reconnecting { attempt, next_retry_in }` → yellow banner: "OBS disconnected. Reconnecting (attempt 3, retrying in 5 s)…" with a "Stop monitoring" link.
- `Failed { reason }` → red banner after attempt 3: "Cannot reconnect to OBS: <reason>. Will keep trying." (Failure is informational, not terminal — the supervisor keeps going.)

The tray icon also flips to a warning state during `Reconnecting` so users with a minimized window notice.

#### Tests

- **Reconnect succeeds on attempt 2.** Stub obs-websocket server: connect succeeds → drop connection → first reconnect fails → second succeeds. Assert exactly one `Connected` status event after the disconnect, attempt counter reaches 2.
- **Reconnect cancelled by `stop()`.** Start reconnect loop, call `stop()` mid-backoff, assert the supervisor task ends within 100ms.
- **Save-in-flight surfaces as failed on disconnect.** Inject a save, drop the connection before `ReplayBufferSaved` arrives, assert one `SaveFailed` is emitted.
- **Backoff schedule.** Unit-test `backoff_for_attempt` against the documented schedule.
- **Reconnect storm guard.** Spam disconnect 100 times in a row; assert that only one supervisor is running at any time (the trigger conditions in §3.12 must be idempotent — starting a supervisor while one is already running is a no-op).

### 3.13 Register the OBS backend

In `src/recorder.rs::create_backend`, add the `"obs"` arm (already shown in §1.1). Make `ObsBackend` available cross-platform — Linux users may also want to use OBS:

```rust
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub mod obs;
```

`gsr` stays Linux-only.

### Phase 3 done criteria

- `ObsBackend` compiles on both Linux and Windows.
- A unit test against a stub obs-websocket server (or `obws`'s test helpers) verifies:
  - `BringYourOwn` mode never calls `SetProfileParameter`.
  - `ManagedRecording` mode pushes `FilePath`, `RecFormat2`, `RecRB`, `RecRBTime`, `RecRBSize` with the configured values.
  - The watchdog emits exactly one `SaveFailed` per `active=false` transition.
- `ObsParameterKeys::for_version` has tests covering OBS 28, 29, and 30+.
- A grep-for-literal check fails CI if `"RecFormat"` or `"RecFormat2"` appears outside `keys.rs`.
- The reconnect supervisor passes all five tests in §3.12.
- The Status tab renders `Reconnecting` and `Failed` banners correctly when the supervisor pushes status updates.

---

## Phase 4 — Windows process detection

### 4.1 `WindowsToolhelpWatcher`

`src/process/windows.rs`:

```rust
#![cfg(target_os = "windows")]

use windows::Win32::Foundation::{CloseHandle, HANDLE, STILL_ACTIVE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
    TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

use crate::process::{CaptureTarget, CaptureTargetError, GameProcessWatcher};

pub struct WindowsToolhelpWatcher;

impl WindowsToolhelpWatcher {
    pub fn new() -> Self { Self }
}

impl GameProcessWatcher for WindowsToolhelpWatcher {
    fn find_running_pid(&self) -> Option<u32> {
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            if Process32FirstW(snapshot, &mut entry).is_err() {
                let _ = CloseHandle(snapshot);
                return None;
            }
            loop {
                let exe_name = String::from_utf16_lossy(
                    &entry.szExeFile[..entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0)],
                );
                if exe_name_matches_ps2(&exe_name) {
                    let pid = entry.th32ProcessID;
                    let _ = CloseHandle(snapshot);
                    return Some(pid);
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
            let _ = CloseHandle(snapshot);
            None
        }
    }

    fn is_running(&self, pid: u32) -> bool {
        unsafe {
            let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                return false;
            };
            let mut exit_code: u32 = 0;
            let result = GetExitCodeProcess(handle, &mut exit_code).is_ok();
            let _ = CloseHandle(handle);
            result && exit_code == STILL_ACTIVE.0 as u32
        }
    }

    fn resolve_capture_target(
        &self,
        _pid: u32,
        _configured_source: &str,
    ) -> Result<CaptureTarget, CaptureTargetError> {
        Ok(CaptureTarget::BackendOwned)
    }
}

fn exe_name_matches_ps2(exe: &str) -> bool {
    let lower = exe.to_ascii_lowercase();
    lower.starts_with("planetside2_x64") && lower.ends_with(".exe")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_known_ps2_exe_names() {
        assert!(exe_name_matches_ps2("PlanetSide2_x64.exe"));
        assert!(exe_name_matches_ps2("PlanetSide2_x64_BE.exe"));
        assert!(!exe_name_matches_ps2("notepad.exe"));
        assert!(!exe_name_matches_ps2("PlanetSide2.exe"));  // no _x64
    }
}
```

### 4.2 Wire up watcher selection

In `App::new` (or `main.rs`):

```rust
#[cfg(target_os = "linux")]
let process_watcher: Arc<dyn GameProcessWatcher> = {
    use crate::process::linux::LinuxProcfsWatcher;
    Arc::new(LinuxProcfsWatcher::new())
};
#[cfg(target_os = "windows")]
let process_watcher: Arc<dyn GameProcessWatcher> = {
    use crate::process::windows::WindowsToolhelpWatcher;
    Arc::new(WindowsToolhelpWatcher::new())
};
```

Store `process_watcher` on `App` and replace the call sites identified in INF-02 Phase 2.3.

### Phase 4 done criteria

- `WindowsToolhelpWatcher` compiles and unit tests pass via `cargo test --target x86_64-pc-windows-msvc`.
- Manual smoke test: launch PS2 on a Windows machine, confirm NaniteClip detects the process within 3 seconds.

---

## Phase 5 — Settings UI for the OBS backend

### 5.1 Backend selector

Add a pick-list at the top of the Recorder section in `src/app/tabs/settings.rs`. Source from a static list:

```rust
const BACKENDS: &[(&str, &str)] = &[
    #[cfg(target_os = "linux")]
    ("gsr", "gpu-screen-recorder (Linux)"),
    ("obs", "OBS Studio (WebSocket)"),
];
```

On Windows, only OBS appears. On Linux, both options appear. Selection writes to `config.capture.backend` and triggers a recorder rebuild via `Recorder::update_config`.

### 5.2 Mode-conditional OBS config block

**Always shown when backend = OBS:**

- WebSocket URL field (default `ws://localhost:4455`).
- Password field, masked, writes to secure store entry `obs_websocket_password`. Never serialized to `config.toml`.
- "Test connection" button → fires `Message::ObsTestConnection` that calls `obws::Client::connect` in a `Task::perform`. Reports success/failure as a toast.
- Status line: "Replay buffer: active / inactive / mismatch" pulled from a periodic `GetReplayBufferStatus`.
- Management mode pick-list with three options:
  - "Bring your own setup (NaniteClip only triggers saves)" → `BringYourOwn`
  - "Managed recording (recommended)" → `ManagedRecording`
  - "Full management (experimental, not yet available)" → disabled in v1, with a tooltip explaining

**Shown only when mode = `ManagedRecording`:**

- Output directory (mirrors `RecorderConfig.save_directory` but labeled "OBS will record clips to").
- Container pick-list, restricted to OBS's supported set: `mkv`, `mp4`, `mov`, `flv`, `ts`. Validate selection on change.
- Replay buffer length slider (mirrors `RecorderConfig.replay_buffer_secs`).
- Info banner: "NaniteClip will push these settings to your active OBS profile. Your scene, capture source, and audio routing remain under your control in OBS."

**Shown only when mode = `BringYourOwn`:**

- Just the connection block.
- Info banner: "NaniteClip will only call SaveReplayBuffer when a rule fires. Configure your scene, capture source, audio routing, and replay buffer length in OBS yourself."

**Shown only when mode = `FullManagement`** (not in v1, but reserve the layout):

- All `ManagedRecording` fields, plus:
- Capture input kind pick-list (`monitor_capture` / `game_capture` / `dshow_input`).
- "Restore my previous OBS profile when NaniteClip disconnects" checkbox.
- Encoder pick-list (SimpleOutput options only: `x264`, `nvenc`, `qsv`, `amd`).
- Video bitrate field.
- Warning banner: "Full management creates a dedicated `nanite-clip` profile and scene collection in OBS. Audio sources from NaniteClip will be mirrored to OBS inputs."

### 5.3 Hide GSR-only settings

When `backend == "obs"` and `mode != FullManagement`, hide:

- Framerate
- Codec
- Container (the OBS section has its own constrained version)
- Quality
- Audio Sources panel

These remain visible under `mode == FullManagement` because that mode drives them through OBS instead of GSR.

### 5.4 Mode-change confirmation

Switching from `FullManagement` back to `BringYourOwn` or `ManagedRecording` while a session is active needs a confirmation dialog: "This will leave the `nanite-clip` profile and scene collection in OBS. Continue?" Don't auto-delete; the user might switch back.

(In v1, since `FullManagement` is unimplemented, this dialog only matters for the future-proofing.)

### 5.5 Backend switch while monitoring

Switching backends while a recorder session is active is rejected by the existing `Recorder::update_config` logic (only swaps backend when `session.is_none()`). Surface this in the UI: disable the backend pick-list with a tooltip "Stop monitoring before changing capture backends" when a session is live.

### Phase 5 done criteria

- The Settings tab renders all three OBS modes correctly and gates fields per the above.
- Saving and reloading the config preserves mode selection.
- The "Test connection" button works against a real OBS instance and a closed-port instance.
- Switching modes mid-session is blocked with a clear message.

---

## Phase 6 — Smoke test plan

Document this checklist in `docs/PLAT-01-windows-smoke-test.md` (created as part of Phase 6, not pre-emptively). Manual verification before each Wave 6 release.

**Setup:**

1. Install OBS Studio 28.0+ on a Windows 10/11 machine.
2. In OBS, create a scene with a Display Capture source pointed at the monitor where PS2 will run.
3. Enable Replay Buffer in `Settings → Output → Replay Buffer`, set duration to 5 minutes.
4. Verify obs-websocket is enabled in `Tools → WebSocket Server Settings`. Note the port (default 4455) and password.
5. Install NaniteClip from the Wave 6 Windows build artifact.

**`BringYourOwn` mode (run first as the lowest-risk path):**

1. Open NaniteClip Settings → Recorder → set backend to OBS, mode to `BringYourOwn`.
2. Enter the WebSocket URL and password. Click "Test connection" → expect a green toast.
3. Save settings.
4. Launch PlanetSide 2. Confirm NaniteClip advances `WaitingForGame` → `WaitingForLogin` → `Monitoring`.
5. Trigger a manual hotkey save. Expect:
   - A clip file appears in OBS's configured replay-buffer output directory.
   - A new entry appears in NaniteClip's Clips tab pointing at the same file.
   - Post-processing runs against the file (premix track is added if configured).
6. Trigger a rule-based save by spawning a low-threshold debug rule. Verify same outcome.
7. Quit OBS while monitoring. Expect:
   - A yellow "OBS disconnected. Reconnecting…" banner appears in the Status tab within 2 seconds.
   - The tray icon flips to a warning state.
   - NaniteClip does not crash.
   - Attempt counter ticks up at the documented backoff cadence (1s, 2s, 5s, 10s, 30s).
8. Restart OBS. Within ~30 seconds, verify the banner clears, the tray icon returns to normal, and monitoring resumes — **without any user action**.
9. Quit PS2. Expect:
   - `Monitoring` → `WaitingForGame`.
   - Replay buffer in OBS is **still running** (we don't touch it in BYOOBS).

**`ManagedRecording` mode:**

10. Switch mode to `ManagedRecording` in NaniteClip Settings. Save.
11. Verify: in OBS Settings → Output → Replay Buffer, the duration matches NaniteClip's `replay_buffer_secs`.
12. Verify: in OBS Settings → Output → Recording, the output path matches NaniteClip's `save_directory`.
13. Change `replay_buffer_secs` in NaniteClip to a different value. Save. Confirm the OBS UI reflects the new value within ~5 seconds.
14. Trigger a save. Confirm the clip lands in the new directory.
15. **External drift test:** with NaniteClip running, manually disable Replay Buffer in OBS UI. Within 5 seconds, expect a yellow status banner: "OBS replay buffer was disabled externally — recordings will not save until it's re-enabled."
16. Re-enable Replay Buffer in OBS UI. Verify the banner clears on the next poll.
17. Quit OBS. Expect the same disconnect handling as BYOOBS.

**Cross-version compatibility:**

18. Repeat steps 10–14 against OBS 28.x and OBS 30+. Confirm the `RecFormat`/`RecFormat2` compat layer picks the right key (verify via OBS's `basic.ini` after a save).

**Linux regression:**

19. On a Linux machine with the same Wave 6 build, run the existing GSR-backend smoke checklist. Confirm zero behavioral changes vs the pre-Wave-6 build. Compare recorder argv via `ps -ef | grep gpu-screen-recorder` against a known-good baseline.

### Phase 6 done criteria

- `docs/PLAT-01-windows-smoke-test.md` exists with the full checklist.
- All checklist items pass on at least one Windows 10 and one Windows 11 machine.
- Linux GSR regression check passes.

---

## Phase 7 — CI

### 7.1 Windows build workflow

`.github/workflows/windows-build.yml` (assumes `auraxis-rs` is already on crates.io per D6):

```yaml
name: Windows build

on:
  pull_request:
  push:
    branches: [main]

jobs:
  build:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust nightly
        uses: dtolnay/rust-toolchain@nightly
        with:
          targets: x86_64-pc-windows-msvc
          components: clippy, rustfmt

      - name: cargo fmt --check
        run: cargo fmt --check

      - name: cargo build
        run: cargo build --release --target x86_64-pc-windows-msvc

      - name: cargo test
        run: cargo test --target x86_64-pc-windows-msvc

      - name: cargo clippy
        run: cargo clippy --target x86_64-pc-windows-msvc --all-targets -- -D warnings

      - name: Upload binary
        uses: actions/upload-artifact@v4
        with:
          name: nanite-clip-windows-x64
          path: target/x86_64-pc-windows-msvc/release/nanite-clip.exe
```

**Fallback if `auraxis-rs` has not yet shipped to crates.io when PR 1 lands:** add a sibling-checkout step before the build:

```yaml
      - name: Checkout auraxis-rs (interim)
        uses: actions/checkout@v4
        with:
          repository: AnotherGenZ/auraxis-rs
          path: ../auraxis-rs
```

…and keep the `path = "../auraxis-rs/auraxis"` dependency in `Cargo.toml`. Once `auraxis-rs` publishes, a single follow-up PR swaps both the workflow and the `Cargo.toml` dep over.

### 7.2 Linux regression workflow

If a Linux build workflow doesn't already exist, add one in the same PR. Match the existing developer-experience commands from `AGENTS.md`:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`

### 7.3 Literal-key guard

Add a small CI step (in both workflows) that fails if `"RecFormat"` or `"RecFormat2"` appears outside `src/capture/obs/keys.rs`:

```bash
if rg -n '"RecFormat2?"' src/ | rg -v 'src/capture/obs/keys.rs' ; then
  echo "OBS parameter literal found outside keys.rs"
  exit 1
fi
```

### Phase 7 done criteria

- Both workflows green on a fresh PR.
- Windows binary appears as a downloadable artifact on every PR.
- Literal-key guard is wired and tested with a deliberate violation in a throwaway commit.

---

## Risks register

| ID | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | Config migration breaks existing user installs | Medium | High | Fixture-based migration test + a one-shot config backup written to `config.toml.pre-wave6` on first load. |
| R2 | `auraxis-rs` path-dep makes Windows CI flaky | High | Medium | Resolve D6 by checking out both repos side-by-side in CI; document the layout in `AGENTS.md`. Long-term, vendor or publish auraxis. |
| R3 | OBS parameter key drift breaks `ManagedRecording` on a future OBS release | High over time | Medium | `ObsParameterKeys::for_version` + literal-key CI guard. New OBS versions get a new test case in `keys.rs`. |
| R4 | OBS Advanced Output users get silent failures | Medium | Medium | On `spawn_managed_recording`, call `GetProfileParameter("Output", "Mode")` and refuse to push parameters with a clear "Switch OBS to SimpleOutput recording, or use BringYourOwn mode" error. |
| R5 | `wasapi_process_output_capture` is fussy on older Windows builds | Low (deferred) | Low | `FullManagement` is out of Wave 6; document the Windows 10 2004+ requirement when v3 lands. |
| R6 | Trying to swap backends mid-session corrupts state | Low | High | Existing `Recorder::update_config` logic prevents this; pin it with a unit test (Phase 3.2). UI gates the pick-list during active sessions (Phase 5.5). |
| R7 | OBS replay-buffer length differs from NaniteClip's `replay_buffer_secs` after the user edits OBS | Medium | Low | Status banner via the watchdog (3.11). NaniteClip authoritatively re-pushes on next session start. |
| R8 | `tray-icon` on a non-KDE Linux desktop has rendering quirks | Medium | Low | Per-DE dispatch (D9) keeps KDE on `ksni`. For other Linux desktops, ship and accept any minor differences from the previous KSNI experience; track regressions as bugs. |
| R11 | Reconnect storm on a flapping OBS connection wastes CPU and spams logs | Medium | Low | Backoff schedule plateaus at 30s. Trigger conditions are idempotent — only one supervisor runs at a time (tested in §3.12). After 3 failed attempts, the banner switches to "Failed" but reconnect keeps trying silently. |
| R12 | `auraxis-rs` crates.io publish lags Wave 6 PR 1 | Low | Low | D6 fallback: sibling-checkout interim in CI until the publish lands, then a one-line PR swaps to the published dep. |
| R9 | The ` /proc` walk → `Toolhelp32` swap misses PS2 variants | Low | Medium | Mirror the existing Linux test cases (`PlanetSide2_x64.exe`, `PlanetSide2_x64_BE.exe`) in `windows.rs::tests::matches_known_ps2_exe_names`. |
| R10 | An OBS reconnect after disconnect leaks the previous client / event task | Medium | Low | `ObsCaptureSession::stop` always aborts `event_task`. `Drop` impl on the session calls `stop` if not already called. Add a leak-detection test using a `Weak<obws::Client>`. |

---

## Execution order

Eight PRs total. Each is independently revertible and leaves the Linux/GSR path identical to the previous PR.

| PR | Scope | Lines (rough) | Hard deps |
|---|---|---|---|
| **1** | INF-02 Phase 1 (config split, factory, `CaptureSourcePlan` refactor, drop `BackendAudioArg::Opaque`, `RecoveryHint`) | ~600 | None |
| **2** | INF-02 Phase 2 (process watcher trait, split `process.rs`, app.rs call-site updates) | ~500 | PR 1 |
| **3** | INF-02 Phase 3 (`MockBackend`, recorder unit tests, fixture migration test) | ~400 | PR 1 |
| **4** | PLAT-01 Phase 1 + 2 (cfg-gating, `cargo check --target windows` green, dep restructure) | ~300 | PR 2 |
| **5a** | PLAT-01 Phase 3 partial: `ObsBackend` skeleton + `BringYourOwn` mode + event task + `ObsParameterKeys` | ~700 | PR 4 |
| **5b** | PLAT-01 Phase 3 remainder: `ManagedRecording` mode + parameter writer + watchdog | ~500 | PR 5a |
| **6** | PLAT-01 Phase 4 + 5 (Windows process watcher + Settings UI) | ~600 | PR 5b |
| **7** | PLAT-01 Phase 6 + 7 (smoke test doc + CI workflows + literal-key guard) | ~200 | PR 6 |

Total ~3,800 lines of churn across eight PRs. The split is deliberately conservative — every PR is reviewable in under an hour and rollback is a single revert.

---

## Wave 6 done criteria (rolled up)

A Wave 6 release is shippable when **all** of the following hold:

### Code

- `cargo test` passes on Linux.
- `cargo test --target x86_64-pc-windows-msvc` passes in CI.
- `cargo clippy --all-targets -- -D warnings` passes on both targets.
- `cargo fmt --check` passes.
- The literal-key CI guard is active.
- `app.rs` contains zero direct `process::find_ps2_pid`, `process::resolve_capture_source`, or non-hotkey `process::detect_display_server` calls.
- A new capture backend can be added without touching `app.rs`.

### Behavior — Linux

- The GSR backend produces a byte-identical `gpu-screen-recorder` command line vs the pre-Wave-6 build (verified by a captured baseline argv).
- All existing pre-Wave-6 user configs migrate cleanly via the fixture test.
- Existing rule, hotkey, post-process, upload, montage, and tray flows are unchanged.

### Behavior — Windows

- `nanite-clip.exe` is downloadable as a CI artifact on every PR.
- A user with OBS 28.0+, replay buffer enabled, and obs-websocket configured can complete the Phase 6 smoke checklist end-to-end without source edits.
- `BringYourOwn` mode never writes any OBS profile parameter (verified by integration test).
- `ManagedRecording` mode round-trips: changes to NaniteClip's `replay_buffer_secs` and `save_directory` reflect in OBS within one buffer restart cycle.
- The watchdog emits exactly one external-drift warning per `active=false` transition.

### Documentation

- `docs/wave-6-plan.md` (this document) is checked in.
- `docs/PLAT-01-windows-smoke-test.md` exists and matches the Phase 6 checklist.
- `docs/FUTURE.md` Wave 6 entries for INF-02 and PLAT-01 are checked off.
- Release notes call out: the management modes, the OBS 28.0 minimum, the SimpleOutput requirement, the Windows tray gap, and the v1 `ClipLength::FullBuffer`-only limitation on OBS.

---

## Resolved questions

1. **Tray on Windows.** ✅ Resolved: per-DE dispatch (D9). `ksni` on KDE Plasma, `tray-icon` everywhere else (other Linux DEs, Windows, future macOS). See §2.4 for the implementation plan. Wave 6 ships Windows with full tray support.
2. **`auraxis-rs` distribution.** ✅ Resolved: it will be published to crates.io. CI uses the published version (Phase 7.1). If publish lags PR 1, the sibling-checkout interim is documented as a fallback. See D6 and R12.
3. **OBS connection lifetime.** ✅ Resolved: automatic reconnect with exponential backoff (1s → 2s → 5s → 10s → 30s steady), no user action required. The reconnect supervisor is fully specified in §3.12.
4. **`FullManagement` mode shape.** ✅ Resolved: lands as an enum branch in v1 (returns `Unsupported` at runtime), tracked as a follow-up issue immediately after PR 7. Not blocking Wave 6.
5. **Audio source UI for OBS in `BringYourOwn`/`ManagedRecording`.** ✅ Resolved: hide entirely. See §5.3.
