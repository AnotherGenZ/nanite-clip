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

mod entities;
mod migrations;

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
        Ok(
            entities::lookup_cache::Entity::find_by_id((kind.as_str().to_string(), lookup_id))
                .one(&self.pool)
                .await?
                .map(|entry| entry.display_name),
        )
    }

    pub async fn find_lookup_by_name(
        &self,
        kind: LookupKind,
        display_name: &str,
    ) -> Result<Option<(i64, String)>, ClipStoreError> {
        let normalized_name = display_name.trim().to_lowercase();
        let mut query = Query::select();
        query
            .columns([
                entities::lookup_cache::Column::LookupId,
                entities::lookup_cache::Column::DisplayName,
            ])
            .from(entities::lookup_cache::Entity)
            .cond_where(entities::lookup_cache::Column::LookupKind.eq(kind.as_str()))
            .cond_where(
                Func::lower(Expr::col((
                    entities::lookup_cache::Entity,
                    entities::lookup_cache::Column::DisplayName,
                )))
                .eq(normalized_name),
            )
            .order_by(entities::lookup_cache::Column::ResolvedTs, Order::Desc)
            .limit(1);

        match primitives::fetch_optional_stmt(&query, &self.pool).await? {
            Some(row) => Ok(Some((
                row.try_get("lookup_id")?,
                row.try_get("display_name")?,
            ))),
            None => Ok(None),
        }
    }

    pub async fn list_lookups(
        &self,
        kind: LookupKind,
    ) -> Result<Vec<(i64, String)>, ClipStoreError> {
        let rows = entities::lookup_cache::Entity::find()
            .filter(entities::lookup_cache::Column::LookupKind.eq(kind.as_str()))
            .order_by_asc(entities::lookup_cache::Column::DisplayName)
            .order_by_asc(entities::lookup_cache::Column::LookupId)
            .all(&self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| (row.lookup_id, row.display_name))
            .collect())
    }

    pub async fn store_lookup(
        &self,
        kind: LookupKind,
        lookup_id: i64,
        display_name: &str,
    ) -> Result<(), ClipStoreError> {
        entities::lookup_cache::Entity::insert(entities::lookup_cache::ActiveModel {
            lookup_kind: Set(kind.as_str().to_string()),
            lookup_id: Set(lookup_id),
            display_name: Set(display_name.to_string()),
            resolved_ts: Set(Utc::now().timestamp_millis()),
        })
        .on_conflict(
            OnConflict::columns([
                entities::lookup_cache::Column::LookupKind,
                entities::lookup_cache::Column::LookupId,
            ])
            .update_columns([
                entities::lookup_cache::Column::DisplayName,
                entities::lookup_cache::Column::ResolvedTs,
            ])
            .to_owned(),
        )
        .exec(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn store_lookups(
        &self,
        kind: LookupKind,
        lookups: &[(i64, String)],
    ) -> Result<(), ClipStoreError> {
        if lookups.is_empty() {
            return Ok(());
        }

        let resolved_ts = Utc::now().timestamp_millis();
        let active_models = lookups
            .iter()
            .map(
                |(lookup_id, display_name)| entities::lookup_cache::ActiveModel {
                    lookup_kind: Set(kind.as_str().to_string()),
                    lookup_id: Set(*lookup_id),
                    display_name: Set(display_name.clone()),
                    resolved_ts: Set(resolved_ts),
                },
            )
            .collect::<Vec<_>>();
        entities::lookup_cache::Entity::insert_many(active_models)
            .on_conflict(
                OnConflict::columns([
                    entities::lookup_cache::Column::LookupKind,
                    entities::lookup_cache::Column::LookupId,
                ])
                .update_columns([
                    entities::lookup_cache::Column::DisplayName,
                    entities::lookup_cache::Column::ResolvedTs,
                ])
                .to_owned(),
            )
            .exec(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn list_weapon_references(
        &self,
    ) -> Result<Vec<WeaponReferenceCacheEntry>, ClipStoreError> {
        let rows = entities::weapon_reference_cache::Entity::find()
            .order_by_asc(entities::weapon_reference_cache::Column::CategoryLabel)
            .order_by_asc(entities::weapon_reference_cache::Column::DisplayName)
            .order_by_asc(entities::weapon_reference_cache::Column::ItemId)
            .all(&self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| WeaponReferenceCacheEntry {
                item_id: row.item_id as u32,
                weapon_id: row.weapon_id as u32,
                display_name: row.display_name,
                category_label: row.category_label,
                faction: row
                    .faction_id
                    .and_then(|value| i16::try_from(value).ok())
                    .and_then(|value| Faction::try_from(value).ok()),
                weapon_group_id: row.weapon_group_id.map(|value| value as u32),
            })
            .collect())
    }

    pub async fn store_weapon_references(
        &self,
        references: &[WeaponReferenceCacheEntry],
    ) -> Result<(), ClipStoreError> {
        let tx = primitives::begin(&self.pool).await?;
        let resolved_ts = Utc::now().timestamp_millis();

        entities::weapon_reference_cache::Entity::delete_many()
            .exec(&*tx)
            .await?;
        entities::lookup_cache::Entity::delete_many()
            .filter(entities::lookup_cache::Column::LookupKind.eq(LookupKind::Weapon.as_str()))
            .exec(&*tx)
            .await?;

        if !references.is_empty() {
            let weapon_models = references
                .iter()
                .map(|reference| entities::weapon_reference_cache::ActiveModel {
                    item_id: Set(i64::from(reference.item_id)),
                    weapon_id: Set(i64::from(reference.weapon_id)),
                    display_name: Set(reference.display_name.clone()),
                    category_label: Set(reference.category_label.clone()),
                    faction_id: Set(reference.faction.map(|faction| i16::from(faction) as i64)),
                    weapon_group_id: Set(reference.weapon_group_id.map(i64::from)),
                    resolved_ts: Set(resolved_ts),
                })
                .collect::<Vec<_>>();
            entities::weapon_reference_cache::Entity::insert_many(weapon_models)
                .exec(&*tx)
                .await?;

            let lookup_models = references
                .iter()
                .map(|reference| entities::lookup_cache::ActiveModel {
                    lookup_kind: Set(LookupKind::Weapon.as_str().to_string()),
                    lookup_id: Set(i64::from(reference.item_id)),
                    display_name: Set(reference.display_name.clone()),
                    resolved_ts: Set(resolved_ts),
                })
                .collect::<Vec<_>>();
            entities::lookup_cache::Entity::insert_many(lookup_models)
                .on_conflict(
                    OnConflict::columns([
                        entities::lookup_cache::Column::LookupKind,
                        entities::lookup_cache::Column::LookupId,
                    ])
                    .update_columns([
                        entities::lookup_cache::Column::DisplayName,
                        entities::lookup_cache::Column::ResolvedTs,
                    ])
                    .to_owned(),
                )
                .exec(&*tx)
                .await?;
        }

        tx.commit().await?;

        Ok(())
    }

    pub async fn cached_character_outfit(
        &self,
        character_id: u64,
    ) -> Result<Option<CharacterOutfitCacheEntry>, ClipStoreError> {
        let min_resolved_ts = Utc::now().timestamp_millis() - CHARACTER_OUTFIT_CACHE_TTL_MS;
        let row = entities::character_outfit_cache::Entity::find_by_id(character_id as i64)
            .filter(entities::character_outfit_cache::Column::ResolvedTs.gte(min_resolved_ts))
            .one(&self.pool)
            .await?;

        if let Some(row) = row {
            return Ok(Some(CharacterOutfitCacheEntry {
                outfit_id: row.outfit_id.map(|value| value as u64),
                outfit_tag: row.outfit_tag,
            }));
        }

        entities::character_outfit_cache::Entity::delete_many()
            .filter(entities::character_outfit_cache::Column::CharacterId.eq(character_id as i64))
            .filter(entities::character_outfit_cache::Column::ResolvedTs.lt(min_resolved_ts))
            .exec(&self.pool)
            .await?;

        Ok(None)
    }

    pub async fn store_character_outfit(
        &self,
        character_id: u64,
        outfit_id: Option<u64>,
        outfit_tag: Option<&str>,
    ) -> Result<(), ClipStoreError> {
        self.prune_expired_character_outfit_cache().await?;

        entities::character_outfit_cache::Entity::insert(
            entities::character_outfit_cache::ActiveModel {
                character_id: Set(character_id as i64),
                outfit_id: Set(outfit_id.map(|value| value as i64)),
                outfit_tag: Set(outfit_tag.map(str::to_string)),
                resolved_ts: Set(Utc::now().timestamp_millis()),
            },
        )
        .on_conflict(
            OnConflict::column(entities::character_outfit_cache::Column::CharacterId)
                .update_columns([
                    entities::character_outfit_cache::Column::OutfitId,
                    entities::character_outfit_cache::Column::OutfitTag,
                    entities::character_outfit_cache::Column::ResolvedTs,
                ])
                .to_owned(),
        )
        .exec(&self.pool)
        .await?;

        Ok(())
    }

    async fn prune_expired_character_outfit_cache(&self) -> Result<(), ClipStoreError> {
        let min_resolved_ts = Utc::now().timestamp_millis() - CHARACTER_OUTFIT_CACHE_TTL_MS;
        entities::character_outfit_cache::Entity::delete_many()
            .filter(entities::character_outfit_cache::Column::ResolvedTs.lt(min_resolved_ts))
            .exec(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn upsert_alert(&self, alert: &AlertInstanceRecord) -> Result<(), ClipStoreError> {
        entities::alert_instances::Entity::insert(entities::alert_instances::ActiveModel {
            alert_key: Set(alert.alert_key.clone()),
            label: Set(alert.label.clone()),
            world_id: Set(i64::from(alert.world_id)),
            zone_id: Set(i64::from(alert.zone_id)),
            metagame_event_id: Set(i64::from(alert.metagame_event_id)),
            started_ts: Set(alert.started_at.timestamp_millis()),
            ended_ts: Set(alert.ended_at.map(|value| value.timestamp_millis())),
            state_name: Set(alert.state_name.clone()),
            winner_faction: Set(alert.winner_faction.clone()),
            faction_nc: Set(alert.faction_nc),
            faction_tr: Set(alert.faction_tr),
            faction_vs: Set(alert.faction_vs),
        })
        .on_conflict(
            OnConflict::column(entities::alert_instances::Column::AlertKey)
                .values([
                    (
                        entities::alert_instances::Column::Label,
                        Expr::cust("\"excluded\".\"label\""),
                    ),
                    (
                        entities::alert_instances::Column::WorldId,
                        Expr::cust("\"excluded\".\"world_id\""),
                    ),
                    (
                        entities::alert_instances::Column::ZoneId,
                        Expr::cust("\"excluded\".\"zone_id\""),
                    ),
                    (
                        entities::alert_instances::Column::MetagameEventId,
                        Expr::cust("\"excluded\".\"metagame_event_id\""),
                    ),
                    (
                        entities::alert_instances::Column::StartedTs,
                        Expr::cust("MIN(\"alert_instances\".\"started_ts\", \"excluded\".\"started_ts\")"),
                    ),
                    (
                        entities::alert_instances::Column::EndedTs,
                        Expr::cust("COALESCE(\"excluded\".\"ended_ts\", \"alert_instances\".\"ended_ts\")"),
                    ),
                    (
                        entities::alert_instances::Column::StateName,
                        Expr::cust("\"excluded\".\"state_name\""),
                    ),
                    (
                        entities::alert_instances::Column::WinnerFaction,
                        Expr::cust("COALESCE(\"excluded\".\"winner_faction\", \"alert_instances\".\"winner_faction\")"),
                    ),
                    (
                        entities::alert_instances::Column::FactionNc,
                        Expr::cust("\"excluded\".\"faction_nc\""),
                    ),
                    (
                        entities::alert_instances::Column::FactionTr,
                        Expr::cust("\"excluded\".\"faction_tr\""),
                    ),
                    (
                        entities::alert_instances::Column::FactionVs,
                        Expr::cust("\"excluded\".\"faction_vs\""),
                    ),
                ])
                .to_owned(),
        )
        .exec(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn insert_clip_upload(&self, upload: ClipUploadDraft) -> Result<i64, ClipStoreError> {
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
        .insert(&self.pool)
        .await?;

        Ok(inserted.id)
    }

    pub async fn update_clip_upload(
        &self,
        upload_id: i64,
        state: ClipUploadState,
        external_id: Option<&str>,
        clip_url: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<(), ClipStoreError> {
        let now = Utc::now().timestamp_millis();
        let Some(existing) = entities::clip_uploads::Entity::find_by_id(upload_id)
            .one(&self.pool)
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
        model.update(&self.pool).await?;

        Ok(())
    }

    pub async fn upsert_background_job(
        &self,
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
        .exec(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn recover_background_jobs(
        &self,
        limit: usize,
    ) -> Result<Vec<BackgroundJobRecord>, ClipStoreError> {
        let now = Utc::now().timestamp_millis();
        let interrupted_jobs = entities::background_jobs::Entity::find()
            .filter(entities::background_jobs::Column::State.is_in([
                BackgroundJobState::Queued.as_str(),
                BackgroundJobState::Running.as_str(),
            ]))
            .all(&self.pool)
            .await?;

        for job in interrupted_jobs {
            let recovered = self.recover_background_job_model(job, now).await?;
            recovered.update(&self.pool).await?;
        }

        self.recent_background_jobs(limit).await
    }

    pub async fn recent_background_jobs(
        &self,
        limit: usize,
    ) -> Result<Vec<BackgroundJobRecord>, ClipStoreError> {
        let rows = entities::background_jobs::Entity::find()
            .order_by_desc(entities::background_jobs::Column::UpdatedTs)
            .order_by_desc(entities::background_jobs::Column::Id)
            .limit(limit as u64)
            .all(&self.pool)
            .await?;

        rows.into_iter()
            .map(background_job_from_model)
            .collect::<Result<Vec<_>, ClipStoreError>>()
    }

    async fn recover_background_job_model(
        &self,
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
                    .one(&self.pool)
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
                        model.detail =
                            Set(Some("Audio post-processing was not required.".to_string()));
                        return Ok(model);
                    }
                    PostProcessStatus::Failed => {
                        model.state = Set(BackgroundJobState::Failed.as_str().to_string());
                        model.detail = Set(Some(clip.post_process_error.unwrap_or_else(|| {
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

    pub async fn delete_background_job(&self, id: BackgroundJobId) -> Result<(), ClipStoreError> {
        entities::background_jobs::Entity::delete_by_id(id.0 as i64)
            .exec(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn insert_montage(
        &self,
        output_path: &str,
        source_clip_ids: &[i64],
    ) -> Result<i64, ClipStoreError> {
        let tx = primitives::begin(&self.pool).await?;
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

    pub async fn update_clip_path(
        &self,
        clip_id: i64,
        path: Option<&str>,
    ) -> Result<(), ClipStoreError> {
        let Some(existing) = entities::clips::Entity::find_by_id(clip_id)
            .one(&self.pool)
            .await?
        else {
            return Ok(());
        };
        let mut model: entities::clips::ActiveModel = existing.into();
        model.path = Set(path.map(str::to_string));
        model.update(&self.pool).await?;

        Ok(())
    }

    pub async fn insert_audio_tracks(
        &self,
        clip_id: i64,
        tracks: Vec<ClipAudioTrackDraft>,
    ) -> Result<(), ClipStoreError> {
        self.delete_audio_tracks(clip_id).await?;
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
            .exec(&self.pool)
            .await?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn load_audio_tracks(
        &self,
        clip_id: i64,
    ) -> Result<Vec<ClipAudioTrackRecord>, ClipStoreError> {
        Ok(entities::clip_audio_tracks::Entity::find()
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
            .collect())
    }

    pub async fn delete_audio_tracks(&self, clip_id: i64) -> Result<(), ClipStoreError> {
        entities::clip_audio_tracks::Entity::delete_many()
            .filter(entities::clip_audio_tracks::Column::ClipId.eq(clip_id))
            .exec(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_post_process_status(
        &self,
        clip_id: i64,
        status: PostProcessStatus,
        error: Option<&str>,
    ) -> Result<(), ClipStoreError> {
        let Some(existing) = entities::clips::Entity::find_by_id(clip_id)
            .one(&self.pool)
            .await?
        else {
            return Ok(());
        };

        let mut model: entities::clips::ActiveModel = existing.into();
        model.post_process_status = Set(status.into_entity());
        model.post_process_error = Set(error.map(str::to_string));
        model.update(&self.pool).await?;
        Ok(())
    }

    pub async fn clips_pending_post_process(&self) -> Result<Vec<i64>, ClipStoreError> {
        Ok(entities::clips::Entity::find()
            .filter(
                entities::clips::Column::PostProcessStatus.eq(entities::PostProcessStatus::Pending),
            )
            .all(&self.pool)
            .await?
            .into_iter()
            .map(|clip| clip.id)
            .collect())
    }

    pub async fn all_clips(&self) -> Result<Vec<ClipRecord>, ClipStoreError> {
        self.fetch_all_clips().await
    }

    pub async fn delete_clip(&self, clip_id: i64) -> Result<(), ClipStoreError> {
        entities::clips::Entity::delete_by_id(clip_id)
            .exec(&self.pool)
            .await?;

        Ok(())
    }

    #[allow(dead_code)]
    pub fn database_path(&self) -> Option<&Path> {
        self.database_path.as_deref()
    }

    pub fn startup_notice(&self) -> Option<&str> {
        self.startup_notice.as_deref()
    }

    pub async fn backup_to(&self, destination: &Path) -> Result<(), ClipStoreError> {
        validate_output_destination(destination)?;
        self.write_sqlite_backup(destination).await?;
        Ok(())
    }

    pub async fn export_json_to(&self, destination: &Path) -> Result<(), ClipStoreError> {
        validate_output_destination(destination)?;
        let payload = serde_json::to_vec_pretty(&self.export_records().await?)?;
        atomic_write(destination, &payload)?;
        Ok(())
    }

    pub async fn export_csv_to(&self, destination: &Path) -> Result<(), ClipStoreError> {
        validate_output_destination(destination)?;

        let mut csv = String::from(
            "id,trigger_event_at,clip_start_at,clip_end_at,saved_at,origin,profile_id,rule_id,clip_duration_secs,session_id,character_id,world_id,zone_id,facility_id,score,honu_session_id,path,file_size_bytes,events_json\n",
        );

        for record in self.export_records().await? {
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
                    record.zone_id.unwrap_or_default().to_string(),
                    record.facility_id.unwrap_or_default().to_string(),
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
        let manager = SchemaManager::new(&self.pool);
        let has_clips_table = manager
            .has_table(entities::clips::Entity.table_name())
            .await?;

        if !has_clips_table {
            self.reset_schema().await?;
            return Ok(None);
        }

        let current_version = primitives::query("PRAGMA user_version")
            .fetch_one(&self.pool)
            .await?
            .try_get_at::<i64>(0)?;

        if current_version > CLIP_STORE_SCHEMA_VERSION {
            return Err(ClipStoreError::UnsupportedSchemaVersion(current_version));
        }

        let legacy_state = migrations::inspect_legacy_schema(&self.pool).await?;
        let requires_backup = legacy_state.pending_migrations() > 0;
        let mut startup_notice = None;

        if requires_backup
            && let Some(backup_path) = self
                .create_pre_migration_backup(current_version, CLIP_STORE_SCHEMA_VERSION)
                .await?
        {
            startup_notice = Some(format!(
                "Clip database migrated to schema v{CLIP_STORE_SCHEMA_VERSION}. A backup was created at {} before the migration ran.",
                backup_path.display()
            ));
        }

        migrations::reconcile_migration_history(&self.pool, legacy_state).await?;
        migrations::Migrator::up(&self.pool, None).await?;
        self.set_schema_version().await?;

        Ok(startup_notice)
    }

    async fn reset_schema(&self) -> Result<(), ClipStoreError> {
        migrations::Migrator::fresh(&self.pool).await?;
        self.set_schema_version().await?;
        Ok(())
    }

    async fn set_schema_version(&self) -> Result<(), ClipStoreError> {
        primitives::query(format!("PRAGMA user_version = {CLIP_STORE_SCHEMA_VERSION}"))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn export_records(&self) -> Result<Vec<ClipExportRecord>, ClipStoreError> {
        Ok(self
            .all_clips()
            .await?
            .into_iter()
            .map(ClipExportRecord::from)
            .collect())
    }

    async fn fetch_all_clips(&self) -> Result<Vec<ClipRecord>, ClipStoreError> {
        let clips = entities::clips::Entity::find()
            .order_by_desc(entities::clips::Column::TriggerEventTs)
            .order_by_desc(entities::clips::Column::Id)
            .all(&self.pool)
            .await?;

        self.hydrate_clip_records(clips).await
    }

    async fn create_pre_migration_backup(
        &self,
        current_version: i64,
        target_version: i64,
    ) -> Result<Option<PathBuf>, ClipStoreError> {
        let Some(database_path) = self.database_path.as_deref() else {
            return Ok(None);
        };

        let backup_path =
            next_migration_backup_path(database_path, current_version, target_version);
        self.write_sqlite_backup(&backup_path)
            .await
            .map_err(|error| {
                ClipStoreError::MigrationBackupFailed(format!(
                    "failed to create pre-migration backup {}: {error}",
                    backup_path.display()
                ))
            })?;

        Ok(Some(backup_path))
    }

    async fn write_sqlite_backup(&self, destination: &Path) -> Result<(), ClipStoreError> {
        let escaped = destination.display().to_string().replace('\'', "''");

        primitives::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&self.pool)
            .await?;
        primitives::query(format!("VACUUM INTO '{escaped}'"))
            .execute(&self.pool)
            .await?;

        Ok(())
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

fn entity_table<E>(entity: E) -> TableCreateStatement
where
    E: EntityTrait,
{
    let schema = Schema::new(DbBackend::Sqlite);
    let mut table = schema.create_table_from_entity(entity);
    table.if_not_exists();
    table
}

#[allow(dead_code)]
fn entity_indexes<E>(entity: E) -> Vec<IndexCreateStatement>
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

fn create_clip_events_table() -> TableCreateStatement {
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

fn create_clip_raw_events_table() -> TableCreateStatement {
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

fn create_clip_uploads_table() -> TableCreateStatement {
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

fn create_clip_audio_tracks_table() -> TableCreateStatement {
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

fn create_clip_overlaps_table() -> TableCreateStatement {
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

fn create_clip_alert_links_table() -> TableCreateStatement {
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

fn create_montage_clips_table() -> TableCreateStatement {
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

fn create_clip_uploads_clip_started_index() -> IndexCreateStatement {
    let mut index = Index::create();
    index
        .name("idx_clip_uploads_clip_id")
        .table(entities::clip_uploads::Entity)
        .if_not_exists()
        .col(entities::clip_uploads::Column::ClipId)
        .col(entities::clip_uploads::Column::StartedTs);
    index.to_owned()
}

fn create_clip_uploads_provider_state_index() -> IndexCreateStatement {
    let mut index = Index::create();
    index
        .name("idx_clip_uploads_provider")
        .table(entities::clip_uploads::Entity)
        .if_not_exists()
        .col(entities::clip_uploads::Column::Provider)
        .col(entities::clip_uploads::Column::State);
    index.to_owned()
}

fn create_clip_audio_tracks_clip_stream_index() -> IndexCreateStatement {
    let mut index = Index::create();
    index
        .name("idx_clip_audio_tracks_clip_stream")
        .table(entities::clip_audio_tracks::Entity)
        .if_not_exists()
        .col(entities::clip_audio_tracks::Column::ClipId)
        .col(entities::clip_audio_tracks::Column::StreamIndex);
    index.to_owned()
}

fn create_background_jobs_state_updated_index() -> IndexCreateStatement {
    let mut index = Index::create();
    index
        .name("idx_background_jobs_state")
        .table(entities::background_jobs::Entity)
        .if_not_exists()
        .col(entities::background_jobs::Column::State)
        .col(entities::background_jobs::Column::UpdatedTs);
    index.to_owned()
}

fn create_alert_instances_zone_world_index() -> IndexCreateStatement {
    let mut index = Index::create();
    index
        .name("idx_alert_instances_zone_id")
        .table(entities::alert_instances::Entity)
        .if_not_exists()
        .col(entities::alert_instances::Column::ZoneId)
        .col(entities::alert_instances::Column::WorldId);
    index.to_owned()
}

fn create_weapon_reference_cache_category_index() -> IndexCreateStatement {
    let mut index = Index::create();
    index
        .name("idx_weapon_reference_cache_category")
        .table(entities::weapon_reference_cache::Entity)
        .if_not_exists()
        .col(entities::weapon_reference_cache::Column::CategoryLabel)
        .col(entities::weapon_reference_cache::Column::DisplayName);
    index.to_owned()
}

fn create_montage_clips_clip_sequence_index() -> IndexCreateStatement {
    let mut index = Index::create();
    index
        .name("idx_montage_clips_clip_id")
        .table(entities::montage_clips::Entity)
        .if_not_exists()
        .col(entities::montage_clips::Column::ClipId)
        .col(entities::montage_clips::Column::SequenceIndex);
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
            events: record.events,
        }
    }
}

fn database_path() -> PathBuf {
    directories::ProjectDirs::from("", "", "nanite-clip")
        .map(|dirs| dirs.data_local_dir().join("clips.sqlite3"))
        .unwrap_or_else(|| PathBuf::from("nanite-clip-clips.sqlite3"))
}

fn next_migration_backup_path(
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

fn validate_output_destination(destination: &Path) -> Result<(), ClipStoreError> {
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

fn atomic_write(destination: &Path, contents: &[u8]) -> Result<(), ClipStoreError> {
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

fn append_csv_row(output: &mut String, columns: &[String]) {
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
fn rows_to_clip_records(rows: Vec<primitives::Row>) -> Result<Vec<ClipRecord>, ClipStoreError> {
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
                overlap_count: row.try_get::<i64>("overlap_count")? as u32,
                alert_count: row.try_get::<i64>("alert_count")? as u32,
                post_process_status: PostProcessStatus::Legacy,
                post_process_error: None,
                path,
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
fn rows_to_clip_alerts(rows: Vec<primitives::Row>) -> Result<Vec<ClipAlertRecord>, ClipStoreError> {
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
fn rows_to_clip_overlaps(
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
fn rows_to_clip_uploads(
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
fn rows_to_background_jobs(
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

fn background_job_from_model(
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

fn interrupted_background_job_detail(existing_detail: Option<String>) -> String {
    match existing_detail {
        Some(detail) if !detail.trim().is_empty() => {
            format!("{detail} {INTERRUPTED_BACKGROUND_JOB_DETAIL}")
        }
        _ => INTERRUPTED_BACKGROUND_JOB_DETAIL.to_string(),
    }
}

#[allow(dead_code)]
fn raw_rows_to_clip_raw_events(
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

fn read_count_rows(rows: Vec<primitives::Row>) -> Result<Vec<CountByLabel>, ClipStoreError> {
    rows.into_iter()
        .map(|row| {
            Ok(CountByLabel {
                label: row.try_get::<String>("label")?,
                count: row.try_get::<i64>("count")? as u32,
            })
        })
        .collect()
}

fn read_base_count_rows(rows: Vec<primitives::Row>) -> Result<Vec<BaseCount>, ClipStoreError> {
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

fn read_string_option_rows(rows: Vec<primitives::Row>) -> Result<Vec<String>, ClipStoreError> {
    rows.into_iter()
        .map(|row| row.try_get::<String>("label").map_err(ClipStoreError::from))
        .collect()
}

fn file_size_bytes_for_path(path: Option<&str>) -> Option<u64> {
    std::fs::metadata(Path::new(path?))
        .ok()
        .map(|metadata| metadata.len())
}

fn timestamp_millis_to_utc(value: i64) -> Result<DateTime<Utc>, ClipStoreError> {
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
