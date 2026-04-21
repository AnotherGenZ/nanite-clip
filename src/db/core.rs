use super::*;

pub(crate) fn entity_table<E>(entity: E) -> TableCreateStatement
where
    E: EntityTrait,
{
    let schema = Schema::new(DbBackend::Sqlite);
    let mut table = schema.create_table_from_entity(entity);
    table.if_not_exists();
    table
}

#[allow(dead_code)]
pub(crate) fn entity_indexes<E>(entity: E) -> Vec<IndexCreateStatement>
where
    E: EntityTrait,
{
    let schema = Schema::new(DbBackend::Sqlite);
    schema
        .create_index_from_entity(entity)
        .into_iter()
        .map(|mut index| {
            index.if_not_exists();
            index
        })
        .collect()
}

pub(crate) fn create_clip_events_table() -> TableCreateStatement {
    let mut table = entity_table(entities::clip_events::Entity);
    table.foreign_key(
        ForeignKey::create()
            .name("fk_clip_events_clip_id")
            .from(
                entities::clip_events::Entity,
                entities::clip_events::Column::ClipId,
            )
            .to(entities::clips::Entity, entities::clips::Column::Id)
            .on_delete(ForeignKeyAction::Cascade),
    );
    table
}

pub(crate) fn create_clip_raw_events_table() -> TableCreateStatement {
    let mut table = entity_table(entities::clip_raw_events::Entity);
    table.foreign_key(
        ForeignKey::create()
            .name("fk_clip_raw_events_clip_id")
            .from(
                entities::clip_raw_events::Entity,
                entities::clip_raw_events::Column::ClipId,
            )
            .to(entities::clips::Entity, entities::clips::Column::Id)
            .on_delete(ForeignKeyAction::Cascade),
    );
    table
}

pub(crate) fn create_clip_uploads_table() -> TableCreateStatement {
    let mut table = entity_table(entities::clip_uploads::Entity);
    table.foreign_key(
        ForeignKey::create()
            .name("fk_clip_uploads_clip_id")
            .from(
                entities::clip_uploads::Entity,
                entities::clip_uploads::Column::ClipId,
            )
            .to(entities::clips::Entity, entities::clips::Column::Id)
            .on_delete(ForeignKeyAction::Cascade),
    );
    table
}

pub(crate) fn create_clip_audio_tracks_table() -> TableCreateStatement {
    let mut table = entity_table(entities::clip_audio_tracks::Entity);
    table.foreign_key(
        ForeignKey::create()
            .name("fk_clip_audio_tracks_clip_id")
            .from(
                entities::clip_audio_tracks::Entity,
                entities::clip_audio_tracks::Column::ClipId,
            )
            .to(entities::clips::Entity, entities::clips::Column::Id)
            .on_delete(ForeignKeyAction::Cascade),
    );
    table
}

pub(crate) fn create_clip_overlaps_table() -> TableCreateStatement {
    let mut table = entity_table(entities::clip_overlaps::Entity);
    table
        .foreign_key(
            ForeignKey::create()
                .name("fk_clip_overlaps_clip_id")
                .from(
                    entities::clip_overlaps::Entity,
                    entities::clip_overlaps::Column::ClipId,
                )
                .to(entities::clips::Entity, entities::clips::Column::Id)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_clip_overlaps_overlap_clip_id")
                .from(
                    entities::clip_overlaps::Entity,
                    entities::clip_overlaps::Column::OverlapClipId,
                )
                .to(entities::clips::Entity, entities::clips::Column::Id)
                .on_delete(ForeignKeyAction::Cascade),
        );
    table
}

pub(crate) fn create_clip_tags_table() -> TableCreateStatement {
    let mut table = entity_table(entities::clip_tags::Entity);
    table.foreign_key(
        ForeignKey::create()
            .name("fk_clip_tags_clip_id")
            .from(
                entities::clip_tags::Entity,
                entities::clip_tags::Column::ClipId,
            )
            .to(entities::clips::Entity, entities::clips::Column::Id)
            .on_delete(ForeignKeyAction::Cascade),
    );
    table
}

pub(crate) fn create_clip_alert_links_table() -> TableCreateStatement {
    let mut table = entity_table(entities::clip_alert_links::Entity);
    table.foreign_key(
        ForeignKey::create()
            .name("fk_clip_alert_links_clip_id")
            .from(
                entities::clip_alert_links::Entity,
                entities::clip_alert_links::Column::ClipId,
            )
            .to(entities::clips::Entity, entities::clips::Column::Id)
            .on_delete(ForeignKeyAction::Cascade),
    );
    table
}

pub(crate) fn create_collection_clips_table() -> TableCreateStatement {
    let mut table = entity_table(entities::collection_clips::Entity);
    table
        .foreign_key(
            ForeignKey::create()
                .name("fk_collection_clips_collection_id")
                .from(
                    entities::collection_clips::Entity,
                    entities::collection_clips::Column::CollectionId,
                )
                .to(
                    entities::collections::Entity,
                    entities::collections::Column::Id,
                )
                .on_delete(ForeignKeyAction::Cascade),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_collection_clips_clip_id")
                .from(
                    entities::collection_clips::Entity,
                    entities::collection_clips::Column::ClipId,
                )
                .to(entities::clips::Entity, entities::clips::Column::Id)
                .on_delete(ForeignKeyAction::Cascade),
        );
    table
}

pub(crate) fn create_montage_clips_table() -> TableCreateStatement {
    let mut table = entity_table(entities::montage_clips::Entity);
    table
        .foreign_key(
            ForeignKey::create()
                .name("fk_montage_clips_montage_id")
                .from(
                    entities::montage_clips::Entity,
                    entities::montage_clips::Column::MontageId,
                )
                .to(entities::montages::Entity, entities::montages::Column::Id)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_montage_clips_clip_id")
                .from(
                    entities::montage_clips::Entity,
                    entities::montage_clips::Column::ClipId,
                )
                .to(entities::clips::Entity, entities::clips::Column::Id)
                .on_delete(ForeignKeyAction::Cascade),
        );
    table
}

pub(crate) fn create_clip_uploads_clip_started_index() -> IndexCreateStatement {
    let mut index = Index::create();
    index
        .name("idx_clip_uploads_clip_id")
        .table(entities::clip_uploads::Entity)
        .if_not_exists()
        .col(entities::clip_uploads::Column::ClipId)
        .col(entities::clip_uploads::Column::StartedTs);
    index.to_owned()
}

pub(crate) fn create_clip_uploads_provider_state_index() -> IndexCreateStatement {
    let mut index = Index::create();
    index
        .name("idx_clip_uploads_provider")
        .table(entities::clip_uploads::Entity)
        .if_not_exists()
        .col(entities::clip_uploads::Column::Provider)
        .col(entities::clip_uploads::Column::State);
    index.to_owned()
}

pub(crate) fn create_clip_audio_tracks_clip_stream_index() -> IndexCreateStatement {
    let mut index = Index::create();
    index
        .name("idx_clip_audio_tracks_clip_stream")
        .table(entities::clip_audio_tracks::Entity)
        .if_not_exists()
        .col(entities::clip_audio_tracks::Column::ClipId)
        .col(entities::clip_audio_tracks::Column::StreamIndex);
    index.to_owned()
}

pub(crate) fn create_background_jobs_state_updated_index() -> IndexCreateStatement {
    let mut index = Index::create();
    index
        .name("idx_background_jobs_state")
        .table(entities::background_jobs::Entity)
        .if_not_exists()
        .col(entities::background_jobs::Column::State)
        .col(entities::background_jobs::Column::UpdatedTs);
    index.to_owned()
}

pub(crate) fn create_alert_instances_zone_world_index() -> IndexCreateStatement {
    let mut index = Index::create();
    index
        .name("idx_alert_instances_zone_id")
        .table(entities::alert_instances::Entity)
        .if_not_exists()
        .col(entities::alert_instances::Column::ZoneId)
        .col(entities::alert_instances::Column::WorldId);
    index.to_owned()
}

pub(crate) fn create_weapon_reference_cache_category_index() -> IndexCreateStatement {
    let mut index = Index::create();
    index
        .name("idx_weapon_reference_cache_category")
        .table(entities::weapon_reference_cache::Entity)
        .if_not_exists()
        .col(entities::weapon_reference_cache::Column::CategoryLabel)
        .col(entities::weapon_reference_cache::Column::DisplayName);
    index.to_owned()
}

