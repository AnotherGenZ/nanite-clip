use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::config::StorageTieringConfig;
use crate::db::ClipRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageTier {
    Primary,
    Archive,
}

impl StorageTier {
    pub fn label(self) -> &'static str {
        match self {
            Self::Primary => "Primary",
            Self::Archive => "Archive",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageTieringCandidate {
    pub clip_id: i64,
    pub path: PathBuf,
    pub score: u32,
    pub trigger_event_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageTieringPlan {
    pub clip_id: i64,
    pub source_path: PathBuf,
    pub destination_path: PathBuf,
    pub target_tier: StorageTier,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageMoveResult {
    pub clip_id: i64,
    pub source_path: PathBuf,
    pub destination_path: PathBuf,
    pub target_tier: StorageTier,
}

pub fn clip_storage_tier(record: &ClipRecord, config: &StorageTieringConfig) -> StorageTier {
    let Some(path) = record.path.as_ref().map(PathBuf::from) else {
        return StorageTier::Primary;
    };
    clip_path_storage_tier(&path, config)
}

pub fn clip_path_storage_tier(path: &Path, config: &StorageTieringConfig) -> StorageTier {
    if path.starts_with(&config.tier_directory) {
        StorageTier::Archive
    } else {
        StorageTier::Primary
    }
}

pub fn tiering_candidate_from_clip(record: &ClipRecord) -> Option<StorageTieringCandidate> {
    if record.favorited {
        return None;
    }
    Some(StorageTieringCandidate {
        clip_id: record.id,
        path: PathBuf::from(record.path.as_ref()?),
        score: record.score,
        trigger_event_at: record.trigger_event_at,
    })
}

pub fn plan_archive_move(
    config: &StorageTieringConfig,
    primary_save_dir: &Path,
    now: DateTime<Utc>,
    candidate: &StorageTieringCandidate,
) -> Option<StorageTieringPlan> {
    if !config.enabled {
        return None;
    }
    if candidate.score > config.max_score {
        return None;
    }
    let age_days = now
        .signed_duration_since(candidate.trigger_event_at)
        .num_days()
        .max(0) as u32;
    if age_days < config.min_age_days {
        return None;
    }

    plan_move(config, primary_save_dir, candidate, StorageTier::Archive)
}

pub fn plan_restore_move(
    config: &StorageTieringConfig,
    primary_save_dir: &Path,
    candidate: &StorageTieringCandidate,
) -> Option<StorageTieringPlan> {
    plan_move(config, primary_save_dir, candidate, StorageTier::Primary)
}

fn plan_move(
    config: &StorageTieringConfig,
    primary_save_dir: &Path,
    candidate: &StorageTieringCandidate,
    target_tier: StorageTier,
) -> Option<StorageTieringPlan> {
    if candidate.path.as_os_str().is_empty() {
        return None;
    }

    let current_tier = clip_path_storage_tier(&candidate.path, config);
    if current_tier == target_tier {
        return None;
    }

    let base_dir = match target_tier {
        StorageTier::Primary => primary_save_dir,
        StorageTier::Archive => &config.tier_directory,
    };

    let file_name = candidate.path.file_name()?;
    let destination_path = unique_destination_path(base_dir, file_name);
    Some(StorageTieringPlan {
        clip_id: candidate.clip_id,
        source_path: candidate.path.clone(),
        destination_path,
        target_tier,
    })
}

pub fn execute_move_plan(plan: &StorageTieringPlan) -> Result<StorageMoveResult, String> {
    if let Some(parent) = plan.destination_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to prepare {}: {error}", parent.display()))?;
    }

    match std::fs::rename(&plan.source_path, &plan.destination_path) {
        Ok(()) => {}
        Err(error) if is_cross_device_error(&error) => {
            copy_delete_move(&plan.source_path, &plan.destination_path)?
        }
        Err(error) => {
            return Err(format!(
                "failed to move {} to {}: {error}",
                plan.source_path.display(),
                plan.destination_path.display()
            ));
        }
    }

    Ok(StorageMoveResult {
        clip_id: plan.clip_id,
        source_path: plan.source_path.clone(),
        destination_path: plan.destination_path.clone(),
        target_tier: plan.target_tier,
    })
}

fn copy_delete_move(source: &Path, destination: &Path) -> Result<(), String> {
    let temp_path = destination.with_extension("move.tmp");
    std::fs::copy(source, &temp_path).map_err(|error| {
        format!(
            "failed to copy {} to {}: {error}",
            source.display(),
            temp_path.display()
        )
    })?;
    std::fs::rename(&temp_path, destination).map_err(|error| {
        format!(
            "failed to finalize moved file {}: {error}",
            destination.display()
        )
    })?;
    std::fs::remove_file(source)
        .map_err(|error| format!("failed to remove {} after copy: {error}", source.display()))
}

fn is_cross_device_error(error: &std::io::Error) -> bool {
    matches!(error.raw_os_error(), Some(libc::EXDEV))
}

fn unique_destination_path(base_dir: &Path, file_name: &OsStr) -> PathBuf {
    let candidate = base_dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("clip");
    let extension = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str());

    for index in 2..10_000 {
        let name = match extension {
            Some(extension) if !extension.is_empty() => format!("{stem}-{index}.{extension}"),
            _ => format!("{stem}-{index}"),
        };
        let candidate = base_dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }

    base_dir.join(format!("{stem}-{}", Utc::now().timestamp_millis()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ClipEventContribution, ClipOrigin};

    fn sample_clip(path: &str, score: u32, days_old: i64) -> ClipRecord {
        let now = Utc::now();
        ClipRecord {
            id: 1,
            trigger_event_at: now - chrono::Duration::days(days_old),
            clip_start_at: now - chrono::Duration::days(days_old),
            clip_end_at: now - chrono::Duration::days(days_old) + chrono::Duration::seconds(30),
            saved_at: now - chrono::Duration::days(days_old),
            origin: ClipOrigin::Rule,
            profile_id: "default".into(),
            rule_id: "rule".into(),
            clip_duration_secs: 30,
            session_id: None,
            character_id: 1,
            world_id: 1,
            zone_id: None,
            facility_id: None,
            zone_name: None,
            facility_name: None,
            score,
            honu_session_id: None,
            path: Some(path.into()),
            thumbnail_path: None,
            file_size_bytes: None,
            favorited: false,
            overlap_count: 0,
            alert_count: 0,
            collection_count: 0,
            collection_sequence_index: None,
            post_process_status: crate::db::PostProcessStatus::Legacy,
            post_process_error: None,
            tags: Vec::new(),
            events: vec![ClipEventContribution {
                event_kind: "kill".into(),
                occurrences: 1,
                points: score,
            }],
        }
    }

    #[test]
    fn archive_plan_requires_age_and_score_threshold() {
        let config = StorageTieringConfig {
            enabled: true,
            tier_directory: PathBuf::from("/archive"),
            min_age_days: 7,
            max_score: 50,
        };
        let clip = sample_clip("/clips/example.mkv", 60, 10);
        let candidate = tiering_candidate_from_clip(&clip).unwrap();

        assert!(plan_archive_move(&config, Path::new("/clips"), Utc::now(), &candidate).is_none());
    }

    #[test]
    fn restore_plan_moves_archive_clip_back_to_primary() {
        let config = StorageTieringConfig {
            enabled: true,
            tier_directory: PathBuf::from("/clips/archive"),
            min_age_days: 7,
            max_score: 50,
        };
        let clip = sample_clip("/clips/archive/example.mkv", 10, 30);
        let candidate = tiering_candidate_from_clip(&clip).unwrap();
        let plan = plan_restore_move(&config, Path::new("/clips"), &candidate).unwrap();

        assert_eq!(plan.target_tier, StorageTier::Primary);
        assert_eq!(plan.destination_path, PathBuf::from("/clips/example.mkv"));
    }

    #[test]
    fn favorited_clips_are_not_tiering_candidates() {
        let mut clip = sample_clip("/clips/favorite.mkv", 10, 30);
        clip.favorited = true;

        assert!(tiering_candidate_from_clip(&clip).is_none());
    }
}
