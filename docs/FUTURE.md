# nanite-clip Future Roadmap Task Tracker

## Purpose

This document replaces the earlier feature essay with a tracker-oriented roadmap.
The previous draft was useful for brainstorming, but it mixed product ideas, implementation notes, and sequencing in a way that makes progress hard to track.

This version is meant to be used as the working backlog:

- every item is framed as a deliverable
- hard dependencies are called out explicitly
- each feature is broken into concrete implementation tasks
- optional follow-on work is separated from blockers

## Evaluation of the Prior Draft

The prior `FUTURE.md` identified good feature candidates, but it had four planning problems:

1. It mixed "what we may want" with "what must be built first".
2. It duplicated the dependency graph and left some hard dependencies implicit.
3. It treated some features as independent even when the richer version clearly depends on shared data-model work.
4. It described features at the concept level, not at the task-tracking level.

Key corrections reflected below:

- Feature 1 (auto-extend clips) does not strictly require the event ring buffer, but richer clip metadata for the extended window benefits from it.
- Feature 14 (weapon tracking) is not truly independent if weapon data must be searchable in clip history; it should ride on the raw-event storage work from Feature 5.
- Feature 20 (stats) and Feature 21 (session summary) can ship in basic form without Feature 5, but their useful version depends on the richer clip/event data pipeline.
- Feature 18 (Discord webhook) can be implemented without the general background runner, but retry/progress/cancellation should reuse that infrastructure if it exists.
- Feature 11B (system tray) should follow 11A (auto-start monitoring) because the startup behavior and tray/minimize behavior belong to the same UX flow.

## Planning Rules

- Keep `src/app.rs` as the coordinator. New low-level behavior belongs in focused modules.
- Prefer durable schema/config changes before UI work that depends on them.
- Prefer deterministic, unit-testable logic in `src/rules/` and other domain modules.
- Treat Census, Honu, ffmpeg, OBS, Discord, and `gpu-screen-recorder` as optional external dependencies that can fail.
- Mark tasks done only when code, persistence, UI wiring, and regression coverage are all in place where practical.

## Task Status Conventions

- `[ ]` not started
- `[~]` in progress
- `[x]` complete
- "Hard deps" means the task should not start before its blockers are done.
- "Follow-ons" means useful related work, but not a blocker.

## Dependency Summary

| ID | Task | Hard deps | Unblocks |
|---|---|---|---|
| INF-01 | Event ring buffer | None | UX-04 follow-up, DATA-01, DATA-02, RULE-02, INT-02 follow-up, INT-03 follow-up |
| INF-02 | Recorder backend abstraction | None | PLAT-01 |
| INF-03 | Background task runner | None | JOB-01, JOB-02, JOB-03, EXP-01 |
| UX-01 | Auto-start monitoring | None | UX-03 |
| UX-02 | Global hotkey manual save | None | Works better with UX-03 |
| UX-03 | System tray / minimize | UX-01 | None |
| UX-04 | Auto-extend clips | None | Better overlap behavior before UX-07 |
| UX-05 | Multi-audio source config | None | PLAT-01 config cleanup |
| UX-06 | Clip naming templates | None | Helps JOB-01, JOB-02, JOB-06 |
| UX-07 | Duplicate / overlap detection | None | Helps JOB-03 candidate selection |
| DATA-01 | Raw event storage per clip | INF-01 | DATA-02, DATA-03, RULE-02, INT-02 follow-up, INT-03 follow-up |
| DATA-02 | Event timeline / chapters / subtitles | DATA-01 | None |
| DATA-03 | Weapon tracking | DATA-01 | INT-02 richer stats |
| DATA-04 | Alert correlation | None | INT-02 richer stats, INT-03 richer summary |
| DATA-05 | Facility map visualization | None | Better with DATA-01 |
| RULE-01 | Automatic profile switching | None | None |
| RULE-02 | Fine-grained event filters | DATA-01 | None |
| JOB-01 | Storage tiering | INF-03 | None |
| JOB-02 | Uploads | INF-03 | None |
| JOB-03 | Montage creation | INF-03 | UX-07 synergy |
| JOB-04 | Discord webhook | None | Can later reuse INF-03 |
| JOB-05 | Local HTTP clip server | None | Better with thumbnails, DATA-03 |
| JOB-06 | Import existing clips | None | Better with UX-06, JOB-07 |
| JOB-07 | Database backup / export | None | Supports all later schema work |
| INT-01 | Honu expansion | None | INT-03 richer summary |
| INT-02 | Statistics dashboard | None for basic, DATA-01/DATA-03/DATA-04 for rich version | None |
| INT-03 | Session summary | None for basic, INT-01/DATA-01/DATA-03/DATA-04 for rich version | None |
| PLAT-01 | OBS / Windows support | INF-02 | None |
| EXP-01 | OCR facility detection | INF-03 | None |

