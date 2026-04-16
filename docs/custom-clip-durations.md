# Custom Clip Durations — Design & Implementation Plan

## Problem

`ClipDuration` (`src/rules/mod.rs:74-106`) is a 6-variant enum: 10s, 30s, 1m, 5m, 10m, full buffer. Users cannot configure arbitrary durations. This is driven by gpu-screen-recorder's signal-based control surface, which exposes only fixed presets.

## gpu-screen-recorder constraint

Replay-save durations are hardcoded in `reference/gpu-screen-recorder/src/main.cpp:3646-3654`:

| Signal      | Seconds     |
|-------------|-------------|
| SIGUSR1     | full buffer |
| SIGRTMIN+1  | 10          |
| SIGRTMIN+2  | 30          |
| SIGRTMIN+3  | 60          |
| SIGRTMIN+4  | 300         |
| SIGRTMIN+5  | 600         |
| SIGRTMIN+6  | 1800        |

Handlers set a static to a constant. Signals use plain `signal()` (no `SA_SIGINFO`), so `sigqueue` payloads can't carry a value. No FIFO/DBus/socket control surface exists. **We cannot request a custom duration directly.**

We currently do not map SIGRTMIN+6 (30 min) — a free preset.

## Chosen approach: Option 2 — preset save + ffmpeg trim

On trigger:
1. Pick the smallest preset ≥ `wanted_secs` and send its signal.
2. Read the saved path from gpu-screen-recorder's stdout.
3. Run `ffmpeg -sseof -<wanted_secs> -i <path> -c copy ...` to trim from the end.
4. Replace the original with the trimmed file.

### Accuracy

- gpu-screen-recorder pads the saved file by `keyint` seconds (`current_save_replay_seconds += arg_parser.keyint` at `main.cpp:4599`).
- `-keyint` defaults to 2.0s.
- Stream-copy `-sseof` snaps to the nearest preceding keyframe, so final clip length is `[wanted_secs, wanted_secs + keyint]`.
- Lowering `-keyint` to 1.0 halves the overshoot at a small bitrate cost.

### Signal → stdout correlation

- gpu-screen-recorder serializes saves: `if(save_replay_thread.valid()) return;` (`main.cpp:1297`). Signals sent while a save is in-flight are dropped silently.
- Path is printed *after* the write thread joins (`main.cpp:4583-4590`), so the file is fully flushed when we read the path.
- Errors go to stdout too: `printf("gsr error: Failed to save replay\n");` — must filter lines starting with `gsr error:` in `read_saved_paths()`.

**State machine:**
- `in_flight: bool` — set when signal sent, cleared on path or error line received.
- `pending_trim: Option<PendingTrim>` — there is at most one in-flight save at any time, so a single slot suffices (no queue).
- New triggers while `in_flight` are rejected with a log message.

### Filesystem strategy

- Save: `<configured_dir>/Replay_<timestamp>.<ext>`.
- Trim to: `<same_dir>/.<basename>.trim.<ext>`.
- On ffmpeg success: atomic rename trimmed → original, original deleted.
- On ffmpeg failure: keep original untrimmed, log error, surface path to UI.

## Edge cases

| Case                                   | Behavior                                                              |
|----------------------------------------|-----------------------------------------------------------------------|
| `wanted_secs > replay_buffer_secs`     | Saved file is whole buffer; `-sseof` starts from beginning; clip is shorter than wanted (OK) |
| `wanted_secs < keyint`                 | ffmpeg may find no keyframe; output could be empty — clamp `wanted_secs >= keyint` at UI level |
| ffmpeg not installed                   | Probe at startup; degrade to preset-only mode with warning           |
| Rapid back-to-back triggers            | Drop second trigger if `in_flight`; log                              |
| `gsr error:` on stdout                 | Clear `in_flight`, clear `pending_trim`, notify UI                   |
| `wanted_secs` exactly equals a preset  | Skip ffmpeg trim; use saved file as-is                               |
| `wanted_secs > 1800`                   | Use SIGUSR1 (full buffer) as the source preset                       |

