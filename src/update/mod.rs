pub mod channel;
pub mod download;
pub mod github;
pub mod helper;
pub mod helper_runner;
pub mod helper_shared;
pub mod manifest;
pub mod types;

use semver::Version;
use sha2::{Digest, Sha256};

use crate::config::UpdateChannel;

pub use channel::{detect_install_channel, detect_system_update_plan};
pub use types::{
    AvailableRelease, DownloadStep, InstallChannel, PreparedUpdate, SystemUpdatePlan,
    UpdateApplyReport, UpdateApplyReportStatus, UpdateAvailability, UpdateErrorKind,
    UpdateErrorState, UpdateInstallBehavior, UpdatePhase, UpdatePrimaryAction, UpdateProgressState,
    UpdateReleasePolicy, UpdateState,
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
    install_id: Option<&str>,
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
    let policy = evaluate_release_policy(&manifest, current_version, install_id)?;
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
        signature: manifest.signature.clone(),
        policy,
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
            signature: manifest.signature.clone(),
            policy: UpdateReleasePolicy::default(),
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
            signature: manifest.signature.clone(),
            policy: UpdateReleasePolicy::default(),
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

fn evaluate_release_policy(
    manifest: &manifest::UpdateManifest,
    current_version: &Version,
    install_id: Option<&str>,
) -> Result<UpdateReleasePolicy, String> {
    let minimum_version = manifest
        .minimum_supported_version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let minimum_supported = minimum_version
        .as_deref()
        .map(Version::parse)
        .transpose()
        .map_err(|error| {
            format!("update manifest minimum supported version was invalid: {error}")
        })?;

    let mut blocked_current_version = false;
    for blocked_version in &manifest.blocked_versions {
        let blocked_version = Version::parse(blocked_version.trim()).map_err(|error| {
            format!(
                "update manifest blocked version `{}` was invalid: {error}",
                blocked_version
            )
        })?;
        if &blocked_version == current_version {
            blocked_current_version = true;
        }
    }

    let rollout_percentage = manifest.rollout.as_ref().map(|rollout| rollout.percentage);
    let rollout_eligible = rollout_percentage.is_none_or(|percentage| {
        percentage >= 100
            || install_id
                .map(|value| rollout_bucket(value, &manifest.tag_name) < u32::from(percentage))
                .unwrap_or(false)
    });
    let availability = if minimum_supported
        .as_ref()
        .is_some_and(|minimum_supported| current_version < minimum_supported)
    {
        UpdateAvailability::RequiresManualUpgrade
    } else if rollout_percentage.is_some() && !rollout_eligible {
        UpdateAvailability::DeferredByRollout
    } else {
        UpdateAvailability::Available
    };

    Ok(UpdateReleasePolicy {
        availability,
        minimum_version,
        blocked_current_version,
        mandatory: manifest.mandatory,
        rollout_percentage,
        rollout_eligible,
        message: manifest
            .message
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
    })
}

fn rollout_bucket(install_id: &str, tag_name: &str) -> u32 {
    let digest = Sha256::digest(format!("{install_id}:{tag_name}").as_bytes());
    u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) % 100
}

#[cfg(test)]
mod tests {
    use super::{
        UpdateAvailability, evaluate_release_policy, parse_update_public_keys, rollout_bucket,
    };
    use semver::Version;

    use crate::update::manifest::{UpdateManifest, UpdateManifestRollout};

    fn sample_manifest() -> UpdateManifest {
        UpdateManifest {
            version: "9.9.9".into(),
            tag_name: "v9.9.9".into(),
            release_notes_url: "https://example.invalid/releases/v9.9.9".into(),
            published_at: None,
            minimum_supported_version: None,
            blocked_versions: Vec::new(),
            rollout: None,
            mandatory: false,
            message: None,
            signature: Default::default(),
            assets: Vec::new(),
        }
    }

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

    #[test]
    fn evaluate_release_policy_marks_manual_upgrade_when_minimum_version_is_newer() {
        let mut manifest = sample_manifest();
        manifest.minimum_supported_version = Some("1.5.0".into());

        let policy = evaluate_release_policy(
            &manifest,
            &Version::parse("1.4.0").unwrap(),
            Some("install-a"),
        )
        .unwrap();

        assert_eq!(
            policy.availability,
            UpdateAvailability::RequiresManualUpgrade
        );
        assert_eq!(policy.minimum_version.as_deref(), Some("1.5.0"));
    }

    #[test]
    fn evaluate_release_policy_marks_blocked_current_version_and_preserves_message() {
        let mut manifest = sample_manifest();
        manifest.blocked_versions = vec!["1.4.0".into()];
        manifest.mandatory = true;
        manifest.message = Some("This build is known-bad.".into());

        let policy = evaluate_release_policy(
            &manifest,
            &Version::parse("1.4.0").unwrap(),
            Some("install-a"),
        )
        .unwrap();

        assert!(policy.blocked_current_version);
        assert!(policy.mandatory);
        assert_eq!(policy.message.as_deref(), Some("This build is known-bad."));
    }

    #[test]
    fn rollout_bucket_is_deterministic_for_install_and_release() {
        let first = rollout_bucket("install-a", "v9.9.9");
        let second = rollout_bucket("install-a", "v9.9.9");

        assert_eq!(first, second);
    }

    #[test]
    fn evaluate_release_policy_defers_release_when_rollout_excludes_install() {
        let mut manifest = sample_manifest();
        manifest.rollout = Some(UpdateManifestRollout { percentage: 25 });
        let install_id = "install-a";
        let bucket = rollout_bucket(install_id, &manifest.tag_name);
        let policy = evaluate_release_policy(
            &manifest,
            &Version::parse("1.4.0").unwrap(),
            Some(install_id),
        )
        .unwrap();

        if bucket < 25 {
            assert_eq!(policy.availability, UpdateAvailability::Available);
            assert!(policy.rollout_eligible);
        } else {
            assert_eq!(policy.availability, UpdateAvailability::DeferredByRollout);
            assert!(!policy.rollout_eligible);
        }
    }
}