## Workstream A: Shared Infrastructure

### INF-01 Event Ring Buffer

Hard deps: None  
Follow-ons: DATA-01, DATA-02, RULE-02, richer analytics

- [x] Add `src/event_log.rs` with an `EventLog` type that supports append, prune, and `query_range(start, end)`.
- [x] Define the retention rule around `replay_buffer_secs` so the buffer always covers the maximum clip window plus save-delay behavior.
- [x] Extend `ClassifiedEvent` only with fields future consumers actually need for storage/search, keeping the rules layer free of UI-only concerns.
- [x] Feed every classified event into the log from `src/app.rs` before or alongside rule-engine ingestion.
- [x] Add unit tests covering prune behavior, empty-range queries, and exact-boundary timestamps.
- [x] Add at least one integration-style test or app-level helper test proving a clip save can harvest the expected event window.

Done when: the app can retrieve all classified events that occurred during a prospective clip window without consulting rule-engine internals.

### INF-02 Recorder Backend Abstraction

Hard deps: None  
Follow-ons: PLAT-01

- [x] Extract a `RecorderBackend` trait from the current recorder flow in `src/recorder.rs`.
- [x] Move the current `gpu-screen-recorder` implementation behind a concrete backend type, preserving current behavior.
- [x] Keep trim/save-result polling semantics in the recorder layer so `src/app.rs` still talks in terms of clip saves, not backend-specific transport.
- [x] Add backend-specific config boundaries so Linux/GSR settings do not leak into future OBS-only paths.
- [x] Add tests around backend-independent save-state transitions where practical.

Done when: `src/app.rs` coordinates recording through a backend-neutral recorder interface and current Linux behavior is unchanged.

### INF-03 Background Task Runner

Hard deps: None  
Follow-ons: JOB-01, JOB-02, JOB-03, EXP-01

- [x] Define a small background job model: job id, job kind, progress updates, completion, failure, and optional cancellation.
- [x] Add app messages and state for background jobs without turning `src/app.rs` into a generic executor.
- [x] Implement a lightweight async runner using `Task`/Tokio channels for long-running work.
- [x] Add a consistent user-feedback pattern for queued/running/completed/failed jobs.
- [x] Prove the job runner can safely handle at least one ffmpeg-based task and one filesystem/network task.
- [x] Persist recent background job history in the clip database and recover interrupted jobs explicitly on startup.

Done when: long-running clip-adjacent work can run off the immediate UI path and report progress back into the iced message flow.

## Workstream B: Runtime and Capture UX

### UX-01 Auto-Start Monitoring

Hard deps: None  
Follow-ons: UX-03

- [x] Add `auto_start_monitoring` to `Config` and normalize old configs safely.
- [x] Update startup flow in `src/app.rs` so the app can enter `WaitingForGame` immediately on launch.
- [x] Show clear status in the Status tab so users understand that monitoring began automatically.
- [x] Add regression coverage around default config and startup state selection.

Done when: a fresh or migrated config can opt into starting directly in monitoring mode without user interaction.