## Implementation checklist

### Phase 1 — schema migration

- [ ] `src/rules/mod.rs`: Replace `ClipDuration` with `u32` seconds field on `ClipRule` and `ClipAction`. Keep a `FullBuffer` sentinel (e.g. `Option<u32>` where `None` = full buffer, or `enum { Seconds(u32), FullBuffer }`).
- [ ] `src/rules/mod.rs`: Remove `ClipDuration::ALL`, `Display`, variant enum.
- [ ] `src/rules/mod.rs`: Update `default_rules()` to use the new representation.
- [ ] `src/rules/engine.rs`: Update `ClipAction` construction sites and tests (`engine.rs:78`, test fixtures at `engine.rs:243-407`).
- [ ] `src/app.rs`: Update `Message::ClipTriggered` variant (line 77), `ClipEntry::duration` (line 59), view code at line 639.
- [ ] Migrate any persisted configs. Field name change → add serde default or rename logic.

### Phase 2 — ffmpeg trim pipeline

- [ ] `src/recorder.rs`: Add `nearest_ceiling_preset(wanted: u32) -> (signal_num, preset_secs)`.
- [ ] `src/recorder.rs`: Replace `save_clip(ClipDuration)` with `save_clip(wanted_secs: Option<u32>)` (None = full buffer).
- [ ] `src/recorder.rs`: Add `in_flight: bool` and `pending_trim: Option<PendingTrim>` fields to `Recorder`.
- [ ] `src/recorder.rs`: Update `read_saved_paths()` → `poll_save_results()` that filters `gsr error:` lines, clears `in_flight`, pops `pending_trim`, returns a typed result (`Saved { path }` | `SaveFailed { reason }`).
- [ ] `src/recorder.rs`: Add `spawn_trim(path, wanted_secs) -> Result<PathBuf, RecorderError>` that runs ffmpeg synchronously (fast with `-c copy`) and atomically replaces the original.
- [ ] `src/recorder.rs`: Probe `ffmpeg -version` at `Recorder::new()`; expose `has_ffmpeg()` for UI warnings.
- [ ] `src/app.rs`: Call `poll_save_results()` on tick; spawn trim on each `Saved { path }`; emit user-visible events.

### Phase 3 — UI

- [ ] `src/app.rs`: Replace duration pick-list with a numeric input (seconds, min = keyint_secs or 1, max = e.g. 3600).
- [ ] `src/app.rs`: Checkbox or special value for "full buffer" mode.
- [ ] `src/app.rs`: Surface "ffmpeg missing" banner if probe failed.

### Phase 4 — config polish (optional)

- [ ] `src/config.rs`: Add `keyint_secs: f32` (default 1.0 — tighter than recorder default for better trim accuracy).
- [ ] `src/recorder.rs`: Pass `-keyint <keyint_secs>` in `start_replay()`.
- [ ] Settings UI: expose `keyint_secs` slider (0.5 – 2.0).

## Files touched

| File               | Lines of change (est.) |
|--------------------|------------------------|
| `src/rules/mod.rs` | ~30                    |
| `src/rules/engine.rs` | ~20 (tests mostly)  |
| `src/recorder.rs`  | ~100                   |
| `src/app.rs`       | ~40                    |
| `src/config.rs`    | ~5                     |
| **Total**          | **~200 LoC**           |

## Rejected alternatives

- **Option 1 (round-up only):** Simple but users still can't get exact durations or durations shorter than 10s. Ruled out once we committed to "custom".
- **Patch gpu-screen-recorder:** Fork-maintenance burden; users need our patched binary. Overkill.
- **Re-encode trim (frame-accurate):** ~10-30s per clip depending on hardware; no gameplay use case needs sub-keyframe precision.

## Open questions

- Do we need per-rule "keep untrimmed original" toggle for users who want the raw clip?
- Should we log `-keyint` value prominently in the UI so users understand their trim accuracy?
- What's the max sensible `wanted_secs`? Soft-capped at `replay_buffer_secs` at the UI level?
