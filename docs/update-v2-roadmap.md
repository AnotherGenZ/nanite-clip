# Auto-Updater V2 Roadmap

## Purpose

This document turns the current GitHub Release-based updater into a concrete v2 delivery plan.

The current updater already supports:

- install-channel detection
- stable vs beta release checks
- signed manifest verification
- staged downloads
- helper-driven apply and relaunch for supported self-update channels
- package-manager guidance for unsupported self-apply channels

The next wave should focus on making updates safer, less disruptive, and more operationally controllable before adding heavier optimizations like binary deltas.

## Current Module Map

- `src/app.rs`: updater orchestration, messages, lifecycle gating, background-job integration
- `src/app/tabs/settings.rs`: updater preferences and actions
- `src/app/tabs/status.rs`: runtime update visibility
- `src/background_jobs.rs`: download/check progress and retry plans
- `src/config.rs`: persisted updater preferences
- `src/update/channel.rs`: install-channel detection
- `src/update/github.rs`: release lookup
- `src/update/manifest.rs`: signed manifest fetch and verification
- `src/update/download.rs`: staged asset download and checksum verification
- `src/update/helper.rs`: helper launch plumbing
- `src/update/helper_shared.rs`: helper payloads and shared plan data
- `src/update/helper_runner.rs`: helper-side apply logic
- `scripts/build-update-manifest.py`: manifest generation in release automation
- `.github/workflows/release.yml`: asset publication and signing pipeline

## Priority Order

1. Install deferral and next-launch apply
2. Richer progress, error, and reminder UX
3. Rollout controls and manifest compatibility rules
4. Delta updates for portable installs
5. Package-managed and Flatpak integration polish

## Wave 1: Deferral And Next-Launch Apply

### User outcome

Users should be able to download an update without interrupting monitoring, then choose one of:

- install now
- install when idle
- install on next launch

### Scope

- Add a persisted install policy for downloaded updates.
- Never auto-apply while monitoring or recording is active.
- Allow the app to detect a staged update on startup and offer immediate apply.
- Allow a downloaded update to remain staged across restarts.

### Files

- `src/config.rs`
  - add `install_behavior` and `pending_update_prompt_dismissed_at`
- `src/update/types.rs`
  - add a pending install mode enum and richer prepared-update state
- `src/update/helper_shared.rs`
  - carry deferred-install metadata into the helper plan only when apply is requested
- `src/app.rs`
  - add messages for `InstallWhenIdle`, `InstallOnNextLaunch`, and `DismissPendingUpdate`
  - gate apply requests behind runtime state checks
  - surface startup detection for staged updates
- `src/app/tabs/settings.rs`
  - add deferred install actions near `Install and Restart`
- `src/app/tabs/status.rs`
  - add a persistent "update ready" banner with defer/apply actions

### Tests

- unit tests for config round-trip and defaults
- unit tests for apply gating while monitoring is active
- regression test for preserving a prepared update across app restart

### Exit criteria

- users can safely download during active use
- install never starts unexpectedly during monitoring
- a staged update survives restart and can be applied later

## Wave 2: Richer Progress And Failure UX

### User outcome

The updater should feel explicit about what it is doing and what failed.

### Scope

- Split status into `checking`, `downloading`, `verifying`, `ready_to_install`, `applying`, and `failed`.
- Show bytes downloaded, total size when known, and current step label.
- Add `Remind Me Later` alongside `Skip This Version`.
- Show `last checked` and `next automatic check` in Settings and Status.
- Improve failure messages so users know whether the problem was networking, manifest verification, checksum mismatch, or apply failure.

### Files

- `src/update/types.rs`
  - add explicit updater phase/status types
- `src/background_jobs.rs`
  - split generic updater progress into step-aware progress
- `src/update/download.rs`
  - emit verify-step progress after download completion
- `src/app.rs`
  - map lower-level errors into user-facing categories
- `src/app/tabs/settings.rs`
  - add `last checked`, `next check`, and reminder controls
- `src/app/tabs/status.rs`
  - expose step-aware progress and actionable retry text

### Tests

- unit tests for error categorization
- unit tests for next automatic check calculation
- regression test for retry path after failed download or failed verification

### Exit criteria

- users can tell what the updater is doing at every stage
- common failure states have clear recovery actions

## Wave 3: Rollout Controls And Compatibility Rules

### User outcome

