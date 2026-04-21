use std::collections::HashSet;

use chrono::Utc;
use sea_orm_migration::{
    DbErr, MigrationName, MigrationTrait, MigratorTrait, SchemaManager, async_trait,
    prelude::ColumnDef,
    sea_orm::{
        ActiveValue, ConnectionTrait, DatabaseConnection, EntityTrait, Iden, QueryFilter,
        sea_query::{Alias, Index},
    },
    seaql_migrations,
};

use super::*;

const MIGRATION_NAMES: [&str; 16] = [
    "m20240408_000001_create_initial_schema",
    "m20240408_000002_add_honu_session_id",
    "m20240408_000003_add_clip_origin",
    "m20240408_000004_add_clip_window_columns",
    "m20240408_000005_create_clip_raw_events_table",
    "m20240408_000006_create_lookup_cache_table",
    "m20240408_000007_create_clip_overlaps_table",
    "m20240408_000008_create_alert_tables",
    "m20240408_000009_create_character_outfit_cache_table",
    "m20240408_000010_create_weapon_reference_cache_table",
    "m20240408_000011_rebuild_weapon_reference_cache_table",
    "m20240408_000012_create_clip_uploads_table",
    "m20240408_000013_create_montages_table",
    "m20240408_000014_create_background_jobs_table",
    "m20240408_000015_create_clip_audio_tracks_and_post_process_state",
    "m20240408_000016_create_clip_organization_tables",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LegacySchemaState {
    applied_prefix: usize,
}

impl LegacySchemaState {
    pub(super) fn pending_migrations(self) -> usize {
        MIGRATION_NAMES.len().saturating_sub(self.applied_prefix)
    }
}

pub(super) struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(CreateInitialSchema),
            Box::new(AddHonuSessionId),
            Box::new(AddClipOrigin),
            Box::new(AddClipWindowColumns),
            Box::new(CreateClipRawEventsTable),
            Box::new(CreateLookupCacheTable),
            Box::new(CreateClipOverlapsTable),
            Box::new(CreateAlertTables),
            Box::new(CreateCharacterOutfitCacheTable),
            Box::new(CreateWeaponReferenceCacheTable),
            Box::new(RebuildWeaponReferenceCacheTable),
            Box::new(CreateClipUploadsTable),
            Box::new(CreateMontagesTable),
            Box::new(CreateBackgroundJobsTable),
            Box::new(CreateClipAudioTracksAndPostProcessState),
            Box::new(CreateClipOrganizationTables),
        ]
    }
}

