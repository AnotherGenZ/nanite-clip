# nanite-clip Future Roadmap: Wave 7+

## Purpose

This document continues the roadmap from `FUTURE.md` with the next generation of features.
Where `FUTURE.md` focused on the core capture-to-distribution pipeline, this wave targets the clip review and organization experience, quality-of-life improvements, and features that make the growing clip library more useful over time.

Before large feature expansion, Wave 7 also needs an architecture-hardening pass. The current code is functionally healthy, but too much product state and too many platform-specific concerns still terminate in a few oversized modules, especially `src/app.rs` and `src/db/mod.rs`. The first workstream in this document exists to reduce that concentration so the later feature work lands on cleaner seams instead of compounding the current complexity.

## Planning Rules

Same as `FUTURE.md`:

- Keep `src/app.rs` as the coordinator, not the implementation home for new product logic. New behavior belongs in focused modules, feature controllers, or service boundaries.
- Prefer durable schema/config changes before UI work that depends on them.
- Treat ffmpeg, external players, and network services as optional dependencies that can fail.
- Mark tasks done only when code, persistence, UI wiring, and regression coverage are all in place where practical.
- Avoid adding new platform-specific logic directly to `App`, tabs, or free functions when the behavior belongs behind a platform abstraction (`hotkeys`, `tray`, `notifications`, `autostart`, `launcher`, `secure_store`, file dialogs).
- Prefer typed configuration and domain enums over stringly-typed selectors when adding new backends, providers, or persisted modes.
- New background workers should prefer explicit service/actor boundaries over ad hoc `Task` + shared-mutex handoff patterns.

## Task Status Conventions

- `[ ]` not started
- `[~]` in progress
- `[x]` complete
- "Hard deps" means the task should not start before its blockers are done.
- "Follow-ons" means useful related work, but not a blocker.

## Dependency Summary

| ID | Task | Hard deps | Unblocks |
|---|---|---|---|
| ARCH-01 | App state and reducer decomposition | None | Most Wave 7 UI-heavy tasks |
| ARCH-02 | Platform services boundary | None | QOL-03, INT-04, future macOS/Windows polish |
| ARCH-03 | ClipStore repository split | None | ORG-01, ORG-02, ORG-03, INTEL-01, INTEL-02, QOL-01, QOL-02, QOL-04 |
| ARCH-04 | Config and migration hardening | None | ORG-01, ORG-02, ORG-03, QOL-01, settings-heavy work |
| ARCH-05 | Command runner and worker cleanup | ARCH-01, ARCH-02 | VIS-01, VIS-02, QOL-03, QOL-04, SHARE-01 |
| VIS-01 | Thumbnail extraction and display | ARCH-05 | VIS-02, SHARE-01, QOL-03 |
| VIS-02 | Scrubbing preview | VIS-01 | None |
| ORG-01 | Tags and collections | ARCH-03, ARCH-04 | ORG-02 synergy |
| ORG-02 | Favorites and pinning | ARCH-03 | ORG-01 synergy |
| ORG-03 | Clip notes | ARCH-03 | None |
| INTEL-01 | Kill streak and multi-kill detection | ARCH-03 | INTEL-02 |
| INTEL-02 | Auto-highlight scoring | ARCH-03, INTEL-01 | INTEL-03 |
| INTEL-03 | Best-of auto-curation | INTEL-02 | None |
| QOL-01 | Soft delete and undo | ARCH-03, ARCH-04 | None |
| QOL-02 | Bulk operations on filtered results | ARCH-01, ARCH-03 | None |
| QOL-03 | Notification quick actions | ARCH-02, ARCH-05, VIS-01 | None |
| QOL-04 | Clip integrity checker | ARCH-03, ARCH-05 | None |
| INT-04 | Stream marker integration | None | None |
| SHARE-01 | Shareable clip pages | ARCH-03, ARCH-05, VIS-01 | None |

## Workstream G: Architecture and Platform Hardening

### ARCH-01 App State and Reducer Decomposition

Hard deps: None
Follow-ons: ARCH-05, practically all feature work that currently touches `src/app.rs`

`src/app.rs` is the largest maintenance hotspot in the repository: it owns runtime orchestration, the global message enum, tab-local editing state, update flows, clip-library actions, background-job wiring, and platform-facing feedback in one place. It still should remain the top-level coordinator, but not the place where every feature lands.

