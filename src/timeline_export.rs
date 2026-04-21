use std::path::{Path, PathBuf};

use crate::db::{ClipDetailRecord, ClipRawEventRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineExportKind {
    Chapters,
    Subtitles,
}

impl TimelineExportKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Chapters => "chapters",
            Self::Subtitles => "subtitles",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineMarker {
    pub start_ms: u64,
    pub end_ms: u64,
    pub label: String,
}

pub fn export_timeline_sidecar(
    detail: &ClipDetailRecord,
    kind: TimelineExportKind,
) -> Result<PathBuf, String> {
    let clip_path = detail
        .clip
        .path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| "This clip does not have a saved file path yet.".to_string())?;

    let markers = build_timeline_markers(detail);
    if markers.is_empty() {
        return Err("This clip does not have any raw event markers to export.".into());
    }

    let output_path = sidecar_path(&clip_path, kind)?;
    let contents = match kind {
        TimelineExportKind::Chapters => render_ffmetadata_chapters(&markers),
        TimelineExportKind::Subtitles => render_srt_subtitles(&markers),
    };

    std::fs::write(&output_path, contents)
        .map_err(|error| format!("failed to write {} export: {error}", kind.label()))?;

    Ok(output_path)
}

pub fn build_timeline_markers(detail: &ClipDetailRecord) -> Vec<TimelineMarker> {
    let clip_duration_ms = detail
        .clip
        .clip_end_at
        .signed_duration_since(detail.clip.clip_start_at)
        .num_milliseconds()
        .max(0) as u64;
    if clip_duration_ms == 0 {
        return Vec::new();
    }

    let starts: Vec<u64> = detail
        .raw_events
        .iter()
        .map(|event| {
            event
                .event_at
                .signed_duration_since(detail.clip.clip_start_at)
                .num_milliseconds()
                .clamp(0, clip_duration_ms as i64) as u64
        })
        .collect();

    let mut markers = Vec::with_capacity(detail.raw_events.len());
    for (index, event) in detail.raw_events.iter().enumerate() {
        let start_ms = starts[index];
        let next_start = starts.get(index + 1).copied().unwrap_or(clip_duration_ms);
        let end_ms = compute_marker_end_ms(start_ms, next_start, clip_duration_ms);
        markers.push(TimelineMarker {
            start_ms,
            end_ms,
            label: marker_label(event),
        });
    }

    markers
}

fn compute_marker_end_ms(start_ms: u64, next_start_ms: u64, clip_duration_ms: u64) -> u64 {
    let preferred_end = start_ms.saturating_add(2_500);
    let bounded_next = next_start_ms
        .saturating_sub(1)
        .max(start_ms.saturating_add(1));
    preferred_end.min(bounded_next).min(clip_duration_ms)
}

fn marker_label(event: &ClipRawEventRecord) -> String {
    let mut parts = vec![event.event_kind.clone()];
    if event.is_headshot {
        parts.push("headshot".into());
    }
    if let Some(target) = event.other_character_name.clone().or_else(|| {
        event
            .other_character_id
            .map(|id| format!("Character #{id}"))
    }) {
        parts.push(format!("target {target}"));
    }
    if let Some(weapon) = event
        .attacker_weapon_name
        .clone()
        .or_else(|| event.attacker_weapon_id.map(|id| format!("Weapon #{id}")))
    {
        parts.push(format!("weapon {weapon}"));
    }
    parts.join(" | ")
}

fn render_ffmetadata_chapters(markers: &[TimelineMarker]) -> String {
    let mut output = String::from(";FFMETADATA1\n");
    for marker in markers {
        output.push_str("[CHAPTER]\n");
        output.push_str("TIMEBASE=1/1000\n");
        output.push_str(format!("START={}\n", marker.start_ms).as_str());
        output.push_str(format!("END={}\n", marker.end_ms).as_str());
        output.push_str(format!("title={}\n", sanitize_chapter_title(&marker.label)).as_str());
    }
    output
}

fn render_srt_subtitles(markers: &[TimelineMarker]) -> String {
    let mut output = String::new();
    for (index, marker) in markers.iter().enumerate() {
        output.push_str(format!("{}\n", index + 1).as_str());
        output.push_str(
            format!(
                "{} --> {}\n",
                format_srt_timestamp(marker.start_ms),
                format_srt_timestamp(marker.end_ms)
            )
            .as_str(),
        );
        output.push_str(marker.label.as_str());
        output.push_str("\n\n");
    }
    output
}

fn sanitize_chapter_title(value: &str) -> String {
    value
        .replace(['\n', '\r'], " ")
        .replace('=', "-")
        .trim()
        .to_string()
}

fn format_srt_timestamp(timestamp_ms: u64) -> String {
    let hours = timestamp_ms / 3_600_000;
    let minutes = (timestamp_ms % 3_600_000) / 60_000;
    let seconds = (timestamp_ms % 60_000) / 1_000;
    let millis = timestamp_ms % 1_000;
    format!("{hours:02}:{minutes:02}:{seconds:02},{millis:03}")
}

