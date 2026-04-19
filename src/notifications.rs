use std::io;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "windows")]
use windows::core::Interface;

use crate::command_runner;
use crate::rules::ClipLength;

#[cfg(target_os = "linux")]
const NOTIFICATION_TIMEOUT_SECS: &str = "3.0";
const REPLAY_ICON: &str = "replay";
#[cfg(target_os = "windows")]
const WINDOWS_APP_USER_MODEL_ID: &str = "AnotherGenZ.NaniteClip";
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(target_os = "windows")]
const WINDOWS_NOTIFICATION_SHORTCUT_DIR_NAME: &str = "NaniteClip";
#[cfg(target_os = "windows")]
const WINDOWS_NOTIFICATION_SHORTCUT_FILE_NAME: &str = "NaniteClip.lnk";
#[cfg(target_os = "windows")]
const WINDOWS_LEGACY_NOTIFICATION_SHORTCUT_FILE_NAME: &str = "NaniteClip Runtime.lnk";
#[cfg(target_os = "windows")]
static WINDOWS_TOASTS_CONFIGURED: AtomicBool = AtomicBool::new(false);

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

        #[cfg(target_os = "windows")]
        {
            if WINDOWS_TOASTS_CONFIGURED.load(Ordering::Relaxed) {
                match show_windows_toast_notification(text) {
                    Ok(()) => return,
                    Err(error) => {
                        tracing::warn!(
                            "Failed to show Windows toast notification: {error}. Falling back to PowerShell balloon notification."
                        );
                    }
                }
            }
        }

        self.spawn_notification_process(text, icon);
    }

    fn spawn_notification_process(&mut self, text: &str, icon: Option<&str>) {
        let mut command = notification_command(text, icon);

        match command_runner::spawn(&mut command) {
            Ok(child) => {
                self.active_notification = Some(child);
            }
            Err(error) => {
                if let command_runner::CommandError::Spawn { source, .. } = &error
                    && source.kind() == io::ErrorKind::NotFound
                {
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
                tracing::warn!("Failed to poll notification backend status: {error}");
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
            tracing::warn!("Failed to stop previous notification backend: {error}");
        }

        if let Err(error) = child.wait() {
            tracing::warn!("Failed waiting for previous notification backend: {error}");
        }
    }
}

#[cfg(target_os = "windows")]
pub fn configure_windows_notifications() -> Result<(), String> {
    use std::env;
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize, IPersistFile, StructuredStorage::PROPVARIANT,
    };
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::GUID;
    use windows::core::HSTRING;

    const APP_USER_MODEL_ID_KEY: PROPERTYKEY = PROPERTYKEY {
        fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
        pid: 5,
    };

    let app_id = HSTRING::from(WINDOWS_APP_USER_MODEL_ID);
    unsafe { SetCurrentProcessExplicitAppUserModelID(&app_id) }
        .map_err(|error| format!("failed to set Windows AppUserModelID: {error}"))?;

    let current_exe = env::current_exe()
        .map_err(|error| format!("failed to resolve the current executable path: {error}"))?;
    let shortcut_path = windows_notification_shortcut_path();
    remove_legacy_windows_notification_shortcuts();
    if let Some(parent) = shortcut_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create the Windows notification shortcut directory `{}`: {error}",
                parent.display()
            )
        })?;
    }

    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        .ok()
        .map_err(|error| format!("failed to initialize COM for Windows notifications: {error}"))?;

    let install_result = (|| -> Result<(), String> {
        let shell_link: IShellLinkW = unsafe {
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|error| {
                format!("failed to allocate the Windows shell shortcut COM object: {error}")
            })?
        };

        let exe = HSTRING::from(current_exe.to_string_lossy().as_ref());
        unsafe { shell_link.SetPath(&exe) }.map_err(|error| {
            format!("failed to assign the notification shortcut target: {error}")
        })?;
        unsafe {
            shell_link
                .SetWorkingDirectory(&HSTRING::from(executable_working_directory(&current_exe)))
        }
        .map_err(|error| {
            format!("failed to assign the notification shortcut working directory: {error}")
        })?;
        unsafe { shell_link.SetIconLocation(&exe, 0) }
            .map_err(|error| format!("failed to assign the notification shortcut icon: {error}"))?;

        let property_store: IPropertyStore = shell_link.cast().map_err(|error| {
            format!("failed to open the notification shortcut property store: {error}")
        })?;
        let app_id_value = PROPVARIANT::from(WINDOWS_APP_USER_MODEL_ID);
        unsafe { property_store.SetValue(&APP_USER_MODEL_ID_KEY, &app_id_value) }.map_err(
            |error| format!("failed to assign the notification shortcut AppUserModelID: {error}"),
        )?;
        unsafe { property_store.Commit() }.map_err(|error| {
            format!("failed to commit the notification shortcut property store: {error}")
        })?;

        let persist_file: IPersistFile = shell_link.cast().map_err(|error| {
            format!("failed to open the notification shortcut persistence interface: {error}")
        })?;
        let shortcut = HSTRING::from(shortcut_path.to_string_lossy().as_ref());
        unsafe { persist_file.Save(&shortcut, true) }.map_err(|error| {
            format!(
                "failed to save the Windows notification shortcut `{}`: {error}",
                shortcut_path.display()
            )
        })?;

        Ok(())
    })();

    unsafe { CoUninitialize() };

    install_result?;
    WINDOWS_TOASTS_CONFIGURED.store(true, Ordering::Relaxed);
    Ok(())
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
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

