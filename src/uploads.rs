use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use reqwest::Url;
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, LOCATION};
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::background_jobs::BackgroundJobContext;
use crate::config::YouTubePrivacyStatus;
use crate::db::{ClipAudioTrackRecord, UploadProvider};
use crate::launcher;

#[derive(Debug, Clone)]
pub struct UploadRequest {
    pub clip_id: i64,
    pub clip_path: PathBuf,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct UploadCompletion {
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
struct TempUploadFile {
    path: PathBuf,
}

impl Drop for TempUploadFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn prepare_youtube_upload_input(
    request: &UploadRequest,
    audio_tracks: &[ClipAudioTrackRecord],
) -> Result<(PathBuf, Option<TempUploadFile>, Option<String>), String> {
    let mix_track = audio_tracks.iter().find(|track| track.role == "mixed");
    let needs_compat_remux = mix_track.is_some() || audio_tracks.len() > 1;
    if !needs_compat_remux {
        return Ok((request.clip_path.clone(), None, None));
    }

    let selected_track = mix_track.map(|track| track.stream_index).unwrap_or(0);
    let temp_path = youtube_temp_upload_path(request);
    remux_youtube_compatible_input(&request.clip_path, &temp_path, selected_track)?;

    let note = if mix_track.is_some() {
        Some("Prepared a YouTube-compatible upload using the premix audio track only.".into())
    } else {
        Some(
            "Prepared a YouTube-compatible upload using audio track 0 because no premix track was available."
                .into(),
        )
    };

    Ok((
        temp_path.clone(),
        Some(TempUploadFile { path: temp_path }),
        note,
    ))
}

fn youtube_temp_upload_path(request: &UploadRequest) -> PathBuf {
    let stem = request
        .clip_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("nanite-clip-upload");
    std::env::temp_dir().join(format!(
        "{stem}-youtube-upload-{}.mp4",
        chrono::Utc::now().timestamp_millis()
    ))
}

fn remux_youtube_compatible_input(
    input: &PathBuf,
    output: &PathBuf,
    audio_stream_index: i32,
) -> Result<(), String> {
    let output_result = Command::new("ffmpeg")
        .arg("-y")
        .arg("-i")
        .arg(input)
        .arg("-map")
        .arg("0:v")
        .arg("-map")
        .arg(format!("0:a:{audio_stream_index}"))
        .arg("-c")
        .arg("copy")
        .arg("-movflags")
        .arg("+faststart")
        .arg(output)
        .output()
        .map_err(|error| {
            format!("failed to start ffmpeg for YouTube compatibility remux: {error}")
        })?;

    if !output_result.status.success() {
        return Err(format!(
            "ffmpeg failed while preparing the YouTube upload input: {}",
            String::from_utf8_lossy(&output_result.stderr).trim()
        ));
    }

    Ok(())
}

pub async fn begin_youtube_oauth(client: YouTubeOAuthClient) -> Result<YouTubeOAuthTokens, String> {
    if client.client_id.trim().is_empty() {
        return Err("Enter a YouTube desktop OAuth client ID first.".into());
    }

    info!(
        client_secret_present = client
            .client_secret
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty()),
        "Starting YouTube OAuth flow"
    );
    tokio::task::spawn_blocking(move || begin_youtube_oauth_blocking(client))
        .await
        .map_err(|error| format!("failed to join YouTube OAuth worker: {error}"))?
}

fn begin_youtube_oauth_blocking(client: YouTubeOAuthClient) -> Result<YouTubeOAuthTokens, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("failed to bind local OAuth callback listener: {error}"))?;
    listener
        .set_nonblocking(false)
        .map_err(|error| format!("failed to configure local OAuth listener: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("failed to inspect local OAuth listener: {error}"))?
        .port();
    info!(port, "YouTube OAuth callback listener ready");

    let state = best_effort_nonce();
    let redirect_uri = format!("http://127.0.0.1:{port}/oauth2/callback");
    let auth_url = Url::parse_with_params(
        "https://accounts.google.com/o/oauth2/v2/auth",
        &[
            ("client_id", client.client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", YOUTUBE_OAUTH_SCOPE_VALUE),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("state", state.as_str()),
        ],
    )
    .map_err(|error| format!("failed to build YouTube OAuth URL: {error}"))?;

    info!(redirect_uri = %redirect_uri, "Opening browser for YouTube OAuth");
    launcher::open_url(auth_url.as_str())?;

    let (mut stream, _) = listener
        .accept()
        .map_err(|error| format!("failed to accept local OAuth callback: {error}"))?;
    let mut buffer = [0_u8; 8192];
    let size = stream
        .read(&mut buffer)
        .map_err(|error| format!("failed to read OAuth callback: {error}"))?;
    let request = String::from_utf8_lossy(&buffer[..size]).to_string();
    let first_line = request
        .lines()
        .next()
        .ok_or_else(|| "received an empty OAuth callback request".to_string())?;
    let path = first_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| "failed to parse OAuth callback path".to_string())?;
    info!(path, "Received YouTube OAuth callback");
    let callback_url = Url::parse(&format!("http://127.0.0.1{path}"))
        .map_err(|error| format!("failed to parse OAuth callback query: {error}"))?;
    let response_body = b"nanite-clip YouTube authentication completed. You can close this tab.";
    let _ = stream.write_all(
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n",
            response_body.len()
        )
        .as_bytes(),
    );
    let _ = stream.write_all(response_body);

    let callback_state = callback_url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default();
    if callback_state != state {
        warn!("YouTube OAuth state verification failed");
        return Err("OAuth state verification failed.".into());
    }
    info!("YouTube OAuth state verified");

    let code = callback_url
        .query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.into_owned())
        .ok_or_else(|| "OAuth callback did not include an authorization code.".to_string())?;
    info!("Exchanging YouTube OAuth authorization code for tokens");

    let token = exchange_youtube_code(client, &redirect_uri, &code)?;
    info!(
        has_access_token = token
            .access_token
            .as_ref()
            .is_some_and(|value| !value.is_empty()),
        has_refresh_token = token
            .refresh_token
            .as_ref()
            .is_some_and(|value| !value.is_empty()),
        "YouTube OAuth token exchange completed"
    );
    Ok(YouTubeOAuthTokens {
        refresh_token: token.refresh_token.ok_or_else(|| {
            "Google did not return a refresh token. Try reconnecting and approving offline access."
                .to_string()
        })?,
    })
}