pub(crate) fn create_montage_clips_clip_sequence_index() -> IndexCreateStatement {
    let mut index = Index::create();
    index
        .name("idx_montage_clips_clip_id")
        .table(entities::montage_clips::Entity)
        .if_not_exists()
        .col(entities::montage_clips::Column::ClipId)
        .col(entities::montage_clips::Column::SequenceIndex);
    index.to_owned()
}

pub(crate) fn create_clip_tags_clip_tag_unique_index() -> IndexCreateStatement {
    let mut index = Index::create();
    index
        .name("idx_clip_tags_clip_tag")
        .table(entities::clip_tags::Entity)
        .if_not_exists()
        .col(entities::clip_tags::Column::ClipId)
        .col(entities::clip_tags::Column::TagName)
        .unique();
    index.to_owned()
}

pub(crate) fn create_collections_name_unique_index() -> IndexCreateStatement {
    let mut index = Index::create();
    index
        .name("idx_collections_name")
        .table(entities::collections::Entity)
        .if_not_exists()
        .col(entities::collections::Column::Name)
        .unique();
    index.to_owned()
}

pub(crate) fn create_collection_clips_clip_sequence_index() -> IndexCreateStatement {
    let mut index = Index::create();
    index
        .name("idx_collection_clips_clip_id")
        .table(entities::collection_clips::Entity)
        .if_not_exists()
        .col(entities::collection_clips::Column::ClipId)
        .col(entities::collection_clips::Column::SequenceIndex);
    index.to_owned()
}

pub(crate) fn create_collection_clips_collection_sequence_index() -> IndexCreateStatement {
    let mut index = Index::create();
    index
        .name("idx_collection_clips_collection_sequence")
        .table(entities::collection_clips::Entity)
        .if_not_exists()
        .col(entities::collection_clips::Column::CollectionId)
        .col(entities::collection_clips::Column::SequenceIndex);
    index.to_owned()
}

impl From<ClipRecord> for ClipExportRecord {
    fn from(record: ClipRecord) -> Self {
        Self {
            id: record.id,
            trigger_event_at: record.trigger_event_at.to_rfc3339(),
            clip_start_at: record.clip_start_at.to_rfc3339(),
            clip_end_at: record.clip_end_at.to_rfc3339(),
            saved_at: record.saved_at.to_rfc3339(),
            origin: record.origin.as_str().into(),
            profile_id: record.profile_id,
            rule_id: record.rule_id,
            clip_duration_secs: record.clip_duration_secs,
            session_id: record.session_id,
            character_id: record.character_id,
            world_id: record.world_id,
            zone_id: record.zone_id,
            facility_id: record.facility_id,
            score: record.score,
            honu_session_id: record.honu_session_id,
            path: record.path,
            file_size_bytes: record.file_size_bytes,
            favorited: record.favorited,
            tags: record.tags,
            events: record.events,
        }
    }
}

pub(crate) fn database_path() -> PathBuf {
    directories::ProjectDirs::from("", "", "nanite-clip")
        .map(|dirs| dirs.data_local_dir().join("clips.sqlite3"))
        .unwrap_or_else(|| PathBuf::from("nanite-clip-clips.sqlite3"))
}

pub(crate) fn next_migration_backup_path(
    database_path: &Path,
    current_version: i64,
    target_version: i64,
) -> PathBuf {
    let parent = database_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = database_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("clips");
    let extension = database_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("sqlite3");
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    let base_name =
        format!("{stem}.pre-migration-v{current_version}-to-v{target_version}-{timestamp}");

    let mut candidate = parent.join(format!("{base_name}.{extension}"));
    let mut suffix = 2_u32;
    while candidate.exists() {
        candidate = parent.join(format!("{base_name}-{suffix}.{extension}"));
        suffix += 1;
    }

    candidate
}

pub(crate) fn validate_output_destination(destination: &Path) -> Result<(), ClipStoreError> {
    if destination.as_os_str().is_empty() {
        return Err(ClipStoreError::InvalidOutputPath(
            "destination path cannot be empty".into(),
        ));
    }

    let Some(parent) = destination.parent() else {
        return Err(ClipStoreError::InvalidOutputPath(format!(
            "destination {} has no parent directory",
            destination.display()
        )));
    };

    if !parent.exists() {
        return Err(ClipStoreError::InvalidOutputPath(format!(
            "destination directory {} does not exist",
            parent.display()
        )));
    }

    if !parent.is_dir() {
        return Err(ClipStoreError::InvalidOutputPath(format!(
            "destination parent {} is not a directory",
            parent.display()
        )));
    }

    if destination.is_dir() {
        return Err(ClipStoreError::InvalidOutputPath(format!(
            "destination {} is a directory",
            destination.display()
        )));
    }

    Ok(())
}

pub(crate) fn atomic_write(destination: &Path, contents: &[u8]) -> Result<(), ClipStoreError> {
    let temp_path = destination.with_extension(format!(
        "{}.tmp",
        destination
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("write")
    ));
    std::fs::write(&temp_path, contents)?;
    std::fs::rename(temp_path, destination)?;
    Ok(())
}

pub(crate) fn append_csv_row(output: &mut String, columns: &[String]) {
    for (index, column) in columns.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push('"');
        output.push_str(&column.replace('"', "\"\""));
        output.push('"');
    }
    output.push('\n');
}

#[allow(dead_code)]
pub(crate) fn rows_to_clip_records(
    rows: Vec<primitives::Row>,
) -> Result<Vec<ClipRecord>, ClipStoreError> {
    let mut records = Vec::new();

    for row in rows {
        let clip_id = row.try_get("id")?;
        if records
            .last()
            .is_none_or(|record: &ClipRecord| record.id != clip_id)
        {
            let path = row.try_get::<Option<String>>("path")?;
            records.push(ClipRecord {
                id: clip_id,
                trigger_event_at: timestamp_millis_to_utc(row.try_get("trigger_event_ts")?)?,
                clip_start_at: timestamp_millis_to_utc(row.try_get("clip_start_ts")?)?,
                clip_end_at: timestamp_millis_to_utc(row.try_get("clip_end_ts")?)?,
                saved_at: timestamp_millis_to_utc(row.try_get("saved_ts")?)?,
                origin: ClipOrigin::from_db(row.try_get::<String>("clip_origin")?.as_str()),
                profile_id: row.try_get("profile_id")?,
                rule_id: row.try_get("rule_id")?,
                clip_duration_secs: row.try_get::<i64>("clip_duration_secs")? as u32,
                session_id: row.try_get("session_id")?,
                character_id: row.try_get::<i64>("character_id")? as u64,
                world_id: row.try_get::<i64>("world_id")? as u32,
                zone_id: row.try_get::<Option<i64>>("zone_id")?.map(|id| id as u32),
                facility_id: row
                    .try_get::<Option<i64>>("facility_id")?
                    .map(|id| id as u32),
                zone_name: row.try_get("zone_name")?,
                facility_name: row.try_get("facility_name")?,
                score: row.try_get::<i64>("score")? as u32,
                honu_session_id: row.try_get::<Option<i64>>("honu_session_id")?,
                file_size_bytes: file_size_bytes_for_path(path.as_deref()),
                favorited: row.try_get("favorited")?,
                overlap_count: row.try_get::<i64>("overlap_count")? as u32,
                alert_count: row.try_get::<i64>("alert_count")? as u32,
                collection_count: row.try_get::<Option<i64>>("collection_count")?.unwrap_or(0)
                    as u32,
                collection_sequence_index: row
                    .try_get::<Option<i64>>("collection_sequence_index")?
                    .map(|value| value as u32),
                post_process_status: PostProcessStatus::Legacy,
                post_process_error: None,
                path,
                tags: Vec::new(),
                events: Vec::new(),
            });
        }

        if row.try_get::<Option<i64>>("clip_event_id")?.is_some() {
            let record = records
                .last_mut()
                .expect("clip record must exist before clip events are attached");
            record.events.push(ClipEventContribution {
                event_kind: row.try_get("event_kind")?,
                occurrences: row.try_get::<i64>("occurrences")? as u32,
                points: row.try_get::<i64>("points")? as u32,
            });
        }
    }

    Ok(records)
}

