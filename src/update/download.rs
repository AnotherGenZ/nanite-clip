use std::path::PathBuf;

use directories::ProjectDirs;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};

use super::github::client;
use super::types::{AvailableRelease, DownloadProgress, PreparedUpdate};

pub fn staging_root() -> PathBuf {
    ProjectDirs::from("", "", "nanite-clip")
        .map(|dirs| dirs.cache_dir().join("updates"))
        .unwrap_or_else(|| std::env::temp_dir().join("nanite-clip-updates"))
}

pub async fn download_release_asset<F>(
    release: &AvailableRelease,
    mut on_progress: F,
) -> Result<PreparedUpdate, String>
where
    F: FnMut(DownloadProgress) -> Result<(), String>,
{
    let asset = release.asset.clone().ok_or_else(|| {
        "No downloadable asset is available for this install channel.".to_string()
    })?;

    tokio::fs::create_dir_all(staging_root().join(&release.tag_name))
        .await
        .map_err(|error| format!("failed to prepare the update staging directory: {error}"))?;

    let final_path = staging_root().join(&release.tag_name).join(&asset.filename);
    if file_matches_checksum(&final_path, &asset.sha256).await? {
        on_progress(DownloadProgress {
            downloaded_bytes: final_path
                .metadata()
                .map(|metadata| metadata.len())
                .unwrap_or_default(),
            total_bytes: asset.size,
        })?;
        return Ok(PreparedUpdate {
            version: release.version.to_string(),
            tag_name: release.tag_name.clone(),
            install_channel: release.install_channel,
            asset_kind: asset.kind,
            asset_name: asset.filename,
            asset_path: final_path,
            release_notes_url: release.html_url.clone(),
        });
    }

    let temp_path = final_path.with_extension("part");
    let response = client()?
        .get(&asset.download_url)
        .send()
        .await
        .map_err(|error| format!("failed to start downloading the update asset: {error}"))?
        .error_for_status()
        .map_err(|error| format!("update asset download failed: {error}"))?;

    let total_bytes = response.content_length().or(asset.size);
    let mut downloaded_bytes = 0_u64;
    let mut hasher = Sha256::new();
    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(|error| format!("failed to create the staged update file: {error}"))?;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|error| format!("failed while downloading the update: {error}"))?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
            .await
            .map_err(|error| format!("failed to write the staged update file: {error}"))?;
        hasher.update(&chunk);
        downloaded_bytes += chunk.len() as u64;
        on_progress(DownloadProgress {
            downloaded_bytes,
            total_bytes,
        })?;
    }

    tokio::io::AsyncWriteExt::flush(&mut file)
        .await
        .map_err(|error| format!("failed to finalize the staged update file: {error}"))?;
    drop(file);

    let checksum = format!("{:x}", hasher.finalize());
    if !checksum.eq_ignore_ascii_case(asset.sha256.as_str()) {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(format!(
            "downloaded update checksum mismatch: expected {}, got {}",
            asset.sha256, checksum
        ));
    }

    if tokio::fs::try_exists(&final_path).await.unwrap_or(false) {
        let _ = tokio::fs::remove_file(&final_path).await;
    }
    tokio::fs::rename(&temp_path, &final_path)
        .await
        .map_err(|error| format!("failed to move the staged update into place: {error}"))?;

    Ok(PreparedUpdate {
        version: release.version.to_string(),
        tag_name: release.tag_name.clone(),
        install_channel: release.install_channel,
        asset_kind: asset.kind,
        asset_name: asset.filename,
        asset_path: final_path,
        release_notes_url: release.html_url.clone(),
    })
}

async fn file_matches_checksum(path: &PathBuf, expected: &str) -> Result<bool, String> {
    if !tokio::fs::try_exists(path).await.unwrap_or(false) {
        return Ok(false);
    }

    let bytes = tokio::fs::read(path)
        .await
        .map_err(|error| format!("failed to read staged update {}: {error}", path.display()))?;
    let checksum = format!("{:x}", Sha256::digest(bytes));
    Ok(checksum.eq_ignore_ascii_case(expected))
}
