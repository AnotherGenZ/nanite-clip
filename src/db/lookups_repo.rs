use super::*;

pub(super) async fn cached_lookup(
    store: &ClipStore,
    kind: LookupKind,
    lookup_id: i64,
) -> Result<Option<String>, ClipStoreError> {
    Ok(
        entities::lookup_cache::Entity::find_by_id((kind.as_str().to_string(), lookup_id))
            .one(&store.pool)
            .await?
            .map(|entry| entry.display_name),
    )
}

pub(super) async fn find_lookup_by_name(
    store: &ClipStore,
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

    match primitives::fetch_optional_stmt(&query, &store.pool).await? {
        Some(row) => Ok(Some((
            row.try_get("lookup_id")?,
            row.try_get("display_name")?,
        ))),
        None => Ok(None),
    }
}

pub(super) async fn list_lookups(
    store: &ClipStore,
    kind: LookupKind,
) -> Result<Vec<(i64, String)>, ClipStoreError> {
    let rows = entities::lookup_cache::Entity::find()
        .filter(entities::lookup_cache::Column::LookupKind.eq(kind.as_str()))
        .order_by_asc(entities::lookup_cache::Column::DisplayName)
        .order_by_asc(entities::lookup_cache::Column::LookupId)
        .all(&store.pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| (row.lookup_id, row.display_name))
        .collect())
}

pub(super) async fn store_lookup(
    store: &ClipStore,
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
    .exec(&store.pool)
    .await?;

    Ok(())
}

pub(super) async fn store_lookups(
    store: &ClipStore,
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
        .exec(&store.pool)
        .await?;

    Ok(())
}

pub(super) async fn list_weapon_references(
    store: &ClipStore,
) -> Result<Vec<WeaponReferenceCacheEntry>, ClipStoreError> {
    let rows = entities::weapon_reference_cache::Entity::find()
        .order_by_asc(entities::weapon_reference_cache::Column::CategoryLabel)
        .order_by_asc(entities::weapon_reference_cache::Column::DisplayName)
        .order_by_asc(entities::weapon_reference_cache::Column::ItemId)
        .all(&store.pool)
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

pub(super) async fn store_weapon_references(
    store: &ClipStore,
    references: &[WeaponReferenceCacheEntry],
) -> Result<(), ClipStoreError> {
    let tx = primitives::begin(&store.pool).await?;
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

pub(super) async fn cached_character_outfit(
    store: &ClipStore,
    character_id: u64,
) -> Result<Option<CharacterOutfitCacheEntry>, ClipStoreError> {
    let min_resolved_ts = Utc::now().timestamp_millis() - CHARACTER_OUTFIT_CACHE_TTL_MS;
    let row = entities::character_outfit_cache::Entity::find_by_id(character_id as i64)
        .filter(entities::character_outfit_cache::Column::ResolvedTs.gte(min_resolved_ts))
        .one(&store.pool)
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
        .exec(&store.pool)
        .await?;

    Ok(None)
}

pub(super) async fn store_character_outfit(
    store: &ClipStore,
    character_id: u64,
    outfit_id: Option<u64>,
    outfit_tag: Option<&str>,
) -> Result<(), ClipStoreError> {
    prune_expired_character_outfit_cache(store).await?;

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
    .exec(&store.pool)
    .await?;

    Ok(())
}

pub(super) async fn prune_expired_character_outfit_cache(
    store: &ClipStore,
) -> Result<(), ClipStoreError> {
    let min_resolved_ts = Utc::now().timestamp_millis() - CHARACTER_OUTFIT_CACHE_TTL_MS;
    entities::character_outfit_cache::Entity::delete_many()
        .filter(entities::character_outfit_cache::Column::ResolvedTs.lt(min_resolved_ts))
        .exec(&store.pool)
        .await?;

    Ok(())
}

pub(super) async fn upsert_alert(
    store: &ClipStore,
    alert: &AlertInstanceRecord,
) -> Result<(), ClipStoreError> {
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
                    Expr::cust("\"excluded\".\"started_ts\""),
                ),
                (
                    entities::alert_instances::Column::EndedTs,
                    Expr::cust("\"excluded\".\"ended_ts\""),
                ),
                (
                    entities::alert_instances::Column::StateName,
                    Expr::cust("\"excluded\".\"state_name\""),
                ),
                (
                    entities::alert_instances::Column::WinnerFaction,
                    Expr::cust("\"excluded\".\"winner_faction\""),
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
    .exec(&store.pool)
    .await?;

    Ok(())
}