### UX-02 Global Hotkey Manual Save

Hard deps: None  
Follow-ons: UX-03

- [x] Add `src/hotkey.rs` using a global hotkey crate compatible with the supported Linux desktop environments.
- [x] Add configurable hotkey bindings and durations to `Config`, including defaults and migration behavior.
- [x] Route hotkey events into `src/app.rs` as explicit messages instead of bypassing the app state machine.
- [x] Store manual clips distinctly in the database so the Clips tab can filter or label them.
- [x] Handle recorder-not-running and save-in-flight errors gracefully.
- [x] Add tests around hotkey config parsing and manual clip metadata generation.

Done when: a configured hotkey saves a clip while the app is running, even if the main window is not focused.

### UX-03 System Tray / Minimize

Hard deps: UX-01  
Follow-ons: UX-02

- [x] Add a small tray integration module, likely `src/tray.rs`, with explicit start/stop/show/quit actions.
- [x] Add config for "start minimized" and "minimize to tray".
- [x] Keep tray state derived from app state (`Idle`, `WaitingForGame`, `WaitingForLogin`, `Monitoring`) rather than introducing a second runtime source of truth.
- [x] Make restore/minimize actions cooperate with iced window lifecycle cleanly.
- [~] Validate behavior across common Linux desktop environments as far as the chosen tray crate allows.

Done when: the app can launch, optionally auto-start monitoring, and stay available through a tray icon without losing monitoring state.

### UX-04 Auto-Extend Clips When Action Continues

Hard deps: None  
Follow-ons: INF-01, UX-07

- [x] Extend the rule-engine action model so a rule can enter an "extending" state instead of emitting only a one-shot save.
- [x] Add rule-definition fields for extension-window behavior and validate them alongside existing duration settings.
- [x] Add `ActiveClipCapture`-style tracking in `src/app.rs` for pending delayed saves, extension windows, and final resolved clip duration.
- [x] Clamp final duration to `replay_buffer_secs` and make the interaction with `save_delay_secs` explicit.
- [x] Prevent extension logic from causing duplicate saves or cooldown bypasses.
- [x] Add rule-engine regression tests covering extend, expire, cooldown, and reset-threshold behavior.

Done when: sustained action results in one longer clip instead of a burst of overlapping clips, without breaking current save-delay behavior.

### UX-05 Multi-Audio Source Configuration

Hard deps: None  
Follow-ons: INF-02

- [x] Replace `RecorderConfig.audio_source` with a backward-compatible `audio_sources` list model.
- [x] Implement config migration so existing users keep their current single-source behavior.
- [x] Update recorder startup to emit one `-a` argument per configured source.
- [x] Update Settings UI to add, remove, reorder, and label audio sources cleanly.
- [x] Add source discovery for PipeWire/PulseAudio using `gpu-screen-recorder --list-audio-devices`.
- [x] Add tests around config migration and empty/multiple source serialization.

Done when: users can configure multiple audio tracks and recorder startup produces the expected command line.

### UX-06 Clip Naming Templates

Hard deps: None  
Follow-ons: JOB-01, JOB-02, JOB-06

- [x] Add a naming-template field to `Config` with placeholder validation and normalization.
- [x] Implement filesystem-safe template rendering after clip save, including invalid-character replacement.
- [x] Handle name collisions deterministically with numeric suffixes.
- [x] Update clip-path persistence after rename without breaking later moves/uploads.
- [x] Add UI help text listing supported placeholders and example output.
- [x] Add tests for template rendering, sanitization, and collision handling.

Done when: saved clips can be renamed predictably from clip metadata and the database path stays correct.

### UX-07 Duplicate / Overlap Detection

Hard deps: None  
Follow-ons: JOB-03

- [x] Define the overlap rule precisely in database terms, including clip time-window calculation and threshold semantics.
- [x] Add persistence for overlap relationships or a reliable query-based way to compute them.
- [x] Run overlap detection after clip insert/link so new clips immediately show related overlaps.
- [x] Surface overlap state in the Clips tab with low-friction review actions.
- [x] Do not auto-delete or merge clips; keep this purely advisory in the first version.
- [x] Add tests for partial overlap, exact boundary overlap, and non-overlap cases.

