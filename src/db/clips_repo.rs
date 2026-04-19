use super::*;

pub(super) async fn insert_clip_upload(
    store: &ClipStore,
    upload: ClipUploadDraft,
) -> Result<i64, ClipStoreError> {
    let now = Utc::now().timestamp_millis();
    let inserted = entities::clip_uploads::ActiveModel {
        id: NotSet,
        clip_id: Set(upload.clip_id),
        provider: Set(upload.provider.as_str().to_string()),
        state: Set(upload.state.as_str().to_string()),
        external_id: Set(upload.external_id),
        clip_url: Set(upload.clip_url),
        error_message: Set(upload.error_message),
        started_ts: Set(now),
        updated_ts: Set(now),
        completed_ts: Set(match upload.state {
            ClipUploadState::Running => None,
            _ => Some(now),
        }),
    }
    .insert(&store.pool)
    .await?;

    Ok(inserted.id)
}

pub(super) async fn update_clip_upload(
    store: &ClipStore,
    upload_id: i64,
    state: ClipUploadState,
    external_id: Option<&str>,
    clip_url: Option<&str>,
    error_message: Option<&str>,
) -> Result<(), ClipStoreError> {
    let now = Utc::now().timestamp_millis();
    let Some(existing) = entities::clip_uploads::Entity::find_by_id(upload_id)
        .one(&store.pool)
        .await?
    else {
        return Ok(());
    };
    let merged_external_id = external_id
        .map(str::to_string)
        .or(existing.external_id.clone());
    let merged_clip_url = clip_url.map(str::to_string).or(existing.clip_url.clone());
    let mut model: entities::clip_uploads::ActiveModel = existing.into();
    model.state = Set(state.as_str().to_string());
    model.external_id = Set(merged_external_id);
    model.clip_url = Set(merged_clip_url);
    model.error_message = Set(error_message.map(str::to_string));
    model.updated_ts = Set(now);
    model.completed_ts = Set(match state {
        ClipUploadState::Running => None,
        _ => Some(now),
    });
    model.update(&store.pool).await?;

    Ok(())
}

pub(super) async fn insert_montage(
    store: &ClipStore,
    output_path: &str,
    source_clip_ids: &[i64],
) -> Result<i64, ClipStoreError> {
    let tx = primitives::begin(&store.pool).await?;
    let created_ts = Utc::now().timestamp_millis();
    let montage = entities::montages::ActiveModel {
        id: NotSet,
        output_path: Set(output_path.to_string()),
        created_ts: Set(created_ts),
    }
    .insert(&*tx)
    .await?;

    if !source_clip_ids.is_empty() {
        let links = source_clip_ids
            .iter()
            .enumerate()
            .map(|(index, clip_id)| entities::montage_clips::ActiveModel {
                montage_id: Set(montage.id),
                clip_id: Set(*clip_id),
                sequence_index: Set(index as i64),
            })
            .collect::<Vec<_>>();
        entities::montage_clips::Entity::insert_many(links)
            .exec(&*tx)
            .await?;
    }

    tx.commit().await?;
    Ok(montage.id)
}

pub(super) async fn update_clip_path(
    store: &ClipStore,
    clip_id: i64,
    path: Option<&str>,
) -> Result<(), ClipStoreError> {
    let Some(existing) = entities::clips::Entity::find_by_id(clip_id)
        .one(&store.pool)
        .await?
    else {
        return Ok(());
    };
    let mut model: entities::clips::ActiveModel = existing.into();
    model.path = Set(path.map(str::to_string));
    model.update(&store.pool).await?;

    Ok(())
}

pub(super) async fn insert_audio_tracks(
    store: &ClipStore,
    clip_id: i64,
    tracks: Vec<ClipAudioTrackDraft>,
) -> Result<(), ClipStoreError> {
    delete_audio_tracks(store, clip_id).await?;
    if tracks.is_empty() {
        return Ok(());
    }

    let models = tracks
        .into_iter()
        .map(|track| entities::clip_audio_tracks::ActiveModel {
            id: NotSet,
            clip_id: Set(clip_id),
            stream_index: Set(track.stream_index),
            role: Set(track.role),
            label: Set(track.label),
            gain_db: Set(track.gain_db),
            muted: Set(track.muted),
            source_kind: Set(track.source_kind),
            source_value: Set(track.source_value),
        })
        .collect::<Vec<_>>();
    entities::clip_audio_tracks::Entity::insert_many(models)
        .exec(&store.pool)
        .await?;
    Ok(())
}

pub(super) async fn load_audio_tracks(
    store: &ClipStore,
    clip_id: i64,
) -> Result<Vec<ClipAudioTrackRecord>, ClipStoreError> {
    Ok(entities::clip_audio_tracks::Entity::find()
        .filter(entities::clip_audio_tracks::Column::ClipId.eq(clip_id))
        .order_by_asc(entities::clip_audio_tracks::Column::StreamIndex)
        .order_by_asc(entities::clip_audio_tracks::Column::Id)
        .all(&store.pool)
        .await?
        .into_iter()
        .map(|track| ClipAudioTrackRecord {
            id: track.id,
            clip_id: track.clip_id,
            stream_index: track.stream_index,
            role: track.role,
            label: track.label,
            gain_db: track.gain_db,
            muted: track.muted,
            source_kind: track.source_kind,
            source_value: track.source_value,
        })
        .collect())
}

pub(super) async fn delete_audio_tracks(
    store: &ClipStore,
    clip_id: i64,
) -> Result<(), ClipStoreError> {
    entities::clip_audio_tracks::Entity::delete_many()
        .filter(entities::clip_audio_tracks::Column::ClipId.eq(clip_id))
        .exec(&store.pool)
        .await?;
    Ok(())
}

pub(super) async fn set_post_process_status(
    store: &ClipStore,
    clip_id: i64,
    status: PostProcessStatus,
    error: Option<&str>,
) -> Result<(), ClipStoreError> {
    let Some(existing) = entities::clips::Entity::find_by_id(clip_id)
        .one(&store.pool)
        .await?
    else {
        return Ok(());
    };

    let mut model: entities::clips::ActiveModel = existing.into();
    model.post_process_status = Set(status.into_entity());
    model.post_process_error = Set(error.map(str::to_string));
    model.update(&store.pool).await?;
    Ok(())
}

pub(super) async fn clips_pending_post_process(
    store: &ClipStore,
) -> Result<Vec<i64>, ClipStoreError> {
    Ok(entities::clips::Entity::find()
        .filter(entities::clips::Column::PostProcessStatus.eq(entities::PostProcessStatus::Pending))
        .all(&store.pool)
        .await?
        .into_iter()
        .map(|clip| clip.id)
        .collect())
}

pub(super) async fn all_clips(store: &ClipStore) -> Result<Vec<ClipRecord>, ClipStoreError> {
    exports::fetch_all_clips(store).await
}

pub(super) async fn delete_clip(store: &ClipStore, clip_id: i64) -> Result<(), ClipStoreError> {
    entities::clips::Entity::delete_by_id(clip_id)
        .exec(&store.pool)
        .await?;

    Ok(())
}