- [ ] Split `App` state into focused feature-state structs, at minimum: `RuntimeState`, `ClipLibraryState`, `RuleEditorState`, `SettingsState`, and `UpdateUiState`. Keep them owned by `App`, but move their fields and helper methods out of the top-level struct.
- [ ] Split the monolithic `Message` enum into nested feature message enums where practical (`runtime`, `clips`, `rules`, `settings`, `updates`). Preserve a thin top-level `Message` wrapper that routes into feature reducers.
- [ ] Extract the monitoring/runtime lifecycle (`Idle` / `WaitingForGame` / `WaitingForLogin` / `Monitoring`) plus recorder-start bookkeeping into a dedicated runtime controller module. `App` should delegate to it instead of owning all transition details inline.
- [ ] Move tab-local draft state out of `App` and into the relevant tab modules or feature-state structs. In particular, reduce the amount of duplicated `Config`-mirroring fields in Settings.
- [ ] Move feature-specific task factories and side-effect orchestration helpers out of `src/app.rs` into adjacent modules so the top-level `update()` function mostly routes instead of implementing every branch directly.
- [ ] Keep `App::subscription()` as the composition root, but extract subscription builders for runtime polling, Census, hotkey capture, and window events into dedicated helpers/modules.
- [ ] Add focused tests for the extracted reducers/controllers so future feature work does not need to lean on `app.rs` integration tests for every state transition.

Done when: `src/app.rs` is materially smaller, feature-specific state lives behind named structs/modules, and adding a Wave 7 feature no longer requires editing a single giant reducer branch plus a giant struct field list.

### ARCH-02 Platform Services Boundary

Hard deps: None
Follow-ons: ARCH-05, QOL-03, INT-04, future platform polish

Platform-specific behavior is currently spread across `src/hotkey.rs`, `src/tray.rs`, `src/notifications.rs`, `src/autostart.rs`, `src/launcher.rs`, `src/secure_store.rs`, and parts of settings/file-dialog logic. The code mostly compiles cleanly, but the abstractions stop at the module boundary instead of giving the app a single platform-service interface.

- [ ] Introduce a new `src/platform/` module that defines app-facing traits/interfaces for `HotkeyService`, `TrayService`, `NotificationService`, `AutostartService`, `OpenService`, `SecretStore`, and file-dialog launching.
- [ ] Create a `PlatformServices` container built at startup and injected into `App`, so top-level code depends on stable app-facing traits rather than directly constructing platform-specific helpers.
- [ ] Move OS selection/dispatch into `platform/` implementations instead of distributing `cfg` and desktop-environment branching across unrelated modules.
- [ ] Isolate Linux desktop-environment special cases such as KDE tray selection and Plasma hotkey sidecar startup behind platform-layer policies.
- [ ] Make unsupported-platform behavior explicit through capability reporting rather than silent stub behavior. The UI should know whether notifications, tray, hotkeys, or launch-at-login are implemented on the current platform.
- [ ] Add unit tests around platform capability selection and dispatch decisions, including desktop-environment-sensitive choices on Linux.
- [ ] Document the supported platform matrix explicitly in code comments and docs so partial `cfg` coverage does not imply full product support where it does not exist yet.

Done when: `App` and the tab modules talk to a coherent platform service boundary, and adding or polishing an OS-specific integration does not require touching unrelated app or UI modules.

### ARCH-03 ClipStore Repository Split

Hard deps: None
Follow-ons: ORG-01, ORG-02, ORG-03, INTEL-01, INTEL-02, QOL-01, QOL-02, QOL-04

`src/db/mod.rs` has become a second monolith. It owns schema bootstrapping, migrations, clip CRUD, lookup caching, analytics, exports, background-job persistence, and filesystem helpers. That is workable today, but it will make the Wave 7 database-heavy features expensive to land and risky to change.

- [ ] Split `ClipStore` internals into dedicated repository modules, for example: `clips_repo`, `lookups_repo`, `jobs_repo`, `analytics_repo`, `exports`, and `schema`.
- [ ] Keep a thin public `ClipStore` facade for the rest of the app initially, but route its methods into those repositories instead of continuing to grow one file.
- [ ] Separate schema creation, drift detection, and migration replay logic into dedicated schema/migration modules so new feature migrations do not share space with query code.
- [ ] Move export helpers, CSV/JSON formatting, and output-destination validation out of the core store module.
- [ ] Group database-facing DTOs and row-hydration helpers by feature rather than keeping all persistence-side types in one namespace.
- [ ] Add repository-level tests beside the extracted modules so later schema features like tags, collections, favorites, notes, and soft-delete can be added locally.

Done when: database changes for a single Wave 7 feature can be implemented by touching one repository area plus the shared facade, not a 5k-line mixed-responsibility module.

### ARCH-04 Config and Migration Hardening

Hard deps: None
Follow-ons: settings-heavy work, schema-heavy work

`Config` is already evolving across multiple waves, but config loading, normalization, migration, and persistence still live in one module with mostly best-effort behavior. That is acceptable for early development, but fragile once Wave 7 starts layering more durable settings and persisted feature state.