Done when: newly saved clips can be flagged as overlapping with prior clips and users can review them in the UI.

## Workstream C: Clip Metadata and Browsing

### DATA-01 Raw Event Storage Per Clip

Hard deps: INF-01  
Follow-ons: DATA-02, DATA-03, RULE-02, richer analytics

- [x] Add a new `clip_raw_events` table and schema migration strategy in `src/clips.rs` instead of resetting the whole database for existing users.
- [x] Expand `ClassifiedEvent` and Census translation with the identifiers required for search and display, especially target-character data.
- [x] At clip-save time, harvest all classified events from the event ring buffer that fall inside the saved clip window.
- [x] Persist per-clip raw events without blocking the save path more than necessary.
- [x] Extend clip queries and filters to support raw-event-backed search use cases such as target player lookup.
- [x] Keep name resolution lazy through the lookup cache rather than making clip save depend on synchronous REST lookups.
- [x] Add migration and query tests covering insert, search, cascade delete, and old-database upgrade behavior.

Done when: clip records can be searched and displayed using per-event metadata instead of only aggregate score contributions.

### DATA-02 Event Timeline, Chapters, and Subtitles

Hard deps: DATA-01  
Follow-ons: INF-03 for chapter/subtitle generation

- [x] Implement 6A first: render approximate event markers in the clip-detail UI using stored raw-event timestamps.
- [x] Decide how timestamp approximation is explained in the UI so users do not assume frame-perfect accuracy.
- [x] Add optional 6B chapter-file generation for MKV as a post-save or on-demand action.
- [x] Add optional 6C subtitle-file generation as a simpler portable output.
- [x] Keep chapter/subtitle generation opt-in and non-destructive.
- [x] Add tests for offset calculation from trigger time and clip duration.

Done when: users can see where important events happened inside a clip, with optional exported chapter/subtitle artifacts.

### DATA-03 Weapon Tracking

Hard deps: DATA-01  
Follow-ons: INT-02 richer stats

- [x] Extend Census event translation to extract `attacker_weapon_id` wherever the event payload supports it.
- [x] Extend `clip_raw_events` and lookup-cache usage to store and resolve weapon ids to names.
- [x] Add weapon display in clip details and a weapon filter in clip search.
- [x] Make lookup caching resilient to failed or slow Census item lookups.
- [x] Add tests for events with and without weapon ids and for cache-backed display behavior.

Done when: clips can be browsed and filtered by the weapon used for the underlying recorded events.

### DATA-04 Alert / Continent Lock Correlation

Hard deps: None  
Follow-ons: INT-02, INT-03

- [x] Subscribe to `MetagameEvent` in `src/census.rs` and translate alert lifecycle events into internal state.
- [x] Track active alerts in `src/app.rs` with enough zone/type context to tag new clips correctly.
- [x] Extend clip persistence to store alert linkage and support retroactive outcome updates at alert end.
- [x] Add Clips UI filtering and detail display for alert context.
- [x] Add tests around overlapping alerts, zone mismatches, and late alert-end updates.

Done when: clips can be tagged as happening during an alert, and completed alerts can update earlier clips with final outcome information.

### DATA-05 Facility Map Visualization

Hard deps: None  
Follow-ons: Better once DATA-01 exists

- [ ] Introduce a small map-data/cache module that resolves facility coordinates per continent.
- [ ] Decide whether to vendor simplified static map geometry or derive a lightweight representation from Census data.
- [ ] Add a compact canvas-based map view to clip details instead of a large new browsing surface.
- [ ] Gracefully handle clips without facility data.
- [ ] Add tests for coordinate lookup and missing-data fallback.

Done when: the clip-detail view can show an approximate map location for clips with facility context.

## Workstream D: Rules and Automation