pub(super) async fn inspect_legacy_schema(
    db: &DatabaseConnection,
) -> Result<LegacySchemaState, DbErr> {
    let manager = SchemaManager::new(db);
    let has_clips_table = manager
        .has_table(entities::clips::Entity.table_name())
        .await?;
    let has_clip_events_table = manager
        .has_table(entities::clip_events::Entity.table_name())
        .await?;
    let has_initial_schema = has_clips_table
        && has_clip_events_table
        && has_column_indexes(
            &manager,
            entities::clips::Entity.table_name(),
            clips_initial_index_columns(),
        )
        .await?
        && has_column_indexes(
            &manager,
            entities::clip_events::Entity.table_name(),
            [entities::clip_events::Column::ClipId],
        )
        .await?;

    let has_honu_session_id = has_clips_table
        && manager
            .has_column(
                entities::clips::Entity.table_name(),
                entities::clips::Column::HonuSessionId.to_string(),
            )
            .await?;
    let has_clip_origin = has_clips_table
        && manager
            .has_column(
                entities::clips::Entity.table_name(),
                entities::clips::Column::ClipOrigin.to_string(),
            )
            .await?;
    let has_clip_window_columns = has_clips_table
        && manager
            .has_column(
                entities::clips::Entity.table_name(),
                entities::clips::Column::ClipStartTs.to_string(),
            )
            .await?
        && manager
            .has_column(
                entities::clips::Entity.table_name(),
                entities::clips::Column::ClipEndTs.to_string(),
            )
            .await?
        && manager
            .has_column(
                entities::clips::Entity.table_name(),
                entities::clips::Column::SessionId.to_string(),
            )
            .await?
        && has_column_indexes(
            &manager,
            entities::clips::Entity.table_name(),
            clips_window_index_columns(),
        )
        .await?;
    let has_clip_raw_events_table = manager
        .has_table(entities::clip_raw_events::Entity.table_name())
        .await?
        && has_column_indexes(
            &manager,
            entities::clip_raw_events::Entity.table_name(),
            clip_raw_events_index_columns(),
        )
        .await?;
    let has_lookup_cache_table = manager
        .has_table(entities::lookup_cache::Entity.table_name())
        .await?
        && has_column_indexes(
            &manager,
            entities::lookup_cache::Entity.table_name(),
            lookup_cache_index_columns(),
        )
        .await?;
    let has_clip_overlaps_table = manager
        .has_table(entities::clip_overlaps::Entity.table_name())
        .await?
        && has_column_indexes(
            &manager,
            entities::clip_overlaps::Entity.table_name(),
            clip_overlaps_index_columns(),
        )
        .await?;
    let has_alert_tables = manager
        .has_table(entities::alert_instances::Entity.table_name())
        .await?
        && has_column_indexes(
            &manager,
            entities::alert_instances::Entity.table_name(),
            alert_instances_index_columns(),
        )
        .await?
        && has_named_index(
            &manager,
            entities::alert_instances::Entity.table_name(),
            "idx_alert_instances_zone_id",
        )
        .await?
        && manager
            .has_table(entities::clip_alert_links::Entity.table_name())
            .await?
        && has_column_indexes(
            &manager,
            entities::clip_alert_links::Entity.table_name(),
            clip_alert_links_index_columns(),
        )
        .await?;
    let has_character_outfit_cache_table = manager
        .has_table(entities::character_outfit_cache::Entity.table_name())
        .await?
        && has_column_indexes(
            &manager,
            entities::character_outfit_cache::Entity.table_name(),
            character_outfit_cache_index_columns(),
        )
        .await?;
    let has_weapon_reference_cache_table = manager
        .has_table(entities::weapon_reference_cache::Entity.table_name())
        .await?
        && has_column_indexes(
            &manager,
            entities::weapon_reference_cache::Entity.table_name(),
            weapon_reference_cache_index_columns(),
        )
        .await?
        && has_named_index(
            &manager,
            entities::weapon_reference_cache::Entity.table_name(),
            "idx_weapon_reference_cache_category",
        )
        .await?;
    let has_weapon_reference_cache_v2 = has_weapon_reference_cache_table
        && manager
            .has_column(
                entities::weapon_reference_cache::Entity.table_name(),
                entities::weapon_reference_cache::Column::FactionId.to_string(),
            )
            .await?;
    let has_clip_uploads_table = manager
        .has_table(entities::clip_uploads::Entity.table_name())
        .await?
        && has_column_indexes(
            &manager,
            entities::clip_uploads::Entity.table_name(),
            clip_uploads_index_columns(),
        )
        .await?
        && has_named_index(
            &manager,
            entities::clip_uploads::Entity.table_name(),
            "idx_clip_uploads_clip_id",
        )
        .await?
        && has_named_index(
            &manager,
            entities::clip_uploads::Entity.table_name(),
            "idx_clip_uploads_provider",
        )
        .await?;
    let has_montages_tables = manager
        .has_table(entities::montages::Entity.table_name())
        .await?
        && manager
            .has_table(entities::montage_clips::Entity.table_name())
            .await?
        && has_column_indexes(
            &manager,
            entities::montage_clips::Entity.table_name(),
            montage_clips_index_columns(),
        )
        .await?
        && has_named_index(
            &manager,
            entities::montage_clips::Entity.table_name(),
            "idx_montage_clips_clip_id",
        )
        .await?;
    let has_background_jobs_table = manager
        .has_table(entities::background_jobs::Entity.table_name())
        .await?
        && has_column_indexes(
            &manager,
            entities::background_jobs::Entity.table_name(),
            background_jobs_index_columns(),
        )
        .await?
        && has_named_index(
            &manager,
            entities::background_jobs::Entity.table_name(),
            "idx_background_jobs_state",
        )
        .await?;
    let has_clip_audio_tracks_table = manager
        .has_table(entities::clip_audio_tracks::Entity.table_name())
        .await?
        && has_column_indexes(
            &manager,
            entities::clip_audio_tracks::Entity.table_name(),
            clip_audio_tracks_index_columns(),
        )
        .await?
        && has_named_index(
            &manager,
            entities::clip_audio_tracks::Entity.table_name(),
            "idx_clip_audio_tracks_clip_stream",
        )
        .await?
        && manager
            .has_column(
                entities::clips::Entity.table_name(),
                entities::clips::Column::PostProcessStatus.to_string(),
            )
            .await?
        && manager
            .has_column(
                entities::clips::Entity.table_name(),
                entities::clips::Column::PostProcessError.to_string(),
            )
            .await?;
    let has_clip_organization_tables = has_clips_table
        && manager
            .has_column(
                entities::clips::Entity.table_name(),
                entities::clips::Column::Favorited.to_string(),
            )
            .await?
        && manager
            .has_table(entities::clip_tags::Entity.table_name())
            .await?
        && has_column_indexes(
            &manager,
            entities::clip_tags::Entity.table_name(),
            clip_tags_index_columns(),
        )
        .await?
        && has_named_index(
            &manager,
            entities::clip_tags::Entity.table_name(),
            "idx_clip_tags_clip_tag",
        )
        .await?
        && manager
            .has_table(entities::collections::Entity.table_name())
            .await?
        && has_column_indexes(
            &manager,
            entities::collections::Entity.table_name(),
            collections_index_columns(),
        )
        .await?
        && has_named_index(
            &manager,
            entities::collections::Entity.table_name(),
            "idx_collections_name",
        )
        .await?
        && manager
            .has_table(entities::collection_clips::Entity.table_name())
            .await?
        && has_column_indexes(
            &manager,
            entities::collection_clips::Entity.table_name(),
            collection_clips_index_columns(),
        )
        .await?
        && has_named_index(
            &manager,
            entities::collection_clips::Entity.table_name(),
            "idx_collection_clips_clip_id",
        )
        .await?
        && has_named_index(
            &manager,
            entities::collection_clips::Entity.table_name(),
            "idx_collection_clips_collection_sequence",
        )
        .await?;

    let applied_prefix = [
        has_initial_schema,
        has_honu_session_id,
        has_clip_origin,
        has_clip_window_columns,
        has_clip_raw_events_table,
        has_lookup_cache_table,
        has_clip_overlaps_table,
        has_alert_tables,
        has_character_outfit_cache_table,
        has_weapon_reference_cache_table,
        has_weapon_reference_cache_v2,
        has_clip_uploads_table,
        has_montages_tables,
        has_background_jobs_table,
        has_clip_audio_tracks_table,
        has_clip_organization_tables,
    ]
    .into_iter()
    .take_while(|applied| *applied)
    .count();

    Ok(LegacySchemaState { applied_prefix })
}