- [ ] Split `src/config.rs` into `schema`, `normalize`, `migrate`, and `store` submodules while preserving the current external API shape initially.
- [ ] Replace plain overwrite saves with atomic write semantics for config persistence, matching the same durability bar already used in parts of the database/export code.
- [ ] Make schema-version handling explicit. Distinguish true migrations from ordinary normalization, and keep legacy-format compatibility code isolated from current-schema logic.
- [ ] Introduce typed enums or tagged serde types for persisted backend/provider selectors that are still represented as free-form strings.
- [ ] Surface config parse and migration failures to the UI in a recoverable way instead of silently dropping to defaults when practical.
- [ ] Add regression fixtures for each historical config shape that still needs to load, and require new settings/schema additions to include round-trip tests.

Done when: configuration changes have a predictable migration path, persistence is atomic, and new settings do not further bloat a single mixed-responsibility file.

### ARCH-05 Command Runner and Worker Cleanup

Hard deps: ARCH-01, ARCH-02
Follow-ons: VIS-01, VIS-02, QOL-03, QOL-04, SHARE-01

This codebase shells out to `ffmpeg`, `ffprobe`, `secret-tool`, `powershell`, `msiexec`, `xdg-open`, terminal launchers, and other OS tools across multiple modules. It also uses a mix of worker threads, `spawn_blocking`, child-process monitors, and ad hoc shared-state handoff patterns. Those patterns work, but they are one of the main sources of incidental complexity.

- [ ] Add a shared command-execution utility/service that standardizes: command construction, availability checks, timeout handling, stderr capture, structured error mapping, and test fakes.
- [ ] Migrate ffmpeg/ffprobe callers (`post_process`, montage helpers, thumbnails, Discord thumbnails, integrity checks) onto that shared command layer.
- [ ] Migrate OS command launchers (`launcher`, notification fallbacks, autostart helpers, updater helper invocations, secure-store shell-outs) onto the same command layer or a closely related platform-exec service.
- [ ] Replace `Task` + `Arc<Mutex<Option<Result<...>>>>` handoff patterns in recorder startup and similar flows with clearer actor/service boundaries and direct message emission.
- [ ] Standardize long-running helper ownership and shutdown semantics for OBS sessions, the Plasma platform sidecar, tray workers, and future background processors.
- [ ] Add test coverage for timeout, unavailable-binary, and stderr-rich failure cases so external integration failures are easier to reason about and present consistently.

Done when: external tool invocation and worker lifecycle management follow one consistent pattern, and new Wave 7 features do not need to invent their own shell-out/error-handling scaffolding.

## Workstream H: Visual Clip Browsing

### VIS-01 Thumbnail Extraction and Display

Hard deps: ARCH-05
Follow-ons: VIS-02, SHARE-01, QOL-03

Clip lists today are walls of text metadata. A single representative frame per clip makes visual scanning dramatically faster.

A basic version of thumbnail extraction already exists in `src/discord.rs` (`extract_thumbnail`) for Discord webhook embeds: it calls ffmpeg to grab a single frame at the 1-second mark and writes it as a PNG. This feature generalizes that pattern, extracts at the trigger moment instead of a fixed offset, persists the thumbnail path, and integrates it into the clips list and detail view.

- [ ] Add a `thumbnail_path` column to the `clips` table with a non-destructive schema migration. Existing clips get `NULL`; the UI handles missing thumbnails gracefully.
- [ ] Extract the thumbnail extraction logic from `src/discord.rs` into a shared utility in a new `src/thumbnails.rs` module. Parameterize the extraction timestamp (trigger-event offset within the clip window) and output format (JPEG for size, configurable quality).
- [ ] Generate a thumbnail automatically after each clip save completes, as a lightweight post-save step. Use the trigger-event timestamp offset within the clip as the seek point, falling back to the midpoint if the offset is invalid or extraction fails.
- [ ] Store thumbnails in a `thumbnails/` subdirectory alongside the clip save directory, named by clip ID, to avoid polluting the clips directory.
- [ ] Update `ClipRecord` and clip queries to include `thumbnail_path`. Load thumbnails lazily in the clips list using iced's `image::Handle` to avoid blocking the UI with disk I/O for large lists.
- [ ] Display thumbnails as small previews in the clips list rows. Use a fixed aspect ratio (16:9) with a fallback placeholder for clips without thumbnails.
- [ ] Display the thumbnail prominently in the clip detail panel as a visual header.
- [ ] Add a background job or on-demand action to bulk-generate thumbnails for existing clips that predate this feature.
- [ ] Add a Settings toggle to disable automatic thumbnail generation for users who prefer smaller disk footprint.
- [ ] Add tests for thumbnail path construction, offset calculation from trigger time, and missing-ffmpeg fallback behavior.

Done when: new clips automatically get a thumbnail displayed in the clips list and detail view, and existing clips can have thumbnails generated retroactively.

### VIS-02 Scrubbing Preview

