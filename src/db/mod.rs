use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use auraxis::Faction;
use chrono::{DateTime, Utc};
use sea_orm::EntityTrait;
use sea_orm::sea_query::{
    Alias, Condition, Expr, ForeignKey, ForeignKeyAction, Func, Index, IndexCreateStatement,
    JoinType, OnConflict, Order, Query, Table, TableCreateStatement,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, ConnectOptions, ConnectionTrait, Database,
    DatabaseConnection, DatabaseTransaction, DbBackend, DbErr, EntityName,
    ExecResult as SeaExecResult, ExprTrait, QueryFilter, QueryOrder, QueryResult as SeaQueryResult,
    QuerySelect, Schema, Set, Statement, StatementBuilder, TransactionTrait, TryGetError,
    TryGetable, Value,
};
use sea_orm_migration::{MigratorTrait, SchemaManager};
use serde::Serialize;

use crate::background_jobs::{
    BackgroundJobId, BackgroundJobKind, BackgroundJobProgress, BackgroundJobRecord,
    BackgroundJobState,
};

const CLIP_STORE_SCHEMA_VERSION: i64 = 13;
const CHARACTER_OUTFIT_CACHE_TTL_MS: i64 = 2 * 60 * 60 * 1000;
const INTERRUPTED_BACKGROUND_JOB_DETAIL: &str =
    "Interrupted because nanite-clip closed before the background job finished.";

#[allow(dead_code)]
mod clips_repo;
mod core;
mod entities;
#[allow(dead_code)]
mod exports;
#[allow(dead_code)]
mod jobs_repo;
#[allow(dead_code)]
mod lookups_repo;
mod migrations;
#[allow(dead_code)]
mod schema;

pub(crate) use core::*;

#[allow(dead_code)]
mod primitives {
    use super::*;
    use std::ops::{Deref, DerefMut};

    pub type Pool = DatabaseConnection;

    #[derive(Debug, thiserror::Error)]
    pub enum Error {
        #[error(transparent)]
        Db(#[from] DbErr),
        #[error("row decode failed: {0:?}")]
        TryGet(TryGetError),
    }

    impl From<TryGetError> for Error {
        fn from(value: TryGetError) -> Self {
            Self::TryGet(value)
        }
    }

    #[derive(Debug)]
    pub struct Row(SeaQueryResult);

    impl Row {
        pub fn try_get<T>(&self, column: &str) -> Result<T, Error>
        where
            T: TryGetable,
        {
            self.0.try_get("", column).map_err(Error::from)
        }

        pub fn try_get_at<T>(&self, index: usize) -> Result<T, Error>
        where
            T: TryGetable,
        {
            self.0.try_get_by_index(index).map_err(Error::from)
        }
    }

    #[derive(Debug)]
    pub struct ExecResult(SeaExecResult);

    impl ExecResult {
        pub fn last_insert_rowid(&self) -> i64 {
            self.0.last_insert_id() as i64
        }
    }

    #[derive(Debug)]
    pub struct Transaction(DatabaseTransaction);

    impl Deref for Transaction {
        type Target = DatabaseTransaction;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl DerefMut for Transaction {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl Transaction {
        pub async fn commit(self) -> Result<(), Error> {
            self.0.commit().await.map_err(Error::from)
        }
    }

    pub trait IntoDbValue {
        fn into_db_value(self) -> Value;
    }

    impl IntoDbValue for i64 {
        fn into_db_value(self) -> Value {
            self.into()
        }
    }

    impl IntoDbValue for bool {
        fn into_db_value(self) -> Value {
            self.into()
        }
    }

    impl IntoDbValue for f32 {
        fn into_db_value(self) -> Value {
            self.into()
        }
    }

    impl IntoDbValue for String {
        fn into_db_value(self) -> Value {
            self.into()
        }
    }

    impl IntoDbValue for &str {
        fn into_db_value(self) -> Value {
            self.to_owned().into()
        }
    }

    impl IntoDbValue for Option<i64> {
        fn into_db_value(self) -> Value {
            self.into()
        }
    }

    impl IntoDbValue for Option<bool> {
        fn into_db_value(self) -> Value {
            self.into()
        }
    }

    impl IntoDbValue for Option<f32> {
        fn into_db_value(self) -> Value {
            self.into()
        }
    }

    impl IntoDbValue for Option<String> {
        fn into_db_value(self) -> Value {
            self.into()
        }
    }

    impl IntoDbValue for Option<&str> {
        fn into_db_value(self) -> Value {
            self.map(str::to_owned).into()
        }
    }

    impl<T> IntoDbValue for &T
    where
        T: Clone + IntoDbValue,
    {
        fn into_db_value(self) -> Value {
            self.clone().into_db_value()
        }
    }

    #[derive(Debug, Clone)]
    pub struct Query {
        sql: String,
        values: Vec<Value>,
    }

    impl Query {
        fn into_statement(self) -> Statement {
            Statement::from_sql_and_values(DbBackend::Sqlite, self.sql, self.values)
        }

        pub fn bind<T>(mut self, value: T) -> Self
        where
            T: IntoDbValue,
        {
            self.values.push(value.into_db_value());
            self
        }

        pub async fn execute<C>(self, connection: &C) -> Result<ExecResult, Error>
        where
            C: ConnectionTrait,
        {
            connection
                .execute_raw(self.into_statement())
                .await
                .map(ExecResult)
                .map_err(Error::from)
        }

        pub async fn fetch_all<C>(self, connection: &C) -> Result<Vec<Row>, Error>
        where
            C: ConnectionTrait,
        {
            connection
                .query_all_raw(self.into_statement())
                .await
                .map(|rows| rows.into_iter().map(Row).collect())
                .map_err(Error::from)
        }

        pub async fn fetch_one<C>(self, connection: &C) -> Result<Row, Error>
        where
            C: ConnectionTrait,
        {
            connection
                .query_one_raw(self.into_statement())
                .await
                .map_err(Error::from)?
                .map(Row)
                .ok_or_else(|| Error::Db(DbErr::RecordNotFound("expected a row".into())))
        }

        pub async fn fetch_optional<C>(self, connection: &C) -> Result<Option<Row>, Error>
        where
            C: ConnectionTrait,
        {
            connection
                .query_one_raw(self.into_statement())
                .await
                .map(|row| row.map(Row))
                .map_err(Error::from)
        }
    }

    #[derive(Debug, Clone)]
    pub struct ScalarQuery(Query);

    impl ScalarQuery {
        pub fn bind<T>(mut self, value: T) -> Self
        where
            T: IntoDbValue,
        {
            self.0 = self.0.bind(value);
            self
        }

        pub async fn fetch_one<T, C>(self, connection: &C) -> Result<T, Error>
        where
            T: TryGetable,
            C: ConnectionTrait,
        {
            self.0.fetch_one(connection).await?.try_get_at(0)
        }

        pub async fn fetch_all<T, C>(self, connection: &C) -> Result<Vec<T>, Error>
        where
            T: TryGetable,
            C: ConnectionTrait,
        {
            self.0
                .fetch_all(connection)
                .await?
                .into_iter()
                .map(|row| row.try_get_at(0))
                .collect()
        }

        pub async fn fetch_optional<T, C>(self, connection: &C) -> Result<Option<T>, Error>
        where
            T: TryGetable,
            C: ConnectionTrait,
        {
            self.0
                .fetch_optional(connection)
                .await?
                .map(|row| row.try_get_at(0))
                .transpose()
        }
    }

    pub fn query(sql: impl Into<String>) -> Query {
        Query {
            sql: sql.into(),
            values: Vec::new(),
        }
    }

    pub fn query_scalar(sql: impl Into<String>) -> ScalarQuery {
        ScalarQuery(query(sql))
    }

    fn build_statement<S>(statement: &S) -> Statement
    where
        S: StatementBuilder,
    {
        DbBackend::Sqlite.build(statement)
    }

    pub async fn execute_stmt<C, S>(statement: &S, connection: &C) -> Result<ExecResult, Error>
    where
        C: ConnectionTrait,
        S: StatementBuilder,
    {
        connection
            .execute_raw(build_statement(statement))
            .await
            .map(ExecResult)
            .map_err(Error::from)
    }

    pub async fn fetch_all_stmt<C, S>(statement: &S, connection: &C) -> Result<Vec<Row>, Error>
    where
        C: ConnectionTrait,
        S: StatementBuilder,
    {
        connection
            .query_all_raw(build_statement(statement))
            .await
            .map(|rows| rows.into_iter().map(Row).collect())
            .map_err(Error::from)
    }

    pub async fn fetch_one_stmt<C, S>(statement: &S, connection: &C) -> Result<Row, Error>
    where
        C: ConnectionTrait,
        S: StatementBuilder,
    {
        connection
            .query_one_raw(build_statement(statement))
            .await
            .map_err(Error::from)?
            .map(Row)
            .ok_or_else(|| Error::Db(DbErr::RecordNotFound("expected a row".into())))
    }

    pub async fn fetch_optional_stmt<C, S>(
        statement: &S,
        connection: &C,
    ) -> Result<Option<Row>, Error>
    where
        C: ConnectionTrait,
        S: StatementBuilder,
    {
        connection
            .query_one_raw(build_statement(statement))
            .await
            .map(|row| row.map(Row))
            .map_err(Error::from)
    }

    pub async fn begin(pool: &Pool) -> Result<Transaction, Error> {
        pool.begin().await.map(Transaction).map_err(Error::from)
    }

    pub async fn connect_at(path: &Path, max_connections: u32) -> Result<Pool, Error> {
        let mut options = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
        options.max_connections(max_connections);
        options.sqlx_logging(false);
        options.after_connect(|connection| {
            Box::pin(async move {
                connection
                    .execute_unprepared("PRAGMA foreign_keys = ON")
                    .await?;
                connection
                    .execute_unprepared("PRAGMA journal_mode = WAL")
                    .await?;
                Ok(())
            })
        });
        Database::connect(options).await.map_err(Error::from)
    }

    pub async fn connect_in_memory(max_connections: u32) -> Result<Pool, Error> {
        let mut options = ConnectOptions::new("sqlite::memory:?cache=shared");
        options.max_connections(max_connections);
        options.sqlx_logging(false);
        options.after_connect(|connection| {
            Box::pin(async move {
                connection
                    .execute_unprepared("PRAGMA foreign_keys = ON")
                    .await?;
                Ok(())
            })
        });
        Database::connect(options).await.map_err(Error::from)
    }
}

#[derive(Clone)]
pub struct ClipStore {
    pool: primitives::Pool,
    database_path: Option<PathBuf>,
    startup_notice: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LookupKind {
    Facility,
    Vehicle,
    Zone,
    Character,
    Outfit,
    Weapon,
}

impl LookupKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Facility => "facility",
            Self::Vehicle => "vehicle",
            Self::Zone => "zone",
            Self::Character => "character",
            Self::Outfit => "outfit",
            Self::Weapon => "weapon",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterOutfitCacheEntry {
    pub outfit_id: Option<u64>,
    pub outfit_tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeaponReferenceCacheEntry {
    pub item_id: u32,
    pub weapon_id: u32,
    pub display_name: String,
    pub category_label: String,
    pub faction: Option<Faction>,
    pub weapon_group_id: Option<u32>,
}

impl std::fmt::Debug for ClipStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipStore").finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ClipFilters {
    pub search: String,
    pub event_after_ts: Option<i64>,
    pub event_before_ts: Option<i64>,
    pub target: String,
    pub weapon: String,
    pub alert: String,
    pub overlap_state: OverlapFilterState,
    pub profile: String,
    pub rule: String,
    pub character: String,
    pub server: String,
    pub continent: String,
    pub base: String,
}

#[derive(Debug, Clone, Default)]
pub struct ClipFilterOptions {
    pub profiles: Vec<String>,
    pub rules: Vec<String>,
    pub characters: Vec<String>,
    pub servers: Vec<String>,
    pub continents: Vec<String>,
    pub bases: Vec<String>,
    pub targets: Vec<String>,
    pub weapons: Vec<String>,
    pub alerts: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverlapFilterState {
    #[default]
    All,
    Overlapping,
    UniqueOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClipEventContribution {
    pub event_kind: String,
    pub occurrences: u32,
    pub points: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ClipOrigin {
    Rule,
    Manual,
    Imported,
}

impl ClipOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::Manual => "manual",
            Self::Imported => "imported",
        }
    }

