use std::env;
use std::path::{Path, PathBuf};

use super::types::{InstallChannel, SystemUpdatePlan};

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

pub fn detect_system_update_plan(install_channel: InstallChannel) -> Option<SystemUpdatePlan> {
    #[cfg(target_os = "linux")]
    {
        let packagekit_available = command_is_available("pkcon");
        let flatpak_available = command_is_available("flatpak");
        return system_update_plan_for_channel(
            install_channel,
            packagekit_available,
            flatpak_available,
        );
    }

    #[allow(unreachable_code)]
    None
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

#[cfg(target_os = "linux")]
fn system_update_plan_for_channel(
    install_channel: InstallChannel,
    packagekit_available: bool,
    flatpak_available: bool,
) -> Option<SystemUpdatePlan> {
    match install_channel {
        InstallChannel::Flatpak => Some(SystemUpdatePlan {
            label: "Flatpak".into(),
            detail: if flatpak_available {
                "Launch the Flatpak update command for this app.".into()
            } else {
                "Update this Flatpak install with `flatpak update dev.angz.NaniteClip` or your software center.".into()
            },
            command_display: Some("flatpak update dev.angz.NaniteClip".into()),
            command_program: flatpak_available.then_some("flatpak".into()),
            command_args: if flatpak_available {
                vec!["update".into(), "dev.angz.NaniteClip".into()]
            } else {
                Vec::new()
            },
        }),
        InstallChannel::LinuxDeb
        | InstallChannel::LinuxRpm
        | InstallChannel::LinuxPacman
        | InstallChannel::LinuxPackageManaged => Some(SystemUpdatePlan {
            label: if packagekit_available {
                "PackageKit".into()
            } else {
                install_channel.label().into()
            },
            detail: if packagekit_available {
                "Launch PackageKit's native update flow. This may include other pending system package updates.".into()
            } else {
                "Use your distro's package manager or software center to update this install."
                    .into()
            },
            command_display: packagekit_available.then_some("pkcon update".into()),
            command_program: packagekit_available.then_some("pkcon".into()),
            command_args: if packagekit_available {
                vec!["update".into()]
            } else {
                Vec::new()
            },
        }),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn command_is_available(program: &str) -> bool {
    env::var_os("PATH").is_some_and(|path| {
        env::split_paths(&path).any(|directory| directory.join(program).is_file())
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn system_update_plan_prefers_packagekit_for_package_managed_installs() {
        let plan = system_update_plan_for_channel(InstallChannel::LinuxDeb, true, false).unwrap();

        assert_eq!(plan.label, "PackageKit");
        assert_eq!(plan.command_display.as_deref(), Some("pkcon update"));
        assert!(plan.can_launch());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn system_update_plan_keeps_flatpak_command_even_without_launcher() {
        let plan = system_update_plan_for_channel(InstallChannel::Flatpak, false, false).unwrap();

        assert_eq!(
            plan.command_display.as_deref(),
            Some("flatpak update dev.angz.NaniteClip")
        );
        assert!(!plan.can_launch());
    }
}
