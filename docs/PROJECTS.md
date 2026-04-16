# Project Overview

## Goal

`nanite-clip` is a desktop companion for PlanetSide 2 that continuously maintains a replay buffer and saves clips automatically when notable gameplay events occur.

The core user outcome is:

- configure one or more PlanetSide 2 characters
- monitor the active session automatically when the game launches
- subscribe to live Census events for the logged-in character
- evaluate those events against user-defined clip rules
- trigger `gpu-screen-recorder` to save the relevant replay segment

The project is intentionally focused on low-friction local automation rather than full video editing, cloud sync, or social features.

## Product Boundaries

The application owns:

- desktop UI and user settings
- character resolution and live Census subscriptions
- gameplay event classification
- local rule evaluation
- replay-buffer process lifecycle for `gpu-screen-recorder`

The application does not own:

- video encoding itself
- Census service reliability
- PlanetSide 2 process internals
- upstream `gpu-screen-recorder` behavior

The `reference/` directory exists for upstream research only. It should inform implementation decisions, but it is not part of the crate's runtime or build surface.

## Technology Guidelines

### Rust and module design

- Keep the codebase modular and explicit. Prefer small files with clear ownership over pushing unrelated logic into `app.rs`.
- Treat `app.rs` as the application coordinator and UI state owner, not as the place for low-level integrations.
- Put domain logic in dedicated modules:
  - Census and event translation in `src/census.rs`
  - config persistence in `src/config.rs`
  - process and capture-source discovery in `src/process.rs`
  - recorder process control in `src/recorder.rs`
  - rule definitions and evaluation in `src/rules/`
- Prefer data types that are easy to serialize and evolve. Configuration schema changes should be deliberate and backward-compatible where practical.

### UI and async behavior

- The UI is built with `iced` and should remain responsive. Network access, character resolution, and long-running subscriptions should stay off the immediate UI path and flow back through messages/tasks.
- Prefer message-driven state transitions. New features should fit the existing model of `Message` values, state transitions, and `Task`-based async work.
- Keep displayed state derived from durable application state instead of introducing duplicated transient UI state unless the UI strictly requires it.

### External integrations

- `auraxis` is the boundary for Daybreak Census REST and realtime APIs. Keep Census-specific translation isolated so the rest of the app deals in internal events and character IDs.
- `gpu-screen-recorder` is an external process, not a library. Interactions with it should be explicit, defensive, and recoverable.
- Platform-specific capture detection belongs in `src/process.rs` and should degrade cleanly across X11, Wayland, and missing-window cases.

### Reliability and failure handling

- Favor graceful degradation over hard failure. Missing config, unresolved characters, missing game process, and recorder startup failures should keep the app usable and observable.
- Log operational failures with enough detail to debug local setups.
- Avoid assumptions that external services are always available or that the recorder process stays healthy after startup.
- Keep file paths, service IDs, and machine-specific inputs configurable and out of source.

### Testing expectations

- Unit test rule evaluation, parsing, and other deterministic logic close to the implementation.
- Avoid over-investing in tests around `iced` view layout or OS-specific process probing unless behavior is isolated behind testable helpers.
- When adding behavior around rule triggering, config migration, or event classification, include regression coverage where practical.

## Broad Architecture

### Runtime flow

At a high level the application works as a coordinator around four subsystems:

1. `App` manages UI state, lifecycle state, and message routing.
2. `process` detects PlanetSide 2 and determines the correct capture target.
3. `census` resolves configured characters and streams live gameplay/login/logout events.
4. `rules` converts classified gameplay events into clip actions, which `recorder` executes through `gpu-screen-recorder`.

### State machine

The current top-level runtime state in `src/app.rs` is:

- `Idle`
- `WaitingForGame`
- `WaitingForLogin`
- `Monitoring`

The intended flow is:

1. User starts monitoring.
2. The app polls for the PlanetSide 2 process.
3. When the game is present, the app starts or maintains the replay recorder.
4. The app waits for a tracked character login, using both an online-status check and the live Census stream.
5. While monitoring, classified events are fed into the rule engine.
6. Triggered rules cause clip-save signals to be sent to the recorder.
7. Logout or game exit returns the app to a waiting state and resets rule state.

### Module responsibilities

### `src/app.rs`

Owns the application state, navigation, subscriptions, periodic ticks, and the orchestration between configuration, Census events, the rule engine, and the recorder.

### `src/config.rs`

Defines persisted configuration for service credentials, tracked characters, rules, and recorder settings. It is responsible for loading defaults, reading TOML from the platform config directory, and saving updates back to disk.

### `src/census.rs`

Wraps character resolution, online-status lookups, realtime subscriptions, and translation from raw Census payloads into internal gameplay events used by the rule engine.

### `src/process.rs`

Handles environment and process discovery, including locating the PlanetSide 2 process, inferring whether the session is X11 or Wayland, and resolving the appropriate recorder capture target.

### `src/recorder.rs`

Owns the `gpu-screen-recorder` child process, replay-buffer startup, save signaling, shutdown, and recorder-facing error handling.

### `src/rules/mod.rs` and `src/rules/engine.rs`

Define the scoring-rule model and the evaluation engine. Rules are rolling time windows with weighted event contributions, threshold crossing, reset hysteresis, cooldowns, and score-based clip durations. This layer should stay pure and deterministic as much as possible so it remains straightforward to test.

### Architectural principles

- Keep a clean separation between external event sources and internal domain events.
- Keep rule evaluation independent from UI concerns and recorder process details.
- Keep platform-specific logic contained so the rest of the code can operate on stable abstractions.
- Prefer one-way data flow: external input -> classified event -> rule action -> recorder command -> UI/log feedback.
- Add new features by extending existing seams rather than coupling unrelated modules directly.

### Near-term extension model

New work should generally fit one of these patterns:

- New scoring inputs or filters: extend the rule model and engine, then expose configuration in the UI.
- New Census-derived behaviors: extend event classification before touching rule logic.
- New recorder capabilities: isolate them in `src/recorder.rs`, with only message-level coordination from `app.rs`.
- New platform capture logic: extend `src/process.rs` without leaking OS-specific behavior across the app.

This project should remain a focused local automation tool: detect meaningful PlanetSide 2 moments through weighted scoring windows, preserve the relevant replay footage, and stay dependable on a single user's machine.
