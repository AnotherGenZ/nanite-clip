use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

mod compat;
mod io;
mod migration;

use chrono::{DateTime, Utc};
use compat::*;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

use crate::rules::{
    AutoSwitchRule, ClipLength, EventKind, RuleDefinition, RuleProfile, default_auto_switch_rules,
    default_rule_definitions, default_rule_profiles,
};
use crate::update::{PreparedUpdate, UpdateApplyReport, UpdateInstallBehavior};

pub use compat::legacy_audio_source_kind_from_value;
pub(crate) use compat::{default_clip_naming_template, normalize_audio_sources};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub schema_version: u32,
    pub service_id: String,
    pub characters: Vec<CharacterConfig>,
    pub rule_definitions: Vec<RuleDefinition>,
    pub rule_profiles: Vec<RuleProfile>,
    #[serde(default = "default_auto_switch_rules")]
    pub auto_switch_rules: Vec<AutoSwitchRule>,
    pub active_profile_id: String,
    #[serde(default, deserialize_with = "deserialize_launch_at_login_config")]
    pub launch_at_login: LaunchAtLoginConfig,
    #[serde(default)]
    pub auto_start_monitoring: bool,
    #[serde(default)]
    pub start_minimized: bool,
    #[serde(default)]
    pub minimize_to_tray: bool,
    #[serde(default = "default_clip_naming_template")]
    pub clip_naming_template: String,
    #[serde(default)]
    pub manual_clip: ManualClipConfig,
    #[serde(default)]
    pub storage_tiering: StorageTieringConfig,
    #[serde(default)]
    pub uploads: UploadsConfig,
    #[serde(default)]
    pub discord_webhook: DiscordWebhookConfig,
    #[serde(default)]
    pub capture: CaptureConfig,
    #[serde(default)]
    pub updates: AppUpdateConfig,
    pub recorder: RecorderConfig,
    #[serde(skip)]
    pub migration_notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterConfig {
    pub name: String,
    pub character_id: Option<u64>,
    #[serde(default)]
    pub world_id: Option<u32>,
    #[serde(default)]
    pub faction_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecorderConfig {
    pub replay_buffer_secs: u32,
    pub save_directory: PathBuf,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audio_sources: Vec<AudioSourceConfig>,
    #[serde(default)]
    pub post_processing: PostProcessingConfig,
    #[serde(default = "default_clip_saved_notifications")]
    pub clip_saved_notifications: bool,
    #[serde(default = "default_true")]
    pub auto_generate_thumbnails: bool,
    #[serde(default = "default_save_delay_secs")]
    pub save_delay_secs: u32,
    #[serde(default)]
    pub backends: BackendConfigs,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BackendConfigs {
    #[serde(default)]
    pub gsr: GsrBackendConfig,
    #[serde(default)]
    pub obs: ObsBackendConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GsrBackendConfig {
    pub capture_source: String,
    pub framerate: u32,
    pub codec: String,
    pub container: String,
    pub quality: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObsBackendConfig {
    #[serde(default = "default_obs_websocket_url")]
    pub websocket_url: String,
    #[serde(default)]
    pub management_mode: ObsManagementMode,
    #[serde(skip)]
    pub websocket_password: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ObsManagementMode {
    BringYourOwn,
    #[default]
    ManagedRecording,
    FullManagement,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CaptureConfig {
    #[serde(
        default = "default_capture_backend",
        deserialize_with = "deserialize_capture_backend"
    )]
    pub backend: CaptureBackend,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaptureBackend {
    Gsr,
    Obs,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AudioSourceConfig {
    #[serde(default)]
    pub label: String,
    pub kind: AudioSourceKind,
    #[serde(default)]
    pub gain_db: f32,
    #[serde(default)]
    pub muted_in_premix: bool,
    #[serde(default = "default_true")]
    pub included_in_premix: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AudioSourceKind {
    DefaultOutput,
    DefaultInput,
    Device { name: String },
    Application { name: String },
    ApplicationInverse { names: Vec<String> },
    Merged { entries: Vec<AudioSourceKind> },
    Raw { backend_id: String, value: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PostProcessAudioCodec {
    #[default]
    Aac,
    Opus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PostProcessingConfig {
    #[serde(default)]
    pub premix: PremixConfig,
    #[serde(default = "default_true")]
    pub preserve_originals: bool,
    #[serde(default = "default_true")]
    pub rewrite_track_titles: bool,
    #[serde(default)]
    pub codec: PostProcessAudioCodec,
    #[serde(default = "default_post_process_bitrate_kbps")]
    pub bitrate_kbps: u32,
    #[serde(default)]
    pub limiter: LimiterConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PremixConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub placement: PremixPlacement,
    #[serde(default)]
    pub normalization: PremixNormalization,
    #[serde(default)]
    pub duration_mode: PremixDurationMode,
    #[serde(default = "default_premix_track_title")]
    pub track_title: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PremixPlacement {
    #[default]
    First,
    Last,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PremixDurationMode {
    #[default]
    Longest,
    First,
    Shortest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PremixNormalization {
    AmixDivide,
    #[default]
    SumThenLimit,
    LoudnessTarget {
        target_lufs: f32,
        tp_db: f32,
        lra: f32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LimiterConfig {
    #[serde(default = "default_limiter_limit")]
    pub limit: f32,
    #[serde(default = "default_limiter_attack_ms")]
    pub attack_ms: f32,
    #[serde(default = "default_limiter_release_ms")]
    pub release_ms: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManualClipConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_manual_clip_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_manual_clip_duration_secs")]
    pub duration_secs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageTieringConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub tier_directory: PathBuf,
    #[serde(default = "default_storage_tiering_min_age_days")]
    pub min_age_days: u32,
    #[serde(default = "default_storage_tiering_max_score")]
    pub max_score: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct UploadsConfig {
    #[serde(default, alias = "streamable")]
    pub copyparty: CopypartyUploadConfig,
    #[serde(default)]
    pub youtube: YouTubeUploadConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CopypartyUploadConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_copyparty_upload_url", alias = "api_base_url")]
    pub upload_url: String,
    #[serde(
        default = "default_copyparty_public_base_url",
        alias = "public_base_url"
    )]
    pub public_base_url: String,
    #[serde(default)]
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct YouTubeUploadConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub privacy_status: YouTubePrivacyStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum YouTubePrivacyStatus {
    Public,
    #[default]
    Unlisted,
    Private,
}

impl std::fmt::Display for YouTubePrivacyStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Public => "Public",
            Self::Unlisted => "Unlisted",
            Self::Private => "Private",
        })
    }
}

impl std::fmt::Display for UpdateChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Stable => "Stable",
            Self::Beta => "Beta",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordWebhookConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_discord_min_score")]
    pub min_score: u32,
    #[serde(default)]
    pub include_thumbnail: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchAtLoginConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub provider: LaunchAtLoginProvider,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppUpdateConfig {
    #[serde(default = "default_true")]
    pub auto_check: bool,
    #[serde(default)]
    pub channel: UpdateChannel,
    #[serde(default)]
    pub install_behavior: UpdateInstallBehavior,
    #[serde(default)]
    pub skipped_version: Option<String>,
    #[serde(default)]
    pub remind_later_version: Option<String>,
    #[serde(default)]
    pub remind_later_until_utc: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_check_utc: Option<DateTime<Utc>>,
    #[serde(default)]
    pub install_id: Option<String>,
    #[serde(default)]
    pub current_version: Option<String>,
    #[serde(default)]
    pub installed_version_history: Vec<String>,
    #[serde(default)]
    pub prepared_update: Option<PreparedUpdate>,
    #[serde(default)]
    pub last_apply_report: Option<UpdateApplyReport>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LaunchAtLoginProvider {
    #[default]
    Auto,
    XdgAutostart,
    SystemdUser,
    WindowsStartupFolder,
    WindowsRegistryRun,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Beta,
}

impl Default for Config {
    fn default() -> Self {
        let rule_profiles = default_rule_profiles();
        let active_profile_id = rule_profiles
            .first()
            .map(|profile| profile.id.clone())
            .unwrap_or_default();
        Self {
            schema_version: 9,
            service_id: "s:example".into(),
            characters: Vec::new(),
            rule_definitions: default_rule_definitions(),
            rule_profiles,
            auto_switch_rules: default_auto_switch_rules(),
            active_profile_id,
            launch_at_login: LaunchAtLoginConfig::default(),
            auto_start_monitoring: false,
            start_minimized: false,
            minimize_to_tray: false,
            clip_naming_template: default_clip_naming_template(),
            manual_clip: ManualClipConfig::default(),
            storage_tiering: StorageTieringConfig::default(),
            uploads: UploadsConfig::default(),
            discord_webhook: DiscordWebhookConfig::default(),
            capture: CaptureConfig::default(),
            updates: AppUpdateConfig::default(),
            recorder: RecorderConfig::default(),
            migration_notice: None,
        }
    }
}

impl Default for RecorderConfig {
    fn default() -> Self {
        Self {
            replay_buffer_secs: 300,
            save_directory: default_save_directory(),
            audio_sources: vec![AudioSourceConfig::new(
                "Game audio",
                AudioSourceKind::DefaultOutput,
            )],
            post_processing: PostProcessingConfig::default(),
            clip_saved_notifications: default_clip_saved_notifications(),
            auto_generate_thumbnails: true,
            save_delay_secs: default_save_delay_secs(),
            backends: BackendConfigs::default(),
        }
    }
}

impl Default for GsrBackendConfig {
    fn default() -> Self {
        Self {
            capture_source: "planetside2".into(),
            framerate: 60,
            codec: "h264".into(),
            container: "mkv".into(),
            quality: "40000".into(),
        }
    }
}

impl Default for ObsBackendConfig {
    fn default() -> Self {
        Self {
            websocket_url: default_obs_websocket_url(),
            management_mode: ObsManagementMode::ManagedRecording,
            websocket_password: None,
        }
    }
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            backend: default_capture_backend(),
        }
    }
}

impl Default for PostProcessingConfig {
    fn default() -> Self {
        Self {
            premix: PremixConfig::default(),
            preserve_originals: true,
            rewrite_track_titles: true,
            codec: PostProcessAudioCodec::Aac,
            bitrate_kbps: default_post_process_bitrate_kbps(),
            limiter: LimiterConfig::default(),
        }
    }
}

impl Default for PremixConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            placement: PremixPlacement::First,
            normalization: PremixNormalization::SumThenLimit,
            duration_mode: PremixDurationMode::Longest,
            track_title: default_premix_track_title(),
        }
    }
}

impl Default for LimiterConfig {
    fn default() -> Self {
        Self {
            limit: default_limiter_limit(),
            attack_ms: default_limiter_attack_ms(),
            release_ms: default_limiter_release_ms(),
        }
    }
}

impl Default for ManualClipConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            hotkey: default_manual_clip_hotkey(),
            duration_secs: default_manual_clip_duration_secs(),
        }
    }
}

impl Default for StorageTieringConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            tier_directory: default_storage_tiering_directory(),
            min_age_days: default_storage_tiering_min_age_days(),
            max_score: default_storage_tiering_max_score(),
        }
    }
}

impl Default for CopypartyUploadConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            upload_url: default_copyparty_upload_url(),
            public_base_url: default_copyparty_public_base_url(),
            username: String::new(),
        }
    }
}

