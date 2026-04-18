# Repository Guidelines

## Project Structure & Module Organization
`src/` contains the application code. `main.rs` boots the `iced` UI, `app.rs` holds most state and message handling, `config.rs` manages TOML persistence, `census.rs` wraps Auraxis Census API and realtime subscriptions, `recorder.rs` controls `gpu-screen-recorder`, and `rules/` contains clip rule types plus the rule engine. `docs/` stores the project-level direction in [PROJECTS.md](docs/PROJECTS.md) plus design notes such as [custom clip durations](docs/custom-clip-durations.md). `reference/` vendors upstream `gpu-screen-recorder` sources for research only; do not treat it as code compiled by this crate.

## Architecture Guidelines
Follow [docs/PROJECTS.md](docs/PROJECTS.md) when making structural changes.

- Keep `app.rs` as the application coordinator and UI state owner; move low-level integrations and domain logic into focused modules.
- Preserve the current boundaries: Census concerns in `src/census.rs`, persistence in `src/config.rs`, process and capture-source detection in `src/process.rs`, recorder control in `src/recorder.rs`, and rule modeling/evaluation in `src/rules/`.
- Prefer message-driven `iced` flows with async work returning through `Task`s instead of blocking UI paths.
- Treat `gpu-screen-recorder` and Census as external dependencies that can fail; favor graceful degradation, clear logging, and recoverable behavior.
- Keep rule evaluation deterministic and easy to test, and extend behavior through existing seams rather than coupling unrelated modules directly.

## Build, Test, and Development Commands
Use the nightly toolchain from `rust-toolchain.toml`.

- `cargo run`: build and launch the desktop app locally.
- `cargo test`: run the current unit test suite; today this covers rule-engine behavior.
- `cargo fmt`: apply standard Rust formatting before committing.
- `cargo fmt --check`: verify formatting in CI or before opening a PR.
- `cargo clippy --all-targets --all-features`: catch lint issues before review.

The app also expects the local path dependency `../auraxis-rs/auraxis` and the runtime binary `gpu-screen-recorder` to be available.

## Coding Style & Naming Conventions
Follow default Rust style: 4-space indentation, `snake_case` for functions/modules, `CamelCase` for types, and short enums/messages with explicit names such as `ClipTriggered` or `SettingsSaveDirChanged`. Prefer small, focused modules and keep UI message handling in `app.rs` aligned with existing `Message` variants. Run `cargo fmt`; the current tree is not fully formatted, so avoid hand-formatting deviations.

## Testing Guidelines
Add unit tests beside the code they exercise. Existing tests live in `src/rules/engine.rs` under `#[cfg(test)]`; follow that pattern for new engine or parser logic. Name tests for the behavior they prove, for example `sequence_gap_exceeded_resets`. Run `cargo test` before every PR and add regression coverage for rule evaluation, config parsing, or process control changes.

## Commit & Pull Request Guidelines
Use Conventional Commits for all future commits, for example `fix: handle census stream logout` or `feat: add OBS replay buffer status banner`. Keep subjects concise and imperative after the type prefix. Keep PRs focused, describe user-visible behavior changes, note required local setup or external binaries, and include screenshots for UI changes.

## Configuration & Runtime Notes
User config is written to the platform config directory via `directories::ProjectDirs` as `config.toml`. Avoid hardcoding secrets or machine-specific paths; prefer defaults that degrade cleanly when Census credentials or recorder dependencies are missing.
