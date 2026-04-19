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
    pub draft: bool,
    pub prerelease: bool,
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
    let client = client()?;
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases?per_page=12");
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

    releases
        .into_iter()
        .find(|release| {
            if release.draft {
                return false;
            }

            match channel {
                UpdateChannel::Stable => !release.prerelease,
                UpdateChannel::Beta => true,
            }
        })
        .ok_or_else(|| "No matching GitHub release was available.".into())
}