impl Default for YouTubeUploadConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            client_id: String::new(),
            privacy_status: YouTubePrivacyStatus::Unlisted,
        }
    }
}

impl Default for DiscordWebhookConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_score: default_discord_min_score(),
            include_thumbnail: false,
        }
    }
}

impl Default for LaunchAtLoginConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: LaunchAtLoginProvider::Auto,
        }
    }
}

impl Default for AppUpdateConfig {
    fn default() -> Self {
        Self {
            auto_check: true,
            channel: UpdateChannel::Stable,
            install_behavior: UpdateInstallBehavior::Manual,
            skipped_version: None,
            remind_later_version: None,
            remind_later_until_utc: None,
            last_check_utc: None,
            install_id: None,
            current_version: None,
            installed_version_history: Vec::new(),
            prepared_update: None,
            last_apply_report: None,
        }
    }
}

impl Config {
    pub fn config_path() -> PathBuf {
        directories::ProjectDirs::from("", "", "nanite-clip")
            .map(|d| d.config_dir().join("config.toml"))
            .unwrap_or_else(|| PathBuf::from("nanite-clip.toml"))
    }

    pub fn load() -> Self {
        io::load_from_path(&Self::config_path())
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        io::save_to_path(&Self::config_path(), self)
    }

    pub fn normalize(&mut self) {
        migration::normalize_config(self);
    }

