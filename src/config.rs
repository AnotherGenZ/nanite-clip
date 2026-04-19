use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};

use crate::rules::schedule::{default_schedule_weekdays, legacy_cron_to_local_schedule};
use crate::rules::{
    AutoSwitchCondition, AutoSwitchRule, ClipLength, EventKind, RuleDefinition, RuleProfile,
    default_auto_switch_rules, default_rule_definitions, default_rule_profiles,
    normalized_active_character_ids, validate_auto_switch_rule,
};
use crate::update::{PreparedUpdate, UpdateInstallBehavior};

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
    #[serde(default = "default_capture_backend")]
    pub backend: String,
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
    pub prepared_update: Option<PreparedUpdate>,
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
            schema_version: 8,
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
            prepared_update: None,
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
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => match toml::from_str::<Config>(&contents) {
                Ok(mut config) => {
                    config.normalize();
                    tracing::info!("Loaded config from {}", path.display());
                    config
                }
                Err(error) => {
                    if let Some(config) = Self::load_legacy(&path, &contents) {
                        return config;
                    }

                    tracing::warn!("Failed to parse config: {error}, using defaults");
                    Self::default()
                }
            },
            Err(_) => {
                tracing::info!("No config found at {}, using defaults", path.display());
                Self::default()
            }
        }
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&path, contents)?;
        tracing::info!("Saved config to {}", path.display());
        Ok(())
    }

    pub fn normalize(&mut self) {
        self.schema_version = 8;

        if self.recorder.backends.gsr.capture_source.trim().is_empty()
            || self.recorder.backends.gsr.capture_source == "screen"
        {
            self.recorder.backends.gsr.capture_source = "planetside2".into();
        }
        self.recorder.audio_sources =
            normalize_audio_sources(std::mem::take(&mut self.recorder.audio_sources));

        if self.clip_naming_template.trim().is_empty() {
            self.clip_naming_template = default_clip_naming_template();
        }
        self.manual_clip.normalize();
        self.storage_tiering.normalize();
        self.uploads.normalize();
        self.discord_webhook.normalize();
        self.launch_at_login.normalize();
        self.capture.normalize();
        self.updates.normalize();
        self.recorder.backends.normalize();
        self.recorder.post_processing.normalize();

        if self.rule_definitions.is_empty() {
            self.rule_definitions = default_rule_definitions();
        }
        if self.rule_profiles.is_empty() {
            self.rule_profiles = default_rule_profiles();
        }
        if self.auto_switch_rules.is_empty() {
            self.auto_switch_rules = default_auto_switch_rules();
        }

        let first_resolved_character_id = self
            .characters
            .iter()
            .find_map(|character| character.character_id);
        let mut auto_switch_migration_notes = Vec::new();
        for rule in &mut self.auto_switch_rules {
            rule.condition = match &rule.condition {
                AutoSwitchCondition::LocalSchedule {
                    weekdays,
                    start_hour,
                    start_minute,
                    end_hour,
                    end_minute,
                } => {
                    let mut weekdays = weekdays.clone();
                    crate::rules::schedule::normalize_schedule_weekdays(&mut weekdays);
                    AutoSwitchCondition::LocalSchedule {
                        weekdays,
                        start_hour: *start_hour,
                        start_minute: *start_minute,
                        end_hour: *end_hour,
                        end_minute: *end_minute,
                    }
                }
                AutoSwitchCondition::ActiveCharacter {
                    character_ids,
                    character_id,
                } => AutoSwitchCondition::ActiveCharacter {
                    character_ids: normalized_active_character_ids(character_ids, *character_id),
                    character_id: None,
                },
                AutoSwitchCondition::LocalTimeRange {
                    start_hour,
                    end_hour,
                } => {
                    auto_switch_migration_notes.push(format!(
                        "Converted legacy local time rule `{}` into a weekday schedule.",
                        rule.name
                    ));
                    AutoSwitchCondition::LocalSchedule {
                        weekdays: default_schedule_weekdays(),
                        start_hour: *start_hour,
                        start_minute: 0,
                        end_hour: *end_hour,
                        end_minute: 0,
                    }
                }
                AutoSwitchCondition::LocalCron { expression } => {
                    match legacy_cron_to_local_schedule(expression) {
                        Ok(schedule) => {
                            auto_switch_migration_notes.push(format!(
                                "Converted legacy cron rule `{}` into the new weekday/time schedule editor.",
                                rule.name
                            ));
                            AutoSwitchCondition::LocalSchedule {
                                weekdays: schedule.weekdays,
                                start_hour: schedule.start_hour,
                                start_minute: schedule.start_minute,
                                end_hour: schedule.end_hour,
                                end_minute: schedule.end_minute,
                            }
                        }
                        Err(_) => {
                            auto_switch_migration_notes.push(format!(
                                "Legacy cron rule `{}` could not be mapped exactly, so it was reset to an every-day 18:00-23:00 schedule.",
                                rule.name
                            ));
                            AutoSwitchCondition::LocalSchedule {
                                weekdays: default_schedule_weekdays(),
                                start_hour: 18,
                                start_minute: 0,
                                end_hour: 23,
                                end_minute: 0,
                            }
                        }
                    }
                }
                AutoSwitchCondition::OnEvent { .. } => {
                    auto_switch_migration_notes.push(format!(
                        "Converted legacy event rule `{}` into an active-character rule that needs a character selection.",
                        rule.name
                    ));
                    AutoSwitchCondition::ActiveCharacter {
                        character_ids: first_resolved_character_id.into_iter().collect(),
                        character_id: None,
                    }
                }
            };
        }
        if !auto_switch_migration_notes.is_empty() && self.migration_notice.is_none() {
            self.migration_notice = Some(auto_switch_migration_notes.join(" "));
        }

        let profile_exists = self
            .rule_profiles
            .iter()
            .any(|profile| profile.id == self.active_profile_id);
        if !profile_exists {
            self.active_profile_id = self
                .rule_profiles
                .first()
                .map(|profile| profile.id.clone())
                .unwrap_or_default();
        }

        let rule_ids: std::collections::HashSet<_> = self
            .rule_definitions
            .iter()
            .map(|rule| rule.id.clone())
            .collect();
        for rule in &mut self.rule_definitions {
            if rule.extension.window_secs == 0 {
                rule.extension.window_secs = 1;
            }
            for scored_event in &mut rule.scored_events {
                scored_event.filters.normalize();
            }
        }
        self.auto_switch_rules.retain(|rule| {
            validate_auto_switch_rule(rule).is_ok()
                && self
                    .rule_profiles
                    .iter()
                    .any(|profile| profile.id == rule.target_profile_id)
        });
        for profile in &mut self.rule_profiles {
            profile
                .enabled_rule_ids
                .retain(|rule_id| rule_ids.contains(rule_id));
        }
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
        self.remind_later_version = self.remind_later_version.take().and_then(|value| {
            let trimmed = value.trim().to_string();
            (!trimmed.is_empty()).then_some(trimmed)
        });
        if self
            .remind_later_until_utc
            .is_some_and(|until| until <= Utc::now())
        {
            self.remind_later_until_utc = None;
            self.remind_later_version = None;
        }
    }
}