#[allow(dead_code)]
pub(crate) fn rows_to_clip_alerts(
    rows: Vec<primitives::Row>,
) -> Result<Vec<ClipAlertRecord>, ClipStoreError> {
    rows.into_iter()
        .map(|row| {
            Ok(ClipAlertRecord {
                alert_key: row.try_get("alert_key")?,
                label: row.try_get("label")?,
                world_id: row.try_get::<i64>("world_id")? as u32,
                zone_id: row.try_get::<i64>("zone_id")? as u32,
                metagame_event_id: row.try_get::<i64>("metagame_event_id")? as u8,
                started_at: timestamp_millis_to_utc(row.try_get("started_ts")?)?,
                ended_at: row
                    .try_get::<Option<i64>>("ended_ts")?
                    .map(timestamp_millis_to_utc)
                    .transpose()?,
                state_name: row.try_get("state_name")?,
                winner_faction: row.try_get("winner_faction")?,
            })
        })
        .collect()
}

#[allow(dead_code)]
pub(crate) fn rows_to_clip_overlaps(
    rows: Vec<primitives::Row>,
) -> Result<Vec<ClipOverlapRecord>, ClipStoreError> {
    rows.into_iter()
        .map(|row| {
            Ok(ClipOverlapRecord {
                clip_id: row.try_get("id")?,
                trigger_event_at: timestamp_millis_to_utc(row.try_get("trigger_event_ts")?)?,
                clip_start_at: timestamp_millis_to_utc(row.try_get("clip_start_ts")?)?,
                clip_end_at: timestamp_millis_to_utc(row.try_get("clip_end_ts")?)?,
                profile_id: row.try_get("profile_id")?,
                rule_id: row.try_get("rule_id")?,
                path: row.try_get("path")?,
                overlap_duration_ms: row.try_get("overlap_duration_ms")?,
            })
        })
        .collect()
}

#[allow(dead_code)]
pub(crate) fn rows_to_clip_uploads(
    rows: Vec<primitives::Row>,
) -> Result<Vec<ClipUploadRecord>, ClipStoreError> {
    rows.into_iter()
        .map(|row| {
            Ok(ClipUploadRecord {
                id: row.try_get("id")?,
                provider: UploadProvider::from_db(row.try_get::<String>("provider")?.as_str()),
                state: ClipUploadState::from_db(row.try_get::<String>("state")?.as_str()),
                external_id: row.try_get("external_id")?,
                clip_url: row.try_get("clip_url")?,
                error_message: row.try_get("error_message")?,
                started_at: timestamp_millis_to_utc(row.try_get("started_ts")?)?,
                updated_at: timestamp_millis_to_utc(row.try_get("updated_ts")?)?,
                completed_at: row
                    .try_get::<Option<i64>>("completed_ts")?
                    .map(timestamp_millis_to_utc)
                    .transpose()?,
            })
        })
        .collect()
}

#[allow(dead_code)]
pub(crate) fn rows_to_background_jobs(
    rows: Vec<primitives::Row>,
) -> Result<Vec<BackgroundJobRecord>, ClipStoreError> {
    rows.into_iter()
        .map(|row| {
            let related_clip_ids_json: String = row.try_get("related_clip_ids_json")?;
            let related_clip_ids: Vec<i64> = serde_json::from_str(&related_clip_ids_json)?;
            let progress_current_step = row.try_get::<Option<i64>>("progress_current_step")?;
            let progress_total_steps = row.try_get::<Option<i64>>("progress_total_steps")?;
            let progress_message = row.try_get::<Option<String>>("progress_message")?;
            let progress = match (
                progress_current_step,
                progress_total_steps,
                progress_message,
            ) {
                (Some(current_step), Some(total_steps), Some(message)) => {
                    Some(BackgroundJobProgress {
                        current_step: std::cmp::max(current_step, 0) as u32,
                        total_steps: std::cmp::max(total_steps, 1) as u32,
                        message,
                    })
                }
                _ => None,
            };

            Ok(BackgroundJobRecord {
                id: BackgroundJobId(row.try_get::<i64>("id")? as u64),
                kind: BackgroundJobKind::from_db(&row.try_get::<String>("kind")?),
                label: row.try_get("label")?,
                state: BackgroundJobState::from_db(&row.try_get::<String>("state")?),
                related_clip_ids,
                progress,
                started_at: timestamp_millis_to_utc(row.try_get("started_ts")?)?,
                updated_at: timestamp_millis_to_utc(row.try_get("updated_ts")?)?,
                finished_at: row
                    .try_get::<Option<i64>>("finished_ts")?
                    .map(timestamp_millis_to_utc)
                    .transpose()?,
                detail: row.try_get("detail")?,
                cancellable: row.try_get("cancellable")?,
            })
        })
        .collect()
}

pub(crate) fn background_job_from_model(
    model: entities::background_jobs::Model,
) -> Result<BackgroundJobRecord, ClipStoreError> {
    let related_clip_ids: Vec<i64> = serde_json::from_str(&model.related_clip_ids_json)?;
    let progress = match (
        model.progress_current_step,
        model.progress_total_steps,
        model.progress_message,
    ) {
        (Some(current_step), Some(total_steps), Some(message)) => Some(BackgroundJobProgress {
            current_step: std::cmp::max(current_step, 0) as u32,
            total_steps: std::cmp::max(total_steps, 1) as u32,
            message,
        }),
        _ => None,
    };

    Ok(BackgroundJobRecord {
        id: BackgroundJobId(model.id as u64),
        kind: BackgroundJobKind::from_db(&model.kind),
        label: model.label,
        state: BackgroundJobState::from_db(&model.state),
        related_clip_ids,
        progress,
        started_at: timestamp_millis_to_utc(model.started_ts)?,
        updated_at: timestamp_millis_to_utc(model.updated_ts)?,
        finished_at: model.finished_ts.map(timestamp_millis_to_utc).transpose()?,
        detail: model.detail,
        cancellable: model.cancellable,
    })
}

pub(crate) fn interrupted_background_job_detail(existing_detail: Option<String>) -> String {
    match existing_detail {
        Some(detail) if !detail.trim().is_empty() => {
            format!("{detail} {INTERRUPTED_BACKGROUND_JOB_DETAIL}")
        }
        _ => INTERRUPTED_BACKGROUND_JOB_DETAIL.to_string(),
    }
}

#[allow(dead_code)]
pub(crate) fn raw_rows_to_clip_raw_events(
    rows: Vec<primitives::Row>,
) -> Result<Vec<ClipRawEventRecord>, ClipStoreError> {
    rows.into_iter()
        .map(|row| {
            Ok(ClipRawEventRecord {
                event_at: timestamp_millis_to_utc(row.try_get("event_ts")?)?,
                event_kind: row.try_get("event_kind")?,
                world_id: row.try_get::<i64>("world_id")? as u32,
                zone_id: row.try_get::<Option<i64>>("zone_id")?.map(|id| id as u32),
                zone_name: row.try_get("zone_name")?,
                facility_id: row
                    .try_get::<Option<i64>>("facility_id")?
                    .map(|id| id as u32),
                facility_name: row.try_get("facility_name")?,
                actor_character_id: row
                    .try_get::<Option<i64>>("actor_character_id")?
                    .map(|id| id as u64),
                actor_character_name: row.try_get("actor_character_name")?,
                other_character_id: row
                    .try_get::<Option<i64>>("other_character_id")?
                    .map(|id| id as u64),
                other_character_name: row.try_get("other_character_name")?,
                actor_class: row.try_get("actor_class")?,
                attacker_weapon_id: row
                    .try_get::<Option<i64>>("attacker_weapon_id")?
                    .map(|id| id as u32),
                attacker_weapon_name: row.try_get("attacker_weapon_name")?,
                attacker_vehicle_id: row
                    .try_get::<Option<i64>>("attacker_vehicle_id")?
                    .map(|id| id as u16),
                attacker_vehicle_name: row.try_get("attacker_vehicle_name")?,
                vehicle_killed_id: row
                    .try_get::<Option<i64>>("vehicle_killed_id")?
                    .map(|id| id as u16),
                vehicle_killed_name: row.try_get("vehicle_killed_name")?,
                characters_killed: row.try_get::<i64>("characters_killed")? as u32,
                is_headshot: row.try_get("is_headshot")?,
                experience_id: row
                    .try_get::<Option<i64>>("experience_id")?
                    .map(|id| id as u16),
            })
        })
        .collect()
}