    fn load_legacy(path: &Path, contents: &str) -> Option<Self> {
        if let Ok(legacy) = toml::from_str::<LegacyStateMachineConfig>(contents) {
            archive_legacy_config(path, contents);
            let mut config = Self::from_legacy_parts(
                legacy.service_id,
                legacy.characters,
                legacy.recorder,
                "Migrated from state-machine rules. The old config was backed up and default scoring rules were installed.",
            );
            config.normalize();
            return Some(config);
        }

        if let Ok(legacy) = toml::from_str::<LegacyPresetConfig>(contents) {
            archive_legacy_config(path, contents);
            let mut config = Self::from_legacy_parts(
                legacy.service_id,
                legacy.characters,
                legacy.recorder,
                "Migrated from preset clip rules. The old config was backed up and default scoring rules were installed.",
            );
            config.normalize();
            return Some(config);
        }

        None
    }

    fn from_legacy_parts(
        service_id: String,
        characters: Vec<CharacterConfig>,
        recorder: RecorderConfig,
        migration_notice: &str,
    ) -> Self {
        let mut config = Self {
            service_id,
            characters,
            recorder,
            ..Self::default()
        };
        config.migration_notice = Some(migration_notice.into());
        config
    }
}

impl LaunchAtLoginConfig {
    pub fn normalize(&mut self) {
        if !self.enabled && self.provider == LaunchAtLoginProvider::Auto {}
    }
}