Hard deps: VIS-01
Follow-ons: None

A lightweight visual preview that lets users see what happens in a clip without launching an external player. Not a video player: a frame strip with a draggable slider that shows the nearest extracted frame.

- [ ] At thumbnail generation time (VIS-01), optionally extract a configurable number of preview frames (default: 10-20) evenly spaced across the clip duration. Store as a sprite sheet (single image with tiled frames) or individual small JPEGs in the thumbnails directory.
- [ ] Add a `preview_frames_path` or equivalent field to the clips table or a sidecar metadata file tracking the frame strip location and frame count.
- [ ] Add a preview widget to the clip detail panel: a horizontal slider spanning the clip duration with the current frame displayed above it. When the user drags or hovers the slider, show the nearest extracted frame.
- [ ] Overlay event markers from `clip_raw_events` on the preview slider, positioned by their timestamp offset within the clip window. Reuse the timestamp-to-offset logic from DATA-02 (event timeline markers).
- [ ] Add event tooltips on the markers showing the event kind and target name when hovered.
- [ ] Include an "Open in player" button next to the preview that launches the clip in the system default video player (or a user-configured player path from Settings).
- [ ] Keep frame extraction as a background operation. If preview frames are not yet generated, show only the single VIS-01 thumbnail with a "Generate preview" action button.
- [ ] Add a Settings control for preview frame count and whether to auto-generate previews or only on demand.
- [ ] Add tests for frame offset calculation, sprite sheet coordinate mapping, and graceful behavior when preview frames are missing or partially generated.

Done when: users can visually scrub through a clip in the detail view, see event markers overlaid on the timeline, and quickly decide whether to open the full clip in an external player.

## Workstream I: Clip Organization

### ORG-01 Tags and Collections

Hard deps: ARCH-03, ARCH-04
Follow-ons: ORG-02, SHARE-01

The existing filter system is query-based and powerful, but users have no way to persistently organize clips by intent. Tags and collections give users a personal organizational layer on top of the metadata-driven filters.

- [ ] Add a `clip_tags` table: `(id, clip_id, tag_name, created_ts)` with a unique constraint on `(clip_id, tag_name)` and an index on `tag_name` for fast lookups. Add a `collections` table: `(id, name, description, created_ts)` and a `collection_clips` junction table: `(collection_id, clip_id, added_ts, sequence_index)`.
- [ ] Add `ClipStore` methods for tag CRUD: `add_tag(clip_id, tag_name)`, `remove_tag(clip_id, tag_name)`, `list_tags() -> Vec<String>`, and `clips_by_tag(tag_name)`. Add collection CRUD: `create_collection`, `add_clip_to_collection`, `remove_clip_from_collection`, `list_collections`, `collection_clips`.
- [ ] Add a tag filter to `ClipFilters` and integrate it into `search_clips` using the same pattern as the existing alert or weapon filters (join against `clip_tags`, LIKE on `tag_name`).
- [ ] Add tag display in the clip detail panel as small badges, with an inline tag editor: type-to-search existing tags or create new ones. Keep the interaction lightweight (no modal).
- [ ] Add a tag column or badge indicator in the clips list rows for at-a-glance visibility.
- [ ] Add a collections sidebar or tab section in the Clips view that lists saved collections. Selecting a collection filters the clip list to its members, ordered by `sequence_index`.
- [ ] Support adding clips to collections from the clip detail panel and from the montage-style multi-select flow.
- [ ] Support drag-to-reorder within a collection view for manual sequencing.
- [ ] Add tests for tag uniqueness constraints, collection ordering, cascade delete behavior (clip deletion should cascade to tags and collection membership), and filter integration.

Done when: users can tag clips with free-text labels, create named collections of clips, filter by tag, and browse collections as curated clip sets.

### ORG-02 Favorites and Pinning

Hard deps: ARCH-03
Follow-ons: ORG-01

A minimal organizational primitive: a boolean "favorite" flag that prevents storage tiering archival and surfaces clips quickly. Simpler than full tagging for users who want a one-click way to mark important clips.

- [ ] Add a `favorited` boolean column to the `clips` table (default `false`), with a non-destructive migration.
- [ ] Add a favorite toggle action in the clip detail panel and as an inline icon in the clips list rows. Use a star or pin icon from the existing Font Awesome icon set.
- [ ] Add a "Favorites" filter preset to the clips view (similar to the overlap filter toggle) that shows only favorited clips.
- [ ] Exclude favorited clips from storage tiering eligibility in `src/storage_tiering.rs`. The tiering sweep should skip clips where `favorited = true` regardless of age or score thresholds.
- [ ] Add a sort option or sort priority that surfaces favorited clips first within any sort order.
- [ ] Support bulk favorite/unfavorite through the multi-select flow.
- [ ] Add tests for tiering exclusion, filter behavior, and migration of existing clips.