pub(crate) fn read_count_rows(
    rows: Vec<primitives::Row>,
) -> Result<Vec<CountByLabel>, ClipStoreError> {
    rows.into_iter()
        .map(|row| {
            Ok(CountByLabel {
                label: row.try_get::<String>("label")?,
                count: row.try_get::<i64>("count")? as u32,
            })
        })
        .collect()
}

pub(crate) fn read_base_count_rows(
    rows: Vec<primitives::Row>,
) -> Result<Vec<BaseCount>, ClipStoreError> {
    rows.into_iter()
        .map(|row| {
            Ok(BaseCount {
                facility_id: row
                    .try_get::<Option<i64>>("facility_id")?
                    .map(|id| id as u32),
                label: row.try_get::<String>("label")?,
                count: row.try_get::<i64>("count")? as u32,
            })
        })
        .collect()
}

pub(crate) fn read_string_option_rows(
    rows: Vec<primitives::Row>,
) -> Result<Vec<String>, ClipStoreError> {
    rows.into_iter()
        .map(|row| row.try_get::<String>("label").map_err(ClipStoreError::from))
        .collect()
}

pub(crate) fn file_size_bytes_for_path(path: Option<&str>) -> Option<u64> {
    std::fs::metadata(Path::new(path?))
        .ok()
        .map(|metadata| metadata.len())
}

pub(crate) fn timestamp_millis_to_utc(value: i64) -> Result<DateTime<Utc>, ClipStoreError> {
    DateTime::<Utc>::from_timestamp_millis(value).ok_or(ClipStoreError::InvalidTimestamp(value))
}

#[derive(Debug, thiserror::Error)]
pub enum ClipStoreError {
    #[error("failed to prepare clip database directory: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite query failed: {0}")]
    Sqlx(#[from] primitives::Error),
    #[error("serialization failed: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("invalid timestamp in clip database: {0}")]
    InvalidTimestamp(i64),
    #[error("unsupported clip database schema version: {0}")]
    UnsupportedSchemaVersion(i64),
    #[error("invalid output path: {0}")]
    InvalidOutputPath(String),
    #[error("automatic pre-migration backup failed: {0}")]
    MigrationBackupFailed(String),
    #[error("invalid clip organization input: {0}")]
    InvalidInput(String),
}

impl From<DbErr> for ClipStoreError {
    fn from(value: DbErr) -> Self {
        Self::Sqlx(primitives::Error::from(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn open_test_store() -> ClipStore {
        ClipStore::open_in_memory().await.unwrap()
    }

    fn sample_clip() -> ClipDraft {
        let event_at = DateTime::<Utc>::from_timestamp(1_710_000_000, 0).unwrap();
        ClipDraft {
            trigger_event_at: event_at,
            clip_start_at: event_at - chrono::Duration::seconds(30),
            clip_end_at: event_at,
            saved_at: event_at,
            origin: ClipOrigin::Rule,
            profile_id: "profile_1".into(),
            rule_id: "rule_kill_streak".into(),
            clip_duration_secs: 30,
            session_id: Some("session-1".into()),
            character_id: 42,
            world_id: 17,
            zone_id: Some(2),
            facility_id: Some(1234),
            score: 9,
            honu_session_id: None,
            path: None,
            alert_keys: Vec::new(),
            events: vec![
                ClipEventContribution {
                    event_kind: "Headshot".into(),
                    occurrences: 1,
                    points: 3,
                },
                ClipEventContribution {
                    event_kind: "Kill".into(),
                    occurrences: 3,
                    points: 6,
                },
            ],
            raw_events: vec![ClipRawEventDraft {
                event_at,
                event_kind: "Kill".into(),
                world_id: 17,
                zone_id: Some(2),
                facility_id: Some(1234),
                actor_character_id: Some(42),
                other_character_id: Some(100),
                actor_class: Some("Heavy Assault".into()),
                attacker_weapon_id: Some(80),
                attacker_vehicle_id: None,
                vehicle_killed_id: None,
                characters_killed: 1,
                is_headshot: false,
                experience_id: None,
            }],
        }
    }

    #[tokio::test]
    async fn inserts_and_reads_recent_clips() {
        let store = open_test_store().await;
        store.insert_clip(sample_clip()).await.unwrap();

        let clips = store.recent_clips(10).await.unwrap();
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].origin, ClipOrigin::Rule);
        assert_eq!(clips[0].profile_id, "profile_1");
        assert_eq!(clips[0].rule_id, "rule_kill_streak");
        assert_eq!(clips[0].character_id, 42);
        assert_eq!(clips[0].world_id, 17);
        assert_eq!(clips[0].path, None);
        assert_eq!(clips[0].events.len(), 2);
    }

    #[tokio::test]
    async fn filters_by_trigger_timestamp() {
        let store = open_test_store().await;
        let clip = sample_clip();
        let event_at = clip.trigger_event_at;
        store.insert_clip(clip).await.unwrap();

        let filters = ClipFilters {
            event_after_ts: Some(event_at.timestamp_millis()),
            event_before_ts: Some(event_at.timestamp_millis()),
            ..ClipFilters::default()
        };
        let clips = store.search_clips(&filters, 10).await.unwrap();

        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].facility_id, Some(1234));
    }

    #[tokio::test]
    async fn searches_raw_event_target_and_weapon_filters() {
        let store = open_test_store().await;
        store
            .store_lookup(LookupKind::Character, 100, "Enemy Example")
            .await
            .unwrap();
        store
            .store_lookup(LookupKind::Weapon, 80, "Gauss Rifle")
            .await
            .unwrap();
        store.insert_clip(sample_clip()).await.unwrap();

        let target_hits = store
            .search_clips(
                &ClipFilters {
                    target: "Enemy Example".into(),
                    ..ClipFilters::default()
                },
                10,
            )
            .await
            .unwrap();
        assert_eq!(target_hits.len(), 1);

        let weapon_hits = store
            .search_clips(
                &ClipFilters {
                    weapon: "Gauss".into(),
                    ..ClipFilters::default()
                },
                10,
            )
            .await
            .unwrap();
        assert_eq!(weapon_hits.len(), 1);

        let options = store.raw_event_filter_options().await.unwrap();
        assert!(options.targets.contains(&"Enemy Example".to_string()));
        assert!(options.weapons.contains(&"Gauss Rifle".to_string()));
    }

    #[tokio::test]
    async fn overlap_detection_flags_partial_overlaps_and_filters_them() {
        let store = open_test_store().await;

        let first_id = store.insert_clip(sample_clip()).await.unwrap();

        let mut overlapping = sample_clip();
        overlapping.trigger_event_at += chrono::Duration::seconds(20);
        overlapping.clip_start_at += chrono::Duration::seconds(20);
        overlapping.clip_end_at += chrono::Duration::seconds(20);
        overlapping.saved_at += chrono::Duration::seconds(20);
        overlapping.rule_id = "rule_followup".into();
        let second_id = store.insert_clip(overlapping).await.unwrap();

        let recent = store.recent_clips(10).await.unwrap();
        assert_eq!(recent.len(), 2);
        assert!(recent.iter().all(|clip| clip.overlap_count == 1));

        let detail = store.clip_detail(first_id).await.unwrap().unwrap();
        assert_eq!(detail.overlaps.len(), 1);
        assert_eq!(detail.overlaps[0].clip_id, second_id);
        assert_eq!(detail.overlaps[0].overlap_duration_ms, 10_000);

        let overlapping_only = store
            .search_clips(
                &ClipFilters {
                    overlap_state: OverlapFilterState::Overlapping,
                    ..ClipFilters::default()
                },
                10,
            )
            .await
            .unwrap();
        assert_eq!(overlapping_only.len(), 2);

        let unique_only = store
            .search_clips(
                &ClipFilters {
                    overlap_state: OverlapFilterState::UniqueOnly,
                    ..ClipFilters::default()
                },
                10,
            )
            .await
            .unwrap();
        assert!(unique_only.is_empty());
    }

