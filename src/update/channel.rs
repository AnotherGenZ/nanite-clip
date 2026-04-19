use std::env;
use std::path::Path;

use super::types::InstallChannel;

const INSTALL_MARKER_FILE: &str = "install-channel.txt";

pub fn detect_install_channel() -> InstallChannel {
    if is_flatpak_environment() {
        return InstallChannel::Flatpak;
    }

    let Ok(current_exe) = env::current_exe() else {
        return InstallChannel::Unsupported;
    };

    if let Some(marker) = read_install_marker(&current_exe) {
        return marker;
    }

    #[cfg(target_os = "windows")]
    {
        return detect_windows_install_channel(&current_exe);
    }

    #[cfg(target_os = "linux")]
    {
        return detect_linux_install_channel(&current_exe);
    }

    #[allow(unreachable_code)]
    InstallChannel::Unsupported
}

fn is_flatpak_environment() -> bool {
    env::var("FLATPAK_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .is_some()
        || Path::new("/.flatpak-info").exists()
}

fn read_install_marker(current_exe: &Path) -> Option<InstallChannel> {
    let mut candidates = Vec::new();
    if let Some(parent) = current_exe.parent() {
        candidates.push(parent.join(INSTALL_MARKER_FILE));
    }
    #[cfg(target_os = "linux")]
    {
        candidates.push(PathBuf::from("/usr/lib/nanite-clip").join(INSTALL_MARKER_FILE));
    }

    for candidate in candidates {
        let Ok(contents) = std::fs::read_to_string(&candidate) else {
            continue;
        };
        let channel = InstallChannel::from_marker(contents.trim());
        if channel != InstallChannel::Unsupported {
            return Some(channel);
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn detect_windows_install_channel(current_exe: &Path) -> InstallChannel {
    let path = current_exe.to_string_lossy().to_ascii_lowercase();
    if path.contains("\\program files\\") || path.contains("\\program files (x86)\\") {
        InstallChannel::WindowsMsi
    } else {
        InstallChannel::WindowsPortable
    }
}

#[cfg(target_os = "linux")]
fn detect_linux_install_channel(current_exe: &Path) -> InstallChannel {
    let path = current_exe.to_string_lossy();
    if path.starts_with("/app/") {
        InstallChannel::Flatpak
    } else if path.starts_with("/usr/bin/") || path.starts_with("/usr/local/bin/") {
        InstallChannel::LinuxPackageManaged
    } else {
        InstallChannel::LinuxPortable
    }
}