impl AppUpdateConfig {
    pub fn normalize(&mut self) {
        self.skipped_version = self.skipped_version.take().and_then(|value| {
            let trimmed = value.trim().to_string();
            (!trimmed.is_empty()).then_some(trimmed)
        });
        self.install_id = self.install_id.take().and_then(|value| {
            let trimmed = value.trim().to_string();
            (!trimmed.is_empty()).then_some(trimmed)
        });
        self.current_version = self.current_version.take().and_then(|value| {
            let trimmed = value.trim().to_string();
            (!trimmed.is_empty()).then_some(trimmed)
        });
        self.remind_later_version = self.remind_later_version.take().and_then(|value| {
            let trimmed = value.trim().to_string();
            (!trimmed.is_empty()).then_some(trimmed)
        });
        let current_version = self.current_version.clone();
        let mut deduped_history = Vec::new();
        for version in std::mem::take(&mut self.installed_version_history) {
            let trimmed = version.trim().to_string();
            if trimmed.is_empty() || current_version.as_deref() == Some(trimmed.as_str()) {
                continue;
            }
            if !deduped_history.contains(&trimmed) {
                deduped_history.push(trimmed);
            }
        }
        deduped_history.truncate(10);
        self.installed_version_history = deduped_history;
        if self
            .remind_later_until_utc
            .is_some_and(|until| until <= Utc::now())
        {
            self.remind_later_until_utc = None;
            self.remind_later_version = None;
        }
    }

    pub fn ensure_install_id(&mut self) -> bool {
        if self
            .install_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            return false;
        }

        let current_exe = std::env::current_exe()
            .ok()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let seed = format!(
            "{}:{}:{}:{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            Config::config_path().display(),
            current_exe
        );
        self.install_id = Some(format!("{:x}", Sha256::digest(seed.as_bytes())));
        true
    }
}

impl CaptureConfig {
    pub fn normalize(&mut self) {
        self.backend = CaptureBackend::from_config_value(self.backend.as_str());
    }
}

impl CaptureBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gsr => "gsr",
            Self::Obs => "obs",
        }
    }

    pub fn from_config_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "obs" => Self::Obs,
            #[cfg(target_os = "linux")]
            "gsr" | "" => Self::Gsr,
            #[cfg(not(target_os = "linux"))]
            "gsr" => Self::Gsr,
            _ => default_capture_backend(),
        }
    }
}

