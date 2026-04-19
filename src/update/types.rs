use std::path::PathBuf;

use chrono::{DateTime, Utc};
use semver::Version;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallChannel {
    WindowsMsi,
    WindowsPortable,
    LinuxPortable,
    LinuxDeb,
    LinuxRpm,
    LinuxPacman,
    LinuxPackageManaged,
    Flatpak,
    Unsupported,
}

impl InstallChannel {
    pub fn label(self) -> &'static str {
        match self {
            Self::WindowsMsi => "Windows MSI",
            Self::WindowsPortable => "Windows Portable",
            Self::LinuxPortable => "Linux Portable",
            Self::LinuxDeb => "Linux DEB",
            Self::LinuxRpm => "Linux RPM",
            Self::LinuxPacman => "Linux pacman",
            Self::LinuxPackageManaged => "Linux Package Manager",
            Self::Flatpak => "Flatpak",
            Self::Unsupported => "Unsupported",
        }
    }

    pub fn supports_self_update(self) -> bool {
        matches!(
            self,
            Self::WindowsMsi | Self::WindowsPortable | Self::LinuxPortable
        )
    }

    pub fn update_instructions(self) -> &'static str {
        match self {
            Self::WindowsMsi | Self::WindowsPortable | Self::LinuxPortable => {
                "Ready for in-app updates."
            }
            Self::LinuxDeb | Self::LinuxRpm | Self::LinuxPacman | Self::LinuxPackageManaged => {
                "Update this install with your system package manager."
            }
            Self::Flatpak => "Update this install with Flatpak or your software center.",
            Self::Unsupported => "Automatic updates are not supported for this install layout.",
        }
    }

    pub fn from_marker(value: &str) -> Self {
        match value.trim() {
            "windows_msi" => Self::WindowsMsi,
            "windows_portable" => Self::WindowsPortable,
            "linux_portable" => Self::LinuxPortable,
            "linux_deb" => Self::LinuxDeb,
            "linux_rpm" => Self::LinuxRpm,
            "linux_pacman" => Self::LinuxPacman,
            "linux_package_managed" => Self::LinuxPackageManaged,
            "flatpak" => Self::Flatpak,
            _ => Self::Unsupported,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateAssetKind {
    Msi,
    Exe,
    TarGz,
}

impl UpdateAssetKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Msi => "MSI installer",
            Self::Exe => "portable executable",
            Self::TarGz => "portable tarball",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UpdateInstallBehavior {
    #[default]
    Manual,
    WhenIdle,
    OnNextLaunch,
}

impl UpdateInstallBehavior {
    pub fn label(self) -> &'static str {
        match self {
            Self::Manual => "Manual",
            Self::WhenIdle => "When idle",
            Self::OnNextLaunch => "On next launch",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Manual => "Keep downloaded updates staged until you choose Install and Restart.",
            Self::WhenIdle => {
                "Apply downloaded updates automatically once monitoring and recording are idle."
            }
            Self::OnNextLaunch => "Keep the update staged and remind you on the next launch.",
        }
    }
}

impl std::fmt::Display for UpdateInstallBehavior {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UpdatePrimaryAction {
    #[default]
    DownloadUpdate,
    InstallAndRestart,
    InstallWhenIdle,
    InstallOnNextLaunch,
    RemindLater,
    SkipThisVersion,
}

impl UpdatePrimaryAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::DownloadUpdate => "Download Update",
            Self::InstallAndRestart => "Install and Restart",
            Self::InstallWhenIdle => "Install When Idle",
            Self::InstallOnNextLaunch => "Install on Next Launch",
            Self::RemindLater => "Remind Me Later",
            Self::SkipThisVersion => "Skip This Version",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::DownloadUpdate => {
                "Download the latest compatible release asset into the local staging area."
            }
            Self::InstallAndRestart => {
                "Apply the staged target immediately and relaunch NaniteClip."
            }
            Self::InstallWhenIdle => {
                "Keep the staged target ready and install it automatically when monitoring is idle."
            }
            Self::InstallOnNextLaunch => {
                "Keep the staged target ready and prompt for installation the next time NaniteClip launches."
            }
            Self::RemindLater => "Hide update reminders for the next 12 hours.",
            Self::SkipThisVersion => {
                "Suppress automatic reminders for the currently detected release."
            }
        }
    }
}

impl std::fmt::Display for UpdatePrimaryAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatePhase {
    Idle,
    Checking,
    Downloading,
    Verifying,
    ReadyToInstall,
    Applying,
    Failed,
}

impl UpdatePhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Checking => "Checking",
            Self::Downloading => "Downloading",
            Self::Verifying => "Verifying",
            Self::ReadyToInstall => "Ready to install",
            Self::Applying => "Applying",
            Self::Failed => "Failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateErrorKind {
    Network,
    Verification,
    Install,
    Unknown,
}

impl UpdateErrorKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Network => "Network",
            Self::Verification => "Verification",
            Self::Install => "Install",
            Self::Unknown => "Updater",
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpdateErrorState {
    pub kind: UpdateErrorKind,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct UpdateProgressState {
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestAsset {
    pub channel: InstallChannel,
    pub kind: UpdateAssetKind,
    pub filename: String,
    pub download_url: String,
    pub sha256: String,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableRelease {
    pub version: Version,
    pub tag_name: String,
    pub release_name: String,
    pub html_url: String,
    pub asset: Option<ManifestAsset>,
    pub install_channel: InstallChannel,
    pub skipped: bool,
}

impl AvailableRelease {
    pub fn supports_download(&self) -> bool {
        self.install_channel.supports_self_update() && self.asset.is_some()
    }
}

impl std::fmt::Display for AvailableRelease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.release_name.trim().is_empty() || self.release_name == self.version.to_string() {
            write!(f, "{}", self.version)
        } else {
            write!(f, "{} ({})", self.version, self.release_name)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedUpdate {
    pub version: String,
    pub tag_name: String,
    pub install_channel: InstallChannel,
    pub asset_kind: UpdateAssetKind,
    pub asset_name: String,
    pub asset_path: PathBuf,
    pub release_notes_url: String,
}

impl PreparedUpdate {
    pub fn parsed_version(&self) -> Option<Version> {
        Version::parse(&self.version).ok()
    }
}

#[derive(Debug, Clone)]
pub struct UpdateState {
    pub install_channel: InstallChannel,
    pub current_version: Version,
    pub previous_installed_version: Option<Version>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub checking: bool,
    pub phase: UpdatePhase,
    pub progress: Option<UpdateProgressState>,
    pub latest_release: Option<AvailableRelease>,
    pub rollback_candidates: Vec<AvailableRelease>,
    pub rollback_catalog_loading: bool,
    pub prepared_update: Option<PreparedUpdate>,
    pub last_error: Option<UpdateErrorState>,
}

impl UpdateState {
    pub fn new(install_channel: InstallChannel, current_version: Version) -> Self {
        Self {
            install_channel,
            current_version,
            previous_installed_version: None,
            last_checked_at: None,
            checking: false,
            phase: UpdatePhase::Idle,
            progress: None,
            latest_release: None,
            rollback_candidates: Vec::new(),
            rollback_catalog_loading: false,
            prepared_update: None,
            last_error: None,
        }
    }

    pub fn has_downloaded_update(&self) -> bool {
        self.prepared_update.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadStep {
    Downloading,
    Verifying,
}

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub step: DownloadStep,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}