impl CaptureConfig {
    pub fn normalize(&mut self) {
        let backend = self.backend.trim().to_ascii_lowercase();
        self.backend = if backend.is_empty() {
            default_capture_backend()
        } else {
            backend
        };
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

fn default_clip_saved_notifications() -> bool {
    true
}

fn default_true() -> bool {
    true
}

fn default_save_delay_secs() -> u32 {
    2
}

fn default_capture_backend() -> String {
    #[cfg(target_os = "windows")]
    {
        "obs".into()
    }

    #[cfg(not(target_os = "windows"))]
    {
        "gsr".into()
    }
}

fn default_post_process_bitrate_kbps() -> u32 {
    192
}

fn default_premix_track_title() -> String {
    "Mixed".into()
}

fn default_limiter_limit() -> f32 {
    0.97
}

fn default_limiter_attack_ms() -> f32 {
    5.0
}

fn default_limiter_release_ms() -> f32 {
    50.0
}

fn default_manual_clip_hotkey() -> String {
    "Ctrl+Shift+F8".into()
}

fn default_manual_clip_duration_secs() -> u32 {
    30
}

fn default_storage_tiering_directory() -> PathBuf {
    RecorderConfig::default().save_directory.join("archive")
}

fn default_storage_tiering_min_age_days() -> u32 {
    7
}

fn default_storage_tiering_max_score() -> u32 {
    50
}

fn default_copyparty_upload_url() -> String {
    String::new()
}

fn default_copyparty_public_base_url() -> String {
    String::new()
}

fn default_discord_min_score() -> u32 {
    100
}

fn default_clip_naming_template() -> String {
    "{timestamp}_{source}_{character}_{rule}_{score}".into()
}

fn normalize_urlish(value: &str, default: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_string_setting(value: &str, default: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_audio_sources(audio_sources: Vec<AudioSourceConfig>) -> Vec<AudioSourceConfig> {
    audio_sources
        .into_iter()
        .filter_map(|mut audio_source| {
            if audio_source.normalize() {
                Some(audio_source)
            } else {
                None
            }
        })
        .collect()
}

impl<'de> Deserialize<'de> for RecorderConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let config = RecorderConfigSerde::deserialize(deserializer)?;
        let mut backends = config.backends.unwrap_or_default();
        if let Some(capture_source) = config.capture_source {
            backends.gsr.capture_source = capture_source;
        }
        if let Some(framerate) = config.framerate {
            backends.gsr.framerate = framerate;
        }
        if let Some(codec) = config.codec {
            backends.gsr.codec = codec;
        }
        if let Some(container) = config.container {
            backends.gsr.container = container;
        }
        if let Some(quality) = config.quality {
            backends.gsr.quality = quality;
        }
        let audio_sources = if config.audio_sources.is_empty() {
            config
                .audio_source
                .into_iter()
                .map(|source| {
                    AudioSourceConfig::new(source.clone(), parse_legacy_audio_source_kind(&source))
                })
                .collect()
        } else {
            config.audio_sources
        };

        Ok(Self {
            replay_buffer_secs: config.replay_buffer_secs,
            save_directory: config.save_directory,
            audio_sources: normalize_audio_sources(audio_sources),
            post_processing: config.post_processing,
            clip_saved_notifications: config.clip_saved_notifications,
            save_delay_secs: config.save_delay_secs,
            backends,
        })
    }
}

impl<'de> Deserialize<'de> for AudioSourceConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let serde = AudioSourceConfigSerde::deserialize(deserializer)?;
        let AudioSourceConfigSerde {
            label,
            source,
            kind,
            gain_db,
            muted_in_premix,
            included_in_premix,
        } = serde;
        let (kind, label) = match (kind, source) {
            (Some(kind), _) => (kind, label),
            (None, Some(source)) => {
                let trimmed = source.trim().to_string();
                let label = if label.trim().is_empty() {
                    trimmed.clone()
                } else {
                    label
                };
                (parse_legacy_audio_source_kind(&trimmed), label)
            }
            (None, None) => (
                AudioSourceKind::Raw {
                    backend_id: default_capture_backend(),
                    value: String::new(),
                },
                label,
            ),
        };

        Ok(Self {
            label,
            kind,
            gain_db: gain_db.unwrap_or(0.0),
            muted_in_premix: muted_in_premix.unwrap_or(false),
            included_in_premix: included_in_premix.unwrap_or(true),
        })
    }
}