Done when: users can favorite clips with one click, filter to favorites, and trust that favorited clips will not be auto-archived.

### ORG-03 Clip Notes

Hard deps: ARCH-03
Follow-ons: None

A free-text notes field per clip for context that metadata cannot capture. "This was the double beacon play during outfit ops" is information that event data, scores, and timestamps cannot reconstruct.

- [ ] Add a `notes` text column to the `clips` table (default `NULL`), with a non-destructive migration.
- [ ] Add an editable text area in the clip detail panel for viewing and editing notes. Auto-save on blur or after a short debounce to avoid requiring an explicit save button.
- [ ] Add a `has_notes` indicator (small icon) in the clips list rows so users can see at a glance which clips have notes without opening the detail view.
- [ ] Include notes content in the full-text search path of `search_clips` so users can find clips by note content.
- [ ] Include notes in the clip metadata export (JOB-07 CSV/JSON export).
- [ ] Add tests for notes persistence, search integration, and null-handling for clips without notes.

Done when: users can annotate clips with free-text notes that are searchable and visible in the clip list.

## Workstream J: Smart Clip Intelligence

### INTEL-01 Kill Streak and Multi-Kill Detection

Hard deps: ARCH-03
Follow-ons: INTEL-02

PlanetSide 2's Census events have per-event timestamps precise enough to detect rapid sequential kills. Tagging clips with streak metadata (double kill, triple kill, etc.) provides objective highlight value beyond the rule-engine score.

- [ ] Define streak detection parameters: maximum gap between consecutive kills (e.g., 5 seconds), minimum streak length (2), and which event kinds count as streak-eligible (Kill, Headshot, VehicleDestroy, Roadkill, etc.).
- [ ] Add a streak detection module (`src/streak.rs` or integrate into `src/rules/`) that takes a time-sorted slice of `ClassifiedEvent` records and returns detected streaks with start/end timestamps, kill count, and constituent event references.
- [ ] Run streak detection at clip-save time using the harvested raw events from the event ring buffer, alongside the existing raw-event persistence in DATA-01.
- [ ] Add a `clip_streaks` table: `(id, clip_id, streak_kind, kill_count, start_ts, end_ts)` to persist detected streaks per clip. Alternatively, store streak metadata as a JSON field on the clip record if the query patterns are simple enough.
- [ ] Display streak badges in the clips list rows (e.g., "3x Kill Streak", "Double Kill") and in the clip detail panel alongside the event timeline.
- [ ] Add streak-based filtering to `ClipFilters`: minimum streak length, streak kind.
- [ ] Make streak detection parameters configurable in Settings with sensible defaults, since different play styles and vehicle combat have different natural kill cadences.
- [ ] Add tests for edge cases: kills exactly at the gap boundary, single kills (no streak), overlapping streaks, and mixed event kinds.

Done when: clips are automatically tagged with kill streak metadata that is visible in the UI and filterable.

### INTEL-02 Auto-Highlight Scoring

Hard deps: ARCH-03, INTEL-01
Follow-ons: INTEL-03

The rule-engine score measures "did this clip meet the save threshold?" but not "how highlight-worthy is this clip compared to others?" A secondary highlight score computed post-save provides a normalized ranking that accounts for event rarity, density, and streak quality.

- [ ] Define a highlight scoring model in a new `src/highlight.rs` module. Inputs: raw event list, detected streaks (INTEL-01), clip duration, event kind rarity weights. Output: a normalized highlight score (0-100 scale).
- [ ] Weight factors to include: streak length and speed (kills per second), headshot ratio, event kind rarity (a Domination Kill or Bounty Claim is rarer than a standard kill), event density relative to clip duration, and multi-kill bonus stacking.
- [ ] Compute highlight scores at clip-save time after streak detection, and store in a `highlight_score` column on the `clips` table.
- [ ] Add highlight score display in the clips list (as a secondary score or a visual indicator like a colored bar) and as a sort column.
- [ ] Backfill highlight scores for existing clips with raw events via a migration background job.
- [ ] Keep the scoring model simple and deterministic in v1. Avoid ML or trained models; use a weighted formula that can be inspected and tuned.
- [ ] Add a Settings section or advanced config for adjusting highlight weights, with a "reset to defaults" option.
- [ ] Add tests for scoring edge cases: empty event list, single event, all-headshot clips, long clips with sparse events, and streak-heavy clips.

Done when: every clip has a highlight score that meaningfully differentiates "barely triggered the rule" from "this is a genuine highlight reel candidate."

### INTEL-03 Best-Of Auto-Curation

Hard deps: INTEL-02
Follow-ons: None

Most users accumulate hundreds of clips and never review them. This feature periodically surfaces the highest-value unwatched/un-uploaded clips as actionable suggestions.

