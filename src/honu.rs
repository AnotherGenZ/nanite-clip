use reqwest::Client;
use serde::Deserialize;

const BASE_URL: &str = "https://wt.honu.pw";

#[derive(Debug, Clone)]
pub struct HonuClient {
    client: Client,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: i64,
    #[serde(rename = "characterID")]
    #[allow(dead_code)]
    pub character_id: String,
    #[allow(dead_code)]
    pub start: String,
    pub end: Option<String>,
}

impl HonuClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent("nanite-clips")
            .build()
            .expect("failed to build HTTP client");
        Self { client }
    }

    /// Fetch the most recent session for a character and return it only if it
    /// is still in progress (i.e. `end` is null).
    pub async fn fetch_active_session(&self, character_id: u64) -> Result<Option<i64>, HonuError> {
        let url = format!("{BASE_URL}/api/character/{character_id}/sessions");

        let sessions: Vec<Session> = self
            .client
            .get(&url)
            .query(&[("limit", "1")])
            .send()
            .await
            .map_err(|e| HonuError::Request(e.to_string()))?
            .error_for_status()
            .map_err(|e| HonuError::Request(e.to_string()))?
            .json()
            .await
            .map_err(|e| HonuError::Request(e.to_string()))?;

        Ok(sessions.into_iter().find(|s| s.end.is_none()).map(|s| s.id))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HonuError {
    #[error("Honu API request failed: {0}")]
    Request(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_session_with_null_end() {
        let json = r#"[{
            "id": 74647730,
            "characterID": "5428885884751511569",
            "start": "2026-04-05T23:07:30Z",
            "end": null,
            "outfitID": "37535059393436717",
            "teamID": 3,
            "summaryCalculated": null,
            "kills": -1,
            "deaths": -1,
            "vehicleKills": -1,
            "experienceGained": -1,
            "heals": -1,
            "revives": -1,
            "shieldRepairs": -1,
            "resupplies": -1,
            "spawns": -1,
            "repairs": -1
        }]"#;

        let sessions: Vec<Session> = serde_json::from_str(json).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, 74647730);
        assert!(sessions[0].end.is_none());
    }

    #[test]
    fn deserializes_session_with_end() {
        let json = r#"[{
            "id": 74647730,
            "characterID": "5428885884751511569",
            "start": "2026-04-05T23:07:30Z",
            "end": "2026-04-05T23:09:49Z",
            "outfitID": "37535059393436717",
            "teamID": 3
        }]"#;

        let sessions: Vec<Session> = serde_json::from_str(json).unwrap();
        assert_eq!(sessions[0].end.as_deref(), Some("2026-04-05T23:09:49Z"));
    }
}
