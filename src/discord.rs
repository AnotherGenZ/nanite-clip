use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use reqwest::multipart::{Form, Part};
use serde::Serialize;
use tracing::{info, warn};

use crate::background_jobs::BackgroundJobContext;

#[derive(Debug, Clone)]
pub struct DiscordWebhookRequest {
    pub webhook_url: String,
    pub clip_title: String,
    pub clip_path: Option<PathBuf>,
    pub clip_url: Option<String>,
    pub score: u32,
    pub profile_name: String,
    pub rule_name: String,
    pub character_name: String,
    pub location_label: String,
    pub event_timestamp_label: String,
    pub include_thumbnail: bool,
}

pub async fn send_clip_webhook(
    ctx: BackgroundJobContext,
    request: DiscordWebhookRequest,
) -> Result<(), String> {
    info!(
        clip_title = %request.clip_title,
        clip_url = ?request.clip_url,
        include_thumbnail = request.include_thumbnail,
        "Starting Discord webhook delivery"
    );
    ctx.progress(1, 3, "Preparing Discord webhook payload.")?;

    let client = reqwest::Client::new();
    let thumbnail_path: Option<PathBuf> = if request.include_thumbnail {
        request
            .clip_path
            .as_ref()
            .map(|path| extract_thumbnail(path.as_path()))
            .transpose()?
            .flatten()
    } else {
        None
    };

    let payload = DiscordWebhookPayload::from_request(&request, thumbnail_path.is_some());
    ctx.progress(2, 3, "Posting Discord webhook.")?;

    post_webhook_with_retries(
        &client,
        &request.webhook_url,
        &payload,
        thumbnail_path.as_deref(),
    )
    .await?;

    if let Some(thumbnail_path) = thumbnail_path {
        let _ = std::fs::remove_file(thumbnail_path);
    }

    ctx.progress(3, 3, "Discord webhook sent.")?;
    info!(clip_title = %request.clip_title, "Discord webhook delivered");
    Ok(())
}

async fn post_webhook_with_retries(
    client: &reqwest::Client,
    webhook_url: &str,
    payload: &DiscordWebhookPayload,
    thumbnail_path: Option<&Path>,
) -> Result<(), String> {
    for attempt in 0..4 {
        let response = if let Some(thumbnail_path) = thumbnail_path {
            let bytes = std::fs::read(thumbnail_path).map_err(|error| {
                format!(
                    "failed to read webhook thumbnail {}: {error}",
                    thumbnail_path.display()
                )
            })?;
            let form = Form::new()
                .text(
                    "payload_json",
                    serde_json::to_string(payload)
                        .map_err(|error| format!("failed to serialize webhook payload: {error}"))?,
                )
                .part(
                    "files[0]",
                    Part::bytes(bytes)
                        .file_name("thumbnail.png")
                        .mime_str("image/png")
                        .map_err(|error| format!("failed to build thumbnail payload: {error}"))?,
                );
            client
                .post(webhook_url)
                .multipart(form)
                .send()
                .await
                .map_err(|error| format!("failed to send Discord webhook: {error}"))?
        } else {
            client
                .post(webhook_url)
                .json(payload)
                .send()
                .await
                .map_err(|error| format!("failed to send Discord webhook: {error}"))?
        };

        if response.status().is_success() {
            return Ok(());
        }

        if response.status().as_u16() == 429 && attempt < 3 {
            warn!(
                webhook_url = webhook_url,
                attempt = attempt + 1,
                "Discord webhook hit a rate limit; retrying"
            );
            let rate_limit: DiscordRateLimit = response
                .json()
                .await
                .unwrap_or(DiscordRateLimit { retry_after: 1.0 });
            tokio::time::sleep(Duration::from_secs_f64(rate_limit.retry_after.max(0.5))).await;
            continue;
        }

        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "unable to read response body".into());
        return Err(format!("Discord webhook failed with {status}: {body}"));
    }

    Err("Discord webhook exhausted retry attempts.".into())
}

fn extract_thumbnail(path: &Path) -> Result<Option<PathBuf>, String> {
    let output_path = path.with_extension("discord-thumb.png");
    let status = Command::new("ffmpeg")
        .arg("-y")
        .arg("-ss")
        .arg("00:00:01")
        .arg("-i")
        .arg(path)
        .arg("-frames:v")
        .arg("1")
        .arg(&output_path)
        .status()
        .map_err(|error| format!("failed to launch ffmpeg thumbnail extraction: {error}"))?;

    if status.success() && output_path.exists() {
        Ok(Some(output_path))
    } else {
        Ok(None)
    }
}

#[derive(Debug, Serialize)]
struct DiscordWebhookPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    embeds: Vec<DiscordEmbed>,
}

impl DiscordWebhookPayload {
    fn from_request(request: &DiscordWebhookRequest, has_thumbnail: bool) -> Self {
        let description = vec![
            format!("Profile: {}", request.profile_name),
            format!("Rule: {}", request.rule_name),
            format!("Character: {}", request.character_name),
            format!("Location: {}", request.location_label),
            format!("Triggered: {}", request.event_timestamp_label),
        ];

        Self {
            content: request.clip_url.clone(),
            embeds: vec![DiscordEmbed {
                title: request.clip_title.clone(),
                description: description.join("\n"),
                url: request.clip_url.clone(),
                color: 0x4A90E2,
                footer: DiscordFooter {
                    text: format!("Score {}", request.score),
                },
                thumbnail: has_thumbnail.then_some(DiscordThumbnail {
                    url: "attachment://thumbnail.png".into(),
                }),
            }],
        }
    }
}

#[derive(Debug, Serialize)]
struct DiscordEmbed {
    title: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    color: u32,
    footer: DiscordFooter,
    #[serde(skip_serializing_if = "Option::is_none")]
    thumbnail: Option<DiscordThumbnail>,
}

#[derive(Debug, Serialize)]
struct DiscordFooter {
    text: String,
}

#[derive(Debug, Serialize)]
struct DiscordThumbnail {
    url: String,
}

#[derive(Debug, Deserialize)]
struct DiscordRateLimit {
    #[serde(default)]
    retry_after: f64,
}

use serde::Deserialize;