- [ ] Add a `reviewed` boolean column to the `clips` table (default `false`). Set it to `true` when a user opens the clip in an external player, uploads it, adds it to a montage, or explicitly marks it reviewed. This tracks whether the user has engaged with the clip.
- [ ] Add a curation query in `ClipStore` that returns the top N un-reviewed clips ranked by highlight score (INTEL-02), optionally filtered by time range (e.g., "best of this week").
- [ ] Add a "Best Of" section or filter preset in the Clips tab that shows curated suggestions. Display as a distinct visual group above the main clip list, or as a toggleable view mode.
- [ ] Add periodic curation triggers: when the user opens the Clips tab, or after a monitoring session ends (alongside the session summary from INT-03), surface a "highlights from this session" prompt.
- [ ] Add bulk actions on the curated list: "Upload all", "Add to collection", "Mark all reviewed", "Dismiss suggestions".
- [ ] Respect favorites (ORG-02) and tags (ORG-01) in curation: favorited clips are already reviewed by definition; clips in collections are already curated.
- [ ] Add a Settings toggle to enable/disable auto-curation suggestions for users who prefer manual review workflows.
- [ ] Add tests for the curation query (ranking, review-state filtering, time-range scoping) and for the `reviewed` state transitions.

Done when: users are proactively shown their best un-reviewed clips with clear actions to upload, organize, or dismiss them.

## Workstream K: Quality of Life

### QOL-01 Soft Delete and Undo

Hard deps: ARCH-03, ARCH-04
Follow-ons: None

Clip deletion is currently immediate after a confirmation dialog. A soft-delete model with a time-limited undo window prevents accidental data loss without adding a permanent trash system.

- [ ] Add a `deleted_ts` timestamp column to the `clips` table (default `NULL`). Clips with a non-null `deleted_ts` are soft-deleted.
- [ ] Update all clip queries (`search_clips`, `recent_clips`, `clip_detail`, stats aggregations) to exclude soft-deleted clips by default. Add an explicit "include deleted" option for the trash view.
- [ ] When a user deletes a clip, set `deleted_ts` instead of removing the row and file. Show an undo toast/banner in the UI with a configurable timeout (default: 30 seconds). If the user clicks undo, clear `deleted_ts`. If the timeout expires, proceed with permanent deletion (row removal + file delete).
- [ ] Add a "Trash" view or filter in the Clips tab that shows soft-deleted clips awaiting permanent deletion. Include a "Restore" action and a "Permanently delete" action.
- [ ] Run a cleanup sweep on startup (or periodically) that permanently deletes clips where `deleted_ts` is older than a configurable retention period (default: 7 days). This handles cases where the app was closed during the undo window.
- [ ] Cascade soft-delete awareness to related operations: soft-deleted clips should not appear in montage selection, upload candidates, or storage tiering sweeps.
- [ ] Add tests for soft-delete, undo within window, permanent deletion after expiry, cascade exclusion, and cleanup sweep behavior.

Done when: deleting a clip provides a time-limited undo opportunity and soft-deleted clips are invisible to normal workflows until permanently purged.

### QOL-02 Bulk Operations on Filtered Results

Hard deps: ARCH-01, ARCH-03
Follow-ons: None

Multi-select today is manual (checkbox per clip, designed for montage creation). Power users with large clip libraries need "select all matching current filter" for batch operations.

- [ ] Add a "Select all" checkbox in the clips list header that selects all clips matching the current filters, not just the visible page. Track the selection as either an explicit ID set (for small results) or a filter-based selection (for large results, execute the filter at action time).
- [ ] Add a selection count indicator showing "N clips selected" with a "clear selection" action.
- [ ] Add a bulk action bar that appears when clips are selected, offering: Delete (soft-delete via QOL-01 if available, otherwise with confirmation), Upload (to configured provider), Add tags (ORG-01), Add to collection (ORG-01), Set favorite (ORG-02), Move storage tier, Export metadata, Mark reviewed (INTEL-03).
- [ ] Execute bulk actions as background jobs (via INF-03) with progress tracking, since operations on hundreds of clips can take significant time.
- [ ] Add guard rails: bulk delete requires explicit confirmation with the count displayed. Bulk upload shows a size estimate before proceeding.
- [ ] Ensure bulk operations do not interfere with the current clip detail view if a selected clip is open.
- [ ] Add tests for filter-based selection correctness, bulk action execution, and edge cases (empty selection, single clip, all clips).

Done when: users can apply actions to all clips matching a filter without manually selecting each one.

### QOL-03 Notification Quick Actions

Hard deps: ARCH-02, ARCH-05, VIS-01
Follow-ons: None

Desktop notifications for saved clips currently show basic information. Adding action buttons to notifications reduces the round-trip of opening the app to act on a just-saved clip.