fn sidecar_path(clip_path: &Path, kind: TimelineExportKind) -> Result<PathBuf, String> {
    let parent = clip_path
        .parent()
        .ok_or_else(|| format!("clip path {} has no parent directory", clip_path.display()))?;
    let stem = clip_path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("clip path {} has no usable file stem", clip_path.display()))?;

    let suffix = match kind {
        TimelineExportKind::Chapters => "chapters.ffmeta",
        TimelineExportKind::Subtitles => "timeline.srt",
    };

    Ok(parent.join(format!("{stem}.{suffix}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{
        ClipAlertRecord, ClipDetailRecord, ClipOrigin, ClipOverlapRecord, ClipRawEventRecord,
        ClipRecord,
    };
    use chrono::{TimeZone, Utc};

    fn ts(secs: i64, millis: u32) -> chrono::DateTime<Utc> {
        Utc.timestamp_opt(1_700_000_000 + secs, millis * 1_000_000)
            .unwrap()
    }

    fn sample_detail() -> ClipDetailRecord {
        let clip_start_at = ts(0, 0);
        let clip_end_at = ts(20, 0);
        ClipDetailRecord {
            clip: ClipRecord {
                id: 1,
                trigger_event_at: ts(15, 0),
                clip_start_at,
                clip_end_at,
                saved_at: clip_end_at,
                origin: ClipOrigin::Rule,
                profile_id: "profile".into(),
                rule_id: "rule".into(),
                clip_duration_secs: 20,
                session_id: Some("session-1".into()),
                character_id: 42,
                world_id: 17,
                zone_id: Some(2),
                facility_id: Some(1234),
                zone_name: Some("Indar".into()),
                facility_name: Some("The Crown".into()),
                score: 10,
                honu_session_id: None,
                path: Some("/tmp/example.mkv".into()),
                file_size_bytes: None,
                favorited: false,
                overlap_count: 0,
                alert_count: 0,
                collection_count: 0,
                collection_sequence_index: None,
                post_process_status: crate::db::PostProcessStatus::Legacy,
                post_process_error: None,
                tags: Vec::new(),
                events: Vec::new(),
            },
            tags: Vec::new(),
            collections: Vec::new(),
            audio_tracks: Vec::new(),
            raw_events: vec![
                ClipRawEventRecord {
                    event_at: ts(3, 250),
                    event_kind: "Kill".into(),
                    world_id: 17,
                    zone_id: Some(2),
                    zone_name: Some("Indar".into()),
                    facility_id: Some(1234),
                    facility_name: Some("The Crown".into()),
                    actor_character_id: Some(42),
                    actor_character_name: Some("Player".into()),
                    other_character_id: Some(100),
                    other_character_name: Some("Enemy".into()),
                    actor_class: Some("Heavy Assault".into()),
                    attacker_weapon_id: Some(80),
                    attacker_weapon_name: Some("Gauss Rifle".into()),
                    attacker_vehicle_id: None,
                    attacker_vehicle_name: None,
                    vehicle_killed_id: None,
                    vehicle_killed_name: None,
                    characters_killed: 1,
                    is_headshot: false,
                    experience_id: None,
                },
                ClipRawEventRecord {
                    event_at: ts(5, 0),
                    event_kind: "Headshot".into(),
                    world_id: 17,
                    zone_id: Some(2),
                    zone_name: Some("Indar".into()),
                    facility_id: Some(1234),
                    facility_name: Some("The Crown".into()),
                    actor_character_id: Some(42),
                    actor_character_name: Some("Player".into()),
                    other_character_id: Some(101),
                    other_character_name: Some("Another Enemy".into()),
                    actor_class: Some("Heavy Assault".into()),
                    attacker_weapon_id: Some(81),
                    attacker_weapon_name: Some("Bishop".into()),
                    attacker_vehicle_id: None,
                    attacker_vehicle_name: None,
                    vehicle_killed_id: None,
                    vehicle_killed_name: None,
                    characters_killed: 1,
                    is_headshot: true,
                    experience_id: None,
                },
            ],
            alerts: vec![ClipAlertRecord {
                alert_key: "alert-1".into(),
                label: "Indar Superiority".into(),
                world_id: 17,
                zone_id: 2,
                metagame_event_id: 1,
                started_at: ts(-300, 0),
                ended_at: Some(ts(600, 0)),
                state_name: "ended".into(),
                winner_faction: Some("VS".into()),
            }],
            overlaps: vec![ClipOverlapRecord {
                clip_id: 2,
                trigger_event_at: ts(18, 0),
                clip_start_at: ts(10, 0),
                clip_end_at: ts(25, 0),
                profile_id: "profile".into(),
                rule_id: "other-rule".into(),
                path: Some("/tmp/other.mkv".into()),
                overlap_duration_ms: 10_000,
            }],
            uploads: Vec::new(),
        }
    }

    #[test]
    fn builds_offsets_from_clip_start_and_event_times() {
        let markers = build_timeline_markers(&sample_detail());
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].start_ms, 3_250);
        assert_eq!(markers[1].start_ms, 5_000);
        assert!(markers[0].end_ms >= markers[0].start_ms);
        assert!(markers[1].end_ms > markers[1].start_ms);
    }

    #[test]
    fn renders_subtitles_with_srt_timestamps() {
        let subtitles = render_srt_subtitles(&build_timeline_markers(&sample_detail()));
        assert!(subtitles.contains("00:00:03,250 -->"));
        assert!(subtitles.contains("Kill | target Enemy | weapon Gauss Rifle"));
    }

    #[test]
    fn renders_ffmetadata_chapters() {
        let chapters = render_ffmetadata_chapters(&build_timeline_markers(&sample_detail()));
        assert!(chapters.starts_with(";FFMETADATA1"));
        assert!(chapters.contains("TIMEBASE=1/1000"));
        assert!(
            chapters.contains("title=Headshot | headshot | target Another Enemy | weapon Bishop")
        );
    }
}
