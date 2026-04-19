use super::*;

pub(super) async fn backup_to(store: &ClipStore, destination: &Path) -> Result<(), ClipStoreError> {
    validate_output_destination(destination)?;
    write_sqlite_backup(store, destination).await?;
    Ok(())
}

pub(super) async fn export_json_to(
    store: &ClipStore,
    destination: &Path,
) -> Result<(), ClipStoreError> {
    validate_output_destination(destination)?;
    let payload = serde_json::to_vec_pretty(&export_records(store).await?)?;
    atomic_write(destination, &payload)?;
    Ok(())
}

pub(super) async fn export_csv_to(
    store: &ClipStore,
    destination: &Path,
) -> Result<(), ClipStoreError> {
    validate_output_destination(destination)?;

    let mut csv = String::from(
        "id,trigger_event_at,clip_start_at,clip_end_at,saved_at,origin,profile_id,rule_id,clip_duration_secs,session_id,character_id,world_id,zone_id,facility_id,score,honu_session_id,path,file_size_bytes,events_json\n",
    );

    for record in export_records(store).await? {
        let events_json = serde_json::to_string(&record.events)?;
        append_csv_row(
            &mut csv,
            &[
                record.id.to_string(),
                record.trigger_event_at,
                record.clip_start_at,
                record.clip_end_at,
                record.saved_at,
                record.origin,
                record.profile_id,
                record.rule_id,
                record.clip_duration_secs.to_string(),
                record.session_id.unwrap_or_default(),
                record.character_id.to_string(),
                record.world_id.to_string(),
                record
                    .zone_id
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                record
                    .facility_id
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                record.score.to_string(),
                record
                    .honu_session_id
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                record.path.unwrap_or_default(),
                record
                    .file_size_bytes
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                events_json,
            ],
        );
    }

    atomic_write(destination, csv.as_bytes())?;
    Ok(())
}

pub(super) async fn export_records(
    store: &ClipStore,
) -> Result<Vec<ClipExportRecord>, ClipStoreError> {
    Ok(store
        .all_clips()
        .await?
        .into_iter()
        .map(ClipExportRecord::from)
        .collect())
}

pub(super) async fn fetch_all_clips(store: &ClipStore) -> Result<Vec<ClipRecord>, ClipStoreError> {
    let clips = entities::clips::Entity::find()
        .order_by_desc(entities::clips::Column::TriggerEventTs)
        .order_by_desc(entities::clips::Column::Id)
        .all(&store.pool)
        .await?;

    store.hydrate_clip_records(clips).await
}

pub(super) async fn write_sqlite_backup(
    store: &ClipStore,
    destination: &Path,
) -> Result<(), ClipStoreError> {
    let escaped = destination.display().to_string().replace('\'', "''");

    primitives::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&store.pool)
        .await?;
    primitives::query(format!("VACUUM INTO '{escaped}'"))
        .execute(&store.pool)
        .await?;

    Ok(())
}