impl std::fmt::Display for CaptureBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl BackendConfigs {
    pub fn normalize(&mut self) {
        self.gsr.normalize();
        self.obs.normalize();
    }
}

impl GsrBackendConfig {
    pub fn normalize(&mut self) {
        if self.capture_source.trim().is_empty() || self.capture_source == "screen" {
            self.capture_source = "planetside2".into();
        }
        self.codec = normalize_string_setting(self.codec.as_str(), "h264");
        self.container = normalize_string_setting(self.container.as_str(), "mkv");
        self.quality = normalize_string_setting(self.quality.as_str(), "40000");
    }
}

impl ObsBackendConfig {
    pub fn normalize(&mut self) {
        self.websocket_url = normalize_urlish(self.websocket_url.as_str(), "ws://127.0.0.1:4455");
        self.websocket_password = self.websocket_password.take().and_then(|value| {
            let trimmed = value.trim().to_string();
            (!trimmed.is_empty()).then_some(trimmed)
        });
    }
}

fn deserialize_launch_at_login_config<'de, D>(
    deserializer: D,
) -> Result<LaunchAtLoginConfig, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum LaunchAtLoginConfigSerde {
        LegacyBool(bool),
        Config(LaunchAtLoginConfig),
    }

    match Option::<LaunchAtLoginConfigSerde>::deserialize(deserializer)? {
        Some(LaunchAtLoginConfigSerde::LegacyBool(enabled)) => Ok(LaunchAtLoginConfig {
            enabled,
            ..LaunchAtLoginConfig::default()
        }),
        Some(LaunchAtLoginConfigSerde::Config(config)) => Ok(config),
        None => Ok(LaunchAtLoginConfig::default()),
    }
}

impl RecorderConfig {
    #[allow(dead_code)]
    pub fn audio_sources(&self) -> &[AudioSourceConfig] {
        &self.audio_sources
    }

    pub fn gsr(&self) -> &GsrBackendConfig {
        &self.backends.gsr
    }

    pub fn gsr_mut(&mut self) -> &mut GsrBackendConfig {
        &mut self.backends.gsr
    }

    pub fn obs(&self) -> &ObsBackendConfig {
        &self.backends.obs
    }

    pub fn obs_mut(&mut self) -> &mut ObsBackendConfig {
        &mut self.backends.obs
    }
}

impl AudioSourceConfig {
    pub fn new(label: impl Into<String>, kind: AudioSourceKind) -> Self {
        Self {
            label: label.into(),
            kind,
            gain_db: 0.0,
            muted_in_premix: false,
            included_in_premix: true,
        }
    }

    pub fn normalize(&mut self) -> bool {
        self.kind.normalize();
        if !self.kind.is_valid() {
            return false;
        }

        self.label = if self.label.trim().is_empty() {
            self.kind.default_label()
        } else {
            self.label.trim().to_string()
        };
        self.gain_db = self.gain_db.clamp(-60.0, 12.0);
        true
    }
}

impl AudioSourceKind {
    pub fn config_display_value(&self) -> String {
        match self {
            Self::DefaultOutput => "default_output".into(),
            Self::DefaultInput => "default_input".into(),
            Self::Device { name } => format!("device:{}", name.trim()),
            Self::Application { name } => format!("app:{}", name.trim()),
            Self::ApplicationInverse { names } => names
                .first()
                .map(|name| format!("app-inverse:{}", name.trim()))
                .unwrap_or_default(),
            Self::Merged { entries } => entries
                .iter()
                .map(Self::config_display_value)
                .collect::<Vec<_>>()
                .join("|"),
            Self::Raw { value, .. } => value.trim().to_string(),
        }
    }

    pub fn default_label(&self) -> String {
        match self {
            Self::DefaultOutput => "Default output".into(),
            Self::DefaultInput => "Default input".into(),
            Self::Device { name } => name.trim().to_string(),
            Self::Application { name } => name.trim().to_string(),
            Self::ApplicationInverse { names } => names
                .first()
                .map(|name| format!("Everything except {}", name.trim()))
                .unwrap_or_else(|| "Application inverse".into()),
            Self::Merged { entries } => entries
                .iter()
                .map(Self::default_label)
                .collect::<Vec<_>>()
                .join(" + "),
            Self::Raw { value, .. } => value.trim().to_string(),
        }
    }