- [ ] Investigate platform notification action support: Linux desktop notifications (via `notify-rust` or the existing notification infrastructure) support up to 3 action buttons. Windows toast notifications support similar action buttons.
- [ ] Add configurable notification actions in `src/notifications.rs`. Default actions: "Open in player" (launches default video player), "Open in app" (brings NaniteClip to front, navigates to clip detail).
- [ ] Include the VIS-01 thumbnail as the notification image/icon where the platform supports rich notifications (both Linux and Windows toast notifications support image attachments).
- [ ] Route notification action callbacks through the app message system. "Open in player" triggers the same external-player launch used elsewhere. "Open in app" sends a window-focus message plus a clip-detail navigation message.
- [ ] Handle the case where the app is minimized to tray (UX-03): notification actions should restore the window before navigating.
- [ ] Add a Settings section for notification behavior: enable/disable notifications, choose which actions to show, toggle thumbnail inclusion.
- [ ] Add tests for action routing and graceful fallback when notification actions are not supported by the platform.

Done when: clip-saved notifications include a thumbnail preview and actionable buttons that let users immediately open or review the clip.

### QOL-04 Clip Integrity Checker

Hard deps: ARCH-03, ARCH-05
Follow-ons: None

Over time, clip files can go missing (manual deletion, drive failures, moved without updating the database), become corrupted, or have stale database references. A periodic integrity check catches these problems before they cause confusion.

- [ ] Add an integrity check module (`src/integrity.rs`) that performs the following checks per clip:
  - File existence: does `clip.path` resolve to an existing file?
  - File readability: can the file be opened and is its size non-zero?
  - Media validity: does `ffprobe -v error -t 0 -i <path>` succeed? (Optional, heavier check.)
  - Database consistency: does every `clip_upload` reference a valid clip? Do `clip_raw_events` have valid `clip_id` references? (Normally enforced by foreign keys, but worth verifying after migrations.)
  - Thumbnail validity: if `thumbnail_path` is set (VIS-01), does the file exist?
- [ ] Define integrity check results as a structured report: `(clip_id, check_kind, status, detail)`. Statuses: `Ok`, `Warning` (e.g., missing thumbnail but clip is fine), `Error` (e.g., file missing).
- [ ] Add an on-demand "Check integrity" action in the Settings tab that runs the check as a background job (INF-03) with progress tracking.
- [ ] Add a periodic automatic check option (e.g., on startup or weekly) with a Settings toggle, disabled by default.
- [ ] Display integrity results in a dedicated report view or modal: group by severity, show affected clip IDs with links to clip detail.
- [ ] Add repair actions for common problems: "Remove database record" for clips with missing files, "Regenerate thumbnail" for missing thumbnails, "Re-probe audio" for clips with stale post-process metadata.
- [ ] Do not auto-repair anything. All repair actions require explicit user confirmation.
- [ ] Add tests for each check type with synthetic scenarios (missing file, zero-byte file, orphaned database record).

Done when: users can verify the health of their clip library and take corrective action on problems, with no automatic data modification.

## Workstream L: Integration and Distribution

### INT-04 Stream Marker Integration

Hard deps: None
Follow-ons: None

Streamers who run NaniteClip while streaming want to find VOD timestamps for the same moments that triggered clip saves. This feature creates stream markers at clip-trigger time via Twitch or YouTube Live APIs, so the streamer can later find the exact moment in their VOD.

- [ ] Add a `stream_markers` configuration section in `Config` with: enable flag, provider (Twitch or YouTube Live), and auth credentials.
- [ ] Implement Twitch marker support first using the Create Stream Marker endpoint. This requires a user access token with `channel:manage:broadcast` scope. Add OAuth device-code or authorization-code flow similar to the existing YouTube OAuth in `src/uploads.rs`.
- [ ] At clip-trigger time (when the rule engine arms and a save is initiated), fire a non-blocking marker creation request. Use the clip's rule name and score as the marker description (Twitch markers support a description field up to 140 characters).
- [ ] Store marker results (marker ID, timestamp, VOD URL if available) in a `clip_stream_markers` table linked to the clip record, so users can later navigate from a clip to the corresponding VOD moment.
- [ ] Display stream marker links in the clip detail panel alongside upload URLs.
- [ ] Handle failure gracefully: if the user is not live, if the API is unreachable, or if rate limits are hit, log the failure and continue without affecting the clip save. Stream markers are best-effort, never blocking.
- [ ] Add YouTube Live marker support as a follow-on using the existing YouTube OAuth credentials from JOB-02. YouTube Live supports inserting cue points via the LiveBroadcasts API.
- [ ] Add Settings UI for stream marker configuration: provider selection, auth flow, enable/disable, and a test button that attempts to create a marker on the current stream.
- [ ] Add tests for marker request construction, auth token refresh, and failure-mode handling.

