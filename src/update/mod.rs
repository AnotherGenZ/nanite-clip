pub mod channel;
pub mod download;
pub mod github;
pub mod helper;
pub mod helper_runner;
pub mod helper_shared;
pub mod manifest;
pub mod types;

use semver::Version;

use crate::config::UpdateChannel;

pub use channel::detect_install_channel;
pub use types::{
    AvailableRelease, DownloadStep, InstallChannel, PreparedUpdate, UpdateApplyReport,
    UpdateApplyReportStatus, UpdateErrorKind, UpdateErrorState, UpdateInstallBehavior, UpdatePhase,
    UpdatePrimaryAction, UpdateProgressState, UpdateState,
};

pub const GITHUB_REPO: &str = "AnotherGenZ/nanite-clip";
pub const MANIFEST_ASSET_NAME: &str = "nanite-clip-update-manifest.json";
pub const MANIFEST_SIGNATURE_ASSET_NAME: &str = "nanite-clip-update-manifest.sig";
const ROLLBACK_RELEASE_LIMIT: usize = 100;

pub fn current_version() -> semver::Version {
    semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .expect("package version should be valid semver")
}

pub fn current_version_label() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn update_public_keys() -> Vec<String> {
    parse_update_public_keys(
        option_env!("NANITE_CLIP_UPDATE_PUBLIC_KEYS"),
        option_env!("NANITE_CLIP_UPDATE_PUBLIC_KEY"),
    )
}

fn parse_update_public_keys(rotated: Option<&str>, fallback: Option<&str>) -> Vec<String> {
    let mut keys = Vec::new();

    for key in rotated
        .into_iter()
        .flat_map(|value| value.split([',', ';', '\n']))
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if !keys.iter().any(|existing| existing == key) {
            keys.push(key.to_string());
        }
    }

    if let Some(key) = fallback.map(str::trim).filter(|value| !value.is_empty())
        && !keys.iter().any(|existing| existing == key)
    {
        keys.push(key.to_string());
    }

    keys
}

pub async fn fetch_available_release(
    channel: UpdateChannel,
    install_channel: InstallChannel,
    current_version: &Version,
    skipped_version: Option<&str>,
) -> Result<Option<AvailableRelease>, String> {
    let release = github::fetch_release(channel).await?;
    let manifest = manifest::fetch_verified_manifest(&release).await?;
    let latest_version = Version::parse(&manifest.version)
        .map_err(|error| format!("update manifest version was invalid: {error}"))?;
    if latest_version <= *current_version {
        return Ok(None);
    }

    let skipped = skipped_version
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some_and(|value| value == manifest.version);
    let asset = manifest.asset_for_channel(install_channel);

    Ok(Some(AvailableRelease {
        version: latest_version,
        tag_name: manifest.tag_name,
        release_name: release
            .name
            .unwrap_or_else(|| format!("NaniteClip {}", manifest.version)),
        html_url: release.html_url,
        changelog_markdown: release.body.unwrap_or_default(),
        published_at: manifest.published_at.or(release.published_at),
        minimum_version: manifest.minimum_version.clone(),
        signature: manifest.signature.clone(),
        asset,
        install_channel,
        skipped,
    }))
}

pub async fn fetch_release_by_version(
    channel: UpdateChannel,
    install_channel: InstallChannel,
    version: &Version,
) -> Result<Option<AvailableRelease>, String> {
    let releases = github::fetch_releases(channel, ROLLBACK_RELEASE_LIMIT).await?;
    for release in releases {
        let manifest = manifest::fetch_verified_manifest(&release).await?;
        let release_version = Version::parse(&manifest.version)
            .map_err(|error| format!("update manifest version was invalid: {error}"))?;
        if release_version != *version {
            continue;
        }
        let asset = manifest.asset_for_channel(install_channel);
        let tag_name = manifest.tag_name.clone();
        let available = AvailableRelease {
            version: release_version,
            tag_name,
            release_name: release
                .name
                .unwrap_or_else(|| format!("NaniteClip {}", manifest.version)),
            html_url: release.html_url,
            changelog_markdown: release.body.unwrap_or_default(),
            published_at: manifest.published_at.or(release.published_at),
            minimum_version: manifest.minimum_version.clone(),
            signature: manifest.signature.clone(),
            asset,
            install_channel,
            skipped: false,
        };
        return Ok(Some(available));
    }
    Ok(None)
}

pub async fn fetch_rollback_candidates(
    channel: UpdateChannel,
    install_channel: InstallChannel,
    current_version: &Version,
) -> Result<Vec<AvailableRelease>, String> {
    let releases = github::fetch_releases(channel, ROLLBACK_RELEASE_LIMIT).await?;
    let mut candidates = Vec::new();
    for release in releases {
        let manifest = manifest::fetch_verified_manifest(&release).await?;
        let release_version = Version::parse(&manifest.version)
            .map_err(|error| format!("update manifest version was invalid: {error}"))?;
        if release_version >= *current_version {
            continue;
        }
        let asset = manifest.asset_for_channel(install_channel);
        let tag_name = manifest.tag_name.clone();
        let candidate = AvailableRelease {
            version: release_version,
            tag_name,
            release_name: release
                .name
                .unwrap_or_else(|| format!("NaniteClip {}", manifest.version)),
            html_url: release.html_url,
            changelog_markdown: release.body.unwrap_or_default(),
            published_at: manifest.published_at.or(release.published_at),
            minimum_version: manifest.minimum_version.clone(),
            signature: manifest.signature.clone(),
            asset,
            install_channel,
            skipped: false,
        };
        if candidate.supports_download() {
            candidates.push(candidate);
        }
    }
    candidates.sort_by(|left, right| right.version.cmp(&left.version));
    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::parse_update_public_keys;

    #[test]
    fn parse_update_public_keys_supports_rotation_and_fallback() {
        let keys = parse_update_public_keys(Some("key-a,\nkey-b; key-c"), Some("key-b"));

        assert_eq!(keys, vec!["key-a", "key-b", "key-c"]);
    }

    #[test]
    fn parse_update_public_keys_uses_fallback_when_rotation_is_empty() {
        let keys = parse_update_public_keys(Some(" "), Some("key-a"));

        assert_eq!(keys, vec!["key-a"]);
    }
}
