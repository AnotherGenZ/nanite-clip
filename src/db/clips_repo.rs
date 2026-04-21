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

pub(super) async fn set_clip_favorited(
    store: &ClipStore,
    clip_id: i64,
    favorited: bool,
) -> Result<(), ClipStoreError> {
    let Some(existing) = entities::clips::Entity::find_by_id(clip_id)
        .one(&store.pool)
        .await?
    else {
        return Ok(());
    };

    let mut model: entities::clips::ActiveModel = existing.into();
    model.favorited = Set(favorited);
    model.update(&store.pool).await?;
    Ok(())
}

pub(super) async fn set_clips_favorited(
    store: &ClipStore,
    clip_ids: &[i64],
    favorited: bool,
) -> Result<(), ClipStoreError> {
    if clip_ids.is_empty() {
        return Ok(());
    }

    entities::clips::Entity::update_many()
        .col_expr(entities::clips::Column::Favorited, Expr::value(favorited))
        .filter(entities::clips::Column::Id.is_in(clip_ids.iter().copied()))
        .exec(&store.pool)
        .await?;
    Ok(())
}

pub(super) async fn add_tag(
    store: &ClipStore,
    clip_id: i64,
    tag_name: &str,
) -> Result<(), ClipStoreError> {
    let tag_name = normalized_tag_name(tag_name)?;
    let exists = entities::clip_tags::Entity::find()
        .filter(entities::clip_tags::Column::ClipId.eq(clip_id))
        .filter(entities::clip_tags::Column::TagName.eq(tag_name.clone()))
        .one(&store.pool)
        .await?
        .is_some();
    if exists {
        return Ok(());
    }

    entities::clip_tags::Entity::insert(entities::clip_tags::ActiveModel {
        id: NotSet,
        clip_id: Set(clip_id),
        tag_name: Set(tag_name),
        created_ts: Set(Utc::now().timestamp_millis()),
    })
    .exec(&store.pool)
    .await?;
    Ok(())
}

pub(super) async fn remove_tag(
    store: &ClipStore,
    clip_id: i64,
    tag_name: &str,
) -> Result<(), ClipStoreError> {
    let tag_name = normalized_tag_name(tag_name)?;
    entities::clip_tags::Entity::delete_many()
        .filter(entities::clip_tags::Column::ClipId.eq(clip_id))
        .filter(entities::clip_tags::Column::TagName.eq(tag_name))
        .exec(&store.pool)
        .await?;
    Ok(())
}

pub(super) async fn list_tags(store: &ClipStore) -> Result<Vec<String>, ClipStoreError> {
    let mut query = Query::select();
    query
        .distinct()
        .expr_as(
            Expr::col((
                entities::clip_tags::Entity,
                entities::clip_tags::Column::TagName,
            )),
            Alias::new("label"),
        )
        .from(entities::clip_tags::Entity)
        .order_by_expr(Expr::cust("LOWER(\"clip_tags\".\"tag_name\")"), Order::Asc);
    let rows = primitives::fetch_all_stmt(&query, &store.pool).await?;
    read_string_option_rows(rows)
}

pub(super) async fn create_collection(
    store: &ClipStore,
    name: &str,
    description: Option<&str>,
) -> Result<ClipCollectionRecord, ClipStoreError> {
    let name = normalized_collection_name(name)?;
    let description = normalized_optional_text(description);
    let created_ts = Utc::now().timestamp_millis();
    let inserted = entities::collections::ActiveModel {
        id: NotSet,
        name: Set(name),
        description: Set(description),
        created_ts: Set(created_ts),
    }
    .insert(&store.pool)
    .await?;

    Ok(ClipCollectionRecord {
        id: inserted.id,
        name: inserted.name,
        description: inserted.description,
        created_at: timestamp_millis_to_utc(inserted.created_ts)?,
        clip_count: 0,
    })
}