#[derive(Debug, Deserialize)]
struct RecorderConfigSerde {
    #[serde(default = "default_replay_buffer_secs")]
    replay_buffer_secs: u32,
    #[serde(default = "default_save_directory")]
    save_directory: PathBuf,
    #[serde(default)]
    audio_sources: Vec<AudioSourceConfig>,
    #[serde(default)]
    audio_source: Option<String>,
    #[serde(default)]
    post_processing: PostProcessingConfig,
    #[serde(default = "default_clip_saved_notifications")]
    clip_saved_notifications: bool,
    #[serde(default = "default_save_delay_secs")]
    save_delay_secs: u32,
    #[serde(default)]
    backends: Option<BackendConfigs>,
    #[serde(default)]
    capture_source: Option<String>,
    #[serde(default)]
    framerate: Option<u32>,
    #[serde(default)]
    codec: Option<String>,
    #[serde(default)]
    container: Option<String>,
    #[serde(default)]
    quality: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AudioSourceConfigSerde {
    #[serde(default)]
    label: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    kind: Option<AudioSourceKind>,
    #[serde(default)]
    gain_db: Option<f32>,
    #[serde(default)]
    muted_in_premix: Option<bool>,
    #[serde(default)]
    included_in_premix: Option<bool>,
}

fn default_replay_buffer_secs() -> u32 {
    RecorderConfig::default().replay_buffer_secs
}

fn default_save_directory() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|d| d.video_dir().map(|v| v.join("nanite-clip")))
        .unwrap_or_else(|| PathBuf::from("~/Videos/nanite-clip"))
}