fn exchange_youtube_code(
    client: YouTubeOAuthClient,
    redirect_uri: &str,
    code: &str,
) -> Result<GoogleOAuthTokenResponse, String> {
    let mut params = vec![
        ("client_id", client.client_id),
        ("code", code.to_string()),
        ("grant_type", "authorization_code".into()),
        ("redirect_uri", redirect_uri.to_string()),
    ];
    if let Some(client_secret) = client.client_secret {
        if !client_secret.trim().is_empty() {
            params.push(("client_secret", client_secret));
        }
    }

    let client = reqwest::blocking::Client::new();
    info!("Submitting YouTube OAuth token exchange request");
    let response = client
        .post("https://oauth2.googleapis.com/token")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(url_encoded_body(&params)?)
        .send()
        .map_err(|error| format!("failed to exchange Google OAuth code: {error}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .unwrap_or_else(|_| "unable to read body".into());
        warn!(%status, body = %body, "YouTube OAuth token exchange failed");
        if status == reqwest::StatusCode::BAD_REQUEST
            && youtube_token_exchange_is_missing_client_secret(&body)
        {
            return Err(
                "Google rejected the OAuth client because no client secret was provided. Enter the matching YouTube OAuth client secret in Settings, or switch to a Google Desktop App OAuth client that does not require one."
                    .into(),
            );
        }
        return Err(format!(
            "Google OAuth token exchange failed with {status}: {body}"
        ));
    }

    let payload: GoogleOAuthTokenResponse = response
        .json()
        .map_err(|error| format!("failed to decode Google OAuth token response: {error}"))?;
    info!(
        has_access_token = payload
            .access_token
            .as_ref()
            .is_some_and(|value| !value.is_empty()),
        has_refresh_token = payload
            .refresh_token
            .as_ref()
            .is_some_and(|value| !value.is_empty()),
        "Decoded YouTube OAuth token exchange response"
    );
    Ok(payload)
}

async fn refresh_youtube_access_token(
    credentials: &YouTubeUploadCredentials,
) -> Result<String, String> {
    let mut params = vec![
        ("client_id", credentials.client_id.clone()),
        ("grant_type", "refresh_token".into()),
        ("refresh_token", credentials.refresh_token.clone()),
    ];
    if let Some(client_secret) = &credentials.client_secret {
        if !client_secret.trim().is_empty() {
            params.push(("client_secret", client_secret.clone()));
        }
    }

    let response = reqwest::Client::new()
        .post("https://oauth2.googleapis.com/token")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(url_encoded_body(&params)?)
        .send()
        .await
        .map_err(|error| format!("failed to refresh Google OAuth token: {error}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "unable to read body".into());
        return Err(format!(
            "Google OAuth token refresh failed with {status}: {body}"
        ));
    }

    let payload: GoogleOAuthTokenResponse = response
        .json()
        .await
        .map_err(|error| format!("failed to decode token refresh response: {error}"))?;
    payload
        .access_token
        .ok_or_else(|| "Google token refresh did not return an access token.".into())
}

async fn start_youtube_resumable_upload(
    access_token: &str,
    metadata: Vec<u8>,
    content_length: usize,
) -> Result<String, String> {
    let response = reqwest::Client::new()
        .post("https://www.googleapis.com/upload/youtube/v3/videos?part=snippet,status&uploadType=resumable")
        .header(AUTHORIZATION, format!("Bearer {access_token}"))
        .header(CONTENT_TYPE, "application/json; charset=UTF-8")
        .header("X-Upload-Content-Type", "application/octet-stream")
        .header("X-Upload-Content-Length", content_length)
        .body(metadata)
        .send()
        .await
        .map_err(|error| format!("failed to start YouTube resumable upload: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "unable to read body".into());
        return Err(format!(
            "YouTube resumable upload setup failed with {status}: {body}"
        ));
    }

    response
        .headers()
        .get(LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .ok_or_else(|| "YouTube resumable upload did not return a session URL.".into())
}

async fn wait_for_youtube_processing(
    ctx: &BackgroundJobContext,
    access_token: &str,
    video_id: &str,
) -> Result<(), String> {
    let client = reqwest::Client::new();

    for attempt in 0..YOUTUBE_PROCESSING_MAX_POLLS {
        let status = fetch_youtube_processing_status(&client, access_token, video_id).await?;
        match status.processing_status.as_str() {
            "succeeded" => {
                info!(video_id = video_id, "YouTube video processing succeeded");
                return Ok(());
            }
            "failed" | "rejected" | "terminated" => {
                let reason = status
                    .failure_reason
                    .or(status.rejection_reason)
                    .unwrap_or_else(|| "unknown reason".into());
                return Err(format!(
                    "YouTube finished processing video {video_id} with status {}: {reason}",
                    status.processing_status
                ));
            }
            _ => {
                let step_message = status
                    .time_left_ms
                    .map(|millis| {
                        format!(
                            "YouTube is still processing the upload. Estimated time left: {}s.",
                            (millis / 1000).max(1)
                        )
                    })
                    .unwrap_or_else(|| "YouTube is still processing the upload.".into());
                ctx.progress(4, 5, step_message)?;
                info!(
                    video_id = video_id,
                    attempt = attempt + 1,
                    processing_status = %status.processing_status,
                    time_left_ms = ?status.time_left_ms,
                    "Waiting for YouTube processing"
                );
                tokio::time::sleep(YOUTUBE_PROCESSING_POLL_INTERVAL).await;
            }
        }
    }

    Err(format!(
        "Timed out waiting for YouTube to finish processing video {video_id}."
    ))
}

async fn fetch_youtube_processing_status(
    client: &reqwest::Client,
    access_token: &str,
    video_id: &str,
) -> Result<YouTubeProcessingStatus, String> {
    let response = client
        .get("https://www.googleapis.com/youtube/v3/videos")
        .bearer_auth(access_token)
        .query(&[("id", video_id), ("part", "processingDetails,status")])
        .send()
        .await
        .map_err(|error| format!("failed to check YouTube processing status: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "unable to read body".into());
        if status == reqwest::StatusCode::FORBIDDEN
            && youtube_access_token_scope_is_insufficient(&body)
        {
            return Err(format!(
                "YouTube processing status check failed with {status}: the stored token is missing the scope required to read processing status. Nanite Clip now needs both `{YOUTUBE_UPLOAD_SCOPE}` and `{YOUTUBE_READONLY_SCOPE}`. Disconnect and reconnect YouTube in Settings so Google issues a new refresh token with both scopes. Response: {body}"
            ));
        }
        return Err(format!(
            "YouTube processing status check failed with {status}: {body}"
        ));
    }

    let payload: YouTubeVideosListResponse = response
        .json()
        .await
        .map_err(|error| format!("failed to decode YouTube processing status response: {error}"))?;
    let item = payload.items.into_iter().next().ok_or_else(|| {
        format!("YouTube processing status check returned no video for {video_id}.")
    })?;
    let processing_details = item.processing_details;
    let status = item.status;
    let processing_status = processing_details
        .as_ref()
        .and_then(|details| details.processing_status.clone())
        .or_else(|| {
            status
                .as_ref()
                .and_then(|status| status.upload_status.clone())
        })
        .unwrap_or_else(|| "processing".into());
    let failure_reason = processing_details
        .as_ref()
        .and_then(|details| details.processing_failure_reason.clone());
    let rejection_reason = status.and_then(|status| status.rejection_reason);
    let time_left_ms = processing_details
        .and_then(|details| details.processing_progress)
        .and_then(|progress| progress.time_left_ms);

    Ok(YouTubeProcessingStatus {
        processing_status,
        failure_reason,
        rejection_reason,
        time_left_ms,
    })
}

fn best_effort_nonce() -> String {
    if let Ok(value) = std::fs::read_to_string("/proc/sys/kernel/random/uuid") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    format!(
        "{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    )
}

fn url_encoded_body(params: &[(impl AsRef<str>, impl AsRef<str>)]) -> Result<String, String> {
    let query = Url::parse_with_params("http://localhost/", params)
        .map_err(|error| format!("failed to encode form body: {error}"))?
        .query()
        .unwrap_or_default()
        .to_string();
    Ok(query)
}

fn build_copyparty_upload_url(base_url: &str) -> Result<Url, String> {
    let mut url = Url::parse(base_url.trim())
        .map_err(|error| format!("invalid Copyparty upload URL `{base_url}`: {error}"))?;
    let mut path = url.path().trim_end_matches('/').to_string();
    path.push('/');
    url.set_path(&path);
    url.query_pairs_mut().append_pair("want", "url");
    Ok(url)
}

fn resolve_copyparty_clip_url(
    response_body: &str,
    credentials: &CopypartyUploadCredentials,
    request: &UploadRequest,
) -> Result<Option<String>, String> {
    let trimmed = response_body.trim();
    if trimmed.is_empty() {
        if credentials.public_base_url.trim().is_empty() {
            return Ok(None);
        }

        let file_name = request
            .clip_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("clip.mp4");
        return join_copyparty_url(&credentials.public_base_url, file_name).map(Some);
    }

    if let Ok(url) = Url::parse(trimmed) {
        return Ok(Some(url.to_string()));
    }

    let base_url = if credentials.public_base_url.trim().is_empty() {
        &credentials.upload_url
    } else {
        &credentials.public_base_url
    };

    join_copyparty_url(base_url, trimmed).map(Some)
}

fn join_copyparty_url(base_url: &str, suffix: &str) -> Result<String, String> {
    let mut url = Url::parse(base_url.trim())
        .map_err(|error| format!("invalid Copyparty base URL `{base_url}`: {error}"))?;
    url.set_query(None);
    url.set_fragment(None);

    let mut path = url.path().trim_end_matches('/').to_string();
    let clean_suffix = suffix.trim().trim_start_matches('/');
    if !clean_suffix.is_empty() {
        if path.is_empty() {
            path.push('/');
        } else {
            path.push('/');
        }
        path.push_str(clean_suffix);
        url.set_path(&path);
    }

    Ok(url.to_string())
}

fn copyparty_external_id_from_url(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    parsed
        .path_segments()
        .and_then(|segments| segments.filter(|segment| !segment.is_empty()).next_back())
        .map(|segment| segment.to_string())
}

#[derive(Debug, Serialize)]
struct YouTubeUploadMetadata {
    snippet: YouTubeUploadSnippet,
    status: YouTubeUploadStatus,
}

#[derive(Debug, Serialize)]
struct YouTubeUploadSnippet {
    title: String,
    description: String,
}

#[derive(Debug, Serialize)]
struct YouTubeUploadStatus {
    #[serde(rename = "privacyStatus")]
    privacy_status: String,
}

#[derive(Debug, Deserialize)]
struct YouTubeVideoInsertResponse {
    #[serde(default)]
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YouTubeVideosListResponse {
    #[serde(default)]
    items: Vec<YouTubeVideoStatusItem>,
}

#[derive(Debug, Deserialize)]
struct YouTubeVideoStatusItem {
    #[serde(rename = "processingDetails")]
    #[serde(default)]
    processing_details: Option<YouTubeProcessingDetails>,
    #[serde(default)]
    status: Option<YouTubeVideoStatus>,
}

#[derive(Debug, Deserialize)]
struct YouTubeProcessingDetails {
    #[serde(rename = "processingStatus")]
    #[serde(default)]
    processing_status: Option<String>,
    #[serde(rename = "processingFailureReason")]
    #[serde(default)]
    processing_failure_reason: Option<String>,
    #[serde(rename = "processingProgress")]
    #[serde(default)]
    processing_progress: Option<YouTubeProcessingProgress>,
}

#[derive(Debug, Deserialize)]
struct YouTubeProcessingProgress {
    #[serde(rename = "timeLeftMs")]
    #[serde(default)]
    time_left_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct YouTubeVideoStatus {
    #[serde(rename = "uploadStatus")]
    #[serde(default)]
    upload_status: Option<String>,
    #[serde(rename = "rejectionReason")]
    #[serde(default)]
    rejection_reason: Option<String>,
}

#[derive(Debug)]
struct YouTubeProcessingStatus {
    processing_status: String,
    failure_reason: Option<String>,
    rejection_reason: Option<String>,
    time_left_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GoogleOAuthTokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
}

impl YouTubePrivacyStatus {
    fn as_api_value(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Unlisted => "unlisted",
            Self::Private => "private",
        }
    }
}

fn youtube_access_token_scope_is_insufficient(body: &str) -> bool {
    body.contains("ACCESS_TOKEN_SCOPE_INSUFFICIENT")
        || body.contains("Request had insufficient authentication scopes.")
        || body.contains("\"reason\": \"insufficientPermissions\"")
}

fn youtube_token_exchange_is_missing_client_secret(body: &str) -> bool {
    body.contains("\"error\": \"invalid_request\"")
        && body.contains("\"error_description\": \"client_secret is missing.\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_privacy_status_maps_to_api_values() {
        assert_eq!(YouTubePrivacyStatus::Public.as_api_value(), "public");
        assert_eq!(YouTubePrivacyStatus::Unlisted.as_api_value(), "unlisted");
        assert_eq!(YouTubePrivacyStatus::Private.as_api_value(), "private");
    }

    #[test]
    fn youtube_oauth_scope_value_includes_upload_and_readonly() {
        assert!(YOUTUBE_OAUTH_SCOPE_VALUE.contains(YOUTUBE_UPLOAD_SCOPE));
        assert!(YOUTUBE_OAUTH_SCOPE_VALUE.contains(YOUTUBE_READONLY_SCOPE));
    }

    #[test]
    fn detects_youtube_scope_insufficient_errors() {
        let body = r#"{
  "error": {
    "message": "Request had insufficient authentication scopes.",
    "errors": [
      {
        "reason": "insufficientPermissions"
      }
    ],
    "details": [
      {
        "reason": "ACCESS_TOKEN_SCOPE_INSUFFICIENT"
      }
    ]
  }
}"#;
        assert!(youtube_access_token_scope_is_insufficient(body));
    }

    #[test]
    fn detects_missing_client_secret_in_token_exchange_error() {
        let body = r#"{
  "error": "invalid_request",
  "error_description": "client_secret is missing."
}"#;
        assert!(youtube_token_exchange_is_missing_client_secret(body));
    }
}