pub(super) async fn reconcile_migration_history(
    db: &DatabaseConnection,
    state: LegacySchemaState,
) -> Result<(), DbErr> {
    Migrator::install(db).await?;

    let existing: HashSet<String> = Migrator::get_migration_models(db)
        .await?
        .into_iter()
        .map(|model| model.version)
        .collect();

    for name in MIGRATION_NAMES.iter().skip(state.applied_prefix) {
        if !existing.contains(*name) {
            continue;
        }

        seaql_migrations::Entity::delete_many()
            .filter(seaql_migrations::Column::Version.eq(*name))
            .exec(db)
            .await?;
    }

    for name in MIGRATION_NAMES.iter().take(state.applied_prefix) {
        if existing.contains(*name) {
            continue;
        }

        seaql_migrations::Entity::insert(seaql_migrations::ActiveModel {
            version: ActiveValue::Set((*name).to_owned()),
            applied_at: ActiveValue::Set(Utc::now().timestamp()),
        })
        .exec(db)
        .await?;
    }

    Ok(())
}

macro_rules! named_migration {
    ($name:ident, $label:expr) => {
        struct $name;

        impl MigrationName for $name {
            fn name(&self) -> &str {
                $label
            }
        }
    };
}

named_migration!(CreateInitialSchema, MIGRATION_NAMES[0]);
named_migration!(AddHonuSessionId, MIGRATION_NAMES[1]);
named_migration!(AddClipOrigin, MIGRATION_NAMES[2]);
named_migration!(AddClipWindowColumns, MIGRATION_NAMES[3]);
named_migration!(CreateClipRawEventsTable, MIGRATION_NAMES[4]);
named_migration!(CreateLookupCacheTable, MIGRATION_NAMES[5]);
named_migration!(CreateClipOverlapsTable, MIGRATION_NAMES[6]);
named_migration!(CreateAlertTables, MIGRATION_NAMES[7]);
named_migration!(CreateCharacterOutfitCacheTable, MIGRATION_NAMES[8]);
named_migration!(CreateWeaponReferenceCacheTable, MIGRATION_NAMES[9]);
named_migration!(RebuildWeaponReferenceCacheTable, MIGRATION_NAMES[10]);
named_migration!(CreateClipUploadsTable, MIGRATION_NAMES[11]);
named_migration!(CreateMontagesTable, MIGRATION_NAMES[12]);
named_migration!(CreateBackgroundJobsTable, MIGRATION_NAMES[13]);
named_migration!(
    CreateClipAudioTracksAndPostProcessState,
    MIGRATION_NAMES[14]
);
named_migration!(CreateClipOrganizationTables, MIGRATION_NAMES[15]);