#[cfg(target_os = "windows")]
fn show_windows_toast_notification(text: &str) -> Result<(), String> {
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    use windows::core::HSTRING;

    let xml = HSTRING::from(build_windows_toast_xml(text));
    let app_id = HSTRING::from(WINDOWS_APP_USER_MODEL_ID);

    let document =
        XmlDocument::new().map_err(|error| format!("failed to allocate toast XML: {error}"))?;
    document
        .LoadXml(&xml)
        .map_err(|error| format!("failed to parse toast XML: {error}"))?;

    let toast = ToastNotification::CreateToastNotification(&document)
        .map_err(|error| format!("failed to create toast notification: {error}"))?;
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&app_id)
        .map_err(|error| format!("failed to create toast notifier for `{app_id}`: {error}"))?;
    notifier
        .Show(&toast)
        .map_err(|error| format!("failed to show toast notification: {error}"))
}

#[cfg(target_os = "windows")]
fn build_windows_toast_xml(text: &str) -> String {
    let (title, body) = split_notification_text(text);
    match body {
        Some(body) => format!(
            concat!(
                "<toast>",
                "<visual><binding template=\"ToastGeneric\">",
                "<text>{}</text>",
                "<text>{}</text>",
                "</binding></visual>",
                "</toast>"
            ),
            xml_escape(title),
            xml_escape(body)
        ),
        None => format!(
            concat!(
                "<toast>",
                "<visual><binding template=\"ToastGeneric\">",
                "<text>{}</text>",
                "</binding></visual>",
                "</toast>"
            ),
            xml_escape(title)
        ),
    }
}

#[cfg(target_os = "windows")]
fn split_notification_text(text: &str) -> (&str, Option<&str>) {
    match text.split_once('\n') {
        Some((title, body)) => {
            let title = title.trim();
            let body = body.trim();
            if body.is_empty() {
                (title, None)
            } else {
                (title, Some(body))
            }
        }
        None => (text.trim(), None),
    }
}

#[cfg(target_os = "windows")]
fn xml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }
    escaped
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
    "Windows toast notification"
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

#[cfg(target_os = "windows")]
fn windows_notification_shortcut_path() -> PathBuf {
    windows_programs_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(WINDOWS_NOTIFICATION_SHORTCUT_DIR_NAME)
        .join(WINDOWS_NOTIFICATION_SHORTCUT_FILE_NAME)
}

#[cfg(target_os = "windows")]
fn legacy_windows_notification_shortcut_path() -> PathBuf {
    windows_programs_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(WINDOWS_NOTIFICATION_SHORTCUT_DIR_NAME)
        .join(WINDOWS_LEGACY_NOTIFICATION_SHORTCUT_FILE_NAME)
}

#[cfg(target_os = "windows")]
fn remove_legacy_windows_notification_shortcuts() {
    let legacy_shortcut = legacy_windows_notification_shortcut_path();
    match std::fs::remove_file(&legacy_shortcut) {
        Ok(()) => {
            tracing::info!(
                shortcut = %legacy_shortcut.display(),
                "removed legacy Windows notification shortcut"
            );
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(
                shortcut = %legacy_shortcut.display(),
                "failed to remove legacy Windows notification shortcut: {error}"
            );
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_programs_dir() -> Result<PathBuf, String> {
    directories::BaseDirs::new()
        .map(|dirs| {
            dirs.config_dir()
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
        })
        .ok_or_else(|| "failed to resolve the Windows Start Menu Programs directory".to_string())
}

#[cfg(target_os = "windows")]
fn executable_working_directory(executable: &Path) -> String {
    executable
        .parent()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::format_clip_duration;
    use crate::rules::ClipLength;

    #[cfg(target_os = "windows")]
    use super::{build_windows_toast_xml, split_notification_text, xml_escape};

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

    #[cfg(target_os = "windows")]
    #[test]
    fn split_notification_uses_first_line_as_title() {
        let (title, body) = split_notification_text("Highlight clipped\nDuration: 30 seconds");
        assert_eq!(title, "Highlight clipped");
        assert_eq!(body, Some("Duration: 30 seconds"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn xml_escape_rewrites_reserved_characters() {
        assert_eq!(
            xml_escape("A&B <test> \"quoted\""),
            "A&amp;B &lt;test&gt; &quot;quoted&quot;"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_toast_xml_includes_title_and_body() {
        let xml = build_windows_toast_xml("Profile activated\nHighlights");
        assert!(xml.contains("<text>Profile activated</text>"));
        assert!(xml.contains("<text>Highlights</text>"));
    }
}