Done when: clip saves during a live stream automatically create a marker in the VOD that links back to the clip's trigger moment.

### SHARE-01 Shareable Clip Pages

Hard deps: ARCH-03, ARCH-05, VIS-01
Follow-ons: JOB-05 (local HTTP clip server from FUTURE.md)

This extends the planned JOB-05 local HTTP server with individual clip pages that have visual context: thumbnail, event timeline, metadata, and playback. A link you can paste into Discord that tells a story beyond just a video file.

- [ ] Implement the JOB-05 prerequisites first: embedded HTTP server (using `axum` or `warp`), config for enable/disable, port, and optional basic auth. Bind to `127.0.0.1` by default with an explicit opt-in for LAN access.
- [ ] Add a `/clips/{clip_id}` route that renders a self-contained HTML page for a single clip. The page should include:
  - Clip metadata: timestamp, rule, character, location, score, duration, highlight score (INTEL-02 if available).
  - Thumbnail image (VIS-01) served from the thumbnails directory.
  - Event timeline: a visual bar showing event markers at their relative positions within the clip, similar to the in-app preview (VIS-02).
  - Event list: a table of raw events with timestamps, event kind, target, weapon.
  - Video playback: an HTML5 `<video>` element serving the clip file directly. Browser-native playback handles seeking, volume, and speed controls without any custom player code.
  - Upload links: if the clip has been uploaded to Copyparty or YouTube, show those links.
  - Tags and notes (ORG-01, ORG-03) if present.
- [ ] Add a `/clips` gallery route that lists recent clips with thumbnail grid, sortable by date or highlight score. Paginated, not infinite scroll.
- [ ] Serve clip files through the HTTP server with proper `Content-Type` and `Content-Length` headers, supporting `Range` requests for seeking in the `<video>` element.
- [ ] Add a "Copy share link" action in the clip detail panel and clips list that copies the local URL to clipboard. Indicate clearly in the UI that this link only works on the local network.
- [ ] Keep the HTML server-rendered with minimal or no JavaScript. Use embedded CSS for styling. Do not introduce a frontend build step or SPA framework.
- [ ] Add security boundaries: only serve files referenced by known clip records (no arbitrary path traversal), validate clip IDs, and enforce auth if configured.
- [ ] Add rate limiting to prevent abuse if exposed on LAN.
- [ ] Add tests for route handling, path traversal prevention, auth enforcement, range request support, and missing-clip responses.

Done when: users can share a local URL that renders a rich clip page with thumbnail, event timeline, metadata, and in-browser video playback.

## Recommended Execution Order

### Wave 7A: Architecture Hardening

- [ ] ARCH-01 App state and reducer decomposition
- [ ] ARCH-02 Platform services boundary
- [ ] ARCH-03 ClipStore repository split
- [ ] ARCH-04 Config and migration hardening
- [ ] ARCH-05 Command runner and worker cleanup

### Wave 7B: Visual Foundation

- [ ] VIS-01 Thumbnail extraction and display
- [ ] ORG-02 Favorites and pinning
- [ ] ORG-03 Clip notes
- [ ] QOL-01 Soft delete and undo
- [ ] QOL-04 Clip integrity checker

### Wave 7C: Organization and Intelligence

- [ ] VIS-02 Scrubbing preview
- [ ] ORG-01 Tags and collections
- [ ] INTEL-01 Kill streak and multi-kill detection
- [ ] INTEL-02 Auto-highlight scoring
- [ ] QOL-02 Bulk operations on filtered results

### Wave 7D: Distribution and Polish

- [ ] INTEL-03 Best-of auto-curation
- [ ] QOL-03 Notification quick actions
- [ ] INT-04 Stream marker integration
- [ ] SHARE-01 Shareable clip pages

## Parking Lot

These ideas are intentionally not broken into active subtasks yet:

- Embedded video player (full in-app playback via `iced_video_player` or custom shader widget) — blocked on GStreamer packaging cost vs. value; revisit after VIS-02 lands and real user demand is measured
- Clip comparison view for overlapping clips (side-by-side scrubbing preview, depends on VIS-02 + UX-07)
- Cloud config sync (rule definitions and settings across machines)
- Outfit/squad clip aggregation (shared clip metadata across multiple NaniteClip instances)
- Export presets (platform-specific re-encode profiles: "Discord 25MB", "Reddit 1GB", "Archive quality")
- Config profiles beyond rule profiles (switch audio sources, upload targets, and Discord webhooks together)
- Alert performance dashboard (win rate, clip quality during alerts vs. off-alert, builds on DATA-04 + INT-02)
- Weapon meta analysis ("your top weapons by clip generation rate", builds on DATA-03 + INT-02)

Keep this list short. If something here becomes real work, promote it into a numbered task above with dependencies and completion criteria.