#[async_trait::async_trait]
impl MigrationTrait for CreateInitialSchema {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(entity_table(entities::clips::Entity))
            .await?;
        ensure_column_indexes(
            manager,
            entities::clips::Entity.table_name(),
            clips_initial_index_columns(),
        )
        .await?;

        manager.create_table(create_clip_events_table()).await?;
        ensure_column_indexes(
            manager,
            entities::clip_events::Entity.table_name(),
            [entities::clip_events::Column::ClipId],
        )
        .await?;

        Ok(())
    }
}

async fn has_column_indexes<C>(
    manager: &SchemaManager<'_>,
    table_name: &str,
    columns: impl IntoIterator<Item = C>,
) -> Result<bool, DbErr>
where
    C: Iden,
{
    for column in columns {
        let column_name = column.to_string();
        let index_name = format!("idx-{table_name}-{column_name}");
        if !manager.has_column(table_name, &column_name).await?
            || !manager.has_index(table_name, &index_name).await?
        {
            return Ok(false);
        }
    }

    Ok(true)
}

async fn ensure_column_indexes<C>(
    manager: &SchemaManager<'_>,
    table_name: &str,
    columns: impl IntoIterator<Item = C>,
) -> Result<(), DbErr>
where
    C: Iden,
{
    for column in columns {
        let column_name = column.to_string();
        let index_name = format!("idx-{table_name}-{column_name}");
        if manager.has_column(table_name, &column_name).await?
            && !manager.has_index(table_name, &index_name).await?
        {
            manager
                .create_index(
                    Index::create()
                        .name(&index_name)
                        .table(Alias::new(table_name.to_owned()))
                        .col(Alias::new(column_name))
                        .to_owned(),
                )
                .await?;
        }
    }

    Ok(())
}

async fn has_named_index(
    manager: &SchemaManager<'_>,
    table_name: &str,
    index_name: &str,
) -> Result<bool, DbErr> {
    manager.has_index(table_name, index_name).await
}

async fn ensure_named_index(
    manager: &SchemaManager<'_>,
    table_name: &str,
    index_name: &str,
    stmt: sea_orm::sea_query::IndexCreateStatement,
) -> Result<(), DbErr> {
    if !manager.has_index(table_name, index_name).await? {
        manager.create_index(stmt).await?;
    }

    Ok(())
}

fn clips_initial_index_columns() -> [entities::clips::Column; 8] {
    [
        entities::clips::Column::TriggerEventTs,
        entities::clips::Column::SavedTs,
        entities::clips::Column::RuleId,
        entities::clips::Column::CharacterId,
        entities::clips::Column::WorldId,
        entities::clips::Column::ZoneId,
        entities::clips::Column::FacilityId,
        entities::clips::Column::ProfileId,
    ]
}

fn clips_window_index_columns() -> [entities::clips::Column; 3] {
    [
        entities::clips::Column::ClipStartTs,
        entities::clips::Column::ClipEndTs,
        entities::clips::Column::SessionId,
    ]
}

fn clip_raw_events_index_columns() -> [entities::clip_raw_events::Column; 4] {
    [
        entities::clip_raw_events::Column::ClipId,
        entities::clip_raw_events::Column::EventTs,
        entities::clip_raw_events::Column::OtherCharacterId,
        entities::clip_raw_events::Column::AttackerWeaponId,
    ]
}

fn lookup_cache_index_columns() -> [entities::lookup_cache::Column; 1] {
    [entities::lookup_cache::Column::ResolvedTs]
}

fn clip_overlaps_index_columns() -> [entities::clip_overlaps::Column; 2] {
    [
        entities::clip_overlaps::Column::ClipId,
        entities::clip_overlaps::Column::OverlapClipId,
    ]
}

