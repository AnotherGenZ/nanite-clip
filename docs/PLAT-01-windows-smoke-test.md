# PLAT-01 Windows Smoke Test

Manual verification checklist for the Wave 6 OBS backend on Windows.

## Setup

1. Install OBS Studio 28.0 or newer on a Windows 10 or Windows 11 machine.
2. In OBS, create a scene with a Display Capture source pointed at the monitor where PlanetSide 2 will run.
3. Enable Replay Buffer in `Settings -> Output -> Replay Buffer` and set the duration to 5 minutes.
4. Enable obs-websocket in `Tools -> WebSocket Server Settings` and note the URL and password.
5. Install the Windows build artifact for NaniteClip.

## Bring Your Own

1. Open `Settings -> Recorder`.
2. Set the capture backend to `OBS Studio (WebSocket)`.
3. Set the management mode to `Bring Your Own`.
4. Enter the OBS websocket URL and password, then click `Test connection`.
5. Save settings.
6. Launch PlanetSide 2 and confirm NaniteClip advances `Waiting for PS2 -> Waiting for login -> Monitoring`.
7. Trigger a manual hotkey save and confirm:
   - OBS writes a replay-buffer clip.
   - NaniteClip adds the same clip to the Clips tab.
   - Post-processing still runs if enabled.
8. Trigger a rule-based save and confirm the same result.
9. Quit OBS while monitoring and confirm:
   - The Status tab shows an OBS reconnect banner within a few seconds.
   - The tray tooltip/menu label switches to the reconnect status.
   - NaniteClip does not crash.
10. Restart OBS and confirm NaniteClip reconnects automatically without user action.
11. Quit PlanetSide 2 and confirm NaniteClip returns to `Waiting for PS2` while OBS replay buffer keeps running.

## Managed Recording

1. Switch OBS management mode to `Managed Recording`.
2. Save settings.
3. In OBS, confirm:
   - `Settings -> Output -> Recording -> Recording Path` matches NaniteClip's save directory.
   - `Settings -> Output -> Replay Buffer -> Maximum Replay Time` matches `replay_buffer_secs`.
4. Change NaniteClip's replay buffer length and save directory, save again, and confirm OBS reflects both changes.
5. Trigger a save and confirm the clip lands in the updated directory.
6. Disable Replay Buffer in OBS while NaniteClip is monitoring and confirm the Status tab shows a warning that recording will not save until replay buffer is re-enabled.
7. Re-enable Replay Buffer in OBS and confirm the warning clears after the next successful reconnect or replay-buffer state update.
8. Quit OBS and repeat the reconnect checks from the Bring Your Own flow.

## Cross-Version Check

1. Repeat the managed-recording save test against OBS 28.x.
2. Repeat it again against OBS 30+.
3. Verify OBS still accepts the recording-format parameter change in both versions.

## Linux Regression

1. On a Linux machine, switch back to the `gpu-screen-recorder` backend.
2. Run the existing monitoring and manual-save flow.
3. Confirm clip save, post-processing, uploads, and tray behavior remain unchanged.
