use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

use super::github::{GithubAsset, GithubRelease, client};
use super::types::{InstallChannel, ManifestAsset};
use super::{MANIFEST_ASSET_NAME, MANIFEST_SIGNATURE_ASSET_NAME, update_public_keys};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    pub tag_name: String,
    pub release_notes_url: String,
    #[serde(default)]
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    #[serde(alias = "minimum_version")]
    pub minimum_supported_version: Option<String>,
    #[serde(default)]
    pub blocked_versions: Vec<String>,
    #[serde(default)]
    pub rollout: Option<UpdateManifestRollout>,
    #[serde(default)]
    pub mandatory: bool,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub signature: super::types::UpdateSignatureInfo,
    pub assets: Vec<ManifestAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifestRollout {
    pub percentage: u8,
}

impl UpdateManifest {
    pub fn asset_for_channel(&self, channel: InstallChannel) -> Option<ManifestAsset> {
        self.assets
            .iter()
            .find(|asset| asset.channel == channel)
            .cloned()
    }
}

pub async fn fetch_verified_manifest(release: &GithubRelease) -> Result<UpdateManifest, String> {
    let manifest_asset = find_asset(&release.assets, MANIFEST_ASSET_NAME)?;
    let signature_asset = find_asset(&release.assets, MANIFEST_SIGNATURE_ASSET_NAME)?;
    let client = client()?;

    let manifest_bytes = client
        .get(&manifest_asset.browser_download_url)
        .send()
        .await
        .map_err(|error| format!("failed to download the update manifest: {error}"))?
        .error_for_status()
        .map_err(|error| format!("manifest download failed: {error}"))?
        .bytes()
        .await
        .map_err(|error| format!("failed to read manifest bytes: {error}"))?;
    let signature_text = client
        .get(&signature_asset.browser_download_url)
        .send()
        .await
        .map_err(|error| format!("failed to download the manifest signature: {error}"))?
        .error_for_status()
        .map_err(|error| format!("manifest signature download failed: {error}"))?
        .text()
        .await
        .map_err(|error| format!("failed to read manifest signature: {error}"))?;

    verify_manifest_signature(&manifest_bytes, signature_text.trim())?;

    let manifest = serde_json::from_slice::<UpdateManifest>(&manifest_bytes)
        .map_err(|error| format!("failed to parse the update manifest: {error}"))?;

    if manifest.tag_name != release.tag_name {
        return Err(format!(
            "update manifest tag `{}` did not match GitHub release tag `{}`",
            manifest.tag_name, release.tag_name
        ));
    }

    if manifest.release_notes_url.trim().is_empty() {
        return Err("update manifest was missing the release notes URL".into());
    }

    if let Some(minimum_supported_version) = manifest
        .minimum_supported_version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        semver::Version::parse(minimum_supported_version).map_err(|error| {
            format!("update manifest minimum supported version was invalid: {error}")
        })?;
    }

    for version in &manifest.blocked_versions {
        semver::Version::parse(version.trim()).map_err(|error| {
            format!(
                "update manifest blocked version `{}` was invalid: {error}",
                version
            )
        })?;
    }

    if let Some(rollout) = &manifest.rollout
        && rollout.percentage > 100
    {
        return Err("update manifest rollout percentage must be between 0 and 100".into());
    }

    Ok(manifest)
}

fn find_asset<'a>(assets: &'a [GithubAsset], name: &str) -> Result<&'a GithubAsset, String> {
    assets
        .iter()
        .find(|asset| asset.name == name)
        .ok_or_else(|| format!("GitHub release is missing required asset `{name}`"))
}

fn verify_manifest_signature(manifest_bytes: &[u8], signature_text: &str) -> Result<(), String> {
    let public_keys = update_public_keys();
    if public_keys.is_empty() {
        return Err("Updater public keys are not configured in this build.".into());
    }

    let signature_bytes = base64::engine::general_purpose::STANDARD
        .decode(signature_text)
        .map_err(|error| format!("failed to decode manifest signature: {error}"))?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|error| format!("failed to parse manifest signature: {error}"))?;

    let mut errors = Vec::new();

    for public_key in public_keys {
        let public_key_bytes = match base64::engine::general_purpose::STANDARD.decode(&public_key) {
            Ok(bytes) => bytes,
            Err(error) => {
                errors.push(format!("failed to decode updater public key: {error}"));
                continue;
            }
        };
        let public_key_bytes: [u8; 32] = match public_key_bytes.try_into() {
            Ok(bytes) => bytes,
            Err(_) => {
                errors.push("updater public key must decode to 32 bytes".into());
                continue;
            }
        };
        let verifying_key = match VerifyingKey::from_bytes(&public_key_bytes) {
            Ok(key) => key,
            Err(error) => {
                errors.push(format!("failed to parse updater public key: {error}"));
                continue;
            }
        };

        if verifying_key
            .verify_strict(manifest_bytes, &signature)
            .is_ok()
        {
            return Ok(());
        }
    }

    Err(format!(
        "manifest signature verification failed for all configured updater keys{}",
        if errors.is_empty() {
            String::new()
        } else {
            format!(" ({})", errors.join("; "))
        }
    ))
}