fn alert_instances_index_columns() -> [entities::alert_instances::Column; 2] {
    [
        entities::alert_instances::Column::ZoneId,
        entities::alert_instances::Column::StartedTs,
    ]
}

fn clip_alert_links_index_columns() -> [entities::clip_alert_links::Column; 1] {
    [entities::clip_alert_links::Column::AlertKey]
}

fn character_outfit_cache_index_columns() -> [entities::character_outfit_cache::Column; 1] {
    [entities::character_outfit_cache::Column::ResolvedTs]
}

fn weapon_reference_cache_index_columns() -> [entities::weapon_reference_cache::Column; 2] {
    [
        entities::weapon_reference_cache::Column::WeaponId,
        entities::weapon_reference_cache::Column::ResolvedTs,
    ]
}

fn clip_uploads_index_columns() -> [entities::clip_uploads::Column; 1] {
    [entities::clip_uploads::Column::ClipId]
}

fn clip_audio_tracks_index_columns() -> [entities::clip_audio_tracks::Column; 1] {
    [entities::clip_audio_tracks::Column::ClipId]
}

fn clip_tags_index_columns() -> [entities::clip_tags::Column; 2] {
    [
        entities::clip_tags::Column::ClipId,
        entities::clip_tags::Column::TagName,
    ]
}

fn collections_index_columns() -> [entities::collections::Column; 1] {
    [entities::collections::Column::Name]
}

fn collection_clips_index_columns() -> [entities::collection_clips::Column; 1] {
    [entities::collection_clips::Column::ClipId]
}

fn montage_clips_index_columns() -> [entities::montage_clips::Column; 1] {
    [entities::montage_clips::Column::ClipId]
}

fn background_jobs_index_columns() -> [entities::background_jobs::Column; 2] {
    [
        entities::background_jobs::Column::State,
        entities::background_jobs::Column::UpdatedTs,
    ]
}

#[async_trait::async_trait]
impl MigrationTrait for AddHonuSessionId {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager
            .has_column(
                entities::clips::Entity.table_name(),
                entities::clips::Column::HonuSessionId.to_string(),
            )
            .await?
        {
            return Ok(());
        }