### RULE-01 Automatic Profile Switching

Hard deps: None  
Follow-ons: None

- [x] Add auto-switch rule types to config with a deliberately small, understandable condition model.
- [x] Implement evaluation points in `src/app.rs` for time-based and event-based switching.
- [x] Define how manual selection interacts with auto-switching and make that behavior visible in the UI.
- [x] Update Rules UI so users can author, inspect, and disable switch rules.
- [x] Add tests covering precedence, manual override, and condition change transitions.
- [x] Refine automatic profile switching to use constrained weekday/time schedules plus active-character conditions with clearer authoring UX.

Done when: the active profile can switch predictably based on configured conditions without confusing manual overrides.

### RULE-02 Fine-Grained Event Filters

Hard deps: DATA-01  
Follow-ons: Could later extend to outfit-based filters

- [x] Add optional event-filter fields to `ScoredEvent` and keep old rules backward-compatible.
- [x] Extend event matching so kind matching stays simple and filter evaluation is layered on top.
- [x] Implement target-character and vehicle-type filters first; defer outfit roster fetching until the simpler filters land.
- [x] Update rule-editing UI to expose filters without overwhelming simple rules.
- [x] Add config migration and rule-engine tests for filtered and unfiltered matching.
- [x] Avoid introducing network lookups into the hot path of rule evaluation.
- [x] Extend filter authoring with attacker-weapon filters backed by cached Census weapon references and category-first browsing.

Done when: rules can target specific players, outfits, vehicles, and weapons while existing rules keep working unchanged.

## Workstream E: Background Processing and Distribution

### JOB-01 Storage Tiering

Hard deps: INF-03  
Follow-ons: UX-06

- [x] Add storage-tier config with path, age threshold, and score threshold semantics.
- [x] Build a background move workflow that handles same-filesystem rename and cross-filesystem copy/delete safely.
- [x] Update clip paths atomically after a successful move.
- [x] Add a manual "move back to fast storage" path before attempting any automatic reverse-tier logic.
- [x] Add clear status/error reporting so missing drives or NAS paths do not silently corrupt metadata.
- [x] Add tests around tier eligibility and move-planning logic.

Done when: eligible clips can be moved to lower-cost storage in the background and remain playable from the app afterward.

### JOB-02 Direct Uploads

Hard deps: INF-03  
Follow-ons: UX-06

- [x] Add an uploader abstraction in a focused module that supports multiple providers without baking platform-specific assumptions into clip or UI code.
- [x] Implement a self-hosted direct upload flow (currently copyparty), auth handling, persistence, and per-clip actions.
- [x] Implement YouTube upload flow in the same priority band, including OAuth 2.0 browser flow, token refresh/storage, and upload status handling.
- [x] Store upload history in a dedicated table linked to clips.
- [x] Keep secrets out of config files; use OS keyring or equivalent secure storage.
- [x] Add per-clip upload actions and upload state in the Clips UI for both providers.
- [x] Handle retryable vs permanent upload failures explicitly.
- [x] Decide whether the first UI milestone exposes both providers directly or gates YouTube behind completed account setup while still keeping implementation in the same roadmap wave.

Done when: a clip can be uploaded to both the self-hosted provider and YouTube through background jobs, and the resulting provider-specific links/statuses are stored and surfaced in the UI.

### JOB-03 Montage Creation

Hard deps: INF-03  
Follow-ons: UX-07

- [x] Implement phase 1 as concat-only montage generation for clips with matching codecs/containers.
- [x] Add multi-select and ordering controls in the Clips UI.
- [x] Persist or at least clearly surface montage output location and source clips.
- [x] Treat transitions/music as separate follow-up work after the stream-copy path is solid.
- [x] Add validation for incompatible input clips before launching ffmpeg.

Done when: users can select multiple compatible clips and produce a simple montage file through a background job.

### JOB-04 Discord Webhook Integration

Hard deps: None  
Follow-ons: Could later reuse INF-03