    #[tokio::test]
    async fn exact_clip_boundaries_do_not_count_as_overlap() {
        let store = open_test_store().await;

        store.insert_clip(sample_clip()).await.unwrap();

        let mut adjacent = sample_clip();
        adjacent.trigger_event_at += chrono::Duration::seconds(30);
        adjacent.clip_start_at += chrono::Duration::seconds(30);
        adjacent.clip_end_at += chrono::Duration::seconds(30);
        adjacent.saved_at += chrono::Duration::seconds(30);
        adjacent.rule_id = "rule_adjacent".into();
        store.insert_clip(adjacent).await.unwrap();

        let recent = store.recent_clips(10).await.unwrap();
        assert_eq!(recent.len(), 2);
        assert!(recent.iter().all(|clip| clip.overlap_count == 0));

        let unique_only = store
            .search_clips(
                &ClipFilters {
                    overlap_state: OverlapFilterState::UniqueOnly,
                    ..ClipFilters::default()
                },
                10,
            )
            .await
            .unwrap();
        assert_eq!(unique_only.len(), 2);
    }

    #[tokio::test]
    async fn alert_links_support_filtering_and_late_outcome_updates() {
        let store = open_test_store().await;
        let started_at = DateTime::<Utc>::from_timestamp(1_710_000_000 - 600, 0).unwrap();
        let alert_key = "17-2-1".to_string();

        store
            .upsert_alert(&AlertInstanceRecord {
                alert_key: alert_key.clone(),
                label: "Indar Meltdown".into(),
                world_id: 17,
                zone_id: 2,
                metagame_event_id: 1,
                started_at,
                ended_at: None,
                state_name: "started".into(),
                winner_faction: None,
                faction_nc: 33.0,
                faction_tr: 34.0,
                faction_vs: 33.0,
            })
            .await
            .unwrap();

        let mut clip = sample_clip();
        clip.alert_keys = vec![alert_key.clone()];
        let clip_id = store.insert_clip(clip).await.unwrap();

        let hits = store
            .search_clips(
                &ClipFilters {
                    alert: "meltdown".into(),
                    ..ClipFilters::default()
                },
                10,
            )
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].alert_count, 1);

        let options = store.raw_event_filter_options().await.unwrap();
        assert!(options.alerts.contains(&"Indar Meltdown".to_string()));

        store
            .upsert_alert(&AlertInstanceRecord {
                alert_key: alert_key.clone(),
                label: "Indar Meltdown".into(),
                world_id: 17,
                zone_id: 2,
                metagame_event_id: 1,
                started_at,
                ended_at: Some(started_at + chrono::Duration::minutes(90)),
                state_name: "ended".into(),
                winner_faction: Some("VS".into()),
                faction_nc: 20.0,
                faction_tr: 25.0,
                faction_vs: 55.0,
            })
            .await
            .unwrap();

        let detail = store.clip_detail(clip_id).await.unwrap().unwrap();
        assert_eq!(detail.alerts.len(), 1);
        assert_eq!(detail.alerts[0].winner_faction.as_deref(), Some("VS"));
        assert_eq!(detail.alerts[0].state_name, "ended");
    }

    #[tokio::test]
    async fn favorites_tags_and_collections_round_trip_through_filters() {
        let store = open_test_store().await;
        let clip_id = store.insert_clip(sample_clip()).await.unwrap();

        store.add_tag(clip_id, "Outfit Ops").await.unwrap();
        store.set_clip_favorited(clip_id, true).await.unwrap();
        let collection = store
            .create_collection("Highlights", Some("Main set"))
            .await
            .unwrap();
        store
            .add_clip_to_collection(collection.id, clip_id)
            .await
            .unwrap();

        let filtered = store
            .search_clips(
                &ClipFilters {
                    tag: "ops".into(),
                    favorites_only: true,
                    collection_id: Some(collection.id),
                    ..ClipFilters::default()
                },
                10,
            )
            .await
            .unwrap();
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].favorited);
        assert_eq!(filtered[0].tags, vec!["Outfit Ops".to_string()]);
        assert_eq!(filtered[0].collection_count, 1);
        assert_eq!(filtered[0].collection_sequence_index, Some(0));

        let detail = store.clip_detail(clip_id).await.unwrap().unwrap();
        assert_eq!(detail.tags, vec!["Outfit Ops".to_string()]);
        assert_eq!(detail.collections.len(), 1);
        assert_eq!(detail.collections[0].name, "Highlights");
    }

    #[tokio::test]
    async fn tag_uniqueness_collection_ordering_and_clip_delete_cascade_hold() {
        let store = open_test_store().await;
        let first_id = store.insert_clip(sample_clip()).await.unwrap();

        let mut second = sample_clip();
        second.trigger_event_at += chrono::Duration::seconds(60);
        second.clip_start_at += chrono::Duration::seconds(60);
        second.clip_end_at += chrono::Duration::seconds(60);
        second.saved_at += chrono::Duration::seconds(60);
        second.rule_id = "rule_second".into();
        let second_id = store.insert_clip(second).await.unwrap();

        store.add_tag(first_id, "Focus").await.unwrap();
        store.add_tag(first_id, "Focus").await.unwrap();
        assert_eq!(store.list_tags().await.unwrap(), vec!["Focus".to_string()]);

        let collection = store.create_collection("Queue", None).await.unwrap();
        store
            .add_clips_to_collection(collection.id, &[second_id, first_id])
            .await
            .unwrap();
        store
            .move_clip_within_collection(collection.id, first_id, -1)
            .await
            .unwrap();

        let ordered = store
            .search_clips(
                &ClipFilters {
                    collection_id: Some(collection.id),
                    ..ClipFilters::default()
                },
                10,
            )
            .await
            .unwrap();
        assert_eq!(
            ordered.iter().map(|clip| clip.id).collect::<Vec<_>>(),
            vec![first_id, second_id]
        );

        store.delete_clip(first_id).await.unwrap();

        let remaining = store
            .search_clips(
                &ClipFilters {
                    collection_id: Some(collection.id),
                    ..ClipFilters::default()
                },
                10,
            )
            .await
            .unwrap();
        assert_eq!(
            remaining.iter().map(|clip| clip.id).collect::<Vec<_>>(),
            vec![second_id]
        );
        assert!(store.list_tags().await.unwrap().is_empty());
        assert_eq!(store.list_collections().await.unwrap()[0].clip_count, 1);
    }

    #[tokio::test]
    async fn caches_lookup_entries() {
        let store = open_test_store().await;

        assert!(
            store
                .cached_lookup(LookupKind::Facility, 100)
                .await
                .unwrap()
                .is_none()
        );

        store
            .store_lookup(LookupKind::Facility, 100, "The Crown")
            .await
            .unwrap();

        assert_eq!(
            store
                .cached_lookup(LookupKind::Facility, 100)
                .await
                .unwrap(),
            Some("The Crown".into())
        );
    }

    #[tokio::test]
    async fn stores_lookup_batches_and_finds_names_case_insensitively() {
        let store = open_test_store().await;

        store
            .store_lookups(
                LookupKind::Vehicle,
                &[
                    (4, "Flash".to_string()),
                    (5, "Sunderer".to_string()),
                    (6, "Lightning".to_string()),
                ],
            )
            .await
            .unwrap();

        let all = store.list_lookups(LookupKind::Vehicle).await.unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0], (4, "Flash".to_string()));

        let found = store
            .find_lookup_by_name(LookupKind::Vehicle, "sunderer")
            .await
            .unwrap();
        assert_eq!(found, Some((5, "Sunderer".to_string())));
    }

    #[tokio::test]
    async fn stores_and_lists_weapon_reference_cache_entries() {
        let store = open_test_store().await;

        store
            .store_weapon_references(&[
                WeaponReferenceCacheEntry {
                    item_id: 180,
                    weapon_id: 80,
                    display_name: "Gauss Rifle".into(),
                    category_label: "Assault Rifle".into(),
                    faction: Some(Faction::NC),
                    weapon_group_id: Some(4),
                },
                WeaponReferenceCacheEntry {
                    item_id: 281,
                    weapon_id: 81,
                    display_name: "Bishop".into(),
                    category_label: "Battle Rifle".into(),
                    faction: Some(Faction::NS),
                    weapon_group_id: Some(7),
                },
            ])
            .await
            .unwrap();

        let references = store.list_weapon_references().await.unwrap();
        assert_eq!(references.len(), 2);
        assert_eq!(references[0].category_label, "Assault Rifle");
        assert_eq!(references[0].display_name, "Gauss Rifle");
        assert_eq!(references[1].category_label, "Battle Rifle");

        assert_eq!(
            store.cached_lookup(LookupKind::Weapon, 281).await.unwrap(),
            Some("Bishop".into())
        );
    }

    #[tokio::test]
    async fn storing_weapon_references_replaces_previous_snapshot() {
        let store = open_test_store().await;

        store
            .store_weapon_references(&[WeaponReferenceCacheEntry {
                item_id: 180,
                weapon_id: 80,
                display_name: "Gauss Rifle".into(),
                category_label: "Assault Rifle".into(),
                faction: Some(Faction::NC),
                weapon_group_id: None,
            }])
            .await
            .unwrap();
        store
            .store_weapon_references(&[WeaponReferenceCacheEntry {
                item_id: 281,
                weapon_id: 81,
                display_name: "Bishop".into(),
                category_label: "Battle Rifle".into(),
                faction: Some(Faction::NS),
                weapon_group_id: None,
            }])
            .await
            .unwrap();

        let references = store.list_weapon_references().await.unwrap();
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].item_id, 281);
        assert_eq!(
            store.cached_lookup(LookupKind::Weapon, 180).await.unwrap(),
            None
        );
        assert_eq!(
            store.cached_lookup(LookupKind::Weapon, 281).await.unwrap(),
            Some("Bishop".into())
        );
    }

    #[tokio::test]
    async fn stores_and_reads_character_outfit_cache_entries() {
        let store = open_test_store().await;

        store
            .store_character_outfit(42, Some(77), Some("TAG"))
            .await
            .unwrap();

        let cached = store.cached_character_outfit(42).await.unwrap();
        assert_eq!(
            cached,
            Some(CharacterOutfitCacheEntry {
                outfit_id: Some(77),
                outfit_tag: Some("TAG".into()),
            })
        );

        store.store_character_outfit(42, None, None).await.unwrap();

        let cached = store.cached_character_outfit(42).await.unwrap();
        assert_eq!(
            cached,
            Some(CharacterOutfitCacheEntry {
                outfit_id: None,
                outfit_tag: None,
            })
        );
    }

    #[tokio::test]
    async fn cached_character_outfit_ignores_and_prunes_stale_entries() {
        let store = open_test_store().await;
        let stale_ts = Utc::now().timestamp_millis() - CHARACTER_OUTFIT_CACHE_TTL_MS - 1;

        primitives::query(
            r#"
            INSERT INTO character_outfit_cache (character_id, outfit_id, outfit_tag, resolved_ts)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(42_i64)
        .bind(77_i64)
        .bind("TAG")
        .bind(stale_ts)
        .execute(&store.pool)
        .await
        .unwrap();

        let cached = store.cached_character_outfit(42).await.unwrap();
        assert_eq!(cached, None);

        let remaining: i64 = primitives::query_scalar(
            "SELECT COUNT(*) FROM character_outfit_cache WHERE character_id = ?",
        )
        .bind(42_i64)
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(remaining, 0);
    }

    #[tokio::test]
    async fn storing_character_outfit_prunes_other_expired_rows() {
        let store = open_test_store().await;
        let stale_ts = Utc::now().timestamp_millis() - CHARACTER_OUTFIT_CACHE_TTL_MS - 1;

        primitives::query(
            r#"
            INSERT INTO character_outfit_cache (character_id, outfit_id, outfit_tag, resolved_ts)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(100_i64)
        .bind(7_i64)
        .bind("OLD")
        .bind(stale_ts)
        .execute(&store.pool)
        .await
        .unwrap();

        store
            .store_character_outfit(42, Some(77), Some("TAG"))
            .await
            .unwrap();

        let remaining: Vec<i64> = primitives::query_scalar(
            "SELECT character_id FROM character_outfit_cache ORDER BY character_id ASC",
        )
        .fetch_all(&store.pool)
        .await
        .unwrap();
        assert_eq!(remaining, vec![42]);
    }

    #[tokio::test]
    async fn updates_clip_path() {
        let store = open_test_store().await;
        let clip_id = store.insert_clip(sample_clip()).await.unwrap();
        let path = std::env::temp_dir().join(format!(
            "nanite-clip-test-{}-{}.mp4",
            std::process::id(),
            clip_id
        ));
        std::fs::write(&path, b"12345").unwrap();

        store
            .update_clip_path(clip_id, Some(path.to_str().unwrap()))
            .await
            .unwrap();

        let clips = store.recent_clips(10).await.unwrap();
        assert_eq!(clips[0].path.as_deref(), path.to_str());
        assert_eq!(clips[0].file_size_bytes, Some(5));

        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn clears_clip_path() {
        let store = open_test_store().await;
        let clip_id = store.insert_clip(sample_clip()).await.unwrap();

        store
            .update_clip_path(clip_id, Some("/tmp/nanite-clip-test.mp4"))
            .await
            .unwrap();
        store.update_clip_path(clip_id, None).await.unwrap();

        let clips = store.recent_clips(10).await.unwrap();
        assert_eq!(clips[0].path, None);
        assert_eq!(clips[0].file_size_bytes, None);
    }

    #[tokio::test]
    async fn deleting_clip_removes_associated_events() {
        let store = open_test_store().await;
        let clip_id = store.insert_clip(sample_clip()).await.unwrap();

        store.delete_clip(clip_id).await.unwrap();

        let clips = store.recent_clips(10).await.unwrap();
        assert!(clips.is_empty());

        let remaining_events: i64 = primitives::query_scalar("SELECT COUNT(*) FROM clip_events")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(remaining_events, 0);
    }

    #[tokio::test]
    async fn fresh_schema_populates_seaql_migrations() {
        let store = open_test_store().await;

        let migration_count: i64 =
            primitives::query_scalar("SELECT COUNT(*) FROM seaql_migrations")
                .fetch_one(&store.pool)
                .await
                .unwrap();
        assert_eq!(
            migration_count,
            migrations::Migrator::migrations().len() as i64
        );
    }

    #[tokio::test]
    async fn migrates_existing_database_to_clip_origin_column() {
        let temp_dir = std::env::temp_dir().join(format!(
            "nanite-clip-clips-migration-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("clips.sqlite3");

        let pool = primitives::connect_at(&db_path, 1).await.unwrap();

        for statement in [
            r#"
            CREATE TABLE clips (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                trigger_event_ts INTEGER NOT NULL,
                saved_ts INTEGER NOT NULL,
                rule_id TEXT NOT NULL,
                clip_duration_secs INTEGER NOT NULL,
                character_id INTEGER NOT NULL,
                world_id INTEGER NOT NULL,
                zone_id INTEGER,
                facility_id INTEGER,
                profile_id TEXT NOT NULL,
                path TEXT,
                score INTEGER NOT NULL,
                honu_session_id INTEGER
            )
            "#,
            r#"
            CREATE TABLE clip_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                clip_id INTEGER NOT NULL,
                event_kind TEXT NOT NULL,
                occurrences INTEGER NOT NULL,
                points INTEGER NOT NULL,
                FOREIGN KEY (clip_id) REFERENCES clips(id) ON DELETE CASCADE
            )
            "#,
            "PRAGMA user_version = 1",
        ] {
            primitives::query(statement).execute(&pool).await.unwrap();
        }
        drop(pool);

        let store = ClipStore::open_at(db_path).await.unwrap();
        let clips = store.recent_clips(10).await.unwrap();
        assert!(clips.is_empty());

        let clip_origin_column_exists: bool = primitives::query_scalar(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clips') WHERE name = 'clip_origin'",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert!(clip_origin_column_exists);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[tokio::test]
    async fn replays_migrations_when_entity_index_drift_is_detected() {
        let temp_dir = std::env::temp_dir().join(format!(
            "nanite-clip-index-drift-entity-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("clips.sqlite3");

        let store = ClipStore::open_at(&db_path).await.unwrap();
        drop(store);

        let pool = primitives::connect_at(&db_path, 1).await.unwrap();
        primitives::query("DROP INDEX IF EXISTS \"idx-clips-session_id\"")
            .execute(&pool)
            .await
            .unwrap();
        drop(pool);

        let store = ClipStore::open_at(&db_path).await.unwrap();
        let notice = store.startup_notice().unwrap_or_default().to_string();
        assert!(notice.contains("backup was created"));

        let index_exists: bool = primitives::query_scalar(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'index' AND name = 'idx-clips-session_id'",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert!(index_exists);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[tokio::test]
    async fn replays_migrations_when_named_index_drift_is_detected() {
        let temp_dir = std::env::temp_dir().join(format!(
            "nanite-clip-index-drift-named-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("clips.sqlite3");

        let store = ClipStore::open_at(&db_path).await.unwrap();
        drop(store);

        let pool = primitives::connect_at(&db_path, 1).await.unwrap();
        primitives::query("DROP INDEX IF EXISTS idx_clip_uploads_provider")
            .execute(&pool)
            .await
            .unwrap();
        drop(pool);

        let store = ClipStore::open_at(&db_path).await.unwrap();
        let notice = store.startup_notice().unwrap_or_default().to_string();
        assert!(notice.contains("backup was created"));

        let index_exists: bool = primitives::query_scalar(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'index' AND name = 'idx_clip_uploads_provider'",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert!(index_exists);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[tokio::test]
    async fn legacy_migration_creates_pre_migration_backup() {
        let temp_dir = std::env::temp_dir().join(format!(
            "nanite-clip-clips-pre-migration-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("clips.sqlite3");

        let pool = primitives::connect_at(&db_path, 1).await.unwrap();

        for statement in [
            r#"
            CREATE TABLE clips (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                trigger_event_ts INTEGER NOT NULL,
                saved_ts INTEGER NOT NULL,
                rule_id TEXT NOT NULL,
                clip_duration_secs INTEGER NOT NULL,
                character_id INTEGER NOT NULL,
                world_id INTEGER NOT NULL,
                zone_id INTEGER,
                facility_id INTEGER,
                profile_id TEXT NOT NULL,
                path TEXT,
                score INTEGER NOT NULL
            )
            "#,
            r#"
            CREATE TABLE clip_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                clip_id INTEGER NOT NULL,
                event_kind TEXT NOT NULL,
                occurrences INTEGER NOT NULL,
                points INTEGER NOT NULL
            )
            "#,
            r#"
            CREATE TABLE lookup_cache (
                lookup_kind TEXT NOT NULL,
                lookup_id INTEGER NOT NULL,
                display_name TEXT NOT NULL,
                resolved_ts INTEGER NOT NULL,
                PRIMARY KEY (lookup_kind, lookup_id)
            )
            "#,
            "INSERT INTO clips (trigger_event_ts, saved_ts, rule_id, clip_duration_secs, character_id, world_id, zone_id, facility_id, profile_id, path, score) VALUES (1710000000000, 1710000000000, 'rule_kill_streak', 30, 42, 17, 2, 1234, 'profile_1', NULL, 9)",
            "PRAGMA user_version = 0",
        ] {
            primitives::query(statement).execute(&pool).await.unwrap();
        }
        drop(pool);

        let store = ClipStore::open_at(&db_path).await.unwrap();

        let notice = store.startup_notice().unwrap_or_default().to_string();
        assert!(notice.contains("backup was created"));

        let backup_paths: Vec<_> = std::fs::read_dir(&temp_dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(|name| {
                        name.contains(&format!(
                            ".pre-migration-v0-to-v{CLIP_STORE_SCHEMA_VERSION}-"
                        ))
                    })
            })
            .collect();
        assert_eq!(backup_paths.len(), 1);

        let clips = store.recent_clips(10).await.unwrap();
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].origin, ClipOrigin::Rule);
        assert_eq!(clips[0].honu_session_id, None);

        let migration_count: i64 =
            primitives::query_scalar("SELECT COUNT(*) FROM seaql_migrations")
                .fetch_one(&store.pool)
                .await
                .unwrap();
        assert_eq!(
            migration_count,
            migrations::Migrator::migrations().len() as i64
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[tokio::test]
    async fn migrates_weapon_reference_cache_to_item_keyed_schema() {
        let temp_dir = std::env::temp_dir().join(format!(
            "nanite-clip-weapon-cache-migration-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("clips.sqlite3");

        let pool = primitives::connect_at(&db_path, 1).await.unwrap();

        for statement in [
            r#"
            CREATE TABLE clips (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                trigger_event_ts INTEGER NOT NULL,
                clip_start_ts INTEGER NOT NULL,
                clip_end_ts INTEGER NOT NULL,
                saved_ts INTEGER NOT NULL,
                clip_origin TEXT NOT NULL DEFAULT 'rule',
                rule_id TEXT NOT NULL,
                clip_duration_secs INTEGER NOT NULL,
                session_id TEXT,
                character_id INTEGER NOT NULL,
                world_id INTEGER NOT NULL,
                zone_id INTEGER,
                facility_id INTEGER,
                profile_id TEXT NOT NULL,
                path TEXT,
                score INTEGER NOT NULL,
                honu_session_id INTEGER
            )
            "#,
            r#"
            CREATE TABLE clip_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                clip_id INTEGER NOT NULL,
                event_kind TEXT NOT NULL,
                occurrences INTEGER NOT NULL,
                points INTEGER NOT NULL
            )
            "#,
            r#"
            CREATE TABLE clip_raw_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                clip_id INTEGER NOT NULL,
                event_ts INTEGER NOT NULL,
                event_kind TEXT NOT NULL,
                world_id INTEGER NOT NULL,
                zone_id INTEGER,
                facility_id INTEGER,
                actor_character_id INTEGER,
                other_character_id INTEGER,
                actor_class TEXT,
                attacker_weapon_id INTEGER,
                attacker_vehicle_id INTEGER,
                vehicle_killed_id INTEGER,
                characters_killed INTEGER NOT NULL,
                is_headshot INTEGER NOT NULL,
                experience_id INTEGER
            )
            "#,
            r#"
            CREATE TABLE lookup_cache (
                lookup_kind TEXT NOT NULL,
                lookup_id INTEGER NOT NULL,
                display_name TEXT NOT NULL,
                resolved_ts INTEGER NOT NULL,
                PRIMARY KEY (lookup_kind, lookup_id)
            )
            "#,
            r#"
            CREATE TABLE clip_overlaps (
                clip_id INTEGER NOT NULL,
                overlap_clip_id INTEGER NOT NULL,
                overlap_duration_ms INTEGER NOT NULL,
                detected_ts INTEGER NOT NULL,
                PRIMARY KEY (clip_id, overlap_clip_id)
            )
            "#,
            r#"
            CREATE TABLE alert_instances (
                alert_key TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                world_id INTEGER NOT NULL,
                zone_id INTEGER NOT NULL,
                metagame_event_id INTEGER NOT NULL,
                started_ts INTEGER NOT NULL,
                ended_ts INTEGER,
                state_name TEXT NOT NULL,
                winner_faction TEXT,
                faction_nc REAL NOT NULL,
                faction_tr REAL NOT NULL,
                faction_vs REAL NOT NULL
            )
            "#,
            r#"
            CREATE TABLE clip_alert_links (
                clip_id INTEGER NOT NULL,
                alert_key TEXT NOT NULL,
                PRIMARY KEY (clip_id, alert_key)
            )
            "#,
            r#"
            CREATE TABLE character_outfit_cache (
                character_id INTEGER PRIMARY KEY,
                outfit_id INTEGER,
                outfit_tag TEXT,
                resolved_ts INTEGER NOT NULL
            )
            "#,
            r#"
            CREATE TABLE weapon_reference_cache (
                weapon_id INTEGER PRIMARY KEY,
                item_id INTEGER NOT NULL,
                display_name TEXT NOT NULL,
                category_label TEXT NOT NULL,
                weapon_group_id INTEGER,
                resolved_ts INTEGER NOT NULL
            )
            "#,
            "PRAGMA user_version = 7",
        ] {
            primitives::query(statement).execute(&pool).await.unwrap();
        }
        drop(pool);

        let store = ClipStore::open_at(&db_path).await.unwrap();
        store
            .store_weapon_references(&[
                WeaponReferenceCacheEntry {
                    item_id: 180,
                    weapon_id: 80,
                    display_name: "Gauss Rifle".into(),
                    category_label: "Assault Rifle".into(),
                    faction: Some(Faction::NC),
                    weapon_group_id: None,
                },
                WeaponReferenceCacheEntry {
                    item_id: 181,
                    weapon_id: 80,
                    display_name: "Gauss Rifle S".into(),
                    category_label: "Assault Rifle".into(),
                    faction: Some(Faction::NC),
                    weapon_group_id: None,
                },
            ])
            .await
            .unwrap();

        let cached = store.list_weapon_references().await.unwrap();
        assert_eq!(cached.len(), 2);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[tokio::test]
    async fn exports_json_and_csv() {
        let store = open_test_store().await;
        let clip_id = store.insert_clip(sample_clip()).await.unwrap();

        let temp_dir = std::env::temp_dir().join(format!(
            "nanite-clip-clips-export-{}-{clip_id}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let json_path = temp_dir.join("clips.json");
        let csv_path = temp_dir.join("clips.csv");

        store.export_json_to(&json_path).await.unwrap();
        store.export_csv_to(&csv_path).await.unwrap();

        let json = std::fs::read_to_string(&json_path).unwrap();
        let csv = std::fs::read_to_string(&csv_path).unwrap();

        assert!(json.contains("\"origin\": \"rule\""));
        assert!(csv.contains("origin"));
        assert!(csv.contains("\"rule\""));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[tokio::test]
    async fn loads_clip_detail_with_raw_events() {
        let store = open_test_store().await;
        let clip_id = store.insert_clip(sample_clip()).await.unwrap();

        let detail = store.clip_detail(clip_id).await.unwrap().unwrap();
        assert_eq!(detail.clip.id, clip_id);
        assert_eq!(detail.raw_events.len(), 1);
        assert_eq!(detail.raw_events[0].event_kind, "Kill");
        assert_eq!(detail.raw_events[0].other_character_id, Some(100));
        assert_eq!(detail.raw_events[0].attacker_weapon_id, Some(80));
    }

    #[tokio::test]
    async fn recovers_background_jobs_and_marks_interrupted_work_failed() {
        let store = open_test_store().await;
        let now = Utc::now();

        store
            .upsert_background_job(&BackgroundJobRecord {
                id: BackgroundJobId(1),
                kind: BackgroundJobKind::Upload,
                label: "Upload clip #1".into(),
                state: BackgroundJobState::Running,
                related_clip_ids: vec![1],
                progress: Some(BackgroundJobProgress {
                    current_step: 2,
                    total_steps: 5,
                    message: "Uploading".into(),
                }),
                started_at: now - chrono::Duration::seconds(30),
                updated_at: now - chrono::Duration::seconds(5),
                finished_at: None,
                detail: None,
                cancellable: true,
            })
            .await
            .unwrap();

        store
            .upsert_background_job(&BackgroundJobRecord {
                id: BackgroundJobId(2),
                kind: BackgroundJobKind::Montage,
                label: "Create montage".into(),
                state: BackgroundJobState::Succeeded,
                related_clip_ids: vec![2, 3],
                progress: None,
                started_at: now - chrono::Duration::minutes(2),
                updated_at: now - chrono::Duration::minutes(1),
                finished_at: Some(now - chrono::Duration::minutes(1)),
                detail: Some("Created montage from 2 clips.".into()),
                cancellable: false,
            })
            .await
            .unwrap();

        let recovered = store.recover_background_jobs(10).await.unwrap();

        let interrupted = recovered
            .iter()
            .find(|job| job.id == BackgroundJobId(1))
            .unwrap();
        assert_eq!(interrupted.state, BackgroundJobState::Failed);
        assert!(interrupted.finished_at.is_some());
        assert!(!interrupted.cancellable);
        assert!(interrupted.progress.is_none());
        assert!(
            interrupted
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("closed before the background job finished"))
        );

        let completed = recovered
            .iter()
            .find(|job| job.id == BackgroundJobId(2))
            .unwrap();
        assert_eq!(completed.state, BackgroundJobState::Succeeded);
        assert_eq!(
            completed.detail.as_deref(),
            Some("Created montage from 2 clips.")
        );
        assert_eq!(completed.related_clip_ids, vec![2, 3]);
    }

    #[tokio::test]
    async fn recovers_completed_post_process_jobs_as_succeeded() {
        let store = open_test_store().await;
        let now = Utc::now();
        let clip_id = store.insert_clip(sample_clip()).await.unwrap();

        store
            .set_post_process_status(clip_id, PostProcessStatus::Completed, None)
            .await
            .unwrap();

        store
            .upsert_background_job(&BackgroundJobRecord {
                id: BackgroundJobId(3),
                kind: BackgroundJobKind::PostProcess,
                label: format!("Post-process clip #{clip_id}"),
                state: BackgroundJobState::Running,
                related_clip_ids: vec![clip_id],
                progress: Some(BackgroundJobProgress {
                    current_step: 4,
                    total_steps: 4,
                    message: "Audio post-process completed.".into(),
                }),
                started_at: now - chrono::Duration::seconds(10),
                updated_at: now - chrono::Duration::seconds(1),
                finished_at: None,
                detail: Some("Running audio post-process.".into()),
                cancellable: true,
            })
            .await
            .unwrap();

        let recovered = store.recover_background_jobs(10).await.unwrap();
        let completed = recovered
            .iter()
            .find(|job| job.id == BackgroundJobId(3))
            .unwrap();

        assert_eq!(completed.state, BackgroundJobState::Succeeded);
        assert!(completed.finished_at.is_some());
        assert!(!completed.cancellable);
        assert!(completed.progress.is_none());
        assert_eq!(
            completed.detail.as_deref(),
            Some("Audio post-processing completed.")
        );
    }

    #[tokio::test]
    async fn deletes_background_job_records() {
        let store = open_test_store().await;
        let now = Utc::now();

        store
            .upsert_background_job(&BackgroundJobRecord {
                id: BackgroundJobId(9),
                kind: BackgroundJobKind::Upload,
                label: "Upload clip #9".into(),
                state: BackgroundJobState::Failed,
                related_clip_ids: vec![9],
                progress: None,
                started_at: now - chrono::Duration::seconds(30),
                updated_at: now,
                finished_at: Some(now),
                detail: Some("Upload failed.".into()),
                cancellable: false,
            })
            .await
            .unwrap();

        store
            .delete_background_job(BackgroundJobId(9))
            .await
            .unwrap();

        let jobs = store.recent_background_jobs(10).await.unwrap();
        assert!(jobs.iter().all(|job| job.id != BackgroundJobId(9)));
    }

    #[tokio::test]
    async fn computes_stats_and_session_summary() {
        let store = open_test_store().await;
        store.insert_clip(sample_clip()).await.unwrap();

        let stats = store.stats_snapshot(None).await.unwrap();
        assert_eq!(stats.total_clips, 1);
        assert_eq!(stats.total_duration_secs, 30);
        assert_eq!(stats.clips_per_rule[0].label, "rule_kill_streak");
        assert_eq!(stats.top_weapons[0].label, "80");
        assert_eq!(stats.top_targets[0].label, "100");
        assert_eq!(stats.raw_event_kinds[0].label, "Kill");

        let summary = store.session_summary("session-1").await.unwrap();
        assert_eq!(summary.total_clips, 1);
        assert_eq!(summary.total_duration_secs, 30);
        assert_eq!(summary.unique_bases, 1);
        assert_eq!(summary.top_clip.as_ref().map(|item| item.score), Some(9));
    }

    #[tokio::test]
    async fn top_targets_excludes_non_kill_counterparties() {
        let store = open_test_store().await;
        let mut clip = sample_clip();
        clip.raw_events.push(ClipRawEventDraft {
            event_at: clip.saved_at,
            event_kind: "Revive".into(),
            world_id: 17,
            zone_id: Some(2),
            facility_id: Some(1234),
            actor_character_id: Some(42),
            other_character_id: Some(999),
            actor_class: Some("Medic".into()),
            attacker_weapon_id: None,
            attacker_vehicle_id: None,
            vehicle_killed_id: None,
            characters_killed: 0,
            is_headshot: false,
            experience_id: Some(7),
        });
        store.insert_clip(clip).await.unwrap();

        let stats = store.stats_snapshot(None).await.unwrap();
        assert_eq!(stats.top_targets.len(), 1);
        assert_eq!(stats.top_targets[0].label, "100");
    }

    #[tokio::test]
    async fn rejects_invalid_export_destination() {
        let store = open_test_store().await;
        let destination = PathBuf::from("/definitely/missing/nanite-clip/export.json");

        let error = store.export_json_to(&destination).await.unwrap_err();
        assert!(matches!(error, ClipStoreError::InvalidOutputPath(_)));
    }
}