        let mut column = ColumnDef::new(entities::clips::Column::HonuSessionId);
        column.integer();
        manager
            .alter_table(
                Table::alter()
                    .table(entities::clips::Entity)
                    .add_column(&mut column)
                    .to_owned(),
            )
            .await
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddClipOrigin {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager
            .has_column(
                entities::clips::Entity.table_name(),
                entities::clips::Column::ClipOrigin.to_string(),
            )
            .await?
        {
            return Ok(());
        }

        let mut column = ColumnDef::new(entities::clips::Column::ClipOrigin);
        column.string().not_null().default("rule");
        manager
            .alter_table(
                Table::alter()
                    .table(entities::clips::Entity)
                    .add_column(&mut column)
                    .to_owned(),
            )
            .await
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddClipWindowColumns {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let has_clip_start_ts = manager
            .has_column(
                entities::clips::Entity.table_name(),
                entities::clips::Column::ClipStartTs.to_string(),
            )
            .await?;
        let has_clip_end_ts = manager
            .has_column(
                entities::clips::Entity.table_name(),
                entities::clips::Column::ClipEndTs.to_string(),
            )
            .await?;
        let has_session_id = manager
            .has_column(
                entities::clips::Entity.table_name(),
                entities::clips::Column::SessionId.to_string(),
            )
            .await?;

        if !has_clip_start_ts {
            let mut column = ColumnDef::new(entities::clips::Column::ClipStartTs);
            column.integer();
            manager
                .alter_table(
                    Table::alter()
                        .table(entities::clips::Entity)
                        .add_column(&mut column)
                        .to_owned(),
                )
                .await?;
        }

        if !has_clip_end_ts {
            let mut column = ColumnDef::new(entities::clips::Column::ClipEndTs);
            column.integer();
            manager
                .alter_table(
                    Table::alter()
                        .table(entities::clips::Entity)
                        .add_column(&mut column)
                        .to_owned(),
                )
                .await?;
        }

        if !has_session_id {
            let mut column = ColumnDef::new(entities::clips::Column::SessionId);
            column.string();
            manager
                .alter_table(
                    Table::alter()
                        .table(entities::clips::Entity)
                        .add_column(&mut column)
                        .to_owned(),
                )
                .await?;
        }

        if !has_clip_start_ts || !has_clip_end_ts {
            manager
                .get_connection()
                .execute_unprepared(
                    r#"
                    UPDATE clips
                    SET
                        clip_end_ts = COALESCE(clip_end_ts, trigger_event_ts),
                        clip_start_ts = COALESCE(
                            clip_start_ts,
                            trigger_event_ts - (clip_duration_secs * 1000)
                        )
                    "#,
                )
                .await?;
        }

        ensure_column_indexes(
            manager,
            entities::clips::Entity.table_name(),
            clips_window_index_columns(),
        )
        .await?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreateClipRawEventsTable {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.create_table(create_clip_raw_events_table()).await?;
        ensure_column_indexes(
            manager,
            entities::clip_raw_events::Entity.table_name(),
            clip_raw_events_index_columns(),
        )
        .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreateLookupCacheTable {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(entity_table(entities::lookup_cache::Entity))
            .await?;
        ensure_column_indexes(
            manager,
            entities::lookup_cache::Entity.table_name(),
            lookup_cache_index_columns(),
        )
        .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreateClipOverlapsTable {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.create_table(create_clip_overlaps_table()).await?;
        ensure_column_indexes(
            manager,
            entities::clip_overlaps::Entity.table_name(),
            clip_overlaps_index_columns(),
        )
        .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreateAlertTables {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(entity_table(entities::alert_instances::Entity))
            .await?;
        ensure_column_indexes(
            manager,
            entities::alert_instances::Entity.table_name(),
            alert_instances_index_columns(),
        )
        .await?;
        ensure_named_index(
            manager,
            entities::alert_instances::Entity.table_name(),
            "idx_alert_instances_zone_id",
            create_alert_instances_zone_world_index(),
        )
        .await?;

        manager
            .create_table(create_clip_alert_links_table())
            .await?;
        ensure_column_indexes(
            manager,
            entities::clip_alert_links::Entity.table_name(),
            clip_alert_links_index_columns(),
        )
        .await?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreateCharacterOutfitCacheTable {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(entity_table(entities::character_outfit_cache::Entity))
            .await?;
        ensure_column_indexes(
            manager,
            entities::character_outfit_cache::Entity.table_name(),
            character_outfit_cache_index_columns(),
        )
        .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreateWeaponReferenceCacheTable {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(entity_table(entities::weapon_reference_cache::Entity))
            .await?;
        ensure_column_indexes(
            manager,
            entities::weapon_reference_cache::Entity.table_name(),
            weapon_reference_cache_index_columns(),
        )
        .await?;
        ensure_named_index(
            manager,
            entities::weapon_reference_cache::Entity.table_name(),
            "idx_weapon_reference_cache_category",
            create_weapon_reference_cache_category_index(),
        )
        .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl MigrationTrait for RebuildWeaponReferenceCacheTable {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager
            .has_table(entities::weapon_reference_cache::Entity.table_name())
            .await?
        {
            return Ok(());
        }

        if manager
            .has_column(
                entities::weapon_reference_cache::Entity.table_name(),
                entities::weapon_reference_cache::Column::FactionId.to_string(),
            )
            .await?
        {
            return Ok(());
        }

        manager
            .drop_table(
                Table::drop()
                    .table(entities::weapon_reference_cache::Entity)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(entity_table(entities::weapon_reference_cache::Entity))
            .await?;
        ensure_column_indexes(
            manager,
            entities::weapon_reference_cache::Entity.table_name(),
            weapon_reference_cache_index_columns(),
        )
        .await?;
        ensure_named_index(
            manager,
            entities::weapon_reference_cache::Entity.table_name(),
            "idx_weapon_reference_cache_category",
            create_weapon_reference_cache_category_index(),
        )
        .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreateClipUploadsTable {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.create_table(create_clip_uploads_table()).await?;
        ensure_column_indexes(
            manager,
            entities::clip_uploads::Entity.table_name(),
            clip_uploads_index_columns(),
        )
        .await?;
        ensure_named_index(
            manager,
            entities::clip_uploads::Entity.table_name(),
            "idx_clip_uploads_clip_id",
            create_clip_uploads_clip_started_index(),
        )
        .await?;
        ensure_named_index(
            manager,
            entities::clip_uploads::Entity.table_name(),
            "idx_clip_uploads_provider",
            create_clip_uploads_provider_state_index(),
        )
        .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreateMontagesTable {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(entity_table(entities::montages::Entity))
            .await?;
        manager.create_table(create_montage_clips_table()).await?;
        ensure_column_indexes(
            manager,
            entities::montage_clips::Entity.table_name(),
            montage_clips_index_columns(),
        )
        .await?;
        ensure_named_index(
            manager,
            entities::montage_clips::Entity.table_name(),
            "idx_montage_clips_clip_id",
            create_montage_clips_clip_sequence_index(),
        )
        .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreateBackgroundJobsTable {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(entity_table(entities::background_jobs::Entity))
            .await?;
        ensure_column_indexes(
            manager,
            entities::background_jobs::Entity.table_name(),
            background_jobs_index_columns(),
        )
        .await?;
        ensure_named_index(
            manager,
            entities::background_jobs::Entity.table_name(),
            "idx_background_jobs_state",
            create_background_jobs_state_updated_index(),
        )
        .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreateClipAudioTracksAndPostProcessState {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(create_clip_audio_tracks_table())
            .await?;
        ensure_column_indexes(
            manager,
            entities::clip_audio_tracks::Entity.table_name(),
            clip_audio_tracks_index_columns(),
        )
        .await?;
        ensure_named_index(
            manager,
            entities::clip_audio_tracks::Entity.table_name(),
            "idx_clip_audio_tracks_clip_stream",
            create_clip_audio_tracks_clip_stream_index(),
        )
        .await?;

        if !manager
            .has_column(
                entities::clips::Entity.table_name(),
                entities::clips::Column::PostProcessStatus.to_string(),
            )
            .await?
        {
            manager
                .get_connection()
                .execute_unprepared(
                    "ALTER TABLE clips ADD COLUMN post_process_status TEXT NOT NULL DEFAULT 'Legacy' \
                     CHECK (post_process_status IN ('NotRequired', 'Pending', 'Completed', 'Failed', 'Legacy'))",
                )
                .await?;
        }

        if !manager
            .has_column(
                entities::clips::Entity.table_name(),
                entities::clips::Column::PostProcessError.to_string(),
            )
            .await?
        {
            manager
                .get_connection()
                .execute_unprepared("ALTER TABLE clips ADD COLUMN post_process_error TEXT NULL")
                .await?;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreateClipOrganizationTables {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager
            .has_column(
                entities::clips::Entity.table_name(),
                entities::clips::Column::Favorited.to_string(),
            )
            .await?
        {
            manager
                .get_connection()
                .execute_unprepared(
                    "ALTER TABLE clips ADD COLUMN favorited INTEGER NOT NULL DEFAULT 0",
                )
                .await?;
        }

        if !manager
            .has_table(entities::clip_tags::Entity.table_name())
            .await?
        {
            manager.create_table(create_clip_tags_table()).await?;
        }
        ensure_column_indexes(
            manager,
            entities::clip_tags::Entity.table_name(),
            clip_tags_index_columns(),
        )
        .await?;
        ensure_named_index(
            manager,
            entities::clip_tags::Entity.table_name(),
            "idx_clip_tags_clip_tag",
            create_clip_tags_clip_tag_unique_index(),
        )
        .await?;

        if !manager
            .has_table(entities::collections::Entity.table_name())
            .await?
        {
            manager
                .create_table(entity_table(entities::collections::Entity))
                .await?;
        }
        ensure_column_indexes(
            manager,
            entities::collections::Entity.table_name(),
            collections_index_columns(),
        )
        .await?;
        ensure_named_index(
            manager,
            entities::collections::Entity.table_name(),
            "idx_collections_name",
            create_collections_name_unique_index(),
        )
        .await?;

        if !manager
            .has_table(entities::collection_clips::Entity.table_name())
            .await?
        {
            manager
                .create_table(create_collection_clips_table())
                .await?;
        }
        ensure_column_indexes(
            manager,
            entities::collection_clips::Entity.table_name(),
            collection_clips_index_columns(),
        )
        .await?;
        ensure_named_index(
            manager,
            entities::collection_clips::Entity.table_name(),
            "idx_collection_clips_clip_id",
            create_collection_clips_clip_sequence_index(),
        )
        .await?;
        ensure_named_index(
            manager,
            entities::collection_clips::Entity.table_name(),
            "idx_collection_clips_collection_sequence",
            create_collection_clips_collection_sequence_index(),
        )
        .await?;

        Ok(())
    }
}
