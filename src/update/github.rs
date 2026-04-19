use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT};
use serde::Deserialize;

use crate::config::UpdateChannel;

use super::GITHUB_REPO;

#[derive(Debug, Clone, Deserialize)]
pub struct GithubAsset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GithubRelease {
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    pub html_url: String,
    pub draft: bool,
    pub prerelease: bool,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub assets: Vec<GithubAsset>,
}

pub fn client() -> Result<reqwest::Client, String> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(format!("nanite-clip/{}", env!("CARGO_PKG_VERSION")).as_str())
            .map_err(|error| format!("failed to build updater user-agent: {error}"))?,
    );
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/vnd.github+json"),
    );

    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|error| format!("failed to build GitHub API client: {error}"))
}

pub async fn fetch_release(channel: UpdateChannel) -> Result<GithubRelease, String> {
    fetch_releases(channel, 12)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| "No matching GitHub release was available.".into())
}

pub async fn fetch_releases(
    channel: UpdateChannel,
    limit: usize,
) -> Result<Vec<GithubRelease>, String> {
    let client = client()?;
    let limit = limit.clamp(1, 100);
    let page_size = limit.min(50);
    let mut page = 1usize;
    let mut matching_releases = Vec::new();

    while matching_releases.len() < limit {
        let url = format!(
            "https://api.github.com/repos/{GITHUB_REPO}/releases?per_page={page_size}&page={page}"
        );
        let releases = client
            .get(url)
            .send()
            .await
            .map_err(|error| format!("failed to contact GitHub Releases: {error}"))?
            .error_for_status()
            .map_err(|error| format!("GitHub Releases returned an error: {error}"))?
            .json::<Vec<GithubRelease>>()
            .await
            .map_err(|error| format!("failed to decode GitHub release metadata: {error}"))?;

        if releases.is_empty() {
            break;
        }

        let fetched_count = releases.len();
        for release in releases {
            if release_matches_channel(&release, channel) {
                matching_releases.push(release);
                if matching_releases.len() >= limit {
                    break;
                }
            }
        }

        if fetched_count < page_size {
            break;
        }
        page += 1;
    }

    Ok(matching_releases)
}

fn release_matches_channel(release: &GithubRelease, channel: UpdateChannel) -> bool {
    if release.draft {
        return false;
    }

    match channel {
        UpdateChannel::Stable => !release.prerelease,
        UpdateChannel::Beta => true,
    }
}
