use super::*;

pub(super) async fn initialize_schema(store: &ClipStore) -> Result<Option<String>, ClipStoreError> {
    let manager = SchemaManager::new(&store.pool);
    let has_clips_table = manager
        .has_table(entities::clips::Entity.table_name())
        .await?;

    if !has_clips_table {
        reset_schema(store).await?;
        return Ok(None);
    }

    let current_version = primitives::query("PRAGMA user_version")
        .fetch_one(&store.pool)
        .await?
        .try_get_at::<i64>(0)?;

    if current_version > CLIP_STORE_SCHEMA_VERSION {
        return Err(ClipStoreError::UnsupportedSchemaVersion(current_version));
    }

    let legacy_state = migrations::inspect_legacy_schema(&store.pool).await?;
    let requires_backup = legacy_state.pending_migrations() > 0;
    let mut startup_notice = None;

    if requires_backup
        && let Some(backup_path) =
            create_pre_migration_backup(store, current_version, CLIP_STORE_SCHEMA_VERSION).await?
    {
        startup_notice = Some(format!(
            "Clip database migrated to schema v{CLIP_STORE_SCHEMA_VERSION}. A backup was created at {} before the migration ran.",
            backup_path.display()
        ));
    }

    migrations::reconcile_migration_history(&store.pool, legacy_state).await?;
    migrations::Migrator::up(&store.pool, None).await?;
    set_schema_version(store).await?;

    Ok(startup_notice)
}

pub(super) async fn reset_schema(store: &ClipStore) -> Result<(), ClipStoreError> {
    migrations::Migrator::fresh(&store.pool).await?;
    set_schema_version(store).await?;
    Ok(())
}

pub(super) async fn set_schema_version(store: &ClipStore) -> Result<(), ClipStoreError> {
    primitives::query(format!("PRAGMA user_version = {CLIP_STORE_SCHEMA_VERSION}"))
        .execute(&store.pool)
        .await?;
    Ok(())
}

pub(super) async fn create_pre_migration_backup(
    store: &ClipStore,
    current_version: i64,
    target_version: i64,
) -> Result<Option<PathBuf>, ClipStoreError> {
    let Some(database_path) = store.database_path.as_deref() else {
        return Ok(None);
    };

    let backup_path = next_migration_backup_path(database_path, current_version, target_version);
    exports::write_sqlite_backup(store, &backup_path)
        .await
        .map_err(|error| {
            ClipStoreError::MigrationBackupFailed(format!(
                "failed to create pre-migration backup {}: {error}",
                backup_path.display()
            ))
        })?;

    Ok(Some(backup_path))
}
