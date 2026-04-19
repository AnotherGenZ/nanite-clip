mod providers;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

pub(crate) use providers::*;
use reqwest::Url;
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, LOCATION};
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::background_jobs::BackgroundJobContext;
use crate::command_runner;
use crate::config::YouTubePrivacyStatus;
use crate::db::{ClipAudioTrackRecord, UploadProvider};

pub use providers::begin_youtube_oauth;

#[derive(Debug, Clone)]
pub struct UploadRequest {
    pub clip_id: i64,
    pub clip_path: PathBuf,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct UploadCompletion {
    #[allow(dead_code)]
    pub provider: UploadProvider,
    pub provider_label: String,
    pub external_id: Option<String>,
    pub clip_url: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CopypartyUploadCredentials {
    pub upload_url: String,
    pub public_base_url: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub struct YouTubeUploadCredentials {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub refresh_token: String,
    pub privacy_status: YouTubePrivacyStatus,
}

#[derive(Debug, Clone)]
pub struct YouTubeOAuthClient {
    pub client_id: String,
    pub client_secret: Option<String>,
}

#[derive(Debug, Clone)]
pub struct YouTubeOAuthTokens {
    pub refresh_token: String,
}

const YOUTUBE_PROCESSING_POLL_INTERVAL: Duration = Duration::from_secs(10);
const YOUTUBE_PROCESSING_MAX_POLLS: u32 = 90;
const YOUTUBE_UPLOAD_SCOPE: &str = "https://www.googleapis.com/auth/youtube.upload";
const YOUTUBE_READONLY_SCOPE: &str = "https://www.googleapis.com/auth/youtube.readonly";
const YOUTUBE_OAUTH_SCOPE_VALUE: &str = "https://www.googleapis.com/auth/youtube.upload https://www.googleapis.com/auth/youtube.readonly";

pub async fn upload_to_copyparty(
    ctx: BackgroundJobContext,
    request: UploadRequest,
    credentials: CopypartyUploadCredentials,
) -> Result<UploadCompletion, String> {
    info!(
        clip_id = request.clip_id,
        provider = %UploadProvider::Copyparty.label(),
        path = %request.clip_path.display(),
        "Starting Copyparty upload"
    );
    ctx.progress(1, 3, "Preparing Copyparty upload.")?;

    if credentials.upload_url.trim().is_empty() {
        return Err("Configure a Copyparty upload URL in Settings before uploading.".into());
    }

    let bytes = tokio::fs::read(&request.clip_path)
        .await
        .map_err(|error| format!("failed to read {}: {error}", request.clip_path.display()))?;
    let file_name = request
        .clip_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("clip.mp4")
        .to_string();
    let part = Part::bytes(bytes).file_name(file_name);
    let form = Form::new().part("f", part);
    let client = reqwest::Client::new();
    let upload_url = build_copyparty_upload_url(&credentials.upload_url)?;

    ctx.progress(2, 3, "Sending Copyparty upload request.")?;

    let mut builder = client.post(upload_url.clone()).multipart(form);
    builder = if credentials.username.trim().is_empty() {
        builder.header("PW", credentials.password.trim())
    } else {
        builder.basic_auth(
            credentials.username.trim(),
            Some(credentials.password.trim()),
        )
    };

    let response = builder
        .send()
        .await
        .map_err(|error| format!("Copyparty upload request failed: {error}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "unable to read body".into());
        return Err(format!("Copyparty upload failed with {status}: {body}"));
    }

    let body = response
        .text()
        .await
        .map_err(|error| format!("failed to read Copyparty response body: {error}"))?;
    let clip_url = resolve_copyparty_clip_url(&body, &credentials, &request)?;
    let external_id = clip_url.as_deref().and_then(copyparty_external_id_from_url);

    ctx.progress(3, 3, "Copyparty upload completed.")?;
    info!(
        clip_id = request.clip_id,
        provider = %UploadProvider::Copyparty.label(),
        upload_url = %upload_url,
        clip_url = ?clip_url,
        "Copyparty upload completed"
    );
    Ok(UploadCompletion {
        provider: UploadProvider::Copyparty,
        provider_label: UploadProvider::Copyparty.label().into(),
        external_id,
        clip_url,
        note: None,
    })
}

pub async fn upload_to_youtube(
    ctx: BackgroundJobContext,
    request: UploadRequest,
    audio_tracks: &[ClipAudioTrackRecord],
    credentials: YouTubeUploadCredentials,
) -> Result<UploadCompletion, String> {
    info!(
        clip_id = request.clip_id,
        provider = %UploadProvider::YouTube.label(),
        path = %request.clip_path.display(),
        "Starting YouTube upload"
    );
    ctx.progress(1, 5, "Refreshing YouTube access token.")?;
    let access_token = refresh_youtube_access_token(&credentials).await?;
    ctx.progress(2, 5, "Creating YouTube upload session.")?;

    let audio_tracks = audio_tracks.to_vec();
    let request_for_prepare = request.clone();
    let (upload_path, _temp_upload, note) = tokio::task::spawn_blocking(move || {
        prepare_youtube_upload_input(&request_for_prepare, &audio_tracks)
    })
    .await
    .map_err(|error| format!("failed to join YouTube upload preparation worker: {error}"))??;

    let bytes = tokio::fs::read(&upload_path)
        .await
        .map_err(|error| format!("failed to read {}: {error}", upload_path.display()))?;
    let metadata = serde_json::to_vec(&YouTubeUploadMetadata {
        snippet: YouTubeUploadSnippet {
            title: request.title.clone(),
            description: request.description.clone(),
        },
        status: YouTubeUploadStatus {
            privacy_status: credentials.privacy_status.as_api_value().into(),
        },
    })
    .map_err(|error| format!("failed to encode YouTube metadata: {error}"))?;

    let session_url = start_youtube_resumable_upload(&access_token, metadata, bytes.len()).await?;
    ctx.progress(3, 5, "Uploading clip to YouTube.")?;

    let client = reqwest::Client::new();
    let response = client
        .put(session_url)
        .bearer_auth(&access_token)
        .header(CONTENT_LENGTH, bytes.len())
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(bytes)
        .send()
        .await
        .map_err(|error| format!("YouTube upload failed: {error}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "unable to read body".into());
        return Err(format!("YouTube upload failed with {status}: {body}"));
    }

    let payload: YouTubeVideoInsertResponse = response
        .json()
        .await
        .map_err(|error| format!("failed to decode YouTube upload response: {error}"))?;
    let video_id = payload
        .id
        .ok_or_else(|| "YouTube did not return a video id.".to_string())?;
    ctx.progress(4, 5, "Waiting for YouTube video processing to finish.")?;
    wait_for_youtube_processing(&ctx, &access_token, &video_id).await?;
    let clip_url = Some(format!("https://www.youtube.com/watch?v={video_id}"));

    ctx.progress(5, 5, "YouTube upload completed.")?;
    info!(
        clip_id = request.clip_id,
        provider = %UploadProvider::YouTube.label(),
        video_id = %video_id,
        clip_url = ?clip_url,
        "YouTube upload completed"
    );
    Ok(UploadCompletion {
        provider: UploadProvider::YouTube,
        provider_label: UploadProvider::YouTube.label().into(),
        external_id: Some(video_id),
        clip_url,
        note,
    })
}

#[derive(Debug)]
pub(crate) struct TempUploadFile {
    path: PathBuf,
}

impl Drop for TempUploadFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
