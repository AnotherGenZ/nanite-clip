# NaniteClip Audio Capture & Post-Processing: Final Enhancement Plan

## Scope & design goals

Turn nanite-clip's audio pipeline from *"one thing passed to GSR"* into a first-class capture → record → post-process → distribute chain that supports per-application capture on PipeWire, multi-track output, automatic remix, per-source gain control, lossless editor-friendly originals, and compatible flattening on the distribution paths that can't handle multi-track. No MVP cuts — this is the shipping design.

**Cross-platform readiness.** The implementation introduces a `CaptureBackend` trait boundary (Part 3) so the GSR-specific logic lives behind one impl and a future Windows port (WASAPI / OBS / gpu-screen-recorder-win) drops in alongside without threading `#[cfg(target_os)]` guards through the rest of the codebase. v1 ships with only the GSR backend; the trait is infrastructure, not a working Windows port.

**Non-goals:** Supporting PulseAudio for per-app capture (GSR itself requires PipeWire, enforced upstream), rewriting GSR, building an in-app audio mixer UI beyond gain/mute/track metadata, shipping a working Windows capture backend in this enhancement.

## Locked defaults

- **Normalization:** `SumThenLimit` (alimiter, limit 0.97, attack 5 ms, release 50 ms).
- **Premix placement:** `First` (index 0 → Discord/browser friendly).
- **Codec / bitrate:** AAC @ 192 kbps.
- **Internal mix format:** 48 kHz / stereo / fltp (hard-coded pre-filter; see §4.3).
- **Preserve originals / rewrite titles:** both on.
- **Audio tracks persistence:** separate `clip_audio_tracks` table.
- **Clip-level post-process state:** new `post_process_status` enum column on `clips` (see §4.8 / §7.1).
- **Settings preview:** argv-only, pure function. No-audio-config still renders a baseline command (see §5.3).
- **Montage mismatch:** automatic per-clip normalization pass to **mix-only** canonical form with diagnostic toast (see §6.4).
- **Track gain UI:** slider only, −60..+12 dB, step **0.5 dB** (Ctrl/Shift = 0.1 dB fine adjust).
- **PostProcess concurrency:** per-kind limiter, `max_concurrent = 1` for `PostProcess` (see §4.5).
- **Loudnorm measurement parsing:** `print_format=json` on the first pass; parse JSON blob from stderr (see §4.3).
- **Minimum ffmpeg version:** **4.2** (released 2019-08). Enforced at startup by the version probe (see §8.3). Earlier versions lack `loudnorm print_format=json`, reliable `aformat` channel-layout handling, and `amix normalize=0`, so the post-process pipeline refuses to run rather than degrading silently.

---

## Part 1 — Data model (`src/config.rs`)

### 1.1 Replace flat `AudioSourceConfig` with a richer structure

Current (`src/config.rs:68-73`):
```rust
pub struct AudioSourceConfig {
    pub label: String,
    pub source: String,
}
```

New model:
```rust
pub struct AudioSourceConfig {
    pub label: String,                   // user-facing track title → written as stream metadata
    pub kind: AudioSourceKind,           // strongly typed rather than prefix-parsed
    pub gain_db: f32,                    // -60.0..+12.0, applied in the premix filter graph
    pub muted_in_premix: bool,           // skip in amix without removing the original stream
    pub included_in_premix: bool,        // default true; allows "reference only" tracks
}

pub enum AudioSourceKind {
    DefaultOutput,                                  // backend's default playback sink
    DefaultInput,                                   // backend's default capture source
    Device { name: String },                        // named device (backend-specific naming)
    Application { name: String },                   // per-process audio capture
    ApplicationInverse { names: Vec<String> },      // everything except listed apps
    Merged { entries: Vec<AudioSourceKind> },       // one output track fed by multiple sources
    Raw { backend_id: String, value: String },      // escape hatch — carries the target backend id
}
```