fn default_obs_websocket_url() -> String {
    "ws://127.0.0.1:4455".into()
}

fn parse_legacy_audio_source_kind(source: &str) -> AudioSourceKind {
    let source = source.trim();
    if source.contains('|') {
        let entries = source
            .split('|')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(parse_legacy_audio_source_kind)
            .collect::<Vec<_>>();
        return AudioSourceKind::Merged { entries };
    }

    if source.eq_ignore_ascii_case("default_output") {
        return AudioSourceKind::DefaultOutput;
    }
    if source.eq_ignore_ascii_case("default_input") {
        return AudioSourceKind::DefaultInput;
    }
    if let Some(name) = source.strip_prefix("device:") {
        return AudioSourceKind::Device {
            name: name.trim().to_string(),
        };
    }
    if let Some(name) = source.strip_prefix("app:") {
        return AudioSourceKind::Application {
            name: name.trim().to_string(),
        };
    }
    if let Some(name) = source.strip_prefix("app-inverse:") {
        return AudioSourceKind::ApplicationInverse {
            names: if name.trim().is_empty() {
                Vec::new()
            } else {
                vec![name.trim().to_string()]
            },
        };
    }

    AudioSourceKind::Raw {
        backend_id: default_capture_backend(),
        value: source.to_string(),
    }
}

pub fn legacy_audio_source_kind_from_value(source: &str) -> AudioSourceKind {
    parse_legacy_audio_source_kind(source)
}

