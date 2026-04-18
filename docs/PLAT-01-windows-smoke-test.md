# PLAT-01 Windows Release Checklist

Manual release validation checklist for Windows builds, with emphasis on MSI packaging,
shell integration, hotkeys, notifications, PlanetSide 2 detection, and OBS replay-buffer
behavior.

## Scope

Run this checklist before publishing a Windows release candidate and again against the final
GitHub release artifacts.

## Test Environment

1. Use a Windows 10 or Windows 11 machine.
2. Install OBS Studio 28.0 or newer.
3. In OBS, create a scene with a Display Capture source pointed at the monitor where PlanetSide 2 will run.
4. Enable obs-websocket in `Tools -> WebSocket Server Settings` and note the URL and password.
5. Prepare at least one PlanetSide 2 character configured in NaniteClip.
6. Keep both release artifacts available:
   - `nanite-clip-<version>-x86_64.msi`
   - `nanite-clip.exe`

## Release Artifact Verification

1. Confirm the GitHub release contains exactly one Windows MSI asset named `nanite-clip-<version>-x86_64.msi`.
2. Confirm the release also includes the portable `nanite-clip.exe`.
3. Confirm the MSI installs the same freshly built `nanite-clip.exe` that was produced for the release build.
4. Confirm the installed Start Menu shortcut points at `nanite-clip.exe` and uses the NaniteClip icon.

## Install, Upgrade, And Uninstall

1. On a clean machine, install the MSI.
2. Confirm NaniteClip appears in the Start Menu as `NaniteClip`.
3. Launch it from the Start Menu and pin it to the taskbar.
4. Confirm both the Start Menu entry and the taskbar button show the NaniteClip icon instead of a generic app icon.
5. Install a newer MSI over the existing version and confirm the upgrade succeeds without leaving duplicate Start Menu entries.
6. Uninstall NaniteClip and confirm the Start Menu entry is removed.

## Launch And Shell Integration

1. Launch the release build from the Start Menu.
2. Confirm only the NaniteClip window appears.
3. Confirm no PowerShell or console window flashes during startup.
4. Open Settings, change a value, and click Save.
5. Confirm the UI stays responsive and no console window flickers during save.
6. Launch the portable `nanite-clip.exe` directly and confirm notifications and icon behavior still work.
7. For a debug build only, confirm a console window is visible so runtime logs can be inspected.

## Notifications

1. Trigger a notification source such as:
   - active profile switching
   - a successful clip save
   - another user-visible status change that already emits a toast
2. Confirm Windows shows a toast.
3. Confirm the toast source name is `NaniteClip`.
4. Confirm the app is not identified as `Windows PowerShell` or `NaniteClip Runtime`.

## PlanetSide 2 Detection

1. Start NaniteClip before launching PlanetSide 2 and confirm the app remains in `Waiting for PS2`.
2. Launch PlanetSide 2 and confirm NaniteClip advances through `Waiting for login` to `Monitoring`.
3. Start NaniteClip while PlanetSide 2 is already running and confirm it still detects the game and reaches `Monitoring`.
4. Exit PlanetSide 2 and confirm NaniteClip returns to `Waiting for PS2`.

## Manual Hotkeys

1. Enable the manual clip hotkey in Settings and save.
2. Verify a standard binding such as `F8` or `Num0` registers successfully and triggers a manual clip while monitoring.
3. Verify keypad-specific bindings such as `NumEnter` and `NumEqual` register successfully on Windows and do not hijack the regular `Enter` or `=` keys.
4. Save settings repeatedly with the same hotkey and confirm NaniteClip does not report a self-conflict when re-registering the binding.
5. If another application already owns the chosen binding, confirm NaniteClip shows a clear conflict error instead of silently failing.

## OBS Bring Your Own

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
   - the Status tab shows an OBS reconnect banner within a few seconds
   - the tray tooltip or menu label switches to the reconnect status
   - NaniteClip does not crash
10. Restart OBS and confirm NaniteClip reconnects automatically without user action.

## OBS Managed Recording

1. Switch OBS management mode to `Managed Recording`.
2. Save settings.
3. In OBS, confirm:
   - `Settings -> Output -> Recording -> Recording Path` matches NaniteClip's save directory
   - `Settings -> Output -> Replay Buffer -> Maximum Replay Time` matches `replay_buffer_secs`
4. Change NaniteClip's replay buffer length and save directory, save again, and confirm OBS reflects both changes.
5. Trigger a save and confirm the clip lands in the updated directory.
6. Disable Replay Buffer in OBS while NaniteClip is monitoring and confirm NaniteClip shows a user-visible warning instead of spamming raw error logs.
7. Re-enable Replay Buffer in OBS and confirm the warning clears after the next successful reconnect or replay-buffer state update.
8. Start NaniteClip when OBS is already open and Replay Buffer is already active.
9. Confirm NaniteClip reuses the active replay buffer when OBS already matches the requested managed settings instead of timing out while trying to restart it.
10. Quit OBS and repeat the reconnect checks from the Bring Your Own flow.

## Cross-Version OBS Check

1. Repeat the managed-recording save test against OBS 28.x.
2. Repeat it again against OBS 30+.
3. Verify OBS still accepts the recording-format parameter change in both versions.

## Sign-Off

Do not publish the Windows release until all checklist items above pass or any remaining failures
are documented as known release blockers with an explicit follow-up plan.