`AudioSourceKind` is **backend-neutral** — it describes user intent, not a CLI argument. The conversion to a backend-specific argument (GSR's `-a` string, a WASAPI session handle, an OBS source name, etc.) lives in the backend trait impl, not on the type itself. See Part 3 for the `CaptureBackend::translate_audio_source` method.

`Raw` carries a `backend_id` field so a config authored for one backend doesn't silently leak into another if the user switches. If the active backend doesn't match, Settings shows a warning on the row and treats the entry as disabled.

Validation (e.g. GSR rejects `app:` and `app-inverse:` on the same merged track, `reference/gpu-screen-recorder/src/main.cpp:3040`) is the backend impl's responsibility — it surfaces per-backend capability errors through `AudioSourceError`. Backend-neutral validation (non-empty labels, gain in range) stays on `AudioSourceConfig`.

### 1.2 New `PostProcessingConfig` section

```rust
pub struct PostProcessingConfig {
    pub premix: PremixConfig,
    pub preserve_originals: bool,        // default true: keep per-app tracks alongside the mix
    pub rewrite_track_titles: bool,      // default true: replace GSR's "app:Name" with our label
    pub codec: PostProcessAudioCodec,    // Aac | Opus — used for the mixed stream
    pub bitrate_kbps: u32,               // default 192
    pub limiter: LimiterConfig,          // alimiter params
}

pub struct PremixConfig {
    pub enabled: bool,                   // default true
    pub placement: PremixPlacement,      // First (default) | Last
    pub normalization: PremixNormalization,
    pub duration_mode: PremixDurationMode, // Longest (default) | First | Shortest
    pub track_title: String,             // default "Mixed"
}

pub enum PremixNormalization {
    AmixDivide,        // normalize=1 (safe, halves with 2 sources)
    SumThenLimit,      // normalize=0 + alimiter (DEFAULT)
    LoudnessTarget { target_lufs: f32, tp_db: f32, lra: f32 }, // loudnorm two-pass
}

pub struct LimiterConfig {
    pub limit: f32,              // alimiter limit (0.0..1.0), default 0.97
    pub attack_ms: f32,          // default 5.0
    pub release_ms: f32,         // default 50.0
}
```

### 1.3 Config migration

- The existing `recorder_config_migrates_single_audio_source` test pattern (`src/config.rs:904`) shows the project already has a migration path. Add a new migration layer: any `[[recorder.audio_sources]]` entry with only `label`/`source` parses into the new form by prefix-matching the `source` (`app:` → `Application { name }`, `app-inverse:` → `ApplicationInverse { names: vec![name] }`, `device:` → `Device { name }`, `|`-separated → `Merged`, else `Raw`). Existing configs keep working untouched.
- Gain defaults to `0.0`, both booleans to `true`. The rest of `PostProcessingConfig` comes from defaults.
- Round-trip test: serialize → deserialize → re-serialize is byte-identical.

---

## Part 2 — Application audio discovery

### 2.1 Discovery moves behind the backend trait

`discover_audio_sources` currently lives as a free function in `src/process.rs:122-147` and shells directly to `gpu-screen-recorder --list-audio-devices`. Move it behind `CaptureBackend::discover_audio_sources` (see Part 3.1) so each backend is responsible for its own enumeration mechanism:

```rust
pub enum DiscoveredAudioKind { Device, Application }

pub struct DiscoveredAudioSource {
    pub kind_hint: AudioSourceKind,   // what to insert if the user adds this entry
    pub display_label: String,        // human-readable name from the backend
    pub kind: DiscoveredAudioKind,
    pub available: bool,              // false when previously-seen app is no longer running
}
```

Note `kind_hint` is an `AudioSourceKind` (backend-neutral), not a raw string. The backend constructs it from its native enumeration (for GSR, from `--list-application-audio` output; for a future Windows backend, from WASAPI session enumeration).

**GSR backend impl specifically:** runs `--list-audio-devices` and `--list-application-audio` concurrently via `tokio::try_join!`, merges the results, tags each with `DiscoveredAudioKind`. If `--list-application-audio` errors (GSR built without `-Dapp_audio`), returns devices plus a non-fatal `AudioSourceError::PerAppUnavailable(reason)` variant that the UI surfaces as info rather than error.

Settings UI calls `backend.discover_audio_sources()` through the active `CaptureBackend` handle rather than a free function, so swapping backends doesn't require rewriting the UI.

### 2.2 Liveness refresh

Currently discovery runs once via `Message::AudioSourcesDiscovered` on settings open (`src/app/tabs/settings.rs:690`). Add a manual "Refresh" button (already exists pattern-wise, just wire it to the backend trait method), plus: re-run discovery automatically when the user opens the audio source picker, because running apps change minute-to-minute.

### 2.3 Caching of "known apps"

When a user adds `app:Foo` while Foo is running and then Foo exits, GSR will happily start the recording anyway — it just records silence until the app reappears, matching the user's intent. But the Settings UI shouldn't flag the entry as broken. Store the last-seen list of app names in a small on-disk cache (`~/.local/state/nanite-clip/known_apps.toml`) and use it to populate the dropdown even when nothing is producing audio right now — flagging entries as "not currently running" without blocking them.

---

## Part 3 — Capture backend abstraction & recorder invocation

### 3.1 `CaptureBackend` trait

New module `src/capture/mod.rs` defines the abstraction. Existing `src/recorder.rs` code that shells to `gpu-screen-recorder` moves into `src/capture/gsr.rs` as `GsrBackend` implementing the trait. A future Windows impl would live in `src/capture/wasapi.rs` or `src/capture/obs_ws.rs`.

```rust
pub trait CaptureBackend: Send + Sync {
    fn id(&self) -> &'static str;                        // "gsr", "obs-ws", "wasapi", etc.
    fn display_name(&self) -> &'static str;              // shown in Settings

    // Capability introspection — drives Settings UI capability gating.
    fn capabilities(&self) -> CaptureCapabilities;

    // Discovery (Part 2).
    async fn discover_audio_sources(&self)
        -> Result<Vec<DiscoveredAudioSource>, AudioSourceError>;

    // Translate a user-intent AudioSourceKind into whatever the backend needs.
    // For GSR this produces a `-a` string; for WASAPI it resolves to a session GUID
    // or process handle stored in `BackendAudioArg::Opaque`.
    fn translate_audio_source(&self, kind: &AudioSourceKind)
        -> Result<BackendAudioArg, AudioSourceError>;

    // Start a capture session. Returns an opaque session handle.
    fn spawn_replay(&self, request: CaptureRequest)
        -> Result<Box<dyn CaptureSession>, CaptureError>;
}

pub struct CaptureCapabilities {
    pub per_app_audio: bool,
    pub application_inverse: bool,
    pub merged_tracks: bool,              // backend can merge in-capture (GSR's '|' syntax)
    pub portal_session_restore: bool,     // Wayland-only concept
    pub replay_buffer: bool,              // does the backend support a rolling buffer?
    pub hdr: bool,
    pub cursor_capture: bool,
}

pub enum BackendAudioArg {
    GsrString(String),                    // GSR "-a" value
    Opaque(Box<dyn Any + Send + Sync>),   // backend-specific handle (WASAPI session, etc.)
}

pub trait CaptureSession: Send {
    fn save_clip(&mut self, length: ClipLength) -> Result<(), CaptureError>;
    fn poll_results(&mut self) -> Vec<SavePollResult>;
    fn stop(&mut self) -> Result<(), CaptureError>;

    // Snapshot of what the backend actually committed to record at session-start time.
    fn active_audio_layout(&self) -> &[ResolvedAudioSource];
}

pub struct ResolvedAudioSource {
    pub config: AudioSourceConfig,         // the user's intent
    pub resolved_display: String,          // human-readable resolution, e.g. "app:PlanetSide2 → pid 12345"
}
```

No `pid()` accessor: POSIX PIDs are a GSR-internal concern and any save-signalling the recorder needs to do (GSR's `SIGUSR2`-based "save now" protocol) stays encapsulated inside `GsrCaptureSession`. The trait's `save_clip` hides that mechanism from the rest of the app, which is the right seam if a Windows backend signals saves via a named pipe or in-process RPC instead of a signal.

### 3.2 Backend capabilities drive the Settings UI

The tracklist UI (§5.1) consults `backend.capabilities()` to enable or disable features:

- `per_app_audio = false` → hide the "Discovered apps" dropdown and grey out `Application` / `ApplicationInverse` entries with a "Not supported by <backend>" tooltip.
- `application_inverse = false` → hide the `ApplicationInverse` option; existing config entries of that kind render in an error state.
- `merged_tracks = false` → the "Merge selected" action auto-expands to separate `[[audio_sources]]` entries, with a toast explaining the fallback. Post-processing then mixes them (§4.3) — the user experience is preserved.
- `portal_session_restore = false` → hide the portal restore checkbox.

This keeps the UI honest about backend limitations without scattering `#[cfg(target_os)]` through the view code.

### 3.3 Capability-based Merged fallback

When the active backend can't do in-capture merging but a user's config contains `AudioSourceKind::Merged { entries }`:

1. At `spawn_replay` time, the backend receives each inner entry as a **separate** audio source, producing N streams in the recording.
2. The `ResolvedAudioSource::resolved_display` notes they're part of a logical group (e.g. "merge group #3").
3. Post-processing (Part 4) sees the group marker in the layout snapshot and mixes those N streams back into one in the final file — with the `Merged` entry's label as the track title and its gain applied.

Users write the config once; the fallback is entirely internal.

### 3.4 `GsrBackend` as the first concrete impl

`GsrBackend` is a thin struct wrapping what's currently in `src/recorder.rs`. Implementation notes:

- `id()` returns `"gsr"`.
- `capabilities()` returns `per_app_audio: true` (gated on `-Dapp_audio` probe at startup), `application_inverse: true`, `merged_tracks: true`, `portal_session_restore: true`, `replay_buffer: true`, `hdr: true`, `cursor_capture: true`.
- `translate_audio_source` produces the GSR `-a` strings (`default_output`, `device:<name>`, `app:<name>`, `app-inverse:<name>`, `<a>|<b>|<c>` for merged). Rejects `app:` + `app-inverse:` in the same merged entry per `reference/gpu-screen-recorder/src/main.cpp:3040`.
- `AudioSourceKind::ApplicationInverse { names }` with `names.len() > 1` is rejected as `AudioSourceError::Unsupported { capability: "multi-name application inverse" }`. GSR's `app-inverse:` takes a single name — multiple exclusions require either a merged config of separate inverses (also unsupported by GSR's `|` per the rule above) or a future backend. Settings UI surfaces this as a validation error on the row, not a silent truncation.
- `spawn_replay` calls the existing GSR command construction (`src/recorder.rs:75-104`), returning a `GsrCaptureSession` that owns the `Child`, stdout/stderr buffers, and save-signal state machine. Everything currently in `src/recorder.rs:176-381` moves into `GsrCaptureSession` methods.

### 3.5 Backend selection in config

Add `capture.backend = "gsr"` to the config root. In v1 only `"gsr"` is accepted; invalid values produce a startup error. Future cross-platform builds add `"wasapi"`, `"obs-ws"`, etc. Backend selection happens once at `App::new` — switching at runtime requires a restart, since active recordings are backend-specific.

### 3.6 Record the track layout snapshot at start

Currently `update_config` (`src/recorder.rs:59`) can mutate config mid-session while the child is still running. The backend's `spawn_replay` captures the audio source list *at session-start time* into the `CaptureSession` via `active_audio_layout()`. Post-processing (Part 4) uses this snapshot, not `self.config`, because the recording already committed to that layout.

### 3.7 Persist the layout so saves survive an app restart

Dump the snapshot to a sidecar JSON file alongside per-backend state (beside the GSR portal token path, `src/recorder.rs:453-456`; equivalent per-backend paths for future backends). On startup, if a recording is still running from a previous session (not common but possible with crash recovery), the layout can be rehydrated. This is belt-and-braces; the primary source of truth is ffprobe on the saved file (§4.2).

---

## Part 4 — Post-processing pipeline (`src/recorder.rs` → new `src/post_process.rs`)

### 4.1 Extract into its own module

`spawn_trim` in `src/recorder.rs:383-412` becomes `post_process::run` in a new `src/post_process.rs`. Recorder calls into it. Rationale: post-processing is growing from "one ffmpeg trim" into "trim + probe + multi-filter remix + metadata rewrite + conditional re-encode", and it shouldn't live inside the recorder state machine.

New shape:
```rust
pub struct PostProcessRequest {
    pub input: PathBuf,
    pub output: PathBuf,          // same as input for in-place rewrite, or temp path
    pub trim: Option<TrimSpec>,   // Some(TrimSpec { tail_secs }) or None
    pub audio_layout: Vec<AudioSourceConfig>,  // captured at record start
    pub post_processing: PostProcessingConfig,
}

pub enum PostProcessResult {
    Unchanged,                    // no trim + no remix + no metadata rewrite → skipped
    Rewritten { output: PathBuf, plan: PostProcessPlan },
}

pub struct PostProcessPlan {
    pub trimmed: bool,
    pub premix_stream_index: Option<usize>,  // which output audio stream is the mix
    pub preserved_stream_count: usize,
    pub codec_used: PostProcessAudioCodec,
}
```

### 4.2 ffprobe the saved file to learn the real audio layout

Add `probe_audio_streams(path)` next to `probe_video_resolution_blocking` (`src/recorder.rs:458`):

```
ffprobe -v error -select_streams a \
  -show_entries stream=index,codec_name,channels,sample_rate:stream_tags=title \
  -of json <file>
```

Returns `Vec<ProbedAudioStream { index, codec, channels, sample_rate, title }>`. The post-processor cross-references this with `audio_layout`:

- **Layout matches exactly** (count + order): use `audio_layout` labels for track titles and gain, treating stream N as source N.
- **Layout count disagrees with probe count** (e.g. GSR dropped a source that didn't exist on PipeWire at start): fall back to probe order, log a warning, use GSR's own titles, and skip gain/label rewrites for unmatched streams. Never block the save.
- **No audio at all** (probe returns empty): skip premix entirely, just trim if needed.

### 4.3 Build the ffmpeg invocation declaratively

Single helper `build_ffmpeg_args(request, probed_streams) -> Vec<OsString>` with exhaustive unit tests covering every permutation: trim-only, remix-only, trim+remix, preserve=false, premix=false, 1-stream, 5-stream, muted source, inverse gain, loudnorm mode, limiter mode, AAC vs opus, first vs last placement.

**Post-process trigger predicate.** Post-processing is not unconditional. The background job only fires when the following predicate is true for the saved clip:

```rust
fn needs_post_process(req: &PostProcessRequest, probed: &[ProbedAudioStream]) -> bool {
    req.trim.is_some()
        || (probed.len() >= 2 && req.post_processing.premix.enabled)
        || (probed.len() >= 1 && req.post_processing.rewrite_track_titles)
        || req.post_processing.force_rewrite // escape hatch used by future consumers
}
```

If the predicate is false, the post-process stage short-circuits to `PostProcessResult::Unchanged` and no ffmpeg process is launched. The caller (`record_save_outcome`, §4.6) uses this to decide whether to enqueue a `BackgroundJobKind::PostProcess` job at all, and to set the initial `post_process_status` on the clip row (§4.8) — `NotRequired` when the predicate is false, `Pending` otherwise. This keeps single-source `default_output` recordings on the zero-ffmpeg fast path.

The general shape:

```
ffmpeg -y
  [input options: -sseof -<N> if trim]
  -i <input>
  -filter_complex "<graph>"          # only if premix enabled AND ≥1 unmuted source
  -map 0:v
  [-map "[mixed]"]                   # if premix enabled
  [-map 0:a]                         # if preserve_originals
  -c:v copy
  -c:a copy
  -c:a:<mix_index> <codec> -b:a:<mix_index> <bitrate>
  -metadata:s:a:<i> title="<label>"  # for each audio output stream
  [-movflags +faststart]             # if output is mp4 — improves seek-start for web playback
  <output>
```

Key decisions baked into the builder:

**Filter graph construction.** For N unmuted sources with gains g₁..gₙ:
```
[0:a:0]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,volume=<g1>dB[a0];
[0:a:1]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,volume=<g2>dB[a1];
...
[a0][a1]...[aN]amix=inputs=N:duration=<mode>:normalize=<0|1>[mix_raw];
[mix_raw]alimiter=limit=<L>:attack=<A>:release=<R>[mixed]
```

> **ffmpeg version floor:** the filter combinations in this section (`loudnorm print_format=json`, `aformat` with explicit `channel_layouts`, `amix normalize=0`) require **ffmpeg ≥ 4.2**. The version is probed at startup and the post-process pipeline is hard-disabled on older builds — see §8.3.

**Why the aresample + aformat prelude is mandatory.** `amix` requires every input leg to share the same sample rate, sample format, and channel layout. PipeWire can hand GSR sources at 44.1 kHz, 48 kHz, or 96 kHz (Discord voice vs. game vs. pro-audio app), mono or stereo, planar or packed. Passing heterogeneous inputs straight into `amix` either errors out or silently downmixes the wrong leg. The fix is to normalize every leg to a canonical internal format **before** the gain stage: **48 kHz, stereo, fltp (planar float)**. 48 kHz is the PipeWire default and matches every codec we target; stereo is what every consumer expects; fltp is the internal format `amix` and the downstream encoders prefer. The originals preserved via `-map 0:a` are stream-copied and are **not** rewritten — their native rate/layout stays intact for editor workflows. Only the mix stream is normalized, and only inside the filter graph.

For the `LoudnessTarget` mode, swap the `alimiter` stage for `loudnorm=I=<lufs>:TP=<tp>:LRA=<lra>` and emit a two-pass run. **First pass** runs `loudnorm=...:print_format=json` and ffmpeg prints a JSON blob on stderr containing `input_i`, `input_tp`, `input_lra`, `input_thresh`, and `target_offset`. The post-processor captures stderr to a buffer, locates the JSON object (last `{ ... }` block on stderr — ffmpeg tucks it after its usual logging), parses with `serde_json`, and extracts the five measured fields. **Second pass** re-runs the full filter graph with `loudnorm=I=<lufs>:TP=<tp>:LRA=<lra>:measured_I=<input_i>:measured_TP=<input_tp>:measured_LRA=<input_lra>:measured_thresh=<input_thresh>:offset=<target_offset>:linear=true:print_format=summary`. Two passes is the only way loudnorm gives deterministic, non-lookahead-distorted results. Regex-scraping the human-readable log is fragile across ffmpeg versions; JSON is the only stable contract ffmpeg offers for these values.

**Stream placement.** `PremixPlacement::First` puts `-map "[mixed]"` before `-map 0:a`, making the mix stream index 0 — this is what Discord/browsers play inline. `Last` reverses it for people who edit in Premiere/DaVinci and prefer originals first.

**Single source corner case.** If only one unmuted source exists and gain is 0 dB, skip the filter graph entirely — just do `-map 0:v -map 0:a -c copy` with metadata rewrites. No re-encode at all. This is the dominant case for users who configure one `default_output` and never touch the rest.

**Preserve + premix both disabled corner case.** Equivalent to the pre-existing trim behaviour; reuse the same code path. Fixes the latent multi-track drop bug in the current trim (`src/recorder.rs:391-400`) because the new builder always emits `-map 0`.

### 4.4 Atomic rewrite semantics

Current `spawn_trim` writes to a temp sibling path then renames over the original (`src/recorder.rs:390, 410`). Keep that shape:

1. Write to `<path>.post.<ext>` sibling.
2. On ffmpeg success: ffprobe the temp file to verify it's playable (catches silent corruption).
3. `rename(temp → final)`.
4. On any error: delete temp, leave original untouched, bubble `PostProcessError` to the save poll loop.

### 4.5 Run post-processing off the iced runtime

Current `spawn_trim` is synchronous inside `poll_save_results` (`src/recorder.rs:207-219`). With remux re-encoding audio it can take several seconds on a long clip — that's too long to block the UI thread and the save poll loop.

Move to the background job manager (`src/background_jobs.rs`) with a new job kind:

```rust
pub enum BackgroundJobKind {
    StorageTiering,
    Upload,
    Montage,
    DiscordWebhook,
    PostProcess,          // new
}
```

Wire it through the existing `BackgroundJobSuccess`/`BackgroundJobNotification` infrastructure (`src/background_jobs.rs:121-152`). The recorder's `poll_save_results` still reports `SavePollResult::Saved`, but it does so *before* the post-process job finishes.

**Per-kind concurrency limiter.** `BackgroundJobManager::start` currently spawns every job on its own tokio task with no ordering or throttling between kinds (`src/background_jobs.rs:282`). That's fine for the existing `StorageTiering` / `DiscordWebhook` / `Upload` workloads which are mostly I/O-bound. `PostProcess` is different: a two-pass loudnorm over a long clip can peg an entire CPU core for tens of seconds, and the user can easily save three clips in quick succession during an action sequence. Running three of these in parallel starves everything else on the box, including the live recorder.

Solution: add a **per-kind async semaphore** to `BackgroundJobManager`:

```rust
struct BackgroundJobManager {
    // ... existing fields ...
    kind_limits: HashMap<BackgroundJobKind, Arc<tokio::sync::Semaphore>>,
}

impl BackgroundJobManager {
    fn new(...) -> Self {
        let mut kind_limits = HashMap::new();
        kind_limits.insert(BackgroundJobKind::PostProcess,    Arc::new(Semaphore::new(1)));
        kind_limits.insert(BackgroundJobKind::Upload,         Arc::new(Semaphore::new(2)));
        kind_limits.insert(BackgroundJobKind::Montage,        Arc::new(Semaphore::new(1)));
        kind_limits.insert(BackgroundJobKind::StorageTiering, Arc::new(Semaphore::new(1)));
        kind_limits.insert(BackgroundJobKind::DiscordWebhook, Arc::new(Semaphore::new(4)));
        // ...
    }
}
```

Inside the spawn closure for each job, acquire a permit before running the inner future:

```rust
let permit = limits.get(&kind).cloned();
tokio::spawn(async move {
    let _guard = match permit {
        Some(sem) => Some(sem.acquire_owned().await.expect("semaphore never closed")),
        None => None,
    };
    run_job(...).await
});
```

The guard drops when the future completes, releasing the next waiter. Jobs queued behind a held permit sit in `tokio::sync::Semaphore::acquire_owned().await` — cheap, no polling, no ordering constraints beyond "same kind is serialized to N at a time". Different kinds run concurrently as before.

`PostProcess` defaults to `max_concurrent = 1` (locked). A rapid save burst queues each clip behind the one ahead; the Clips tab shows each as `post_process_status = Pending` until its turn (§4.8). The semaphore limits are not user-tunable in v1 — they're picked for correctness, not performance tuning.

Add a follow-up notification:

```rust
pub enum SavePollResult {
    Saved { path: PathBuf, duration: ClipLength, post_process_job: Option<BackgroundJobId> },
    SaveFailed(String),
}
```

The `BackgroundJobSuccess::PostProcess { clip_id, final_path, plan: PostProcessPlan }` variant lets `app.rs` update the DB row with the new path (if the rename produced a different name) and show a toast.

### 4.6 Interaction with `save_delay_secs`

`record_save_outcome` (`src/app.rs:2535`) currently gates DB linkage on the save outcome arriving. Post-processing should run **after** the DB link is established, not before — the `PendingClipLink` needs a `clip_id` so the background job can reference it on completion. Sequence:

```
SavePollResult::Saved → record_save_outcome → resolve_pending_clip_links
  → Message::ClipPathLinked (success) → spawn PostProcess background job
  → (later) BackgroundJobNotification::Finished → update clip path + DB metadata
```

This matches the existing `rename_saved_clip` flow in `resolve_pending_clip_links` (`src/app.rs:2593-2622`), which already handles a path rewrite after save. Post-processing is the second rewriter in that chain.

### 4.7 Persist track layout in the clips DB

Add a new table that records the audio track layout per clip:

```rust
pub mod clip_audio_tracks {
    pub struct Model {
        pub id: i64,
        pub clip_id: i64,
        pub stream_index: i32,     // position in the final file (post post-process)
        pub role: String,          // "mixed" | "source"
        pub label: String,
        pub gain_db: f32,
        pub muted: bool,
        pub source_kind: String,   // serialized AudioSourceKind discriminant
        pub source_value: String,  // e.g. "app:PlanetSide2"
    }
}
```

Written after post-processing completes. Enables:
- Clips tab to display per-track info and offer "play track N" via mpv.
- YouTube uploader (§6.1) knowing which stream to publish.
- Future: re-render a clip with different gains without re-recording.

Schema migration added to `src/db/migrations.rs` following the existing pattern (`src/db/migrations.rs:215-230`).

### 4.8 Clip-level `post_process_status` state machine

The `clip_audio_tracks` table answers *"what's in the final file"*, but the Clips tab also needs to know *"is this clip's post-processing still happening, did it fail, or was it never required"* before it lets the user upload or add the clip to a montage. Add a new column to the `clips` table:

```rust
pub enum PostProcessStatus {
    NotRequired,   // needs_post_process() returned false at save time
    Pending,       // job enqueued or running
    Completed,     // job finished, clip_audio_tracks rows inserted
    Failed,        // job errored; original file untouched, retry available
    Legacy,        // row existed before this feature shipped; layout unknown
}
```

Stored as a TEXT column with a CHECK constraint (SeaORM enum), nullable disallowed. Transitions:

```
 ┌──────────────┐
 │ (new clip)   │
 └──────┬───────┘
        │ save_clip succeeds
        ▼
 ┌──────────────────────────┐
 │ needs_post_process() ?   │
 └──┬────────────────────┬──┘
    │ false              │ true
    ▼                    ▼
 NotRequired          Pending ──(job starts)──▶ Pending
                        │                         │
                        │ success                 │ failure
                        ▼                         ▼
                    Completed                   Failed
                                                  │
                                                  │ user clicks "Retry"
                                                  ▼
                                                Pending
```

**Initial state.** `record_save_outcome` sets the status when it writes the `clips` row:
- `NotRequired` if `needs_post_process()` is false (§4.3 predicate).
- `Pending` if the predicate is true, set in the same transaction that enqueues the `BackgroundJobKind::PostProcess` job.

**Terminal transitions.** The `BackgroundJobSuccess::PostProcess` handler in `app.rs` flips `Pending → Completed` alongside inserting `clip_audio_tracks` rows; the error branch flips `Pending → Failed` and stores the failure message in a companion column (`post_process_error: Option<String>`) for display in the Clips tab.

**Legacy migration.** The migration that adds the column (§7.1) backfills every existing row with `Legacy`. The Clips tab treats `Legacy` identically to `NotRequired` for action gating (upload, montage, Discord) but displays a subtle "legacy layout" badge so power users know why no track list is available. A future housekeeping job could re-probe legacy clips to convert them to `Completed` with a populated `clip_audio_tracks`, but that's out of scope here.

**Gating behavior.** The Clips tab reads `post_process_status` when the user clicks upload / add-to-montage:
- `NotRequired`, `Completed`, `Legacy` → action proceeds normally.
- `Pending` → action is disabled with a "Post-processing in progress…" spinner badge; re-enables automatically on the notification.
- `Failed` → action is disabled with a red "Post-processing failed" badge. A context menu offers "Retry" (re-enqueues the job, status → `Pending`) and "Use original anyway" (flips status → `NotRequired` and enables actions at the cost of the multi-track layout).

**Interaction with concurrency limiter.** Because `PostProcess` is serialized to `max_concurrent = 1` (§4.5), a rapid save burst can leave several clips sitting in `Pending` simultaneously while the first one runs. The badge copy ("Queued — N ahead") reflects the semaphore position.

**Persistence is the single source of truth.** No in-memory `HashMap<ClipId, JobState>` — every transition writes the DB first, then fires a message to the UI. If the app crashes mid-job, the next start-up finds a row in `Pending` whose background job is gone. The startup path scans for `post_process_status = Pending` rows with no corresponding live job and transitions them to `Failed` with a "interrupted by shutdown" message so the user can retry.

---

## Part 5 — Settings UI (`src/app/tabs/settings.rs`)

### 5.1 Replace the "Available Source → Add" flow

Current (`src/app/tabs/settings.rs:824-854`) is a `pick_list` of discovered devices + a single "Add" button, producing read-only rows below (`src/app/tabs/settings.rs:1277-1307`). Rebuild into a proper tracklist:

```
┌─ Audio Capture ────────────────────────────────────────────────┐
│ Discovered devices ▾  [device:alsa_output.pci-0000_00_1b.0]   │
│ Discovered apps    ▾  [app:PlanetSide2]          [+ Add]      │
│ ──────────────────────────────────────────────────────────── │
│ # │ Label         │ Source             │ Gain │ Mute │ Mix │ ▲│
│ 1 │ Game          │ app:PlanetSide2    │ ±0dB │ ☐    │ ☑   │ ▼│
│ 2 │ Voice         │ app:TeamSpeak      │ +3dB │ ☐    │ ☑   │ ▼│
│ 3 │ Desktop (ref) │ default_output     │ ±0dB │ ☐    │ ☐   │ ▼│
│ ──────────────────────────────────────────────────────────── │
│ [+ Add custom source] [+ Merge selected into one track]      │
└────────────────────────────────────────────────────────────────┘
```

Each row is editable: label (text), gain (slider −60..+12 dB with **0.5 dB step** — holding Ctrl or Shift while dragging switches to **0.1 dB fine-adjust** for precise level-matching), mute (checkbox), include-in-mix (checkbox), up/down/remove. The slider displays the current value with one decimal place and shows `+0.0 dB` at the detent. "Merge selected" takes two or more checked rows and collapses them into an `AudioSourceKind::Merged` entry (GSR's `|` syntax on a single `-a`).

"Add custom source" opens a free-text field for `Raw { value }` — the escape hatch.

### 5.2 Post-processing section

New subsection below Audio Capture:

```
┌─ Audio Post-Processing ────────────────────────────────────────┐
│ Enable premix track          [✓]                              │
│ Premix placement              [First ▾]   (First|Last)        │
│ Normalization                 [Sum + limiter ▾]                │
│   Sum + limiter               → alimiter params ▶             │
│   Amix divide                 → amix=normalize=1              │
│   Loudness target (EBU R128)  → -14 LUFS, -1 dBTP, 11 LRA ▶  │
│ Mixed track title             [Mixed]                         │
│ Codec                         [AAC ▾]                         │
│ Bitrate                       [192] kbps                      │
│ Preserve original tracks      [✓]                              │
│ Rewrite track titles          [✓]                              │
│                                                                │
│ ⚠ ffmpeg not found — post-processing disabled.                │
└────────────────────────────────────────────────────────────────┘
```

Disabled state surfaces the existing `recorder.has_ffmpeg()` check (`src/recorder.rs:63`).

### 5.3 Preview button (argv only)

"Preview generated ffmpeg command" button next to the section header. Runs `build_ffmpeg_args` against the current draft config with a synthesized 3-stream input and shows the full argv in a read-only textarea. Invaluable for debugging and building user trust in the tool before they hit save. Pure function, no ffmpeg exec needed. **No live-record preview** — argv display only.

**No-audio-config behaviour.** If the user has zero audio sources configured, the preview still renders something meaningful rather than showing an error or an empty box. The preview synthesizes the baseline post-process command the runtime would emit — either the pure trim-only shape (`ffmpeg -y -sseof -<N> -i <input> -map 0 -c copy <output>`) if `trim.tail_secs > 0`, or the trivial stream-copy no-op (`ffmpeg -y -i <input> -map 0 -c copy <output>`) if trim is also disabled. Both are rendered with a header comment — `# preview: no audio sources configured → mix pipeline disabled, showing baseline trim behaviour` — so the user understands they're looking at what the tool would actually run, not an error state. This also makes the preview a reliable smoke test for the `needs_post_process()` predicate (§4.3): if the no-audio preview shows the trivial no-op, the runtime will also skip the post-process job entirely on save.

---

## Part 6 — Distribution path fixes

### 6.1 YouTube uploader (`src/uploads.rs:145-221`)

**Problem:** YouTube ingest mixes all audio streams into one, layering the premix over the originals and duplicating everything.

**Fix:** Before upload, if the clip has a `clip_audio_tracks` row flagged `role = "mixed"`, transcode-select the mix stream only:

```
ffmpeg -i <clip> -map 0:v -map 0:a:<mix_index> -c copy <youtube_temp.mp4>
```

If no mix stream exists (user disabled premix) and the clip has multiple audio streams, fall back to `-map 0:a:0` with a warning surfaced in the job detail. Clean up `<youtube_temp.mp4>` in a `Drop` guard or `scopeguard` after the upload finishes.

This transcode is stream-copy (no re-encode), adds <1s. Preferable to teaching the uploader about track semantics inline because it keeps the on-disk archive full-fidelity while the upload gets a compat-friendly version.

### 6.2 Copyparty uploader (`src/uploads.rs:66-143`)

No action needed — copyparty is generic file hosting, just serves the file. Multi-track mp4 plays correctly in browsers (track 0 only, which is fine). Document this in the settings tooltip.

### 6.3 Discord webhook (`src/discord.rs:143`)

Discord webhook attachments play track 0 only inline. Because §4.3 places the mix at index 0 by default, this Just Works for the default config. If a user configures `PremixPlacement::Last` they've explicitly opted into "originals first", and Discord will play whatever's at index 0 — document that trade-off in the placement dropdown's tooltip.

### 6.4 Montage builder (`src/montage.rs:108-140`) — automatic normalization

`validate_concat_inputs` uses `probe_media_signature` (`src/montage.rs:142-165`) to require identical stream layouts across concat inputs — it already inspects per-stream codec/channels, so a mixed+originals clip will refuse to concat with a legacy single-track clip.

**Fix:** Before running concat, scan every selected clip's probed audio layout. If *any* two clips differ in audio stream count, stream order, or mix-stream presence, trigger the normalization pre-pass.

**Canonical target: mix-only.** Each mismatched clip is remuxed into a single-video + single-audio temp file containing only the clip's **mix stream**:

- If the clip has `post_process_status = Completed` and a `clip_audio_tracks` row with `role = "mixed"`, use `-map 0:a:<mix_index>` to extract that stream. Stream-copy the video. The audio may need a transcode if the source clip's mix codec differs from the montage target codec (see below), but in the common case where every clip was produced by the same nanite-clip install, it's all AAC @ 192 kbps and stream-copy works.
- If the clip has no mix stream (legacy single-track clip, `post_process_status = Legacy` or `NotRequired`), extract `-map 0:a:0`. The legacy clip becomes the "mix" for montage purposes.
- If the clip's mix codec disagrees with the dominant codec across the selection, that clip's audio is re-encoded to the dominant codec (majority vote; AAC @ 192 kbps wins ties because it's the locked default). Video is always stream-copied.

This gives the concat demuxer a uniform layout: exactly one video stream + exactly one AAC audio stream per input. `validate_concat_inputs` is then called a second time on the normalized temps and must succeed — if it doesn't, abort with a "montage normalization failed" error rather than attempting a fallback.

**Diagnostic toast copy.** When normalization runs, the UI shows a toast:

> **Montage:** Normalizing N clip(s) to a single mix track before concat. This keeps multi-track clips compatible with legacy clips in the montage. No original files are modified.

On completion, a second toast:

> **Montage ready.** Merged N clips (M normalized). Output: `<path>`.

**Temp cleanup.** Each normalized temp lives in the existing montage working directory alongside the concat list file (`src/montage.rs:54`). All temps are deleted unconditionally at the end of the job — including on error — via a scopeguard-style cleanup block wrapping the `create_concat_montage_blocking` body.

**When normalization is skipped.** If every clip in the selection has identical layouts — same stream count, same codecs, same channels — concat runs directly on the originals with no temps, same as today. This is the fast path for selections made entirely from post-enhancement clips.

Rationale: the whole point of this enhancement is to make multi-track transparent to the user. Restricting the canonical form to mix-only (rather than trying to preserve per-source tracks through concat) keeps the normalization pass tractable — concat demuxer is happiest with matching layouts, and asking users to resolve track-layout ambiguity across heterogeneous clips is worse UX than silently losing originals in the montage output. Originals are never touched on disk; only the montage output is mix-only.

---

## Part 7 — Schema, persistence, and the database

### 7.1 New migration

Add to `src/db/migrations.rs` following the existing `has_table` / `has_column` guard pattern:

1. **Create `clip_audio_tracks` table** per §4.7. Add index on `(clip_id, stream_index)`.
2. **Add columns to `clips`:**
   - `post_process_status TEXT NOT NULL DEFAULT 'Legacy'` with a `CHECK` constraint limiting values to `NotRequired | Pending | Completed | Failed | Legacy` (§4.8).
   - `post_process_error TEXT NULL` — populated only when status is `Failed`.
3. **Backfill:** every existing `clips` row is assigned `post_process_status = 'Legacy'` by the column default. No further backfill — the Clips tab treats `Legacy` as "assume single track, upload / montage allowed" (§4.8 gating table).
4. **Startup sweep:** on app startup, a one-shot query flips any `post_process_status = 'Pending'` rows (left over from a crash) to `'Failed'` with `post_process_error = 'interrupted by shutdown'`. This is in `App::new` before the Clips tab loads, so the user never sees a stuck-in-pending clip after a crash.

All three steps land in one migration file — the clip_audio_tracks table and the clips columns are co-dependent and need to be visible in the same schema version.

### 7.2 Entity definition

Add `pub mod clip_audio_tracks` to `src/db/entities.rs:288` following the existing `clips` struct style (`src/db/entities.rs:288-329`). Extend `clips::Model` with `post_process_status: PostProcessStatus` (SeaORM enum derived via `DeriveActiveEnum`) and `post_process_error: Option<String>`.

### 7.3 Repository methods on `ClipStore`

- `insert_audio_tracks(clip_id, Vec<ClipAudioTrack>)` — called from the PostProcess job's success handler.
- `load_audio_tracks(clip_id) -> Vec<ClipAudioTrack>` — called from clip detail view.
- `delete_audio_tracks(clip_id)` — called from clip deletion path to keep FK clean.
- `set_post_process_status(clip_id, status, error: Option<String>)` — single atomic helper used by every transition in §4.8. All callers (initial enqueue, success handler, failure handler, retry, "use original anyway", startup sweep) go through this method so the state machine has exactly one write path.
- `clips_pending_post_process() -> Vec<ClipId>` — called by the startup sweep in §7.1 step 4.

---

## Part 8 — Error handling, diagnostics, and logging

### 8.1 Error taxonomy

New errors in `src/post_process.rs`:
```rust
pub enum PostProcessError {
    FfmpegMissing,
    FfprobeFailed(String),
    LayoutMismatch { expected: usize, actual: usize },
    FilterGraphBuild(String),
    FfmpegExit { status: ExitStatus, stderr: String },
    VerificationFailed(String),   // post-rename ffprobe couldn't open the result
    Io(String),
}
```

All errors short-circuit the rewrite, leave the original file intact, and bubble up as `BackgroundJobNotification::Finished { error: Some(...) }`. The clips tab shows a "Post-processing failed — using original" badge on the affected clip.

### 8.2 Logging & traces

Every stage emits `tracing` spans with the clip path, ffmpeg argv (redacted of nothing — these are local files), and durations. Add a new tracing target `nanite_clip::post_process` so users can enable just this subsystem via `RUST_LOG=nanite_clip::post_process=debug`.

### 8.3 Status tab surface + ffmpeg version probe

**Version probe.** On startup, the post-process module runs `ffmpeg -version` once, parses the first line (`ffmpeg version 6.1.1 Copyright…`), and extracts the semver. The minimum supported version is **4.2** — this is the floor for the three filters the pipeline depends on:

- `loudnorm` with `print_format=json` (added in 4.1, but 4.2 is the first release where the JSON output is stable across platforms).
- `aformat` with reliable `channel_layouts=stereo` normalization (pre-4.2 versions silently passed through mismatched layouts in some build configurations).
- `amix` with `normalize=0` (added in 4.1; available in 4.2).

The parsed version is cached in `FfmpegCapabilities { present: bool, version: Option<semver::Version>, meets_floor: bool }` and stored on the `App` at startup, next to the existing `has_ffmpeg()` boolean (`src/recorder.rs:63`). When `meets_floor = false`, the post-process pipeline is disabled the same way `!has_ffmpeg()` disables it today — `needs_post_process()` (§4.3) is gated behind `meets_floor`, and every new save writes `post_process_status = NotRequired` with a companion error log entry. Existing multi-track clips are still playable; only the rewrite path is disabled.

Non-standard ffmpeg builds (unusual version strings, git snapshots without a parseable version) are treated as "unknown version — assume supported, warn in status tab". Rejecting them outright would punish users on distros that ship nightly builds.

**Status tab rows.** `src/app/tabs/status.rs` currently shows ffmpeg availability implicitly. Add an explicit "Audio post-processing" row:
- ✅ ffmpeg ≥ 4.2 present + config valid
- ⚠ ffmpeg present but version unknown (non-standard build) — post-processing enabled with a warning
- ❌ ffmpeg present but version < 4.2 — post-processing disabled, "Upgrade ffmpeg to 4.2 or later" hint
- ⚠ ffmpeg present but config has validation errors
- ❌ ffmpeg missing — post-processing disabled
- ℹ GSR built without app_audio — per-app capture unavailable

The last row comes from caching the `AudioSourceError::PerAppUnavailable` state observed during discovery (§2.1). The version-floor row is surfaced during discovery of the `FfmpegCapabilities` struct at startup.

---

## Part 9 — Testing strategy

### 9.1 Unit tests

- `GsrBackend::translate_audio_source` — exhaustive coverage of every `AudioSourceKind` variant, including:
  - `DefaultOutput` → `default_output`, `DefaultInput` → `default_input`.
  - `Device { name }` → `device:<name>` with shell-safe round-trip.
  - `Application { name }` → `app:<name>`.
  - `ApplicationInverse { names: vec!["X"] }` → `app-inverse:X`.
  - `ApplicationInverse { names: vec!["X", "Y"] }` → returns `AudioSourceError::Unsupported { capability: "multi-name application inverse" }` (§3.4).
  - `Merged { entries }` with pure device entries → `a|b|c`.
  - `Merged` containing both `Application` and `ApplicationInverse` → returns `AudioSourceError::Unsupported` per `reference/gpu-screen-recorder/src/main.cpp:3040`.
  - `Raw { backend_id: "gsr", value: "…" }` → passthrough; `Raw { backend_id: "wasapi", … }` → `AudioSourceError::WrongBackend`.
- `GsrBackend::capabilities()` — asserts the expected capability bits are set.
- Config migration — legacy `[[recorder.audio_sources]]` rows parse into new `AudioSourceKind` variants correctly, round-trip stable; no `AudioSourceKind::to_gsr_arg` method on the type (compile-time check via a negative trait bound or a doc-test).
- `needs_post_process` predicate (§4.3) — truth-table coverage: trim-only, premix-only, rewrite-titles-only, all-three, all-off, empty-probe, single-stream-probe.
- `build_ffmpeg_args` — snapshot tests per the permutation matrix in §4.3. Use `insta` if already in the tree; otherwise plain `assert_eq!` on `Vec<OsString>`. Snapshots cover:
  - 1-stream fast path (no filter graph).
  - N-stream mix with every normalization mode.
  - LoudnessTarget two-pass — first-pass and second-pass argv both rendered.
  - Preserve-on vs preserve-off stream mapping.
  - `PremixPlacement::First` vs `Last`.
- Filter graph construction — for N=1..5 sources, with various gain/mute combinations. Specifically asserts every leg begins with `aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo` before the `volume` stage (§4.3 sample-rate normalization).
- Loudnorm JSON parser — fixtures under `tests/fixtures/loudnorm/` containing real ffmpeg stderr output; parser extracts `input_i` / `input_tp` / `input_lra` / `input_thresh` / `target_offset` and handles the "JSON blob embedded in a sea of non-JSON log lines" case (§4.3).
- `probe_audio_streams` parser — sample ffprobe JSON fixtures (stored under `tests/fixtures/ffprobe/`).
- `BackgroundJobManager` per-kind semaphore (§4.5) — spawn 3 `PostProcess` jobs, assert they serialize via the semaphore (timing test with mocked inner futures); spawn 3 `DiscordWebhook` jobs, assert they run concurrently (4-permit capacity). Use `tokio::time::pause()` for deterministic timing.
- `PostProcessStatus` state machine (§4.8) — every transition (`NotRequired | Pending | Completed | Failed | Legacy`) goes through `set_post_process_status`; round-trip the enum through SeaORM's `DeriveActiveEnum` fixtures; startup-sweep logic flips `Pending → Failed` with the interrupted-by-shutdown message.
- Montage normalization policy (§6.4) — given mixed selections (all-legacy, all-completed, mixed, codec-disagreement), assert the temp-file layout is mix-only and validates on the second `validate_concat_inputs` pass.
- Preview button no-audio rendering (§5.3) — `build_ffmpeg_args` with zero audio sources returns the trivial stream-copy argv; with `trim.tail_secs > 0` and zero audio sources returns the trim-only argv; both cases prefix the preview with the expected "mix pipeline disabled" comment string.
- ffmpeg version probe (§8.3) — fixture table of `ffmpeg -version` first-line outputs covering 3.4.2, 4.1.0, 4.2.0, 4.4.2, 5.1.3, 6.1.1, `N-12345-g<sha>` git builds, and empty/garbage inputs. Assert `meets_floor` is false for <4.2, true for ≥4.2, and unknown-with-warning for unparseable strings.

### 9.2 Integration tests

- End-to-end with a generated 5-second silent multi-track mkv (using `ffmpeg -f lavfi`) as input. Confirm:
  - Unchanged input → output is identical (no post-process triggered).
  - Multi-track input → output has mix stream at index 0 and original streams following.
  - Trim + remix combined → single ffmpeg pass, correct duration, correct streams.
  - Loudness two-pass mode runs and produces deterministic output.
- Gated behind `#[cfg(feature = "integration")]` or a `has_ffmpeg()` skip so CI without ffmpeg still passes.

### 9.3 Manual QA checklist

- PS2 + Teamspeak + Spotify running; configure exclude Spotify; verify mp4 has 3 streams (mix, PS2, TS) and Spotify audio is absent from all of them.
- Configure premix-first; upload to Discord webhook; verify inline playback has full mix.
- Configure premix-last; open in Kdenlive; verify per-source tracks are accessible.
- YouTube upload with premix on; confirm only mix audio appears on the uploaded video.
- Montage with mixed clips (some pre-enhancement, some post) triggers the normalization pass (§6.4) and produces a valid output.
- Pull the plug during post-process (SIGKILL ffmpeg): verify original file is untouched, temp is cleaned up, DB consistent.
- GSR built without `-Dapp_audio`: verify the Settings UI shows the info banner and the app dropdown gracefully degrades.

---

## Part 10 — Rollout and implementation order

Implementation lands in one branch but in reviewable commits, in this order:

1. **Data model & migration.** `AudioSourceConfig` refactor + `PostProcessingConfig` + legacy migration + round-trip tests. `AudioSourceKind` is backend-neutral; no `to_gsr_arg` method on the type. Ships behind no feature flag but has zero runtime effect.
2. **`CaptureBackend` trait extraction.** Create `src/capture/mod.rs` with the trait + capability struct + `CaptureSession` trait. This commit is pure infrastructure — no concrete impl yet, no runtime behavior change. Establishes the seam early so every subsequent commit lands on the right side of it.
3. **`GsrBackend` impl + recorder refactor.** Move everything in `src/recorder.rs` into `src/capture/gsr.rs` as `GsrBackend` + `GsrCaptureSession`. `Recorder` becomes a thin `CaptureController` owning `Box<dyn CaptureBackend>`, selected from `capture.backend` config. Legacy configs automatically resolve to `"gsr"`. All existing tests keep passing; new tests cover `translate_audio_source` and the capability accessors.
4. **Discovery through the backend.** `discover_audio_sources` becomes `GsrBackend::discover_audio_sources`, adds `--list-application-audio` concurrent probe + `DiscoveredAudioKind`. Settings UI calls through `app.capture_backend.discover_audio_sources()` — still uses old pick list in the view, this commit is pure rewiring.
5. **Active layout snapshot.** `active_audio_layout` capture on `GsrCaptureSession` + sidecar file. Recorder behaviour unchanged for legacy configs.
6. **Post-process module extraction.** Move `spawn_trim` into `src/post_process.rs`, fix the latent multi-track drop with `-map 0`, add ffprobe helper, add the `FfmpegCapabilities` version probe (§8.3) alongside the existing `has_ffmpeg()` check. Functionally a bug fix commit.
7. **Schema: post_process_status + clip_audio_tracks.** New migration adds the `clip_audio_tracks` table, the `clips.post_process_status` column (default `Legacy`), and `clips.post_process_error`. Entity + repository methods (`set_post_process_status`, `insert_audio_tracks`, `clips_pending_post_process`). Startup sweep flips stale `Pending` rows to `Failed`. No runtime behavior change yet — the column is written as `NotRequired` for every new save until step 9 starts using `Pending`.
8. **Background job plumbing + per-kind limiter.** New `PostProcess` job kind + `BackgroundJobSuccess::PostProcess` variant + `BackgroundJobManager` per-kind `tokio::sync::Semaphore` map (§4.5). Wire through `src/app.rs:record_save_outcome`. Still a no-op by default — job fires but only runs the old trim path.
9. **Filter graph builder + needs_post_process predicate.** `build_ffmpeg_args` with exhaustive tests, sample-rate/channel normalization prelude, loudnorm two-pass with JSON parsing, the `needs_post_process()` trigger predicate (§4.3). Not yet invoked from runtime.
10. **Enable premix + post_process_status state machine.** Flip the job to call the new builder. `record_save_outcome` now sets `post_process_status = Pending` (or `NotRequired`) on save based on `needs_post_process()`, and transitions through `Completed`/`Failed` on the background job notification (§4.8). Default config preserves single-source behaviour via the fast path. Capability-based `Merged` fallback (§3.3) wires in here. `clip_audio_tracks` rows are inserted by the success handler.
11. **Settings UI rewrite.** Tracklist widget, gain sliders (0.5 dB step, Ctrl/Shift fine-adjust), post-processing section, preview button (with no-audio baseline rendering). Capability gating reads from `backend.capabilities()` (§3.2). Clips tab adds the `post_process_status` badges + retry / "use original anyway" context menu.
12. **Distribution fixes.** YouTube `-map` filter, montage mix-only normalization (§6.4), status tab diagnostics.
13. **Documentation.** Update `docs/FUTURE.md` to reflect what's now shipped, add `docs/audio-pipeline.md` describing the model and the backend trait, add a troubleshooting section to the README for per-app audio on PipeWire, and stub `docs/capture-backends.md` as a reference for anyone adding a Windows or OBS backend later.

Each step is independently valuable and leaves main shippable. The "runtime still uses old behaviour" property of steps 1–8 means regressions are impossible until step 9 builds the new argv and step 10 flips the switch — and even then the single-source fast path (via `needs_post_process()`) means existing users see zero change.
