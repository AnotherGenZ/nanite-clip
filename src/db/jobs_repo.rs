use super::*;

pub(super) async fn upsert_background_job(
    store: &ClipStore,
    record: &BackgroundJobRecord,
) -> Result<(), ClipStoreError> {
    let related_clip_ids = serde_json::to_string(&record.related_clip_ids)?;
    let progress = record.progress.as_ref();
    entities::background_jobs::Entity::insert(entities::background_jobs::ActiveModel {
        id: Set(record.id.0 as i64),
        kind: Set(record.kind.as_str().to_string()),
        label: Set(record.label.clone()),
        state: Set(record.state.as_str().to_string()),
        related_clip_ids_json: Set(related_clip_ids),
        progress_current_step: Set(progress.map(|progress| i64::from(progress.current_step))),
        progress_total_steps: Set(progress.map(|progress| i64::from(progress.total_steps))),
        progress_message: Set(progress.map(|progress| progress.message.clone())),
        started_ts: Set(record.started_at.timestamp_millis()),
        updated_ts: Set(record.updated_at.timestamp_millis()),
        finished_ts: Set(record.finished_at.map(|value| value.timestamp_millis())),
        detail: Set(record.detail.clone()),
        cancellable: Set(record.cancellable),
    })
    .on_conflict(
        OnConflict::column(entities::background_jobs::Column::Id)
            .update_columns([
                entities::background_jobs::Column::Kind,
                entities::background_jobs::Column::Label,
                entities::background_jobs::Column::State,
                entities::background_jobs::Column::RelatedClipIdsJson,
                entities::background_jobs::Column::ProgressCurrentStep,
                entities::background_jobs::Column::ProgressTotalSteps,
                entities::background_jobs::Column::ProgressMessage,
                entities::background_jobs::Column::StartedTs,
                entities::background_jobs::Column::UpdatedTs,
                entities::background_jobs::Column::FinishedTs,
                entities::background_jobs::Column::Detail,
                entities::background_jobs::Column::Cancellable,
            ])
            .to_owned(),
    )
    .exec(&store.pool)
    .await?;

    Ok(())
}

pub(super) async fn recover_background_jobs(
    store: &ClipStore,
    limit: usize,
) -> Result<Vec<BackgroundJobRecord>, ClipStoreError> {
    let now = Utc::now().timestamp_millis();
    let interrupted_jobs = entities::background_jobs::Entity::find()
        .filter(entities::background_jobs::Column::State.is_in([
            BackgroundJobState::Queued.as_str(),
            BackgroundJobState::Running.as_str(),
        ]))
        .all(&store.pool)
        .await?;

    for job in interrupted_jobs {
        let recovered = recover_background_job_model(store, job, now).await?;
        recovered.update(&store.pool).await?;
    }

    recent_background_jobs(store, limit).await
}

pub(super) async fn recent_background_jobs(
    store: &ClipStore,
    limit: usize,
) -> Result<Vec<BackgroundJobRecord>, ClipStoreError> {
    let rows = entities::background_jobs::Entity::find()
        .order_by_desc(entities::background_jobs::Column::UpdatedTs)
        .order_by_desc(entities::background_jobs::Column::Id)
        .limit(limit as u64)
        .all(&store.pool)
        .await?;

    rows.into_iter()
        .map(background_job_from_model)
        .collect::<Result<Vec<_>, ClipStoreError>>()
}

pub(super) async fn recover_background_job_model(
    store: &ClipStore,
    job: entities::background_jobs::Model,
    now: i64,
) -> Result<entities::background_jobs::ActiveModel, ClipStoreError> {
    let mut model: entities::background_jobs::ActiveModel = job.clone().into();
    model.progress_current_step = Set(None);
    model.progress_total_steps = Set(None);
    model.progress_message = Set(None);
    model.updated_ts = Set(now);
    model.finished_ts = Set(Some(now));
    model.cancellable = Set(false);

    if job.kind == BackgroundJobKind::PostProcess.as_str() {
        let related_clip_ids: Vec<i64> = serde_json::from_str(&job.related_clip_ids_json)?;
        if let Some(clip_id) = related_clip_ids.first().copied()
            && let Some(clip) = entities::clips::Entity::find_by_id(clip_id)
                .one(&store.pool)
                .await?
        {
            let recovered_state = PostProcessStatus::from_entity(clip.post_process_status);
            match recovered_state {
                PostProcessStatus::Completed => {
                    model.state = Set(BackgroundJobState::Succeeded.as_str().to_string());
                    model.detail = Set(Some("Audio post-processing completed.".to_string()));
                    return Ok(model);
                }
                PostProcessStatus::NotRequired => {
                    model.state = Set(BackgroundJobState::Succeeded.as_str().to_string());
                    model.detail = Set(Some("Audio post-processing was not required.".to_string()));
                    return Ok(model);
                }
                PostProcessStatus::Failed => {
                    model.state = Set(BackgroundJobState::Failed.as_str().to_string());
                    model.detail =
                        Set(Some(clip.post_process_error.unwrap_or_else(|| {
                            interrupted_background_job_detail(job.detail.clone())
                        })));
                    return Ok(model);
                }
                PostProcessStatus::Pending | PostProcessStatus::Legacy => {}
            }
        }
    }

    model.state = Set(BackgroundJobState::Failed.as_str().to_string());
    model.detail = Set(Some(interrupted_background_job_detail(job.detail)));
    Ok(model)
}

pub(super) async fn delete_background_job(
    store: &ClipStore,
    id: BackgroundJobId,
) -> Result<(), ClipStoreError> {
    entities::background_jobs::Entity::delete_by_id(id.0 as i64)
        .exec(&store.pool)
        .await?;
    Ok(())
}