    fn normalize(&mut self) {
        match self {
            Self::DefaultOutput | Self::DefaultInput => {}
            Self::Device { name } | Self::Application { name } => {
                *name = name.trim().to_string();
            }
            Self::ApplicationInverse { names } => {
                let mut normalized = Vec::new();
                for name in std::mem::take(names) {
                    let trimmed = name.trim();
                    if trimmed.is_empty()
                        || normalized
                            .iter()
                            .any(|existing: &String| existing.eq_ignore_ascii_case(trimmed))
                    {
                        continue;
                    }
                    normalized.push(trimmed.to_string());
                }
                *names = normalized;
            }
            Self::Merged { entries } => {
                for entry in entries.iter_mut() {
                    entry.normalize();
                }
                entries.retain(AudioSourceKind::is_valid);
            }
            Self::Raw { backend_id, value } => {
                *backend_id = backend_id.trim().to_ascii_lowercase();
                *value = value.trim().to_string();
            }
        }
    }

    fn is_valid(&self) -> bool {
        match self {
            Self::DefaultOutput | Self::DefaultInput => true,
            Self::Device { name } | Self::Application { name } => !name.trim().is_empty(),
            Self::ApplicationInverse { names } => !names.is_empty(),
            Self::Merged { entries } => !entries.is_empty(),
            Self::Raw { backend_id, value } => {
                !backend_id.trim().is_empty() && !value.trim().is_empty()
            }
        }
    }
}

impl PostProcessingConfig {
    pub fn normalize(&mut self) {
        self.premix.normalize();
        self.bitrate_kbps = self.bitrate_kbps.clamp(32, 512);
        self.limiter.normalize();
    }
}

impl PremixConfig {
    pub fn normalize(&mut self) {
        if self.track_title.trim().is_empty() {
            self.track_title = default_premix_track_title();
        } else {
            self.track_title = self.track_title.trim().to_string();
        }
        if let PremixNormalization::LoudnessTarget {
            target_lufs,
            tp_db,
            lra,
        } = &mut self.normalization
        {
            *target_lufs = target_lufs.clamp(-70.0, -1.0);
            *tp_db = tp_db.clamp(-9.0, 0.0);
            *lra = lra.clamp(1.0, 20.0);
        }
    }
}

impl LimiterConfig {
    pub fn normalize(&mut self) {
        self.limit = self.limit.clamp(0.0, 1.0);
        self.attack_ms = self.attack_ms.clamp(0.1, 500.0);
        self.release_ms = self.release_ms.clamp(1.0, 5000.0);
    }
}

impl ManualClipConfig {
    pub fn normalize(&mut self) {
        if self.hotkey.trim().is_empty() {
            self.hotkey = default_manual_clip_hotkey();
        }
        self.duration_secs = self.duration_secs.clamp(5, 300);
    }
}

impl StorageTieringConfig {
    pub fn normalize(&mut self) {
        if self.tier_directory.as_os_str().is_empty() {
            self.tier_directory = default_storage_tiering_directory();
        }
        self.min_age_days = self.min_age_days.clamp(1, 3650);
        self.max_score = self.max_score.clamp(1, 10_000);
    }
}

impl UploadsConfig {
    pub fn normalize(&mut self) {
        self.copyparty.normalize();
        self.youtube.normalize();
    }
}

impl CopypartyUploadConfig {
    pub fn normalize(&mut self) {
        self.upload_url = normalize_urlish(
            self.upload_url.as_str(),
            default_copyparty_upload_url().as_str(),
        );
        self.public_base_url = normalize_urlish(
            self.public_base_url.as_str(),
            default_copyparty_public_base_url().as_str(),
        );
        self.username = self.username.trim().to_string();
    }
}

impl YouTubeUploadConfig {
    pub fn normalize(&mut self) {
        self.client_id = self.client_id.trim().to_string();
    }
}

impl DiscordWebhookConfig {
    pub fn normalize(&mut self) {
        self.min_score = self.min_score.clamp(1, 10_000);
    }
}
