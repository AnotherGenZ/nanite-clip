use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "Text")]
pub enum PostProcessStatus {
    #[sea_orm(string_value = "NotRequired")]
    NotRequired,
    #[sea_orm(string_value = "Pending")]
    Pending,
    #[sea_orm(string_value = "Completed")]
    Completed,
    #[sea_orm(string_value = "Failed")]
    Failed,
    #[sea_orm(string_value = "Legacy")]
    Legacy,
}

pub mod lookup_cache {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "lookup_cache")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub lookup_kind: String,
        #[sea_orm(primary_key, auto_increment = false)]
        pub lookup_id: i64,
        pub display_name: String,
        #[sea_orm(indexed)]
        pub resolved_ts: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod weapon_reference_cache {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "weapon_reference_cache")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub item_id: i64,
        #[sea_orm(indexed)]
        pub weapon_id: i64,
        pub display_name: String,
        pub category_label: String,
        pub faction_id: Option<i64>,
        pub weapon_group_id: Option<i64>,
        #[sea_orm(indexed)]
        pub resolved_ts: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod character_outfit_cache {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "character_outfit_cache")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub character_id: i64,
        pub outfit_id: Option<i64>,
        pub outfit_tag: Option<String>,
        #[sea_orm(indexed)]
        pub resolved_ts: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod clip_uploads {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "clip_uploads")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        #[sea_orm(indexed)]
        pub clip_id: i64,
        pub provider: String,
        pub state: String,
        pub external_id: Option<String>,
        pub clip_url: Option<String>,
        pub error_message: Option<String>,
        pub started_ts: i64,
        pub updated_ts: i64,
        pub completed_ts: Option<i64>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod clip_events {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "clip_events")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        #[sea_orm(indexed)]
        pub clip_id: i64,
        pub event_kind: String,
        pub occurrences: i64,
        pub points: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod clip_raw_events {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "clip_raw_events")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        #[sea_orm(indexed)]
        pub clip_id: i64,
        #[sea_orm(indexed)]
        pub event_ts: i64,
        pub event_kind: String,
        pub world_id: i64,
        pub zone_id: Option<i64>,
        pub facility_id: Option<i64>,
        pub actor_character_id: Option<i64>,
        #[sea_orm(indexed)]
        pub other_character_id: Option<i64>,
        pub actor_class: Option<String>,
        #[sea_orm(indexed)]
        pub attacker_weapon_id: Option<i64>,
        pub attacker_vehicle_id: Option<i64>,
        pub vehicle_killed_id: Option<i64>,
        pub characters_killed: i64,
        pub is_headshot: bool,
        pub experience_id: Option<i64>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod clip_overlaps {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "clip_overlaps")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        #[sea_orm(indexed)]
        pub clip_id: i64,
        #[sea_orm(primary_key, auto_increment = false)]
        #[sea_orm(indexed)]
        pub overlap_clip_id: i64,
        pub overlap_duration_ms: i64,
        pub detected_ts: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod clip_tags {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "clip_tags")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        #[sea_orm(indexed)]
        pub clip_id: i64,
        #[sea_orm(indexed)]
        pub tag_name: String,
        pub created_ts: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod alert_instances {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "alert_instances")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub alert_key: String,
        pub label: String,
        pub world_id: i64,
        #[sea_orm(indexed)]
        pub zone_id: i64,
        pub metagame_event_id: i64,
        #[sea_orm(indexed)]
        pub started_ts: i64,
        pub ended_ts: Option<i64>,
        pub state_name: String,
        pub winner_faction: Option<String>,
        pub faction_nc: f32,
        pub faction_tr: f32,
        pub faction_vs: f32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod clip_alert_links {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "clip_alert_links")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub clip_id: i64,
        #[sea_orm(primary_key, auto_increment = false)]
        #[sea_orm(indexed)]
        pub alert_key: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod collections {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "collections")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        #[sea_orm(indexed)]
        pub name: String,
        pub description: Option<String>,
        pub created_ts: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod collection_clips {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "collection_clips")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub collection_id: i64,
        #[sea_orm(primary_key, auto_increment = false)]
        #[sea_orm(indexed)]
        pub clip_id: i64,
        pub added_ts: i64,
        pub sequence_index: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod background_jobs {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "background_jobs")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: i64,
        pub kind: String,
        pub label: String,
        #[sea_orm(indexed)]
        pub state: String,
        pub related_clip_ids_json: String,
        pub progress_current_step: Option<i64>,
        pub progress_total_steps: Option<i64>,
        pub progress_message: Option<String>,
        pub started_ts: i64,
        #[sea_orm(indexed)]
        pub updated_ts: i64,
        pub finished_ts: Option<i64>,
        pub detail: Option<String>,
        pub cancellable: bool,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod montages {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "montages")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        pub output_path: String,
        pub created_ts: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod montage_clips {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "montage_clips")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub montage_id: i64,
        #[sea_orm(primary_key, auto_increment = false)]
        #[sea_orm(indexed)]
        pub clip_id: i64,
        pub sequence_index: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod clips {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "clips")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        #[sea_orm(indexed)]
        pub trigger_event_ts: i64,
        #[sea_orm(indexed)]
        pub clip_start_ts: i64,
        #[sea_orm(indexed)]
        pub clip_end_ts: i64,
        #[sea_orm(indexed)]
        pub saved_ts: i64,
        pub clip_origin: String,
        #[sea_orm(indexed)]
        pub rule_id: String,
        pub clip_duration_secs: i64,
        #[sea_orm(indexed)]
        pub session_id: Option<String>,
        #[sea_orm(indexed)]
        pub character_id: i64,
        #[sea_orm(indexed)]
        pub world_id: i64,
        #[sea_orm(indexed)]
        pub zone_id: Option<i64>,
        #[sea_orm(indexed)]
        pub facility_id: Option<i64>,
        #[sea_orm(indexed)]
        pub profile_id: String,
        pub path: Option<String>,
        pub score: i64,
        pub honu_session_id: Option<i64>,
        #[sea_orm(indexed)]
        pub favorited: bool,
        pub post_process_status: PostProcessStatus,
        pub post_process_error: Option<String>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod clip_audio_tracks {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "clip_audio_tracks")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        #[sea_orm(indexed)]
        pub clip_id: i64,
        pub stream_index: i32,
        pub role: String,
        pub label: String,
        pub gain_db: f32,
        pub muted: bool,
        pub source_kind: String,
        pub source_value: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}