fn archive_legacy_config(path: &Path, contents: &str) {
    let backup_path = path.with_extension("legacy-state-machine.toml.bak");
    if backup_path.exists() {
        return;
    }
    if let Err(error) = std::fs::write(&backup_path, contents) {
        tracing::warn!(
            "Failed to back up legacy config to {}: {error}",
            backup_path.display()
        );
    } else {
        tracing::info!("Backed up legacy config to {}", backup_path.display());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyStateMachineConfig {
    pub service_id: String,
    pub characters: Vec<CharacterConfig>,
    pub rule_definitions: Vec<LegacyStateMachineRuleDefinition>,
    pub rule_profiles: Vec<RuleProfile>,
    pub active_profile_id: String,
    pub recorder: RecorderConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyStateMachineRuleDefinition {
    pub id: String,
    pub name: String,
    pub base_duration: ClipLength,
    pub cooldown_secs: Option<u32>,
    pub activation_class: Option<crate::rules::CharacterClass>,
    pub graph: LegacyStateMachineRuleGraph,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyStateMachineRuleGraph {
    pub start_state_id: String,
    pub states: Vec<LegacyStateMachineRuleState>,
    pub transitions: Vec<LegacyStateMachineRuleTransition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyStateMachineRuleState {
    pub id: String,
    pub name: String,
    pub is_trigger: bool,
    pub position: LegacyGraphPoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyGraphPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyStateMachineRuleTransition {
    pub id: String,
    pub from_state_id: String,
    pub to_state_id: String,
    pub event: EventKind,
    pub within_secs: u32,
    pub extend_by_secs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyPresetConfig {
    pub service_id: String,
    pub characters: Vec<CharacterConfig>,
    pub rules: Vec<LegacyClipRule>,
    pub recorder: RecorderConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyClipRule {
    pub name: String,
    pub enabled: bool,
    pub trigger: LegacyTrigger,
    pub clip_duration: ClipLength,
    pub cooldown_secs: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum LegacyTrigger {
    Count {
        event: EventKind,
        count: usize,
        window_secs: u32,
    },
    Sequence {
        steps: Vec<LegacySequenceStep>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacySequenceStep {
    pub event: EventKind,
    pub max_gap_secs: u32,
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn recorder_config_migrates_single_audio_source() {
        let config: RecorderConfig = toml::from_str(
            r#"
            replay_buffer_secs = 300
            save_directory = "/tmp/clips"
            capture_source = "planetside2"
            framerate = 60
            codec = "h264"
            audio_source = "default_output"
            container = "mkv"
            quality = "40000"
            "#,
        )
        .unwrap();

        assert_eq!(config.audio_sources.len(), 1);
        assert_eq!(config.audio_sources[0].kind, AudioSourceKind::DefaultOutput);
        assert_eq!(config.audio_sources[0].label, "default_output");
        assert_eq!(config.audio_sources[0].gain_db, 0.0);
        assert!(config.audio_sources[0].included_in_premix);
        assert!(!config.audio_sources[0].muted_in_premix);
    }

    #[test]
    fn config_normalize_sets_wave_one_defaults() {
        let mut config = Config::default();
        config.schema_version = 0;
        config.clip_naming_template.clear();
        config.manual_clip.hotkey.clear();
        config.manual_clip.duration_secs = 0;
        config.recorder.audio_sources =
            vec![AudioSourceConfig::new("", AudioSourceKind::DefaultOutput)];

        config.normalize();

        assert_eq!(config.schema_version, 8);
        assert_eq!(
            config.clip_naming_template,
            "{timestamp}_{source}_{character}_{rule}_{score}"
        );
        assert_eq!(config.manual_clip.hotkey, "Ctrl+Shift+F8");
        assert_eq!(config.manual_clip.duration_secs, 5);
        assert_eq!(config.recorder.audio_sources.len(), 1);
        assert_eq!(
            config.recorder.audio_sources[0].kind,
            AudioSourceKind::DefaultOutput
        );
        assert_eq!(config.recorder.audio_sources[0].label, "Default output");
        assert!(!config.launch_at_login.enabled);
        assert_eq!(config.launch_at_login.provider, LaunchAtLoginProvider::Auto);
        assert_eq!(
            config.updates.install_behavior,
            UpdateInstallBehavior::Manual
        );
        assert!(config.updates.prepared_update.is_none());
        assert!(config.updates.remind_later_until_utc.is_none());
    }

    #[test]
    fn uploads_config_migrates_legacy_streamable_block_to_copyparty() {
        let config: Config = toml::from_str(
            r#"
            schema_version = 7
            service_id = "s:example"
            active_profile_id = "default"
            characters = []
            rule_definitions = []
            rule_profiles = []
            auto_switch_rules = []
            auto_start_monitoring = false
            start_minimized = false
            minimize_to_tray = false
            clip_naming_template = "{timestamp}"

            [launch_at_login]
            enabled = false
            provider = "auto"

            [manual_clip]
            enabled = false
            hotkey = "Ctrl+Shift+F8"
            duration_secs = 30

            [storage_tiering]
            enabled = false
            tier_directory = "/tmp/archive"
            min_age_days = 7
            max_score = 50

            [uploads.streamable]
            enabled = true
            api_base_url = "https://clips.example.com/incoming"
            public_base_url = "https://cdn.example.com/incoming"
            username = "alice"

            [uploads.youtube]
            enabled = false
            client_id = ""
            privacy_status = "unlisted"

            [discord_webhook]
            enabled = false
            min_score = 100
            include_thumbnail = false

            [recorder]
            replay_buffer_secs = 300
            save_directory = "/tmp/clips"
            capture_source = "planetside2"
            framerate = 60
            codec = "h264"
            audio_sources = []
            container = "mkv"
            quality = "40000"
            clip_saved_notifications = true
            save_delay_secs = 2
            "#,
        )
        .unwrap();

        assert!(config.uploads.copyparty.enabled);
        assert_eq!(
            config.uploads.copyparty.upload_url,
            "https://clips.example.com/incoming"
        );
        assert_eq!(
            config.uploads.copyparty.public_base_url,
            "https://cdn.example.com/incoming"
        );
        assert_eq!(config.uploads.copyparty.username, "alice");
    }

    #[test]
    fn recorder_config_serializes_audio_sources_list() {
        let config = RecorderConfig {
            audio_sources: vec![
                AudioSourceConfig::new("Game", AudioSourceKind::DefaultOutput),
                AudioSourceConfig::new("Mic", AudioSourceKind::DefaultInput),
            ],
            ..RecorderConfig::default()
        };

        let toml = toml::to_string(&config).unwrap();

        assert!(toml.contains("audio_sources"));
        assert!(!toml.contains("audio_source ="));
        assert!(toml.contains("[backends.gsr]"));
    }

    #[test]
    fn legacy_audio_source_rows_migrate_to_typed_kinds() {
        let config: RecorderConfig = toml::from_str(
            r#"
            replay_buffer_secs = 300
            save_directory = "/tmp/clips"
            capture_source = "planetside2"
            framerate = 60
            codec = "h264"
            container = "mkv"
            quality = "40000"

            [[audio_sources]]
            label = "Game"
            source = "app:PlanetSide2"

            [[audio_sources]]
            label = "Voice"
            source = "app-inverse:Spotify"

            [[audio_sources]]
            label = "Desktop"
            source = "device:alsa_output.pci.monitor|default_input"
            "#,
        )
        .unwrap();

        assert_eq!(
            config.audio_sources[0].kind,
            AudioSourceKind::Application {
                name: "PlanetSide2".into()
            }
        );
        assert_eq!(
            config.audio_sources[1].kind,
            AudioSourceKind::ApplicationInverse {
                names: vec!["Spotify".into()]
            }
        );
        assert_eq!(
            config.audio_sources[2].kind,
            AudioSourceKind::Merged {
                entries: vec![
                    AudioSourceKind::Device {
                        name: "alsa_output.pci.monitor".into()
                    },
                    AudioSourceKind::DefaultInput,
                ]
            }
        );
    }

    #[test]
    fn recorder_config_round_trips_with_typed_audio_sources() {
        let config = RecorderConfig {
            audio_sources: vec![
                AudioSourceConfig {
                    label: "Game".into(),
                    kind: AudioSourceKind::Application {
                        name: "PlanetSide2".into(),
                    },
                    gain_db: 1.5,
                    muted_in_premix: false,
                    included_in_premix: true,
                },
                AudioSourceConfig {
                    label: "Everything else".into(),
                    kind: AudioSourceKind::ApplicationInverse {
                        names: vec!["PlanetSide2".into()],
                    },
                    gain_db: -3.0,
                    muted_in_premix: true,
                    included_in_premix: false,
                },
            ],
            ..RecorderConfig::default()
        };

        let first = toml::to_string_pretty(&config).unwrap();
        let decoded: RecorderConfig = toml::from_str(&first).unwrap();
        let second = toml::to_string_pretty(&decoded).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn config_migrates_pre_inf02_fixture_into_backend_configs() {
        let fixture_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/config_pre_inf02.toml");
        let fixture = std::fs::read_to_string(&fixture_path).unwrap();

        let config: Config = toml::from_str(&fixture).unwrap();

        assert_eq!(config.recorder.backends.gsr.framerate, 60);
        assert_eq!(config.recorder.backends.gsr.capture_source, "planetside2");
        assert_eq!(config.recorder.backends.gsr.codec, "h264");
        assert_eq!(config.recorder.backends.gsr.container, "mkv");
        assert_eq!(config.recorder.backends.gsr.quality, "40000");

        let serialized = toml::to_string_pretty(&config).unwrap();
        assert!(serialized.contains("[recorder.backends.gsr]"));
        assert!(!serialized.contains("[recorder]\ncapture_source"));
        assert!(!serialized.contains("[recorder]\nframerate"));

        let decoded: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(decoded.recorder.backends, config.recorder.backends);
    }

    #[test]
    fn config_migrates_legacy_launch_at_login_bool() {
        let config: Config = toml::from_str(
            r#"
            schema_version = 3
            service_id = "s:example"
            active_profile_id = "default"
            launch_at_login = true

            characters = []
            rule_definitions = []
            rule_profiles = []

            [manual_clip]
            enabled = false
            hotkey = "Ctrl+Shift+F8"
            duration_secs = 30

            [recorder]
            replay_buffer_secs = 300
            save_directory = "/tmp/clips"
            capture_source = "planetside2"
            framerate = 60
            codec = "h264"
            audio_sources = []
            container = "mkv"
            quality = "40000"
            "#,
        )
        .unwrap();

        assert!(config.launch_at_login.enabled);
        assert_eq!(config.launch_at_login.provider, LaunchAtLoginProvider::Auto);
    }

    #[test]
    fn config_deserializes_structured_launch_at_login() {
        let config: Config = toml::from_str(
            r#"
            schema_version = 4
            service_id = "s:example"
            active_profile_id = "default"

            characters = []
            rule_definitions = []
            rule_profiles = []

            [launch_at_login]
            enabled = true
            provider = "systemd_user"

            [manual_clip]
            enabled = false
            hotkey = "Ctrl+Shift+F8"
            duration_secs = 30

            [recorder]
            replay_buffer_secs = 300
            save_directory = "/tmp/clips"
            capture_source = "planetside2"
            framerate = 60
            codec = "h264"
            audio_sources = []
            container = "mkv"
            quality = "40000"
            "#,
        )
        .unwrap();

        assert!(config.launch_at_login.enabled);
        assert_eq!(
            config.launch_at_login.provider,
            LaunchAtLoginProvider::SystemdUser
        );
    }

    #[test]
    fn config_normalize_converts_legacy_local_cron_auto_switch_rules() {
        let mut config: Config = toml::from_str(
            r#"
            schema_version = 7
            service_id = "s:example"
            active_profile_id = "profile_default"
            characters = []
            rule_definitions = []

            [[rule_profiles]]
            id = "profile_default"
            name = "Default"
            enabled_rule_ids = []

            [[auto_switch_rules]]
            id = "schedule_1"
            name = "Weekend nights"
            enabled = true
            target_profile_id = "profile_default"

            [auto_switch_rules.condition]
            type = "local_cron"
            expression = "0 19 * * fri,sat"

            [manual_clip]
            enabled = false
            hotkey = "Ctrl+Shift+F8"
            duration_secs = 30

            [recorder]
            replay_buffer_secs = 300
            save_directory = "/tmp/clips"
            capture_source = "planetside2"
            framerate = 60
            codec = "h264"
            audio_sources = []
            container = "mkv"
            quality = "40000"
            "#,
        )
        .unwrap();

        config.normalize();

        assert_eq!(config.auto_switch_rules.len(), 1);
        assert_eq!(
            config.auto_switch_rules[0].condition,
            crate::rules::AutoSwitchCondition::LocalSchedule {
                weekdays: vec![
                    crate::rules::schedule::ScheduleWeekday::Friday,
                    crate::rules::schedule::ScheduleWeekday::Saturday,
                ],
                start_hour: 19,
                start_minute: 0,
                end_hour: 20,
                end_minute: 0,
            }
        );
    }

    #[test]
    fn config_normalize_converts_legacy_event_auto_switch_rules_to_active_character() {
        let mut config = Config::default();
        config.characters = vec![CharacterConfig {
            name: "Example".into(),
            character_id: Some(42),
            world_id: None,
            faction_id: None,
        }];
        config.rule_profiles = vec![RuleProfile {
            id: "profile_default".into(),
            name: "Default".into(),
            enabled_rule_ids: Vec::new(),
        }];
        config.auto_switch_rules = vec![AutoSwitchRule {
            id: "event_1".into(),
            name: "Legacy event".into(),
            enabled: true,
            target_profile_id: "profile_default".into(),
            condition: crate::rules::AutoSwitchCondition::OnEvent {
                event: EventKind::FacilityCapture,
            },
        }];

        config.normalize();

        assert_eq!(
            config.auto_switch_rules[0].condition,
            crate::rules::AutoSwitchCondition::ActiveCharacter {
                character_ids: vec![42],
                character_id: None,
            }
        );
    }
}
