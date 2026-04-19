use std::path::Path;

use super::Config;

pub(super) fn load_from_path(path: &Path) -> Config {
    match std::fs::read_to_string(path) {
        Ok(contents) => match toml::from_str::<Config>(&contents) {
            Ok(mut config) => {
                config.normalize();
                tracing::info!("Loaded config from {}", path.display());
                config
            }
            Err(error) => {
                if let Some(config) = Config::load_legacy(path, &contents) {
                    return config;
                }

                tracing::warn!("Failed to parse config: {error}, using defaults");
                Config {
                    migration_notice: Some(format!(
                        "Failed to parse {} and loaded defaults instead: {error}",
                        path.display()
                    )),
                    ..Config::default()
                }
            }
        },
        Err(_) => {
            tracing::info!("No config found at {}, using defaults", path.display());
            Config::default()
        }
    }
}

pub(super) fn save_to_path(path: &Path, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let contents = toml::to_string_pretty(config)?;
    let tmp_path = path.with_extension("toml.tmp");
    std::fs::write(&tmp_path, contents)?;
    std::fs::rename(&tmp_path, path)?;
    tracing::info!("Saved config to {}", path.display());
    Ok(())
}
