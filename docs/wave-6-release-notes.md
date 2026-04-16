# Wave 6 Release Notes

Wave 6 completes the recorder-backend split and adds an OBS-based capture path that works with the same monitoring, rule evaluation, clip save, post-process, and distribution pipeline as the existing Linux `gpu-screen-recorder` flow.

## Highlights

- Two capture backends are now available on Linux: `gpu-screen-recorder` and `OBS Studio (WebSocket)`.
- OBS support introduces three management modes in the settings model:
  - `Bring Your Own`
  - `Managed Recording`
  - `Full Management` (reserved in the UI, still runtime-unsupported in this release)
- OBS support requires OBS Studio 28.0 or newer.
- `Managed Recording` supports only OBS `Simple` output mode.
- Windows tray support is wired through the cross-platform tray backend.
- OBS saves the full replay buffer in this release. Per-save trimming for `ClipLength::Seconds(n)` remains a follow-up.

## Behavior Notes

- Under `Bring Your Own` and `Managed Recording`, OBS owns the scene setup and audio routing.
- NaniteClip stores the OBS websocket password in the secure store rather than `config.toml`.
- If OBS disconnects during monitoring, NaniteClip automatically reconnects with backoff and surfaces that state in the Status tab and tray.