pub(super) async fn list_collections(
    store: &ClipStore,
) -> Result<Vec<ClipCollectionRecord>, ClipStoreError> {
    let mut query = Query::select();
    query
        .expr_as(
            Expr::col((
                entities::collections::Entity,
                entities::collections::Column::Id,
            )),
            Alias::new("id"),
        )
        .expr_as(
            Expr::col((
                entities::collections::Entity,
                entities::collections::Column::Name,
            )),
            Alias::new("name"),
        )
        .expr_as(
            Expr::col((
                entities::collections::Entity,
                entities::collections::Column::Description,
            )),
            Alias::new("description"),
        )
        .expr_as(
            Expr::col((
                entities::collections::Entity,
                entities::collections::Column::CreatedTs,
            )),
            Alias::new("created_ts"),
        )
        .expr_as(
            Func::count(Expr::col((
                entities::collection_clips::Entity,
                entities::collection_clips::Column::ClipId,
            ))),
            Alias::new("clip_count"),
        )
        .from(entities::collections::Entity)
        .join(
            JoinType::LeftJoin,
            entities::collection_clips::Entity,
            Expr::col((
                entities::collections::Entity,
                entities::collections::Column::Id,
            ))
            .equals((
                entities::collection_clips::Entity,
                entities::collection_clips::Column::CollectionId,
            )),
        )
        .group_by_col((
            entities::collections::Entity,
            entities::collections::Column::Id,
        ))
        .group_by_col((
            entities::collections::Entity,
            entities::collections::Column::Name,
        ))
        .group_by_col((
            entities::collections::Entity,
            entities::collections::Column::Description,
        ))
        .group_by_col((
            entities::collections::Entity,
            entities::collections::Column::CreatedTs,
        ))
        .order_by_expr(Expr::cust("LOWER(\"collections\".\"name\")"), Order::Asc);

    let rows = primitives::fetch_all_stmt(&query, &store.pool).await?;
    rows.into_iter()
        .map(|row| {
            Ok(ClipCollectionRecord {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                description: row.try_get("description")?,
                created_at: timestamp_millis_to_utc(row.try_get("created_ts")?)?,
                clip_count: row.try_get::<i64>("clip_count")? as u32,
            })
        })
        .collect()
}

pub(super) async fn add_clip_to_collection(
    store: &ClipStore,
    collection_id: i64,
    clip_id: i64,
) -> Result<(), ClipStoreError> {
    let exists = entities::collection_clips::Entity::find()
        .filter(entities::collection_clips::Column::CollectionId.eq(collection_id))
        .filter(entities::collection_clips::Column::ClipId.eq(clip_id))
        .one(&store.pool)
        .await?
        .is_some();
    if exists {
        return Ok(());
    }

    let next_index = next_collection_sequence_index(store, collection_id).await?;
    entities::collection_clips::Entity::insert(entities::collection_clips::ActiveModel {
        collection_id: Set(collection_id),
        clip_id: Set(clip_id),
        added_ts: Set(Utc::now().timestamp_millis()),
        sequence_index: Set(next_index),
    })
    .exec(&store.pool)
    .await?;
    Ok(())
}

pub(super) async fn add_clips_to_collection(
    store: &ClipStore,
    collection_id: i64,
    clip_ids: &[i64],
) -> Result<(), ClipStoreError> {
    if clip_ids.is_empty() {
        return Ok(());
    }

    let existing = entities::collection_clips::Entity::find()
        .filter(entities::collection_clips::Column::CollectionId.eq(collection_id))
        .filter(entities::collection_clips::Column::ClipId.is_in(clip_ids.iter().copied()))
        .all(&store.pool)
        .await?
        .into_iter()
        .map(|membership| membership.clip_id)
        .collect::<std::collections::HashSet<_>>();

    let mut next_index = next_collection_sequence_index(store, collection_id).await?;
    let now = Utc::now().timestamp_millis();
    let mut models = Vec::new();
    for clip_id in clip_ids {
        if existing.contains(clip_id) {
            continue;
        }
        models.push(entities::collection_clips::ActiveModel {
            collection_id: Set(collection_id),
            clip_id: Set(*clip_id),
            added_ts: Set(now),
            sequence_index: Set(next_index),
        });
        next_index += 1;
    }

    if !models.is_empty() {
        entities::collection_clips::Entity::insert_many(models)
            .exec(&store.pool)
            .await?;
    }
    Ok(())
}