    fn from_db(value: &str) -> Self {
        match value {
            "manual" => Self::Manual,
            "imported" => Self::Imported,
            _ => Self::Rule,
        }
    }
}

impl std::fmt::Display for ClipOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Rule => "Rule",
            Self::Manual => "Manual",
            Self::Imported => "Imported",
        })
    }
}

#[derive(Debug, Clone)]
pub struct ClipDraft {
    pub trigger_event_at: DateTime<Utc>,
    pub clip_start_at: DateTime<Utc>,
    pub clip_end_at: DateTime<Utc>,
    pub saved_at: DateTime<Utc>,
    pub origin: ClipOrigin,
    pub profile_id: String,
    pub rule_id: String,
    pub clip_duration_secs: u32,
    pub session_id: Option<String>,
    pub character_id: u64,
    pub world_id: u32,
    pub zone_id: Option<u32>,
    pub facility_id: Option<u32>,
    pub score: u32,
    pub honu_session_id: Option<i64>,
    pub path: Option<String>,
    pub events: Vec<ClipEventContribution>,
    pub raw_events: Vec<ClipRawEventDraft>,
    pub alert_keys: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ClipRecord {
    pub id: i64,
    pub trigger_event_at: DateTime<Utc>,
    pub clip_start_at: DateTime<Utc>,
    pub clip_end_at: DateTime<Utc>,
    pub saved_at: DateTime<Utc>,
    pub origin: ClipOrigin,
    pub profile_id: String,
    pub rule_id: String,
    pub clip_duration_secs: u32,
    pub session_id: Option<String>,
    pub character_id: u64,
    pub world_id: u32,
    pub zone_id: Option<u32>,
    pub facility_id: Option<u32>,
    pub zone_name: Option<String>,
    pub facility_name: Option<String>,
    pub score: u32,
    pub honu_session_id: Option<i64>,
    pub path: Option<String>,
    pub file_size_bytes: Option<u64>,
    pub overlap_count: u32,
    pub alert_count: u32,
    pub post_process_status: PostProcessStatus,
    pub post_process_error: Option<String>,
    pub events: Vec<ClipEventContribution>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClipRawEventDraft {
    pub event_at: DateTime<Utc>,
    pub event_kind: String,
    pub world_id: u32,
    pub zone_id: Option<u32>,
    pub facility_id: Option<u32>,
    pub actor_character_id: Option<u64>,
    pub other_character_id: Option<u64>,
    pub actor_class: Option<String>,
    pub attacker_weapon_id: Option<u32>,
    pub attacker_vehicle_id: Option<u16>,
    pub vehicle_killed_id: Option<u16>,
    pub characters_killed: u32,
    pub is_headshot: bool,
    pub experience_id: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClipRawEventRecord {
    pub event_at: DateTime<Utc>,
    pub event_kind: String,
    pub world_id: u32,
    pub zone_id: Option<u32>,
    pub zone_name: Option<String>,
    pub facility_id: Option<u32>,
    pub facility_name: Option<String>,
    pub actor_character_id: Option<u64>,
    pub actor_character_name: Option<String>,
    pub other_character_id: Option<u64>,
    pub other_character_name: Option<String>,
    pub actor_class: Option<String>,
    pub attacker_weapon_id: Option<u32>,
    pub attacker_weapon_name: Option<String>,
    pub attacker_vehicle_id: Option<u16>,
    pub attacker_vehicle_name: Option<String>,
    pub vehicle_killed_id: Option<u16>,
    pub vehicle_killed_name: Option<String>,
    pub characters_killed: u32,
    pub is_headshot: bool,
    pub experience_id: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct ClipDetailRecord {
    pub clip: ClipRecord,
    pub audio_tracks: Vec<ClipAudioTrackRecord>,
    pub raw_events: Vec<ClipRawEventRecord>,
    pub alerts: Vec<ClipAlertRecord>,
    pub overlaps: Vec<ClipOverlapRecord>,
    pub uploads: Vec<ClipUploadRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClipAudioTrackRecord {
    pub id: i64,
    pub clip_id: i64,
    pub stream_index: i32,
    pub role: String,
    pub label: String,
    pub gain_db: f32,
    pub muted: bool,
    pub source_kind: String,
    pub source_value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClipAudioTrackDraft {
    pub stream_index: i32,
    pub role: String,
    pub label: String,
    pub gain_db: f32,
    pub muted: bool,
    pub source_kind: String,
    pub source_value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PostProcessStatus {
    NotRequired,
    Pending,
    Completed,
    Failed,
    Legacy,
}

impl PostProcessStatus {
    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotRequired => "NotRequired",
            Self::Pending => "Pending",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::Legacy => "Legacy",
        }
    }

    fn from_entity(value: entities::PostProcessStatus) -> Self {
        match value {
            entities::PostProcessStatus::NotRequired => Self::NotRequired,
            entities::PostProcessStatus::Pending => Self::Pending,
            entities::PostProcessStatus::Completed => Self::Completed,
            entities::PostProcessStatus::Failed => Self::Failed,
            entities::PostProcessStatus::Legacy => Self::Legacy,
        }
    }

    fn into_entity(self) -> entities::PostProcessStatus {
        match self {
            Self::NotRequired => entities::PostProcessStatus::NotRequired,
            Self::Pending => entities::PostProcessStatus::Pending,
            Self::Completed => entities::PostProcessStatus::Completed,
            Self::Failed => entities::PostProcessStatus::Failed,
            Self::Legacy => entities::PostProcessStatus::Legacy,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClipAlertRecord {
    pub alert_key: String,
    pub label: String,
    pub world_id: u32,
    pub zone_id: u32,
    pub metagame_event_id: u8,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub state_name: String,
    pub winner_faction: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipOverlapRecord {
    pub clip_id: i64,
    pub trigger_event_at: DateTime<Utc>,
    pub clip_start_at: DateTime<Utc>,
    pub clip_end_at: DateTime<Utc>,
    pub profile_id: String,
    pub rule_id: String,
    pub path: Option<String>,
    pub overlap_duration_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum UploadProvider {
    Copyparty,
    YouTube,
}

impl UploadProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Copyparty => "copyparty",
            Self::YouTube => "youtube",
        }
    }

    fn from_db(value: &str) -> Self {
        match value {
            "youtube" => Self::YouTube,
            "streamable" | "copyparty" => Self::Copyparty,
            _ => Self::Copyparty,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Copyparty => "Copyparty",
            Self::YouTube => "YouTube",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ClipUploadState {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl ClipUploadState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn from_db(value: &str) -> Self {
        match value {
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => Self::Running,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ClipUploadRecord {
    pub id: i64,
    pub provider: UploadProvider,
    pub state: ClipUploadState,
    pub external_id: Option<String>,
    pub clip_url: Option<String>,
    pub error_message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct ClipUploadDraft {
    pub clip_id: i64,
    pub provider: UploadProvider,
    pub state: ClipUploadState,
    pub external_id: Option<String>,
    pub clip_url: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MontageRecord {
    pub id: i64,
    pub output_path: String,
    pub created_at: DateTime<Utc>,
    pub source_clip_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlertInstanceRecord {
    pub alert_key: String,
    pub label: String,
    pub world_id: u32,
    pub zone_id: u32,
    pub metagame_event_id: u8,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub state_name: String,
    pub winner_faction: Option<String>,
    pub faction_nc: f32,
    pub faction_tr: f32,
    pub faction_vs: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountByLabel {
    pub label: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseCount {
    pub facility_id: Option<u32>,
    pub label: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipStatsSnapshot {
    pub total_clips: u32,
    pub total_duration_secs: u64,
    pub total_score_sum: u64,
    pub clips_per_day: Vec<CountByLabel>,
    pub clips_per_rule: Vec<CountByLabel>,
    pub score_distribution: Vec<CountByLabel>,
    pub top_bases: Vec<BaseCount>,
    pub top_weapons: Vec<CountByLabel>,
    pub top_targets: Vec<CountByLabel>,
    pub raw_event_kinds: Vec<CountByLabel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub session_id: String,
    pub total_clips: u32,
    pub total_duration_secs: u64,
    pub unique_bases: u32,
    pub top_clip: Option<ClipSummaryItem>,
    pub rule_breakdown: Vec<CountByLabel>,
    pub base_breakdown: Vec<BaseCount>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipSummaryItem {
    pub clip_id: i64,
    pub rule_id: String,
    pub score: u32,
    pub trigger_event_at: DateTime<Utc>,
    pub clip_duration_secs: u32,
}

impl ClipStore {
    pub async fn open_default() -> Result<Self, ClipStoreError> {
        let path = database_path();
        Self::open_at(path).await
    }

    pub async fn open_at(path: impl Into<PathBuf>) -> Result<Self, ClipStoreError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let pool = primitives::connect_at(&path, 4).await?;

        let store = Self {
            pool,
            database_path: Some(path),
            startup_notice: None,
        };
        let mut store = store;
        store.startup_notice = store.initialize_schema().await?;
        Ok(store)
    }

    #[cfg(test)]
    pub(crate) async fn open_in_memory() -> Result<Self, ClipStoreError> {
        let pool = primitives::connect_in_memory(1).await?;
        let store = Self {
            pool,
            database_path: None,
            startup_notice: None,
        };
        store.reset_schema().await?;
        Ok(store)
    }

    pub async fn insert_clip(&self, clip: ClipDraft) -> Result<i64, ClipStoreError> {
        let clip_start_ms = clip.clip_start_at.timestamp_millis();
        let clip_end_ms = clip.clip_end_at.timestamp_millis();
        let tx = primitives::begin(&self.pool).await?;

        let inserted = entities::clips::ActiveModel {
            id: NotSet,
            trigger_event_ts: Set(clip.trigger_event_at.timestamp_millis()),
            clip_start_ts: Set(clip_start_ms),
            clip_end_ts: Set(clip_end_ms),
            saved_ts: Set(clip.saved_at.timestamp_millis()),
            clip_origin: Set(clip.origin.as_str().to_string()),
            rule_id: Set(clip.rule_id),
            clip_duration_secs: Set(i64::from(clip.clip_duration_secs)),
            session_id: Set(clip.session_id),
            character_id: Set(clip.character_id as i64),
            world_id: Set(clip.world_id as i64),
            zone_id: Set(clip.zone_id.map(i64::from)),
            facility_id: Set(clip.facility_id.map(i64::from)),
            profile_id: Set(clip.profile_id),
            path: Set(clip.path),
            score: Set(i64::from(clip.score)),
            honu_session_id: Set(clip.honu_session_id),
            post_process_status: Set(entities::PostProcessStatus::NotRequired),
            post_process_error: Set(None),
        }
        .insert(&*tx)
        .await?;
        let clip_id = inserted.id;

        if !clip.events.is_empty() {
            let event_models = clip
                .events
                .into_iter()
                .map(|event| entities::clip_events::ActiveModel {
                    id: NotSet,
                    clip_id: Set(clip_id),
                    event_kind: Set(event.event_kind),
                    occurrences: Set(i64::from(event.occurrences)),
                    points: Set(i64::from(event.points)),
                })
                .collect::<Vec<_>>();
            entities::clip_events::Entity::insert_many(event_models)
                .exec(&*tx)
                .await?;
        }

        if !clip.raw_events.is_empty() {
            let raw_event_models = clip
                .raw_events
                .into_iter()
                .map(|event| entities::clip_raw_events::ActiveModel {
                    id: NotSet,
                    clip_id: Set(clip_id),
                    event_ts: Set(event.event_at.timestamp_millis()),
                    event_kind: Set(event.event_kind),
                    world_id: Set(i64::from(event.world_id)),
                    zone_id: Set(event.zone_id.map(i64::from)),
                    facility_id: Set(event.facility_id.map(i64::from)),
                    actor_character_id: Set(event.actor_character_id.map(|id| id as i64)),
                    other_character_id: Set(event.other_character_id.map(|id| id as i64)),
                    actor_class: Set(event.actor_class),
                    attacker_weapon_id: Set(event.attacker_weapon_id.map(i64::from)),
                    attacker_vehicle_id: Set(event.attacker_vehicle_id.map(i64::from)),
                    vehicle_killed_id: Set(event.vehicle_killed_id.map(i64::from)),
                    characters_killed: Set(i64::from(event.characters_killed)),
                    is_headshot: Set(event.is_headshot),
                    experience_id: Set(event.experience_id.map(i64::from)),
                })
                .collect::<Vec<_>>();
            entities::clip_raw_events::Entity::insert_many(raw_event_models)
                .exec(&*tx)
                .await?;
        }

        if !clip.alert_keys.is_empty() {
            let alert_links = clip
                .alert_keys
                .into_iter()
                .map(|alert_key| entities::clip_alert_links::ActiveModel {
                    clip_id: Set(clip_id),
                    alert_key: Set(alert_key),
                })
                .collect::<Vec<_>>();
            entities::clip_alert_links::Entity::insert_many(alert_links)
                .on_conflict(
                    OnConflict::columns([
                        entities::clip_alert_links::Column::ClipId,
                        entities::clip_alert_links::Column::AlertKey,
                    ])
                    .do_nothing()
                    .to_owned(),
                )
                .exec(&*tx)
                .await?;
        }

        let overlapping_clips = entities::clips::Entity::find()
            .filter(entities::clips::Column::Id.ne(clip_id))
            .filter(entities::clips::Column::ClipEndTs.gt(clip_start_ms))
            .filter(entities::clips::Column::ClipStartTs.lt(clip_end_ms))
            .all(&*tx)
            .await?;

        for other_clip in overlapping_clips {
            let overlap_duration_ms = std::cmp::min(clip_end_ms, other_clip.clip_end_ts)
                - std::cmp::max(clip_start_ms, other_clip.clip_start_ts);
            if overlap_duration_ms <= 0 {
                continue;
            }

            let (left_id, right_id) = if clip_id < other_clip.id {
                (clip_id, other_clip.id)
            } else {
                (other_clip.id, clip_id)
            };

            entities::clip_overlaps::Entity::insert(entities::clip_overlaps::ActiveModel {
                clip_id: Set(left_id),
                overlap_clip_id: Set(right_id),
                overlap_duration_ms: Set(overlap_duration_ms),
                detected_ts: Set(Utc::now().timestamp_millis()),
            })
            .on_conflict(
                OnConflict::columns([
                    entities::clip_overlaps::Column::ClipId,
                    entities::clip_overlaps::Column::OverlapClipId,
                ])
                .update_columns([
                    entities::clip_overlaps::Column::OverlapDurationMs,
                    entities::clip_overlaps::Column::DetectedTs,
                ])
                .to_owned(),
            )
            .exec(&*tx)
            .await?;
        }

        tx.commit().await?;
        Ok(clip_id)
    }

    pub async fn recent_clips(&self, limit: i64) -> Result<Vec<ClipRecord>, ClipStoreError> {
        let clips = entities::clips::Entity::find()
            .order_by_desc(entities::clips::Column::TriggerEventTs)
            .order_by_desc(entities::clips::Column::Id)
            .limit(std::cmp::max(limit, 0) as u64)
            .all(&self.pool)
            .await?;

        self.hydrate_clip_records(clips).await
    }

    pub async fn search_clips(
        &self,
        filters: &ClipFilters,
        limit: i64,
    ) -> Result<Vec<ClipRecord>, ClipStoreError> {
        let target_filter = filters.target.trim().to_string();
        let weapon_filter = filters.weapon.trim().to_string();
        let target_like = format!("%{target_filter}%");
        let target_like_lower = target_like.to_lowercase();
        let weapon_like = format!("%{weapon_filter}%");
        let weapon_like_lower = weapon_like.to_lowercase();
        let alert_filter = filters.alert.trim().to_string();
        let alert_like_lower = format!("%{alert_filter}%").to_lowercase();

        let mut query = entities::clips::Entity::find();

        if let Some(after_ts) = filters.event_after_ts {
            query = query.filter(entities::clips::Column::TriggerEventTs.gte(after_ts));
        }
        if let Some(before_ts) = filters.event_before_ts {
            query = query.filter(entities::clips::Column::TriggerEventTs.lte(before_ts));
        }

        if !target_filter.is_empty() {
            let target_lookup = Alias::new("target_lookup");
            let mut exists = Query::select();
            exists
                .expr(Expr::val(1))
                .from(entities::clip_raw_events::Entity)
                .join_as(
                    JoinType::LeftJoin,
                    entities::lookup_cache::Entity,
                    target_lookup.clone(),
                    Condition::all()
                        .add(
                            Expr::col((
                                target_lookup.clone(),
                                entities::lookup_cache::Column::LookupKind,
                            ))
                            .eq(LookupKind::Character.as_str()),
                        )
                        .add(
                            Expr::col((
                                target_lookup.clone(),
                                entities::lookup_cache::Column::LookupId,
                            ))
                            .equals((
                                entities::clip_raw_events::Entity,
                                entities::clip_raw_events::Column::OtherCharacterId,
                            )),
                        ),
                )
                .cond_where(
                    Expr::col((
                        entities::clip_raw_events::Entity,
                        entities::clip_raw_events::Column::ClipId,
                    ))
                    .equals((entities::clips::Entity, entities::clips::Column::Id)),
                )
                .cond_where(
                    Condition::any()
                        .add(Expr::cust_with_values(
                            "CAST(\"clip_raw_events\".\"other_character_id\" AS TEXT) LIKE ?",
                            [target_like.clone()],
                        ))
                        .add(Expr::cust_with_values(
                            "LOWER(COALESCE(\"target_lookup\".\"display_name\", '')) LIKE ?",
                            [target_like_lower.clone()],
                        )),
                );
            query = query.filter(Expr::exists(exists));
        }

        if !weapon_filter.is_empty() {
            let weapon_lookup = Alias::new("weapon_lookup");
            let mut exists = Query::select();
            exists
                .expr(Expr::val(1))
                .from(entities::clip_raw_events::Entity)
                .join_as(
                    JoinType::LeftJoin,
                    entities::lookup_cache::Entity,
                    weapon_lookup.clone(),
                    Condition::all()
                        .add(
                            Expr::col((
                                weapon_lookup.clone(),
                                entities::lookup_cache::Column::LookupKind,
                            ))
                            .eq(LookupKind::Weapon.as_str()),
                        )
                        .add(
                            Expr::col((
                                weapon_lookup.clone(),
                                entities::lookup_cache::Column::LookupId,
                            ))
                            .equals((
                                entities::clip_raw_events::Entity,
                                entities::clip_raw_events::Column::AttackerWeaponId,
                            )),
                        ),
                )
                .cond_where(
                    Expr::col((
                        entities::clip_raw_events::Entity,
                        entities::clip_raw_events::Column::ClipId,
                    ))
                    .equals((entities::clips::Entity, entities::clips::Column::Id)),
                )
                .cond_where(
                    Condition::any()
                        .add(Expr::cust_with_values(
                            "CAST(\"clip_raw_events\".\"attacker_weapon_id\" AS TEXT) LIKE ?",
                            [weapon_like.clone()],
                        ))
                        .add(Expr::cust_with_values(
                            "LOWER(COALESCE(\"weapon_lookup\".\"display_name\", '')) LIKE ?",
                            [weapon_like_lower.clone()],
                        )),
                );
            query = query.filter(Expr::exists(exists));
        }

        if !alert_filter.is_empty() {
            let mut exists = Query::select();
            exists
                .expr(Expr::val(1))
                .from(entities::clip_alert_links::Entity)
                .join(
                    JoinType::InnerJoin,
                    entities::alert_instances::Entity,
                    Expr::col((
                        entities::alert_instances::Entity,
                        entities::alert_instances::Column::AlertKey,
                    ))
                    .equals((
                        entities::clip_alert_links::Entity,
                        entities::clip_alert_links::Column::AlertKey,
                    )),
                )
                .cond_where(
                    Expr::col((
                        entities::clip_alert_links::Entity,
                        entities::clip_alert_links::Column::ClipId,
                    ))
                    .equals((entities::clips::Entity, entities::clips::Column::Id)),
                )
                .cond_where(Expr::cust_with_values(
                    "LOWER(\"alert_instances\".\"label\") LIKE ?",
                    [alert_like_lower],
                ));
            query = query.filter(Expr::exists(exists));
        }

        match filters.overlap_state {
            OverlapFilterState::All => {}
            OverlapFilterState::Overlapping => {
                let mut exists = Query::select();
                exists
                    .expr(Expr::val(1))
                    .from(entities::clip_overlaps::Entity)
                    .cond_where(
                        Condition::any()
                            .add(
                                Expr::col((
                                    entities::clip_overlaps::Entity,
                                    entities::clip_overlaps::Column::ClipId,
                                ))
                                .equals((entities::clips::Entity, entities::clips::Column::Id)),
                            )
                            .add(
                                Expr::col((
                                    entities::clip_overlaps::Entity,
                                    entities::clip_overlaps::Column::OverlapClipId,
                                ))
                                .equals((entities::clips::Entity, entities::clips::Column::Id)),
                            ),
                    );
                query = query.filter(Expr::exists(exists));
            }
            OverlapFilterState::UniqueOnly => {
                let mut exists = Query::select();
                exists
                    .expr(Expr::val(1))
                    .from(entities::clip_overlaps::Entity)
                    .cond_where(
                        Condition::any()
                            .add(
                                Expr::col((
                                    entities::clip_overlaps::Entity,
                                    entities::clip_overlaps::Column::ClipId,
                                ))
                                .equals((entities::clips::Entity, entities::clips::Column::Id)),
                            )
                            .add(
                                Expr::col((
                                    entities::clip_overlaps::Entity,
                                    entities::clip_overlaps::Column::OverlapClipId,
                                ))
                                .equals((entities::clips::Entity, entities::clips::Column::Id)),
                            ),
                    );
                query = query.filter(Expr::not_exists(exists));
            }
        }

        let clips = query
            .order_by_desc(entities::clips::Column::TriggerEventTs)
            .order_by_desc(entities::clips::Column::Id)
            .limit(std::cmp::max(limit, 0) as u64)
            .all(&self.pool)
            .await?;

        self.hydrate_clip_records(clips).await
    }

    pub async fn raw_event_filter_options(&self) -> Result<ClipFilterOptions, ClipStoreError> {
        let character_lookup = Alias::new("character_lookup");
        let mut target_query = Query::select();
        target_query
            .distinct()
            .expr_as(
                Expr::cust(
                    "COALESCE(\"character_lookup\".\"display_name\", CAST(\"clip_raw_events\".\"other_character_id\" AS TEXT))",
                ),
                Alias::new("label"),
            )
            .from(entities::clip_raw_events::Entity)
            .join_as(
                JoinType::LeftJoin,
                entities::lookup_cache::Entity,
                character_lookup.clone(),
                Condition::all()
                    .add(
                        Expr::col((
                            character_lookup.clone(),
                            entities::lookup_cache::Column::LookupKind,
                        ))
                        .eq(LookupKind::Character.as_str()),
                    )
                    .add(
                        Expr::col((
                            character_lookup.clone(),
                            entities::lookup_cache::Column::LookupId,
                        ))
                        .equals((
                            entities::clip_raw_events::Entity,
                            entities::clip_raw_events::Column::OtherCharacterId,
                        )),
                    ),
            )
            .cond_where(
                entities::clip_raw_events::Column::OtherCharacterId.is_not_null(),
            )
            .order_by_expr(
                Expr::cust(
                    "LOWER(COALESCE(\"character_lookup\".\"display_name\", CAST(\"clip_raw_events\".\"other_character_id\" AS TEXT)))",
                ),
                Order::Asc,
            );
        let target_rows = primitives::fetch_all_stmt(&target_query, &self.pool).await?;

        let weapon_lookup = Alias::new("weapon_lookup");
        let mut weapon_query = Query::select();
        weapon_query
            .distinct()
            .expr_as(
                Expr::cust(
                    "COALESCE(\"weapon_lookup\".\"display_name\", CAST(\"clip_raw_events\".\"attacker_weapon_id\" AS TEXT))",
                ),
                Alias::new("label"),
            )
            .from(entities::clip_raw_events::Entity)
            .join_as(
                JoinType::LeftJoin,
                entities::lookup_cache::Entity,
                weapon_lookup.clone(),
                Condition::all()
                    .add(
                        Expr::col((
                            weapon_lookup.clone(),
                            entities::lookup_cache::Column::LookupKind,
                        ))
                        .eq(LookupKind::Weapon.as_str()),
                    )
                    .add(
                        Expr::col((
                            weapon_lookup.clone(),
                            entities::lookup_cache::Column::LookupId,
                        ))
                        .equals((
                            entities::clip_raw_events::Entity,
                            entities::clip_raw_events::Column::AttackerWeaponId,
                        )),
                    ),
            )
            .cond_where(
                entities::clip_raw_events::Column::AttackerWeaponId.is_not_null(),
            )
            .order_by_expr(
                Expr::cust(
                    "LOWER(COALESCE(\"weapon_lookup\".\"display_name\", CAST(\"clip_raw_events\".\"attacker_weapon_id\" AS TEXT)))",
                ),
                Order::Asc,
            );
        let weapon_rows = primitives::fetch_all_stmt(&weapon_query, &self.pool).await?;

        let mut alert_query = Query::select();
        alert_query
            .distinct()
            .column(entities::alert_instances::Column::Label)
            .from(entities::alert_instances::Entity)
            .order_by_expr(
                Expr::cust("LOWER(\"alert_instances\".\"label\")"),
                Order::Asc,
            );
        let alert_rows = primitives::fetch_all_stmt(&alert_query, &self.pool).await?;

        Ok(ClipFilterOptions {
            targets: read_string_option_rows(target_rows)?,
            weapons: read_string_option_rows(weapon_rows)?,
            alerts: read_string_option_rows(alert_rows)?,
            ..ClipFilterOptions::default()
        })
    }

    pub async fn clip_detail(
        &self,
        clip_id: i64,
    ) -> Result<Option<ClipDetailRecord>, ClipStoreError> {
        let Some(clip_model) = entities::clips::Entity::find_by_id(clip_id)
            .one(&self.pool)
            .await?
        else {
            return Ok(None);
        };
        let clip = self
            .hydrate_clip_records(vec![clip_model])
            .await?
            .into_iter()
            .next()
            .expect("clip detail hydration should return the requested clip");

        let raw_models = entities::clip_raw_events::Entity::find()
            .filter(entities::clip_raw_events::Column::ClipId.eq(clip_id))
            .order_by_asc(entities::clip_raw_events::Column::EventTs)
            .order_by_asc(entities::clip_raw_events::Column::Id)
            .all(&self.pool)
            .await?;
        let raw_zone_names = self
            .lookup_name_map(
                LookupKind::Zone,
                raw_models
                    .iter()
                    .filter_map(|event| event.zone_id)
                    .collect::<BTreeSet<_>>(),
            )
            .await?;
        let raw_facility_names = self
            .lookup_name_map(
                LookupKind::Facility,
                raw_models
                    .iter()
                    .filter_map(|event| event.facility_id)
                    .collect::<BTreeSet<_>>(),
            )
            .await?;
        let actor_names = self
            .lookup_name_map(
                LookupKind::Character,
                raw_models
                    .iter()
                    .flat_map(|event| [event.actor_character_id, event.other_character_id])
                    .flatten()
                    .collect::<BTreeSet<_>>(),
            )
            .await?;
        let weapon_names = self
            .lookup_name_map(
                LookupKind::Weapon,
                raw_models
                    .iter()
                    .filter_map(|event| event.attacker_weapon_id)
                    .collect::<BTreeSet<_>>(),
            )
            .await?;
        let vehicle_names = self
            .lookup_name_map(
                LookupKind::Vehicle,
                raw_models
                    .iter()
                    .flat_map(|event| [event.attacker_vehicle_id, event.vehicle_killed_id])
                    .flatten()
                    .collect::<BTreeSet<_>>(),
            )
            .await?;
        let raw_events = raw_models
            .into_iter()
            .map(|event| {
                Ok(ClipRawEventRecord {
                    event_at: timestamp_millis_to_utc(event.event_ts)?,
                    event_kind: event.event_kind,
                    world_id: event.world_id as u32,
                    zone_id: event.zone_id.map(|id| id as u32),
                    zone_name: event
                        .zone_id
                        .and_then(|id| raw_zone_names.get(&id).cloned()),
                    facility_id: event.facility_id.map(|id| id as u32),
                    facility_name: event
                        .facility_id
                        .and_then(|id| raw_facility_names.get(&id).cloned()),
                    actor_character_id: event.actor_character_id.map(|id| id as u64),
                    actor_character_name: event
                        .actor_character_id
                        .and_then(|id| actor_names.get(&id).cloned()),
                    other_character_id: event.other_character_id.map(|id| id as u64),
                    other_character_name: event
                        .other_character_id
                        .and_then(|id| actor_names.get(&id).cloned()),
                    actor_class: event.actor_class,
                    attacker_weapon_id: event.attacker_weapon_id.map(|id| id as u32),
                    attacker_weapon_name: event
                        .attacker_weapon_id
                        .and_then(|id| weapon_names.get(&id).cloned()),
                    attacker_vehicle_id: event.attacker_vehicle_id.map(|id| id as u16),
                    attacker_vehicle_name: event
                        .attacker_vehicle_id
                        .and_then(|id| vehicle_names.get(&id).cloned()),
                    vehicle_killed_id: event.vehicle_killed_id.map(|id| id as u16),
                    vehicle_killed_name: event
                        .vehicle_killed_id
                        .and_then(|id| vehicle_names.get(&id).cloned()),
                    characters_killed: event.characters_killed as u32,
                    is_headshot: event.is_headshot,
                    experience_id: event.experience_id.map(|id| id as u16),
                })
            })
            .collect::<Result<Vec<_>, ClipStoreError>>()?;

        let alert_links = entities::clip_alert_links::Entity::find()
            .filter(entities::clip_alert_links::Column::ClipId.eq(clip_id))
            .all(&self.pool)
            .await?;
        let mut alerts = if alert_links.is_empty() {
            Vec::new()
        } else {
            let alert_models = entities::alert_instances::Entity::find()
                .filter(
                    entities::alert_instances::Column::AlertKey
                        .is_in(alert_links.iter().map(|link| link.alert_key.clone())),
                )
                .all(&self.pool)
                .await?;
            let mut alerts = alert_models
                .into_iter()
                .map(|alert| {
                    Ok(ClipAlertRecord {
                        alert_key: alert.alert_key,
                        label: alert.label,
                        world_id: alert.world_id as u32,
                        zone_id: alert.zone_id as u32,
                        metagame_event_id: alert.metagame_event_id as u8,
                        started_at: timestamp_millis_to_utc(alert.started_ts)?,
                        ended_at: alert.ended_ts.map(timestamp_millis_to_utc).transpose()?,
                        state_name: alert.state_name,
                        winner_faction: alert.winner_faction,
                    })
                })
                .collect::<Result<Vec<_>, ClipStoreError>>()?;
            alerts.sort_by(|left, right| {
                right
                    .started_at
                    .cmp(&left.started_at)
                    .then(left.alert_key.cmp(&right.alert_key))
            });
            alerts
        };

        let overlap_models = entities::clip_overlaps::Entity::find()
            .filter(
                Condition::any()
                    .add(entities::clip_overlaps::Column::ClipId.eq(clip_id))
                    .add(entities::clip_overlaps::Column::OverlapClipId.eq(clip_id)),
            )
            .all(&self.pool)
            .await?;
        let other_clip_ids = overlap_models
            .iter()
            .map(|overlap| {
                if overlap.clip_id == clip_id {
                    overlap.overlap_clip_id
                } else {
                    overlap.clip_id
                }
            })
            .collect::<Vec<_>>();
        let other_clips = if other_clip_ids.is_empty() {
            HashMap::new()
        } else {
            entities::clips::Entity::find()
                .filter(entities::clips::Column::Id.is_in(other_clip_ids))
                .all(&self.pool)
                .await?
                .into_iter()
                .map(|clip| (clip.id, clip))
                .collect::<HashMap<_, _>>()
        };
        let mut overlaps = overlap_models
            .into_iter()
            .filter_map(|overlap| {
                let other_clip_id = if overlap.clip_id == clip_id {
                    overlap.overlap_clip_id
                } else {
                    overlap.clip_id
                };
                other_clips.get(&other_clip_id).map(|other_clip| {
                    Ok::<_, ClipStoreError>(ClipOverlapRecord {
                        clip_id: other_clip.id,
                        trigger_event_at: timestamp_millis_to_utc(other_clip.trigger_event_ts)?,
                        clip_start_at: timestamp_millis_to_utc(other_clip.clip_start_ts)?,
                        clip_end_at: timestamp_millis_to_utc(other_clip.clip_end_ts)?,
                        profile_id: other_clip.profile_id.clone(),
                        rule_id: other_clip.rule_id.clone(),
                        path: other_clip.path.clone(),
                        overlap_duration_ms: overlap.overlap_duration_ms,
                    })
                })
            })
            .collect::<Result<Vec<_>, ClipStoreError>>()?;
        overlaps.sort_by(|left, right| {
            right
                .overlap_duration_ms
                .cmp(&left.overlap_duration_ms)
                .then(right.trigger_event_at.cmp(&left.trigger_event_at))
        });

        let uploads = entities::clip_uploads::Entity::find()
            .filter(entities::clip_uploads::Column::ClipId.eq(clip_id))
            .order_by_desc(entities::clip_uploads::Column::StartedTs)
            .order_by_desc(entities::clip_uploads::Column::Id)
            .all(&self.pool)
            .await?
            .into_iter()
            .map(|upload| {
                Ok(ClipUploadRecord {
                    id: upload.id,
                    provider: UploadProvider::from_db(&upload.provider),
                    state: ClipUploadState::from_db(&upload.state),
                    external_id: upload.external_id,
                    clip_url: upload.clip_url,
                    error_message: upload.error_message,
                    started_at: timestamp_millis_to_utc(upload.started_ts)?,
                    updated_at: timestamp_millis_to_utc(upload.updated_ts)?,
                    completed_at: upload
                        .completed_ts
                        .map(timestamp_millis_to_utc)
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>, ClipStoreError>>()?;

        let audio_tracks = entities::clip_audio_tracks::Entity::find()
            .filter(entities::clip_audio_tracks::Column::ClipId.eq(clip_id))
            .order_by_asc(entities::clip_audio_tracks::Column::StreamIndex)
            .order_by_asc(entities::clip_audio_tracks::Column::Id)
            .all(&self.pool)
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
            .collect::<Vec<_>>();

        Ok(Some(ClipDetailRecord {
            clip,
            audio_tracks,
            raw_events,
            alerts: {
                alerts.shrink_to_fit();
                alerts
            },
            overlaps,
            uploads,
        }))
    }

    pub async fn stats_snapshot(
        &self,
        since_ts: Option<i64>,
    ) -> Result<ClipStatsSnapshot, ClipStoreError> {
        let time_cond =
            since_ts.map(|ts| Expr::col(entities::clips::Column::TriggerEventTs).gte(ts));

        let mut total_query = Query::select();
        total_query
            .expr_as(Expr::cust("COUNT(*)"), Alias::new("clip_count"))
            .expr_as(
                Expr::cust("COALESCE(SUM(\"clip_duration_secs\"), 0)"),
                Alias::new("total_duration_secs"),
            )
            .expr_as(
                Expr::cust("COALESCE(SUM(\"score\"), 0)"),
                Alias::new("total_score_sum"),
            )
            .from(entities::clips::Entity);
        if let Some(cond) = &time_cond {
            total_query.cond_where(cond.clone());
        }
        let total_row = primitives::fetch_one_stmt(&total_query, &self.pool).await?;

        let total_clips = total_row.try_get::<i64>("clip_count")? as u32;
        let total_duration_secs = total_row.try_get::<i64>("total_duration_secs")? as u64;
        let total_score_sum = total_row.try_get::<i64>("total_score_sum")? as u64;

        let mut clips_per_day_query = Query::select();
        clips_per_day_query
            .expr_as(
                Expr::cust(
                    "strftime('%Y-%m-%d', \"trigger_event_ts\" / 1000, 'unixepoch', 'localtime')",
                ),
                Alias::new("label"),
            )
            .expr_as(Expr::cust("COUNT(*)"), Alias::new("count"))
            .from(entities::clips::Entity);
        if let Some(cond) = &time_cond {
            clips_per_day_query.cond_where(cond.clone());
        }
        clips_per_day_query
            .group_by_columns([Alias::new("label")])
            .order_by(Alias::new("label"), Order::Desc)
            .limit(30);
        let clips_per_day =
            read_count_rows(primitives::fetch_all_stmt(&clips_per_day_query, &self.pool).await?)?;

        let mut clips_per_rule_query = Query::select();
        clips_per_rule_query
            .expr_as(
                Expr::col((entities::clips::Entity, entities::clips::Column::RuleId)),
                Alias::new("label"),
            )
            .expr_as(Expr::cust("COUNT(*)"), Alias::new("count"))
            .from(entities::clips::Entity);
        if let Some(cond) = &time_cond {
            clips_per_rule_query.cond_where(cond.clone());
        }
        clips_per_rule_query
            .group_by_col((entities::clips::Entity, entities::clips::Column::RuleId))
            .order_by(Alias::new("count"), Order::Desc)
            .order_by(Alias::new("label"), Order::Asc)
            .limit(10);
        let clips_per_rule =
            read_count_rows(primitives::fetch_all_stmt(&clips_per_rule_query, &self.pool).await?)?;

        let mut score_distribution_query = Query::select();
        score_distribution_query
            .expr_as(
                Expr::cust("printf('%d-%d', (\"score\" / 10) * 10, ((\"score\" / 10) * 10) + 9)"),
                Alias::new("label"),
            )
            .expr_as(Expr::cust("COUNT(*)"), Alias::new("count"))
            .from(entities::clips::Entity);
        if let Some(cond) = &time_cond {
            score_distribution_query.cond_where(cond.clone());
        }
        score_distribution_query
            .group_by_columns([Alias::new("label")])
            .order_by_expr(Expr::cust("(\"score\" / 10)"), Order::Asc);
        let score_distribution = read_count_rows(
            primitives::fetch_all_stmt(&score_distribution_query, &self.pool).await?,
        )?;

        let facility_lookup = Alias::new("facility_lookup");
        let mut top_bases_query = Query::select();
        top_bases_query
            .expr_as(
                Expr::col((entities::clips::Entity, entities::clips::Column::FacilityId)),
                Alias::new("facility_id"),
            )
            .expr_as(
                Expr::cust(
                    "COALESCE(\"facility_lookup\".\"display_name\", printf('Facility #%d', \"clips\".\"facility_id\"))",
                ),
                Alias::new("label"),
            )
            .expr_as(Expr::cust("COUNT(*)"), Alias::new("count"))
            .from(entities::clips::Entity)
            .join_as(
                JoinType::LeftJoin,
                entities::lookup_cache::Entity,
                facility_lookup.clone(),
                Condition::all()
                    .add(
                        Expr::col((
                            facility_lookup.clone(),
                            entities::lookup_cache::Column::LookupKind,
                        ))
                        .eq(LookupKind::Facility.as_str()),
                    )
                    .add(
                        Expr::col((
                            facility_lookup.clone(),
                            entities::lookup_cache::Column::LookupId,
                        ))
                        .equals((entities::clips::Entity, entities::clips::Column::FacilityId)),
                    ),
            )
            .cond_where(entities::clips::Column::FacilityId.is_not_null());
        if let Some(cond) = &time_cond {
            top_bases_query.cond_where(cond.clone());
        }
        top_bases_query
            .group_by_col((entities::clips::Entity, entities::clips::Column::FacilityId))
            .group_by_columns([Alias::new("label")])
            .order_by(Alias::new("count"), Order::Desc)
            .order_by(Alias::new("label"), Order::Asc)
            .limit(10);
        let top_bases_rows = primitives::fetch_all_stmt(&top_bases_query, &self.pool).await?;

        // For clip_raw_events queries, join to clips when a time filter is active.
        let raw_event_time_cond = since_ts.map(|ts| {
            Expr::col((
                entities::clip_raw_events::Entity,
                entities::clip_raw_events::Column::ClipId,
            ))
            .in_subquery({
                let mut sub = Query::select();
                sub.column(entities::clips::Column::Id)
                    .from(entities::clips::Entity)
                    .cond_where(Expr::col(entities::clips::Column::TriggerEventTs).gte(ts));
                sub
            })
        });

        let weapon_lookup = Alias::new("weapon_lookup");
        let mut top_weapons_query = Query::select();
        top_weapons_query
            .expr_as(
                Expr::cust(
                    "COALESCE(\"weapon_lookup\".\"display_name\", CAST(\"clip_raw_events\".\"attacker_weapon_id\" AS TEXT))",
                ),
                Alias::new("label"),
            )
            .expr_as(Expr::cust("COUNT(*)"), Alias::new("count"))
            .from(entities::clip_raw_events::Entity)
            .join_as(
                JoinType::LeftJoin,
                entities::lookup_cache::Entity,
                weapon_lookup.clone(),
                Condition::all()
                    .add(
                        Expr::col((
                            weapon_lookup.clone(),
                            entities::lookup_cache::Column::LookupKind,
                        ))
                        .eq(LookupKind::Weapon.as_str()),
                    )
                    .add(
                        Expr::col((
                            weapon_lookup.clone(),
                            entities::lookup_cache::Column::LookupId,
                        ))
                        .equals((
                            entities::clip_raw_events::Entity,
                            entities::clip_raw_events::Column::AttackerWeaponId,
                        )),
                    ),
            )
            .cond_where(entities::clip_raw_events::Column::AttackerWeaponId.is_not_null());
        if let Some(cond) = &raw_event_time_cond {
            top_weapons_query.cond_where(cond.clone());
        }
        top_weapons_query
            .group_by_col((
                entities::clip_raw_events::Entity,
                entities::clip_raw_events::Column::AttackerWeaponId,
            ))
            .group_by_columns([Alias::new("label")])
            .order_by(Alias::new("count"), Order::Desc)
            .order_by(Alias::new("label"), Order::Asc)
            .limit(10);
        let top_weapons =
            read_count_rows(primitives::fetch_all_stmt(&top_weapons_query, &self.pool).await?)?;

        let character_lookup = Alias::new("character_lookup");
        let mut top_targets_query = Query::select();
        top_targets_query
            .expr_as(
                Expr::cust(
                    "COALESCE(\"character_lookup\".\"display_name\", CAST(\"clip_raw_events\".\"other_character_id\" AS TEXT))",
                ),
                Alias::new("label"),
            )
            .expr_as(Expr::cust("COUNT(*)"), Alias::new("count"))
            .from(entities::clip_raw_events::Entity)
            .join_as(
                JoinType::LeftJoin,
                entities::lookup_cache::Entity,
                character_lookup.clone(),
                Condition::all()
                    .add(
                        Expr::col((
                            character_lookup.clone(),
                            entities::lookup_cache::Column::LookupKind,
                        ))
                        .eq(LookupKind::Character.as_str()),
                    )
                    .add(
                        Expr::col((
                            character_lookup.clone(),
                            entities::lookup_cache::Column::LookupId,
                        ))
                        .equals((
                            entities::clip_raw_events::Entity,
                            entities::clip_raw_events::Column::OtherCharacterId,
                        )),
                    ),
            )
            .cond_where(entities::clip_raw_events::Column::OtherCharacterId.is_not_null())
            .cond_where(entities::clip_raw_events::Column::CharactersKilled.gt(0));
        if let Some(cond) = &raw_event_time_cond {
            top_targets_query.cond_where(cond.clone());
        }
        top_targets_query
            .group_by_col((
                entities::clip_raw_events::Entity,
                entities::clip_raw_events::Column::OtherCharacterId,
            ))
            .group_by_columns([Alias::new("label")])
            .order_by(Alias::new("count"), Order::Desc)
            .order_by(Alias::new("label"), Order::Asc)
            .limit(10);
        let top_targets =
            read_count_rows(primitives::fetch_all_stmt(&top_targets_query, &self.pool).await?)?;

        let mut raw_event_kinds_query = Query::select();
        raw_event_kinds_query
            .expr_as(
                Expr::col((
                    entities::clip_raw_events::Entity,
                    entities::clip_raw_events::Column::EventKind,
                )),
                Alias::new("label"),
            )
            .expr_as(Expr::cust("COUNT(*)"), Alias::new("count"))
            .from(entities::clip_raw_events::Entity);
        if let Some(cond) = &raw_event_time_cond {
            raw_event_kinds_query.cond_where(cond.clone());
        }
        raw_event_kinds_query
            .group_by_col((
                entities::clip_raw_events::Entity,
                entities::clip_raw_events::Column::EventKind,
            ))
            .order_by(Alias::new("count"), Order::Desc)
            .order_by(Alias::new("label"), Order::Asc)
            .limit(10);
        let raw_event_kinds =
            read_count_rows(primitives::fetch_all_stmt(&raw_event_kinds_query, &self.pool).await?)?;

        Ok(ClipStatsSnapshot {
            total_clips,
            total_duration_secs,
            total_score_sum,
            clips_per_day,
            clips_per_rule,
            score_distribution,
            top_bases: read_base_count_rows(top_bases_rows)?,
            top_weapons,
            top_targets,
            raw_event_kinds,
        })
    }

    pub async fn session_summary(
        &self,
        session_id: &str,
    ) -> Result<SessionSummary, ClipStoreError> {
        let mut summary_query = Query::select();
        summary_query
            .expr_as(Expr::cust("COUNT(*)"), Alias::new("clip_count"))
            .expr_as(
                Expr::cust("COALESCE(SUM(\"clip_duration_secs\"), 0)"),
                Alias::new("total_duration_secs"),
            )
            .expr_as(
                Expr::cust("COUNT(DISTINCT \"facility_id\")"),
                Alias::new("unique_bases"),
            )
            .from(entities::clips::Entity)
            .cond_where(entities::clips::Column::SessionId.eq(session_id));
        let summary_row = primitives::fetch_one_stmt(&summary_query, &self.pool).await?;

        let top_clip = entities::clips::Entity::find()
            .filter(entities::clips::Column::SessionId.eq(session_id))
            .order_by_desc(entities::clips::Column::Score)
            .order_by_desc(entities::clips::Column::TriggerEventTs)
            .order_by_desc(entities::clips::Column::Id)
            .one(&self.pool)
            .await?
            .map(|clip| {
                Ok::<_, ClipStoreError>(ClipSummaryItem {
                    clip_id: clip.id,
                    rule_id: clip.rule_id,
                    score: clip.score as u32,
                    trigger_event_at: timestamp_millis_to_utc(clip.trigger_event_ts)?,
                    clip_duration_secs: clip.clip_duration_secs as u32,
                })
            })
            .transpose()?;

        let mut rule_breakdown_query = Query::select();
        rule_breakdown_query
            .expr_as(
                Expr::col((entities::clips::Entity, entities::clips::Column::RuleId)),
                Alias::new("label"),
            )
            .expr_as(Expr::cust("COUNT(*)"), Alias::new("count"))
            .from(entities::clips::Entity)
            .cond_where(entities::clips::Column::SessionId.eq(session_id))
            .group_by_col((entities::clips::Entity, entities::clips::Column::RuleId))
            .order_by(Alias::new("count"), Order::Desc)
            .order_by(Alias::new("label"), Order::Asc);
        let rule_breakdown =
            read_count_rows(primitives::fetch_all_stmt(&rule_breakdown_query, &self.pool).await?)?;

        let facility_lookup = Alias::new("facility_lookup");
        let mut base_breakdown_query = Query::select();
        base_breakdown_query
            .expr_as(
                Expr::col((entities::clips::Entity, entities::clips::Column::FacilityId)),
                Alias::new("facility_id"),
            )
            .expr_as(
                Expr::cust(
                    "COALESCE(\"facility_lookup\".\"display_name\", printf('Facility #%d', \"clips\".\"facility_id\"))",
                ),
                Alias::new("label"),
            )
            .expr_as(Expr::cust("COUNT(*)"), Alias::new("count"))
            .from(entities::clips::Entity)
            .join_as(
                JoinType::LeftJoin,
                entities::lookup_cache::Entity,
                facility_lookup.clone(),
                Condition::all()
                    .add(
                        Expr::col((
                            facility_lookup.clone(),
                            entities::lookup_cache::Column::LookupKind,
                        ))
                        .eq(LookupKind::Facility.as_str()),
                    )
                    .add(
                        Expr::col((
                            facility_lookup.clone(),
                            entities::lookup_cache::Column::LookupId,
                        ))
                        .equals((entities::clips::Entity, entities::clips::Column::FacilityId)),
                    ),
            )
            .cond_where(entities::clips::Column::SessionId.eq(session_id))
            .cond_where(entities::clips::Column::FacilityId.is_not_null())
            .group_by_col((entities::clips::Entity, entities::clips::Column::FacilityId))
            .group_by_columns([Alias::new("label")])
            .order_by(Alias::new("count"), Order::Desc)
            .order_by(Alias::new("label"), Order::Asc)
            .limit(5);
        let base_breakdown = read_base_count_rows(
            primitives::fetch_all_stmt(&base_breakdown_query, &self.pool).await?,
        )?;

        Ok(SessionSummary {
            session_id: session_id.to_string(),
            total_clips: summary_row.try_get::<i64>("clip_count")? as u32,
            total_duration_secs: summary_row.try_get::<i64>("total_duration_secs")? as u64,
            unique_bases: summary_row.try_get::<i64>("unique_bases")? as u32,
            top_clip,
            rule_breakdown,
            base_breakdown,
        })
    }

    pub async fn cached_lookup(
        &self,
        kind: LookupKind,
        lookup_id: i64,
    ) -> Result<Option<String>, ClipStoreError> {
        lookups_repo::cached_lookup(self, kind, lookup_id).await
    }

    pub async fn find_lookup_by_name(
        &self,
        kind: LookupKind,
        display_name: &str,
    ) -> Result<Option<(i64, String)>, ClipStoreError> {
        lookups_repo::find_lookup_by_name(self, kind, display_name).await
    }

    pub async fn list_lookups(
        &self,
        kind: LookupKind,
    ) -> Result<Vec<(i64, String)>, ClipStoreError> {
        lookups_repo::list_lookups(self, kind).await
    }

    pub async fn store_lookup(
        &self,
        kind: LookupKind,
        lookup_id: i64,
        display_name: &str,
    ) -> Result<(), ClipStoreError> {
        lookups_repo::store_lookup(self, kind, lookup_id, display_name).await
    }

    pub async fn store_lookups(
        &self,
        kind: LookupKind,
        lookups: &[(i64, String)],
    ) -> Result<(), ClipStoreError> {
        lookups_repo::store_lookups(self, kind, lookups).await
    }

    pub async fn list_weapon_references(
        &self,
    ) -> Result<Vec<WeaponReferenceCacheEntry>, ClipStoreError> {
        lookups_repo::list_weapon_references(self).await
    }

    pub async fn store_weapon_references(
        &self,
        references: &[WeaponReferenceCacheEntry],
    ) -> Result<(), ClipStoreError> {
        lookups_repo::store_weapon_references(self, references).await
    }

    pub async fn cached_character_outfit(
        &self,
        character_id: u64,
    ) -> Result<Option<CharacterOutfitCacheEntry>, ClipStoreError> {
        lookups_repo::cached_character_outfit(self, character_id).await
    }

    pub async fn store_character_outfit(
        &self,
        character_id: u64,
        outfit_id: Option<u64>,
        outfit_tag: Option<&str>,
    ) -> Result<(), ClipStoreError> {
        lookups_repo::store_character_outfit(self, character_id, outfit_id, outfit_tag).await
    }

    #[allow(dead_code)]
    async fn prune_expired_character_outfit_cache(&self) -> Result<(), ClipStoreError> {
        lookups_repo::prune_expired_character_outfit_cache(self).await
    }

    pub async fn upsert_alert(&self, alert: &AlertInstanceRecord) -> Result<(), ClipStoreError> {
        lookups_repo::upsert_alert(self, alert).await
    }

    pub async fn insert_clip_upload(&self, upload: ClipUploadDraft) -> Result<i64, ClipStoreError> {
        clips_repo::insert_clip_upload(self, upload).await
    }

    pub async fn update_clip_upload(
        &self,
        upload_id: i64,
        state: ClipUploadState,
        external_id: Option<&str>,
        clip_url: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<(), ClipStoreError> {
        clips_repo::update_clip_upload(self, upload_id, state, external_id, clip_url, error_message)
            .await
    }

    pub async fn upsert_background_job(
        &self,
        record: &BackgroundJobRecord,
    ) -> Result<(), ClipStoreError> {
        jobs_repo::upsert_background_job(self, record).await
    }

    pub async fn recover_background_jobs(
        &self,
        limit: usize,
    ) -> Result<Vec<BackgroundJobRecord>, ClipStoreError> {
        jobs_repo::recover_background_jobs(self, limit).await
    }

    #[allow(dead_code)]
    pub async fn recent_background_jobs(
        &self,
        limit: usize,
    ) -> Result<Vec<BackgroundJobRecord>, ClipStoreError> {
        jobs_repo::recent_background_jobs(self, limit).await
    }

    #[allow(dead_code)]
    async fn recover_background_job_model(
        &self,
        job: entities::background_jobs::Model,
        now: i64,
    ) -> Result<entities::background_jobs::ActiveModel, ClipStoreError> {
        jobs_repo::recover_background_job_model(self, job, now).await
    }

    pub async fn delete_background_job(&self, id: BackgroundJobId) -> Result<(), ClipStoreError> {
        jobs_repo::delete_background_job(self, id).await
    }

    pub async fn insert_montage(
        &self,
        output_path: &str,
        source_clip_ids: &[i64],
    ) -> Result<i64, ClipStoreError> {
        clips_repo::insert_montage(self, output_path, source_clip_ids).await
    }

    pub async fn update_clip_path(
        &self,
        clip_id: i64,
        path: Option<&str>,
    ) -> Result<(), ClipStoreError> {
        clips_repo::update_clip_path(self, clip_id, path).await
    }

    pub async fn insert_audio_tracks(
        &self,
        clip_id: i64,
        tracks: Vec<ClipAudioTrackDraft>,
    ) -> Result<(), ClipStoreError> {
        clips_repo::insert_audio_tracks(self, clip_id, tracks).await
    }

    #[allow(dead_code)]
    pub async fn load_audio_tracks(
        &self,
        clip_id: i64,
    ) -> Result<Vec<ClipAudioTrackRecord>, ClipStoreError> {
        clips_repo::load_audio_tracks(self, clip_id).await
    }

    pub async fn delete_audio_tracks(&self, clip_id: i64) -> Result<(), ClipStoreError> {
        clips_repo::delete_audio_tracks(self, clip_id).await
    }

    pub async fn set_post_process_status(
        &self,
        clip_id: i64,
        status: PostProcessStatus,
        error: Option<&str>,
    ) -> Result<(), ClipStoreError> {
        clips_repo::set_post_process_status(self, clip_id, status, error).await
    }

    pub async fn clips_pending_post_process(&self) -> Result<Vec<i64>, ClipStoreError> {
        clips_repo::clips_pending_post_process(self).await
    }

    pub async fn all_clips(&self) -> Result<Vec<ClipRecord>, ClipStoreError> {
        clips_repo::all_clips(self).await
    }

    pub async fn delete_clip(&self, clip_id: i64) -> Result<(), ClipStoreError> {
        clips_repo::delete_clip(self, clip_id).await
    }

    #[allow(dead_code)]
    pub fn database_path(&self) -> Option<&Path> {
        self.database_path.as_deref()
    }

    pub fn startup_notice(&self) -> Option<&str> {
        self.startup_notice.as_deref()
    }

    pub async fn backup_to(&self, destination: &Path) -> Result<(), ClipStoreError> {
        exports::backup_to(self, destination).await
    }

    pub async fn export_json_to(&self, destination: &Path) -> Result<(), ClipStoreError> {
        exports::export_json_to(self, destination).await
    }

    pub async fn export_csv_to(&self, destination: &Path) -> Result<(), ClipStoreError> {
        exports::export_csv_to(self, destination).await
    }

    async fn lookup_name_map(
        &self,
        kind: LookupKind,
        ids: BTreeSet<i64>,
    ) -> Result<HashMap<i64, String>, ClipStoreError> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = entities::lookup_cache::Entity::find()
            .filter(entities::lookup_cache::Column::LookupKind.eq(kind.as_str()))
            .filter(entities::lookup_cache::Column::LookupId.is_in(ids))
            .all(&self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| (row.lookup_id, row.display_name))
            .collect())
    }

    async fn hydrate_clip_records(
        &self,
        clip_models: Vec<entities::clips::Model>,
    ) -> Result<Vec<ClipRecord>, ClipStoreError> {
        if clip_models.is_empty() {
            return Ok(Vec::new());
        }

        let clip_ids = clip_models.iter().map(|clip| clip.id).collect::<Vec<_>>();
        let clip_id_set = clip_ids.iter().copied().collect::<BTreeSet<_>>();

        let clip_events = entities::clip_events::Entity::find()
            .filter(entities::clip_events::Column::ClipId.is_in(clip_ids.clone()))
            .order_by_asc(entities::clip_events::Column::ClipId)
            .order_by_asc(entities::clip_events::Column::Id)
            .all(&self.pool)
            .await?;
        let mut events_by_clip = clip_events.into_iter().fold(
            HashMap::<i64, Vec<ClipEventContribution>>::new(),
            |mut acc, event| {
                acc.entry(event.clip_id)
                    .or_default()
                    .push(ClipEventContribution {
                        event_kind: event.event_kind,
                        occurrences: event.occurrences as u32,
                        points: event.points as u32,
                    });
                acc
            },
        );

        let clip_overlaps = entities::clip_overlaps::Entity::find()
            .filter(
                Condition::any()
                    .add(entities::clip_overlaps::Column::ClipId.is_in(clip_ids.clone()))
                    .add(entities::clip_overlaps::Column::OverlapClipId.is_in(clip_ids.clone())),
            )
            .all(&self.pool)
            .await?;
        let mut overlap_counts = HashMap::<i64, u32>::new();
        for overlap in clip_overlaps {
            if clip_id_set.contains(&overlap.clip_id) {
                *overlap_counts.entry(overlap.clip_id).or_default() += 1;
            }
            if clip_id_set.contains(&overlap.overlap_clip_id) {
                *overlap_counts.entry(overlap.overlap_clip_id).or_default() += 1;
            }
        }

        let clip_alert_links = entities::clip_alert_links::Entity::find()
            .filter(entities::clip_alert_links::Column::ClipId.is_in(clip_ids))
            .all(&self.pool)
            .await?;
        let mut alert_counts = HashMap::<i64, u32>::new();
        for alert_link in clip_alert_links {
            *alert_counts.entry(alert_link.clip_id).or_default() += 1;
        }

        let zone_ids = clip_models
            .iter()
            .filter_map(|clip| clip.zone_id)
            .collect::<BTreeSet<_>>();
        let facility_ids = clip_models
            .iter()
            .filter_map(|clip| clip.facility_id)
            .collect::<BTreeSet<_>>();
        let zone_names = self.lookup_name_map(LookupKind::Zone, zone_ids).await?;
        let facility_names = self
            .lookup_name_map(LookupKind::Facility, facility_ids)
            .await?;

        clip_models
            .into_iter()
            .map(|clip| {
                let path = clip.path.clone();
                Ok(ClipRecord {
                    id: clip.id,
                    trigger_event_at: timestamp_millis_to_utc(clip.trigger_event_ts)?,
                    clip_start_at: timestamp_millis_to_utc(clip.clip_start_ts)?,
                    clip_end_at: timestamp_millis_to_utc(clip.clip_end_ts)?,
                    saved_at: timestamp_millis_to_utc(clip.saved_ts)?,
                    origin: ClipOrigin::from_db(&clip.clip_origin),
                    profile_id: clip.profile_id,
                    rule_id: clip.rule_id,
                    clip_duration_secs: clip.clip_duration_secs as u32,
                    session_id: clip.session_id,
                    character_id: clip.character_id as u64,
                    world_id: clip.world_id as u32,
                    zone_id: clip.zone_id.map(|id| id as u32),
                    facility_id: clip.facility_id.map(|id| id as u32),
                    zone_name: clip.zone_id.and_then(|id| zone_names.get(&id).cloned()),
                    facility_name: clip
                        .facility_id
                        .and_then(|id| facility_names.get(&id).cloned()),
                    score: clip.score as u32,
                    honu_session_id: clip.honu_session_id,
                    path,
                    file_size_bytes: file_size_bytes_for_path(clip.path.as_deref()),
                    overlap_count: overlap_counts.remove(&clip.id).unwrap_or_default(),
                    alert_count: alert_counts.remove(&clip.id).unwrap_or_default(),
                    post_process_status: PostProcessStatus::from_entity(clip.post_process_status),
                    post_process_error: clip.post_process_error,
                    events: events_by_clip.remove(&clip.id).unwrap_or_default(),
                })
            })
            .collect()
    }

    async fn initialize_schema(&self) -> Result<Option<String>, ClipStoreError> {
        schema::initialize_schema(self).await
    }

    #[allow(dead_code)]
    async fn reset_schema(&self) -> Result<(), ClipStoreError> {
        schema::reset_schema(self).await
    }

    #[allow(dead_code)]
    async fn set_schema_version(&self) -> Result<(), ClipStoreError> {
        schema::set_schema_version(self).await
    }

    #[allow(dead_code)]
    async fn export_records(&self) -> Result<Vec<ClipExportRecord>, ClipStoreError> {
        exports::export_records(self).await
    }

    #[allow(dead_code)]
    async fn fetch_all_clips(&self) -> Result<Vec<ClipRecord>, ClipStoreError> {
        exports::fetch_all_clips(self).await
    }

    #[allow(dead_code)]
    async fn create_pre_migration_backup(
        &self,
        current_version: i64,
        target_version: i64,
    ) -> Result<Option<PathBuf>, ClipStoreError> {
        schema::create_pre_migration_backup(self, current_version, target_version).await
    }

    #[allow(dead_code)]
    async fn write_sqlite_backup(&self, destination: &Path) -> Result<(), ClipStoreError> {
        exports::write_sqlite_backup(self, destination).await
    }
}

#[derive(Debug, Serialize)]
struct ClipExportRecord {
    id: i64,
    trigger_event_at: String,
    clip_start_at: String,
    clip_end_at: String,
    saved_at: String,
    origin: String,
    profile_id: String,
    rule_id: String,
    clip_duration_secs: u32,
    session_id: Option<String>,
    character_id: u64,
    world_id: u32,
    zone_id: Option<u32>,
    facility_id: Option<u32>,
    score: u32,
    honu_session_id: Option<i64>,
    path: Option<String>,
    file_size_bytes: Option<u64>,
    events: Vec<ClipEventContribution>,
}