- [x] Add webhook config with enable flag, threshold, and optional thumbnail behavior.
- [x] Build a focused Discord sender module that posts embeds without pulling unrelated app logic into `src/app.rs`.
- [x] Trigger webhook sends only after a provider upload succeeds, using the uploaded clip URL when available.
- [x] Respect rate limits and fail quietly but observably when Discord is unavailable.
- [x] Keep thumbnail extraction optional and explicitly separate from the first webhook milestone.

Done when: qualifying uploaded clips can send a Discord webhook notification with stable metadata and no UI blocking.

### JOB-05 Local HTTP Clip Server

Hard deps: None  
Follow-ons: Better with thumbnails and richer metadata

- [ ] Add config for enable/disable, port, and optional basic auth.
- [ ] Stand up a minimal embedded server that only serves known clip records, not arbitrary paths.
- [ ] Render a simple clip gallery and playback page without introducing a SPA stack.
- [ ] Make the security posture explicit in the UI because network serving is sensitive.
- [ ] Add tests around auth and path restriction behavior.

Done when: users can opt into a basic local network gallery for existing clips without exposing arbitrary filesystem access.

### JOB-06 Import Existing Clips

Hard deps: None  
Follow-ons: UX-06, JOB-07

- [ ] Add an import flow in the Clips tab with directory selection, preview, and confirmation.
- [ ] Use ffprobe for duration/media metadata when available and degrade gracefully when it is not.
- [ ] Insert imported clips with clearly marked provenance and minimal metadata assumptions.
- [ ] Avoid duplicate imports by tracking path or file fingerprint strategy.
- [ ] Keep event metadata optional; do not block initial import on any attempt to reconstruct historical gameplay context.

Done when: existing video files can be added to the clip catalog with enough metadata to browse and manage them in-app.

### JOB-07 Database Backup / Export

Hard deps: None  
Follow-ons: Useful before later schema migrations

- [x] Add explicit backup methods in `src/clips.rs` rather than relying on ad hoc file copies in UI code.
- [x] Add CSV and/or JSON export for clip metadata and aggregate event data.
- [x] Add Settings UI actions for backup and export with success/failure feedback.
- [x] Automatically create a backup before schema-changing migrations on an existing clip database.
- [x] Add tests for export formatting and backup destination validation.

Done when: users can back up the database and export clip metadata without leaving the app.

## Workstream F: Session Context and Analytics

### INT-01 Honu Expansion

Hard deps: None  
Follow-ons: INT-03

- [ ] Add session URL construction and "open in browser" support first.
- [ ] Add optional session-stat retrieval in a way that never blocks primary monitoring.
- [ ] Decide whether periodic session refresh is needed for correctness or only polish.
- [ ] Keep all Honu calls rate-limited and optional.
- [ ] Surface failures as degraded data, not runtime errors.

Done when: Honu session context is available where useful and absent without harming the main app flow.

### INT-02 Statistics Dashboard

Hard deps: None for basic counts; DATA-01, DATA-03, and DATA-04 for richer analytics  
Follow-ons: None

- [x] Add a new Stats tab backed by aggregate queries in `src/clips.rs`.
- [x] Ship a basic milestone first: clips per day, clips per rule, score distribution, top bases.
- [~] Add richer cuts only after the necessary metadata exists: weapon-based and raw-event-backed metrics are in place, and alert correlation data now exists, but alert-specific stats are not surfaced in the dashboard yet.
- [x] Keep expensive queries on-demand when the Stats tab opens instead of recalculating constantly.
- [x] Add tests for aggregate queries and empty-database behavior.

Done when: users can view useful aggregate trends from the clip database, with richer breakdowns arriving as metadata features land.

### INT-03 Session Summary / Report

Hard deps: None for basic summary; INT-01, DATA-01, DATA-03, and DATA-04 for rich version  
Follow-ons: Optional markdown export