pub(super) async fn remove_clip_from_collection(
    store: &ClipStore,
    collection_id: i64,
    clip_id: i64,
) -> Result<(), ClipStoreError> {
    entities::collection_clips::Entity::delete_many()
        .filter(entities::collection_clips::Column::CollectionId.eq(collection_id))
        .filter(entities::collection_clips::Column::ClipId.eq(clip_id))
        .exec(&store.pool)
        .await?;
    resequence_collection(store, collection_id).await
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) async fn move_clip_within_collection(
    store: &ClipStore,
    collection_id: i64,
    clip_id: i64,
    direction: i32,
) -> Result<(), ClipStoreError> {
    if direction == 0 {
        return Ok(());
    }

    let mut memberships = entities::collection_clips::Entity::find()
        .filter(entities::collection_clips::Column::CollectionId.eq(collection_id))
        .order_by_asc(entities::collection_clips::Column::SequenceIndex)
        .order_by_asc(entities::collection_clips::Column::AddedTs)
        .all(&store.pool)
        .await?;
    let Some(index) = memberships
        .iter()
        .position(|membership| membership.clip_id == clip_id)
    else {
        return Ok(());
    };
    let target = if direction < 0 {
        index.saturating_sub(1)
    } else {
        (index + 1).min(memberships.len().saturating_sub(1))
    };
    if index == target {
        return Ok(());
    }

    memberships.swap(index, target);
    write_collection_sequence(store, memberships).await
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

fn normalized_tag_name(tag_name: &str) -> Result<String, ClipStoreError> {
    let tag_name = tag_name.trim();
    if tag_name.is_empty() {
        return Err(ClipStoreError::InvalidInput(
            "tag name cannot be empty".into(),
        ));
    }
    Ok(tag_name.to_string())
}

fn normalized_collection_name(name: &str) -> Result<String, ClipStoreError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ClipStoreError::InvalidInput(
            "collection name cannot be empty".into(),
        ));
    }
    Ok(name.to_string())
}

fn normalized_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

async fn next_collection_sequence_index(
    store: &ClipStore,
    collection_id: i64,
) -> Result<i64, ClipStoreError> {
    let max_index = entities::collection_clips::Entity::find()
        .filter(entities::collection_clips::Column::CollectionId.eq(collection_id))
        .order_by_desc(entities::collection_clips::Column::SequenceIndex)
        .one(&store.pool)
        .await?
        .map(|membership| membership.sequence_index + 1)
        .unwrap_or(0);
    Ok(max_index)
}

async fn resequence_collection(
    store: &ClipStore,
    collection_id: i64,
) -> Result<(), ClipStoreError> {
    let memberships = entities::collection_clips::Entity::find()
        .filter(entities::collection_clips::Column::CollectionId.eq(collection_id))
        .order_by_asc(entities::collection_clips::Column::SequenceIndex)
        .order_by_asc(entities::collection_clips::Column::AddedTs)
        .all(&store.pool)
        .await?;
    write_collection_sequence(store, memberships).await
}

async fn write_collection_sequence(
    store: &ClipStore,
    memberships: Vec<entities::collection_clips::Model>,
) -> Result<(), ClipStoreError> {
    for (index, membership) in memberships.into_iter().enumerate() {
        let mut model: entities::collection_clips::ActiveModel = membership.into();
        model.sequence_index = Set(index as i64);
        model.update(&store.pool).await?;
    }
    Ok(())
}
