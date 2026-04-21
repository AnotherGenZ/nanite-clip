use super::*;

pub(crate) fn default_clip_saved_notifications() -> bool {
    true
}

pub(crate) fn default_true() -> bool {
    true
}

pub(crate) fn default_save_delay_secs() -> u32 {
    2
}

pub(crate) fn default_capture_backend() -> CaptureBackend {
    #[cfg(target_os = "windows")]
    {
        CaptureBackend::Obs
    }

    #[cfg(not(target_os = "windows"))]
    {
        CaptureBackend::Gsr
    }
}

pub(crate) fn deserialize_capture_backend<'de, D>(
    deserializer: D,
) -> Result<CaptureBackend, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?
        .as_deref()
        .map(CaptureBackend::from_config_value)
        .unwrap_or_else(default_capture_backend))
}

pub(crate) fn default_post_process_bitrate_kbps() -> u32 {
    192
}

pub(crate) fn default_premix_track_title() -> String {
    "Mixed".into()
}

pub(crate) fn default_limiter_limit() -> f32 {
    0.97
}

pub(crate) fn default_limiter_attack_ms() -> f32 {
    5.0
}

pub(crate) fn default_limiter_release_ms() -> f32 {
    50.0
}

pub(crate) fn default_manual_clip_hotkey() -> String {
    "Ctrl+Shift+F8".into()
}

pub(crate) fn default_manual_clip_duration_secs() -> u32 {
    30
}

pub(crate) fn default_storage_tiering_directory() -> PathBuf {
    RecorderConfig::default().save_directory.join("archive")
}

pub(crate) fn default_storage_tiering_min_age_days() -> u32 {
    7
}

pub(crate) fn default_storage_tiering_max_score() -> u32 {
    50
}

pub(crate) fn default_copyparty_upload_url() -> String {
    String::new()
}

pub(crate) fn default_copyparty_public_base_url() -> String {
    String::new()
}

pub(crate) fn default_discord_min_score() -> u32 {
    100
}

pub(crate) fn default_clip_naming_template() -> String {
    "{timestamp}_{source}_{character}_{rule}_{score}".into()
}

pub(crate) fn normalize_urlish(value: &str, default: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn normalize_string_setting(value: &str, default: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn normalize_audio_sources(
    audio_sources: Vec<AudioSourceConfig>,
) -> Vec<AudioSourceConfig> {
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
            auto_generate_thumbnails: config.auto_generate_thumbnails,
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
                    backend_id: default_capture_backend().as_str().to_string(),
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
    #[serde(default = "default_true")]
    auto_generate_thumbnails: bool,
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

pub(crate) fn default_replay_buffer_secs() -> u32 {
    RecorderConfig::default().replay_buffer_secs
}

pub(crate) fn default_save_directory() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|d| d.video_dir().map(|v| v.join("nanite-clip")))
        .unwrap_or_else(|| PathBuf::from("~/Videos/nanite-clip"))
}

pub(crate) fn default_obs_websocket_url() -> String {
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
        backend_id: default_capture_backend().as_str().to_string(),
        value: source.to_string(),
    }
}

pub fn legacy_audio_source_kind_from_value(source: &str) -> AudioSourceKind {
    parse_legacy_audio_source_kind(source)
}

pub(crate) fn archive_legacy_config(path: &Path, contents: &str) {
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
pub(crate) struct LegacyStateMachineConfig {
    pub service_id: String,
    pub characters: Vec<CharacterConfig>,
    rule_definitions: Vec<LegacyStateMachineRuleDefinition>,
    rule_profiles: Vec<RuleProfile>,
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
pub(crate) struct LegacyPresetConfig {
    pub service_id: String,
    pub characters: Vec<CharacterConfig>,
    rules: Vec<LegacyClipRule>,
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

        assert_eq!(config.schema_version, 9);
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
        assert!(config.updates.current_version.is_none());
        assert!(config.updates.installed_version_history.is_empty());
    }

    #[test]
    fn update_config_normalize_trims_and_dedupes_version_history() {
        let mut updates = AppUpdateConfig::default();
        updates.install_id = Some(" install-a ".into());
        updates.current_version = Some(" 1.4.0 ".into());
        updates.installed_version_history = vec![
            " 1.4.0 ".into(),
            "1.3.0".into(),
            " 1.2.0 ".into(),
            "1.3.0".into(),
            "".into(),
            "1.1.0".into(),
            "1.0.0".into(),
            "0.9.0".into(),
            "0.8.0".into(),
            "0.7.0".into(),
            "0.6.0".into(),
            "0.5.0".into(),
            "0.4.0".into(),
        ];

        updates.normalize();

        assert_eq!(updates.install_id.as_deref(), Some("install-a"));
        assert_eq!(updates.current_version.as_deref(), Some("1.4.0"));
        assert_eq!(
            updates.installed_version_history,
            vec![
                "1.3.0", "1.2.0", "1.1.0", "1.0.0", "0.9.0", "0.8.0", "0.7.0", "0.6.0", "0.5.0",
                "0.4.0",
            ]
        );
    }

    #[test]
    fn update_config_generates_install_id_once() {
        let mut updates = AppUpdateConfig::default();

        assert!(updates.ensure_install_id());
        let install_id = updates.install_id.clone();
        assert!(install_id.is_some());
        assert!(!updates.ensure_install_id());
        assert_eq!(updates.install_id, install_id);
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

    #[test]
    fn capture_backend_deserialization_uses_typed_values() {
        let config: Config = toml::from_str(
            r#"
            schema_version = 9
            service_id = "s:example"
            active_profile_id = "profile_default"
            characters = []
            rule_definitions = []

            [[rule_profiles]]
            id = "profile_default"
            name = "Default"
            enabled_rule_ids = []

            [capture]
            backend = "obs"

            [recorder]
            replay_buffer_secs = 300
            save_directory = "/tmp/clips"
            audio_sources = []
            "#,
        )
        .unwrap();

        assert_eq!(config.capture.backend, CaptureBackend::Obs);
    }

    #[test]
    fn unknown_capture_backend_falls_back_to_platform_default() {
        let backend = CaptureBackend::from_config_value("mystery");
        #[cfg(target_os = "windows")]
        assert_eq!(backend, CaptureBackend::Obs);
        #[cfg(not(target_os = "windows"))]
        assert_eq!(backend, CaptureBackend::Gsr);
    }

    #[test]
    fn invalid_config_load_surfaces_recoverable_notice() {
        let path = std::env::temp_dir().join(format!(
            "nanite-clip-config-invalid-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "this is not toml = [").unwrap();
        let config = io::load_from_path(&path);
        let _ = std::fs::remove_file(&path);

        assert!(config.migration_notice.is_some());
    }
}