- [x] Track session boundaries reliably in app state so clips can be grouped by a monitoring session.
- [x] On logout or game exit, generate a basic summary from persisted clips: count, duration, top clip, bases played, rule breakdown.
- [x] Add an ephemeral UI presentation in the Status tab first.
- [x] Add optional markdown export only after the in-app summary feels useful.
- [ ] Layer richer context into the session summary from raw events, weapons, and alerts, then add Honu context once `INT-01` lands.

Done when: ending a session produces a clear recap without requiring users to inspect the Clips tab manually.

## Workstream G: Platform Expansion and Speculative Ideas

### PLAT-01 OBS / Windows Platform Support

Hard deps: INF-02  
Follow-ons: Revisit capture/process abstractions as needed

- [x] Split platform-independent recorder behavior from Linux/GSR specifics.
- [~] Define Windows process detection and capture-target discovery behind stable interfaces instead of sprinkling `#[cfg]` branches throughout `src/app.rs`.
- [x] Implement an OBS-backed recorder backend that supports replay-buffer start/save/stop and save-result handling.
- [x] Decide what the config UX looks like when Linux uses recorder-managed audio but OBS owns audio setup on Windows.
- [~] Treat Windows support as a major milestone with explicit smoke-test coverage, not as an opportunistic feature branch.

Done when: the app can run on Windows with OBS as the recording backend while preserving the same app/rules/clips workflow.

### EXP-01 OCR Facility Detection

Hard deps: INF-03  
Follow-ons: None

- [ ] Prove the data problem first: measure how often Census facility metadata is actually insufficient.
- [ ] If the problem is real, prototype screenshot capture plus OCR outside the main monitoring path.
- [ ] Define facility-name matching and confidence thresholds before integrating it into clip metadata.
- [ ] Keep OCR strictly opt-in and clearly labeled experimental.
- [ ] Do not blend OCR guesses into rule evaluation until reliability is demonstrated.

Done when: there is a measured need and a contained experimental implementation that can enrich clip metadata without destabilizing monitoring.

## Recommended Execution Order

### Wave 1: High-value, low-risk UX wins

- [x] UX-01 Auto-start monitoring
- [x] UX-02 Global hotkey manual save
- [~] UX-03 System tray / minimize
- [x] UX-05 Multi-audio source config
- [x] UX-06 Clip naming templates
- [x] JOB-07 Database backup / export

### Wave 2: Core clip-metadata pipeline

- [x] INF-01 Event ring buffer
- [x] DATA-01 Raw event storage per clip
- [x] DATA-03 Weapon tracking
- [x] DATA-02 UI timeline markers
- [x] INT-02 basic stats tab
- [x] INT-03 basic session summary

### Wave 3: Capture quality and rule quality

- [x] UX-04 Auto-extend clips
- [x] UX-07 Duplicate / overlap detection
- [x] RULE-01 Automatic profile switching
- [x] RULE-02 Fine-grained event filters
- [x] DATA-04 Alert correlation

### Wave 4: Background workflows

- [x] INF-03 Background task runner
- [x] JOB-01 Storage tiering
- [x] JOB-02 Direct uploads
- [x] JOB-03 Montage creation
- [x] JOB-04 Discord webhook

### Wave 5: Contextual polish

- [ ] INT-01 Honu expansion
- [ ] DATA-05 Facility map visualization
- [ ] JOB-06 Import existing clips
- [ ] JOB-05 Local HTTP clip server
- [x] DATA-02 chapters / subtitles follow-up

### Wave 6: Platform and experimental work

- [x] INF-02 Recorder backend abstraction
- [~] PLAT-01 OBS / Windows platform support
- [ ] EXP-01 OCR facility detection

## Parking Lot

These ideas are intentionally not broken into active subtasks yet:

- outfit-based rule filters after character and vehicle filters land
- advanced montage transitions and music overlay after concat mode is stable
- automatic thumbnail extraction beyond webhook/upload use cases

Keep this list short. If something here becomes real work, promote it into a numbered task above with dependencies and completion criteria.
