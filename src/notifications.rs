use std::io;
use std::process::{Child, Command, Stdio};

use crate::rules::ClipLength;

#[cfg(target_os = "linux")]
const NOTIFICATION_TIMEOUT_SECS: &str = "3.0";
const REPLAY_ICON: &str = "replay";

pub struct NotificationCenter {
    active_notification: Option<Child>,
    notify_available: bool,
}

impl NotificationCenter {
    pub fn new() -> Self {
        Self {
            active_notification: None,
            notify_available: true,
        }
    }

    pub fn notify_clip_saved(&mut self, duration: ClipLength) {
        let text = format!(
            "Highlight clipped\nDuration: {}",
            format_clip_duration(duration)
        );
        self.notify(&text, Some(REPLAY_ICON));
    }

    pub fn notify_character_confirmed(&mut self, character_name: &str) {
        let text = format!("Character confirmed\nNow monitoring {character_name}");
        self.notify(&text, None);
    }

    pub fn notify_profile_activated(&mut self, profile_name: &str) {
        let text = format!("Profile activated\n{profile_name}");
        self.notify(&text, None);
    }

    fn notify(&mut self, text: &str, icon: Option<&str>) {
        if !self.notify_available {
            return;
        }

        self.reap_notification();
        self.stop_active_notification();

        let mut command = notification_command(text, icon);

        match command.spawn() {
            Ok(child) => {
                self.active_notification = Some(child);
            }
            Err(error) => {
                if error.kind() == io::ErrorKind::NotFound {
                    self.notify_available = false;
                }
                tracing::warn!("Failed to launch {}: {error}", notification_backend_name());
            }
        }
    }

    fn reap_notification(&mut self) {
        let Some(child) = self.active_notification.as_mut() else {
            return;
        };

        match child.try_wait() {
            Ok(Some(_)) => {
                self.active_notification = None;
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!("Failed to poll gsr-notify status: {error}");
                self.active_notification = None;
            }
        }
    }

    fn stop_active_notification(&mut self) {
        let Some(mut child) = self.active_notification.take() else {
            return;
        };

        if let Err(error) = child.kill()
            && error.kind() != io::ErrorKind::InvalidInput
        {
            tracing::warn!("Failed to stop previous gsr-notify process: {error}");
        }

        if let Err(error) = child.wait() {
            tracing::warn!("Failed waiting for previous gsr-notify process: {error}");
        }
    }
}

#[cfg(target_os = "linux")]
fn notification_command(text: &str, icon: Option<&str>) -> Command {
    let mut command = Command::new("gsr-notify");
    command
        .arg("--text")
        .arg(text)
        .arg("--timeout")
        .arg(NOTIFICATION_TIMEOUT_SECS)
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    if let Some(icon) = icon {
        command.arg("--icon").arg(icon);
    }

    command
}

#[cfg(target_os = "windows")]
fn notification_command(text: &str, _icon: Option<&str>) -> Command {
    let script = format!(
        concat!(
            "Add-Type -AssemblyName System.Windows.Forms | Out-Null;",
            "Add-Type -AssemblyName System.Drawing | Out-Null;",
            "$notify = New-Object System.Windows.Forms.NotifyIcon;",
            "$notify.Icon = [System.Drawing.SystemIcons]::Information;",
            "$notify.BalloonTipIcon = [System.Windows.Forms.ToolTipIcon]::Info;",
            "$notify.BalloonTipTitle = 'NaniteClip';",
            "$notify.BalloonTipText = '{text}';",
            "$notify.Visible = $true;",
            "$notify.ShowBalloonTip(3000);",
            "Start-Sleep -Milliseconds 3500;",
            "$notify.Dispose()"
        ),
        text = powershell_single_quoted(text),
    );
    let mut command = Command::new("powershell");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-Command",
        ])
        .arg(script)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn notification_command(text: &str, _icon: Option<&str>) -> Command {
    let mut command = Command::new("printf");
    command
        .arg("%s")
        .arg(text)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

#[cfg(target_os = "linux")]
fn notification_backend_name() -> &'static str {
    "gsr-notify"
}

#[cfg(target_os = "windows")]
fn notification_backend_name() -> &'static str {
    "PowerShell"
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn notification_backend_name() -> &'static str {
    "notification backend"
}

#[cfg(target_os = "windows")]
fn powershell_single_quoted(value: &str) -> String {
    value.replace('\'', "''")
}

fn format_clip_duration(duration: ClipLength) -> String {
    match duration {
        ClipLength::Seconds(value) => {
            if value == 1 {
                "1 second".into()
            } else {
                format!("{value} seconds")
            }
        }
        ClipLength::FullBuffer => "full buffer".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::format_clip_duration;
    use crate::rules::ClipLength;

    #[test]
    fn clip_duration_formats_seconds() {
        assert_eq!(format_clip_duration(ClipLength::Seconds(45)), "45 seconds");
    }

    #[test]
    fn clip_duration_formats_singular_second() {
        assert_eq!(format_clip_duration(ClipLength::Seconds(1)), "1 second");
    }

    #[test]
    fn clip_duration_formats_full_buffer() {
        assert_eq!(format_clip_duration(ClipLength::FullBuffer), "full buffer");
    }
}