Bad releases can be stopped quickly, and incompatible releases can communicate their requirements clearly.

### Scope

- Extend the signed manifest with:
  - `minimum_supported_version`
  - `blocked_versions`
  - `rollout`
  - `mandatory`
  - `message`
- Add a release kill-switch that can stop a broken release without shipping a new app build.
- Support phased rollout percentages keyed by stable install identity.
- Support a blocked-version message for known bad builds.

### Files

- `src/update/manifest.rs`
  - parse and validate new manifest fields
- `src/update/types.rs`
  - add rollout and compatibility data structures
- `src/update/mod.rs`
  - evaluate manifest compatibility before presenting a release
- `src/config.rs`
  - persist a stable anonymous install identifier for rollout bucketing
- `src/app.rs`
  - show mandatory-update and blocked-version messaging
- `scripts/build-update-manifest.py`
  - generate new manifest fields
- `.github/workflows/release.yml`
  - validate manifest schema before upload

### Tests

- manifest parsing tests for each new field
- rollout bucketing determinism tests
- regression tests for blocked current version and minimum-version checks

### Exit criteria

- maintainers can stop or throttle a release with manifest-only changes
- the app can explain when an update is blocked, mandatory, or withheld by rollout

## Wave 4: Delta Updates For Portable Installs

### User outcome

Portable users download much less data for routine upgrades.

### Scope

- Start with portable channels only:
  - `WindowsPortable`
  - `LinuxPortable`
- Keep full-package fallback for every release.
- Ship binary delta assets only for adjacent supported versions.
- Only offer delta when:
  - base version matches exactly
  - current install layout is portable
  - local file verification succeeds before patch apply

### Proposed format

- patch asset plus checksum in the manifest
- helper applies patch into a new staging directory, verifies output hash, then swaps atomically enough for the current install layout

### Files

- `src/update/types.rs`
  - add patch-capable asset metadata
- `src/update/manifest.rs`
  - parse optional delta assets and base-version constraints
- `src/update/download.rs`
  - download delta or full asset based on eligibility
- `src/update/helper_runner.rs`
  - add patch-apply flow before swap/relaunch
- `scripts/build-update-manifest.py`
  - emit delta asset metadata
- `.github/workflows/release.yml`
  - build and upload delta artifacts

### Tests

- eligibility tests for base-version matching
- helper tests for patch verification and full-package fallback
- regression tests for corrupted local base files

### Exit criteria

- eligible portable installs prefer a delta update
- failed delta apply falls back cleanly to full-package download

## Wave 5: Package-Managed And Flatpak Polish

### User outcome

Non-self-updating installs should still get first-class guidance and, where possible, a system-native update handoff.

### Scope

- Detect whether PackageKit is available on Linux package-managed installs.
- If available, offer a native system update action instead of only documentation text.
- Improve Flatpak detection details and show the exact command the user can run.
- Keep self-apply disabled for package-managed installs.

### Files

- `src/update/channel.rs`
  - enrich package-managed install detection where needed
- `src/update/mod.rs`
  - add PackageKit capability checks and action routing
- `src/app.rs`
  - surface system-updater actions without pretending the app owns the install
- `src/app/tabs/settings.rs`
  - show install-specific guidance and system-action buttons
- `src/app/tabs/status.rs`
  - reflect package-managed update recommendations clearly

### Tests

- unit tests for install-guidance messaging
- capability tests for PackageKit detection helpers

### Exit criteria

- distro installs present a native update path when available
- Flatpak users get exact, correct instructions for their install

## Low-Priority Hardening

- Key rotation support for updater signing keys
- richer helper/apply logs surfaced in the UI
- rollback metadata and restore flow for failed applies
- optional in-app release-notes rendering
- telemetry hooks for update-check and apply failures if the product later adds opt-in diagnostics

## Recommended Delivery Strategy

Ship Waves 1 through 3 before starting delta updates.

That sequence gives the most user value and operational safety:

- Wave 1 prevents disruptive installs
- Wave 2 makes failures understandable
- Wave 3 gives maintainers a release brake pedal

Delta updates are valuable, but they add the most complexity to packaging, helper logic, and support.

## Suggested PR Breakdown

1. `feat: add deferred install and next-launch update apply`
2. `feat: improve updater progress and reminder UX`
3. `feat: add manifest rollout and compatibility controls`
4. `feat: add portable delta update support`
5. `feat: improve package-managed update guidance`
