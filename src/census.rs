use std::time::Duration;

use auraxis::api::client::{ApiClient, ApiClientConfig};
use auraxis::api::request::{FilterType, Join};
use auraxis::realtime::client::{RealtimeClient, RealtimeClientConfig};
use auraxis::realtime::event::{Event, EventNames};
use auraxis::realtime::subscription::{
    CharacterSubscription, EventSubscription, SubscriptionSettings, WorldSubscription,
};
use auraxis::{CharacterID, Faction, Loadout, WorldID};
use chrono::Utc;
use iced::futures::{SinkExt, Stream};
use serde_json::Value;
use tokio::sync::mpsc::Receiver;

use crate::rules::{CharacterClass, ClassifiedEvent, EventKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CensusLookupResult {
    pub id: i64,
    pub display_name: String,
    pub world_id: Option<u32>,
    pub faction_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCharacter {
    pub id: CharacterID,
    pub display_name: String,
    pub world_id: Option<u32>,
    pub faction_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutfitLookupResult {
    pub id: i64,
    pub tag: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeaponReference {
    pub item_id: i64,
    pub weapon_id: i64,
    pub display_name: String,
    pub category_label: String,
    pub faction: Option<Faction>,
    pub weapon_group_id: Option<i64>,
}

const WEAPON_REFERENCE_JOIN: &str = "item^on:item_id^to:item_id^inject_at:item^show:item_id'name.en'item_category_id'faction_id(item_category^on:item_category_id^to:item_category_id^inject_at:category^show:name.en),weapon^on:weapon_id^to:weapon_id^inject_at:weapon^show:weapon_id'weapon_group_id";

const EXPERIENCE_EVENT_MAP: &[(u16, EventKind)] = &[
    (7, EventKind::Revive),
    (8, EventKind::KillStreakBonus),
    (10, EventKind::DominationKill),
    (11, EventKind::RevengeKill),
    (15, EventKind::ControlPointDefend),
    (16, EventKind::ControlPointAttack),
    (21, EventKind::ObjectiveDestroyed),
    (25, EventKind::MultipleKill),
    (26, EventKind::VehicleRoadkill),
    (29, EventKind::MaxKill),
    (32, EventKind::NemesisKill),
    (53, EventKind::Revive),
    (72, EventKind::VehicleRamBonus),
    (86, EventKind::ExplosiveDestruction),
    (143, EventKind::DropPodKill),
    (234, EventKind::FacilityBombPlanted),
    (235, EventKind::FacilityBombDefused),
    (236, EventKind::FacilityTerminalHack),
    (237, EventKind::FacilityTurretHack),
    (272, EventKind::CapturePointConverted),
    (278, EventKind::PriorityKill),
    (279, EventKind::HighPriorityKill),
    (293, EventKind::MotionDetect),
    (335, EventKind::SaviorKill),
    (336, EventKind::Saved),
    (353, EventKind::ScoutRadarDetect),
    (370, EventKind::MotionSensorSpotterKill),
    (592, EventKind::SaviorKill),
    (593, EventKind::BountyKill),
    (594, EventKind::BountyCashedIn),
    (595, EventKind::BountyKillStreak),
    (556, EventKind::ObjectivePulseDefend),
    (557, EventKind::ObjectivePulseCapture),
    (1409, EventKind::RouterKill),
    (2133, EventKind::CtfFlagCaptured),
    (2134, EventKind::CtfFlagReturned),
];

/// Resolve a character name to a character ID plus stable character metadata.
pub async fn resolve_character(
    service_id: &str,
    name: &str,
) -> Result<ResolvedCharacter, CensusError> {
    resolve_character_reference(service_id, name)
        .await?
        .map(|lookup| ResolvedCharacter {
            id: lookup.id as CharacterID,
            display_name: lookup.display_name,
            world_id: lookup.world_id,
            faction_id: lookup.faction_id,
        })
        .ok_or_else(|| CensusError::CharacterNotFound(name.to_string()))
}

pub async fn resolve_character_reference(
    service_id: &str,
    name: &str,
) -> Result<Option<CensusLookupResult>, CensusError> {
    let client = api_client(service_id);

    let response = client
        .get("character")
        .filter(
            "name.first_lower",
            FilterType::EqualTo,
            &name.to_lowercase(),
        )
        .show("character_id")
        .show("name.first")
        .show("name.first_lower")
        .show("faction_id")
        .join(
            Join::new(
                "characters_world",
                "character_id",
                "character_id",
                "characters_world",
            )
            .show(["world_id"]),
        )
        .limit(1)
        .build()
        .await
        .map_err(|e| CensusError::Api(e.to_string()))?;

    let Some(item) = response.items.first() else {
        return Ok(None);
    };

    let character_id = item["character_id"]
        .as_str()
        .ok_or_else(|| CensusError::Api("missing character_id in response".into()))?
        .parse::<i64>()
        .map_err(|e| CensusError::Api(e.to_string()))?;
    let display_name = first_string_field(item, &["name.first", "name.first_lower"])
        .unwrap_or_else(|| name.trim().to_string());
    let world_id = parse_non_zero_u32_field(item, &["characters_world.world_id", "world_id"]);
    let faction_id = parse_non_zero_u32_field(item, &["faction_id"]);

    Ok(Some(CensusLookupResult {
        id: character_id,
        display_name,
        world_id,
        faction_id,
    }))
}

pub async fn resolve_outfit_reference(
    service_id: &str,
    tag: &str,
) -> Result<Option<OutfitLookupResult>, CensusError> {
    if service_id.trim().is_empty() {
        return Ok(None);
    }

    let client = api_client(service_id);
    let normalized_tag = tag.trim().trim_start_matches('[').trim_end_matches(']');
    if normalized_tag.is_empty() {
        return Ok(None);
    }

    let response = client
        .get("outfit")
        .filter("alias", FilterType::EqualTo, normalized_tag.to_uppercase())
        .show("outfit_id")
        .show("alias")
        .show("name")
        .lang("en")
        .limit(100)
        .build()
        .await
        .map_err(|e| CensusError::Api(e.to_string()))?;

    let Some(item) = response.items.first() else {
        return Ok(None);
    };

    let outfit_id = item["outfit_id"]
        .as_str()
        .ok_or_else(|| CensusError::Api("missing outfit_id in response".into()))?
        .parse::<i64>()
        .map_err(|e| CensusError::Api(e.to_string()))?;
    let resolved_tag =
        first_string_field(item, &["alias"]).unwrap_or_else(|| normalized_tag.to_uppercase());
    let display_name =
        first_string_field(item, &["alias", "name"]).unwrap_or_else(|| resolved_tag.clone());

    Ok(Some(OutfitLookupResult {
        id: outfit_id,
        tag: resolved_tag,
        display_name,
    }))
}

pub async fn resolve_character_outfit_reference(
    service_id: &str,
    character_id: u64,
) -> Result<Option<OutfitLookupResult>, CensusError> {
    if service_id.trim().is_empty() || character_id == 0 {
        return Ok(None);
    }

    let client = api_client(service_id);
    let response = client
        .get("outfit_member")
        .filter(
            "character_id",
            FilterType::EqualTo,
            character_id.to_string(),
        )
        .show("outfit_id")
        .limit(1)
        .build()
        .await
        .map_err(|e| CensusError::Api(e.to_string()))?;

    let Some(item) = response.items.first() else {
        return Ok(None);
    };

    let outfit_id = item["outfit_id"]
        .as_str()
        .ok_or_else(|| CensusError::Api("missing outfit_id in response".into()))?
        .parse::<i64>()
        .map_err(|e| CensusError::Api(e.to_string()))?;

    Ok(Some(OutfitLookupResult {
        id: outfit_id,
        tag: String::new(),
        display_name: format!("Outfit #{outfit_id}"),
    }))
}

/// Query the Census REST API for the online status of the given characters.
/// Returns the subset of character IDs that are currently logged in (to any
/// world).
pub async fn fetch_online_status(
    service_id: &str,
    character_ids: &[CharacterID],
) -> Result<Vec<CharacterID>, CensusError> {
    if character_ids.is_empty() {
        return Ok(Vec::new());
    }

    let client = api_client(service_id);

    // Census accepts a comma-separated list of IDs for a single filter field.
    let ids_csv: String = character_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let response = client
        .get("characters_online_status")
        .filter("character_id", FilterType::EqualTo, &ids_csv)
        .limit(character_ids.len() as u32)
        .build()
        .await
        .map_err(|e| CensusError::Api(e.to_string()))?;

    // `online_status` is the world ID as a string; "0" means offline.
    let mut online = Vec::new();
    for item in &response.items {
        let Some(id_str) = item["character_id"].as_str() else {
            continue;
        };
        let Ok(id) = id_str.parse::<CharacterID>() else {
            continue;
        };
        let Some(status) = item["online_status"].as_str() else {
            continue;
        };
        if status != "0" {
            online.push(id);
        }
    }
    Ok(online)
}

/// Connect to the Census event stream and subscribe to events for the given
/// characters.
pub async fn start_event_stream(
    service_id: &str,
    character_ids: &[CharacterID],
) -> Result<Receiver<Event>, CensusError> {
    let config = RealtimeClientConfig {
        service_id: service_id.to_string(),
        ..RealtimeClientConfig::default()
    };

    // NOTE: SubscriptionSettings::default() sets worlds to WorldSubscription::All.
    // Census OR's the character and world filters unless
    // logical_and_characters_with_worlds is true, so leaving worlds=All would
    // deliver every world's events on top of our character filter. Set worlds
    // to None so the character filter is the only one applied.
    let subscription = SubscriptionSettings {
        event_names: Some(EventSubscription::Ids({
            let mut event_names = vec![
                EventNames::Death,
                EventNames::VehicleDestroy,
                EventNames::PlayerFacilityCapture,
                EventNames::PlayerFacilityDefend,
                EventNames::PlayerLogin,
                EventNames::PlayerLogout,
                EventNames::MetagameEvent,
            ];
            event_names.extend(
                EXPERIENCE_EVENT_MAP
                    .iter()
                    .map(|(experience_id, _)| EventNames::GainExperienceId(*experience_id)),
            );
            event_names
        })),
        characters: Some(CharacterSubscription::Ids(character_ids.to_vec())),
        logical_and_characters_with_worlds: Some(true),
        worlds: Some(WorldSubscription::Ids(vec![
            WorldID::Connery,
            WorldID::Emerald,
            WorldID::Miller,
            WorldID::Cobalt,
            WorldID::Jaeger,
            WorldID::Soltech,
        ])),
        ..SubscriptionSettings::default()
    };

    let mut client = RealtimeClient::new(config);
    client.subscribe(subscription);

    let receiver = client
        .connect()
        .await
        .map_err(|e| CensusError::Connection(e.to_string()))?;

    tracing::info!(
        "Connected to Census event stream for {} character(s)",
        character_ids.len()
    );
    Ok(receiver)
}

/// Classify a raw Census event into our rule-event system.
/// Returns None if the event is not relevant to our character or rule system.
pub fn classify_event(event: &Event, character_id: CharacterID) -> Option<ClassifiedEvent> {
    match event {
        Event::Death(death) => {
            if death.attacker_character_id == character_id
                && death.attacker_character_id != death.character_id
            {
                let kind = if death.is_headshot {
                    EventKind::Headshot
                } else {
                    EventKind::Kill
                };
                Some(ClassifiedEvent {
                    kind,
                    timestamp: death.timestamp,
                    world_id: death.world_id as u32,
                    zone_id: Some(death.zone_id),
                    facility_id: None,
                    actor_character_id: Some(death.attacker_character_id),
                    other_character_id: Some(death.character_id),
                    other_character_outfit_id: None,
                    characters_killed: 1,
                    attacker_weapon_id: non_zero_weapon_id(death.attacker_weapon_id),
                    attacker_vehicle_id: non_zero_vehicle_id(death.attacker_vehicle_id),
                    vehicle_killed_id: non_zero_vehicle_id(death.vehicle_id),
                    is_headshot: death.is_headshot,
                    actor_class: character_class_from_loadout(death.attacker_loadout_id),
                    experience_id: None,
                })
            } else if death.character_id == character_id {
                Some(ClassifiedEvent {
                    kind: EventKind::Death,
                    timestamp: death.timestamp,
                    world_id: death.world_id as u32,
                    zone_id: Some(death.zone_id),
                    facility_id: None,
                    actor_character_id: Some(death.character_id),
                    other_character_id: Some(death.attacker_character_id),
                    other_character_outfit_id: None,
                    characters_killed: 0,
                    attacker_weapon_id: non_zero_weapon_id(death.attacker_weapon_id),
                    attacker_vehicle_id: non_zero_vehicle_id(death.attacker_vehicle_id),
                    vehicle_killed_id: None,
                    is_headshot: death.is_headshot,
                    actor_class: character_class_from_loadout(death.character_loadout_id),
                    experience_id: None,
                })
            } else {
                None
            }
        }
        Event::VehicleDestroy(vd) => {
            if vd.attacker_character_id == character_id {
                Some(ClassifiedEvent {
                    kind: EventKind::VehicleDestroy,
                    timestamp: vd.timestamp,
                    world_id: vd.world_id as u32,
                    zone_id: Some(vd.zone_id),
                    facility_id: Some(vd.facility_id),
                    actor_character_id: Some(vd.attacker_character_id),
                    other_character_id: Some(vd.character_id),
                    other_character_outfit_id: None,
                    characters_killed: 0,
                    attacker_weapon_id: non_zero_weapon_id(vd.attacker_weapon_id),
                    attacker_vehicle_id: non_zero_vehicle_id(vd.attacker_vehicle_id),
                    vehicle_killed_id: non_zero_vehicle_id(vd.vehicle_id),
                    is_headshot: false,
                    actor_class: character_class_from_loadout(vd.attacker_loadout_id),
                    experience_id: None,
                })
            } else {
                None
            }
        }
        Event::GainExperience(xp) => {
            if xp.character_id == character_id {
                let kind = classify_experience_event(xp.experience_id)?;
                Some(ClassifiedEvent {
                    kind,
                    timestamp: xp.timestamp,
                    world_id: xp.world_id as u32,
                    zone_id: Some(xp.zone_id),
                    facility_id: None,
                    actor_character_id: Some(xp.character_id),
                    other_character_id: non_zero_character_id(xp.other_id),
                    other_character_outfit_id: None,
                    characters_killed: 0,
                    attacker_weapon_id: None,
                    attacker_vehicle_id: None,
                    vehicle_killed_id: None,
                    is_headshot: false,
                    actor_class: character_class_from_loadout(xp.loadout_id),
                    experience_id: Some(xp.experience_id),
                })
            } else {
                None
            }
        }
        Event::PlayerFacilityCapture(cap) => {
            if cap.character_id == character_id {
                Some(ClassifiedEvent {
                    kind: EventKind::FacilityCapture,
                    timestamp: cap.timestamp,
                    world_id: cap.world_id as u32,
                    zone_id: Some(cap.zone_id),
                    facility_id: Some(cap.facility_id),
                    actor_character_id: Some(cap.character_id),
                    other_character_id: None,
                    other_character_outfit_id: None,
                    characters_killed: 0,
                    attacker_weapon_id: None,
                    attacker_vehicle_id: None,
                    vehicle_killed_id: None,
                    is_headshot: false,
                    actor_class: None,
                    experience_id: None,
                })
            } else {
                None
            }
        }
        Event::PlayerFacilityDefend(def) => {
            if def.character_id == character_id {
                Some(ClassifiedEvent {
                    kind: EventKind::FacilityDefend,
                    timestamp: def.timestamp,
                    world_id: def.world_id as u32,
                    zone_id: Some(def.zone_id),
                    facility_id: Some(def.facility_id),
                    actor_character_id: Some(def.character_id),
                    other_character_id: None,
                    other_character_outfit_id: None,
                    characters_killed: 0,
                    attacker_weapon_id: None,
                    attacker_vehicle_id: None,
                    vehicle_killed_id: None,
                    is_headshot: false,
                    actor_class: None,
                    experience_id: None,
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

fn classify_experience_event(experience_id: u16) -> Option<EventKind> {
    EXPERIENCE_EVENT_MAP
        .iter()
        .find_map(|(id, kind)| (*id == experience_id).then_some(*kind))
}

fn character_class_from_loadout(loadout: Loadout) -> Option<CharacterClass> {
    match loadout {
        Loadout::NCInfiltrator
        | Loadout::TRInfiltrator
        | Loadout::VSInfiltrator
        | Loadout::NSInfiltrator => Some(CharacterClass::Infiltrator),
        Loadout::NCLightAssault
        | Loadout::TRLightAssault
        | Loadout::VSLightAssault
        | Loadout::NSLightAssault => Some(CharacterClass::LightAssault),
        Loadout::NCMedic | Loadout::TRMedic | Loadout::VSMedic | Loadout::NSMedic => {
            Some(CharacterClass::Medic)
        }
        Loadout::NCEngineer | Loadout::TREngineer | Loadout::VSEngineer | Loadout::NSEngineer => {
            Some(CharacterClass::Engineer)
        }
        Loadout::NCHeavyAssault
        | Loadout::TRHeavyAssault
        | Loadout::VSHeavyAssault
        | Loadout::NSHeavyAssault => Some(CharacterClass::HeavyAssault),
        Loadout::NCMAX | Loadout::TRMAX | Loadout::VSMAX | Loadout::NSMAX => {
            Some(CharacterClass::Max)
        }
        Loadout::Unknown => None,
    }
}

pub fn world_name(world_id: u32) -> String {
    match WorldID::try_from(world_id as i16) {
        Ok(WorldID::Connery) => "Osprey".into(),
        Ok(WorldID::Miller) => "Wainwright".into(),
        Ok(world) => world.to_string(),
        Err(_) => format!("World #{world_id}"),
    }
}

pub fn faction_name(faction_id: u32) -> String {
    match Faction::try_from(faction_id as i16) {
        Ok(Faction::VS) => "VS".into(),
        Ok(Faction::NC) => "NC".into(),
        Ok(Faction::TR) => "TR".into(),
        Ok(Faction::NS) => "NS".into(),
        Ok(Faction::Unknown) => format!("Faction #{faction_id}"),
        Err(_) => format!("Faction #{faction_id}"),
    }
}

pub fn continent_name(zone_id: Option<u32>) -> String {
    match zone_id {
        Some(2) => "Indar".into(),
        Some(4) => "Hossin".into(),
        Some(6) => "Amerish".into(),
        Some(8) => "Esamir".into(),
        Some(96) => "VR Training".into(),
        Some(344) => "Oshur".into(),
        Some(362) => "Sanctuary".into(),
        Some(id) => format!("Zone #{id}"),
        None => "Unknown".into(),
    }
}

pub fn base_name(facility_id: Option<u32>) -> Option<String> {
    facility_id.map(|id| format!("Facility #{id}"))
}

pub fn alert_label(metagame_event_id: u8, zone_id: u32) -> String {
    let event_label = match metagame_event_id {
        1 => "Meltdown Alert",
        2 => "Territory Alert",
        3 => "Facility Alert",
        4 => "Sudden Death Alert",
        _ => "Metagame Alert",
    };
    format!("{event_label} ({})", continent_name(Some(zone_id)))
}

pub fn vehicle_name(vehicle_id: Option<u16>) -> Option<String> {
    vehicle_id.map(|id| format!("Vehicle #{id}"))
}

pub async fn resolve_zone_name(
    service_id: &str,
    zone_id: u32,
) -> Result<Option<String>, CensusError> {
    resolve_name_from_collection(
        service_id,
        "zone",
        "zone_id",
        zone_id,
        &["name.en", "name", "code"],
    )
    .await
}

pub async fn resolve_facility_name(
    service_id: &str,
    facility_id: u32,
) -> Result<Option<String>, CensusError> {
    resolve_name_from_collection(
        service_id,
        "facility",
        "facility_id",
        facility_id,
        &["facility_name", "facility_name.en", "name.en", "name"],
    )
    .await
}

pub async fn resolve_vehicle_name(
    service_id: &str,
    vehicle_id: u16,
) -> Result<Option<String>, CensusError> {
    resolve_name_from_collection(
        service_id,
        "vehicle",
        "vehicle_id",
        vehicle_id,
        &["name.en", "name", "description.en"],
    )
    .await
}

pub async fn resolve_outfit_name(
    service_id: &str,
    outfit_id: u64,
) -> Result<Option<String>, CensusError> {
    resolve_name_from_collection(
        service_id,
        "outfit",
        "outfit_id",
        outfit_id,
        &["alias", "name"],
    )
    .await
}

pub async fn fetch_vehicle_references(
    service_id: &str,
) -> Result<Vec<CensusLookupResult>, CensusError> {
    if service_id.trim().is_empty() {
        return Ok(Vec::new());
    }

    let client = api_client(service_id);
    let response = client
        .get("vehicle")
        .show("vehicle_id")
        .show("name.en")
        .show("name")
        .show("description.en")
        .lang("en")
        .limit(1_000)
        .build()
        .await
        .map_err(|e| CensusError::Api(e.to_string()))?;

    let mut vehicles = response
        .items
        .iter()
        .filter_map(|item| {
            let id = item["vehicle_id"].as_str()?.parse::<i64>().ok()?;
            let display_name = first_string_field(item, &["name.en", "name", "description.en"])?;
            Some(CensusLookupResult {
                id,
                display_name,
                world_id: None,
                faction_id: None,
            })
        })
        .collect::<Vec<_>>();

    vehicles.sort_by(|left, right| {
        left.display_name
            .to_lowercase()
            .cmp(&right.display_name.to_lowercase())
            .then(left.id.cmp(&right.id))
    });
    vehicles.dedup_by(|left, right| left.id == right.id);

    Ok(vehicles)
}

pub async fn fetch_weapon_references(
    service_id: &str,
) -> Result<Vec<WeaponReference>, CensusError> {
    let Some(normalized_service_id) = normalize_service_id(service_id) else {
        return Ok(Vec::new());
    };

    let client = reqwest::Client::builder()
        .user_agent("nanite-clip")
        .build()
        .map_err(|error| CensusError::Connection(error.to_string()))?;
    let url = format!(
        "https://census.daybreakgames.com/s:{normalized_service_id}/get/ps2:v2/item_to_weapon/"
    );
    let response = client
        .get(&url)
        .query(&[("c:limit", "10000"), ("c:join", WEAPON_REFERENCE_JOIN)])
        .send()
        .await
        .map_err(|error| CensusError::Connection(error.to_string()))?
        .error_for_status()
        .map_err(|error| CensusError::Api(error.to_string()))?
        .json::<Value>()
        .await
        .map_err(|error| CensusError::Api(error.to_string()))?;

    let Some(items) = response["item_to_weapon_list"].as_array() else {
        return Err(CensusError::Api(
            "missing item_to_weapon_list in weapon reference response".into(),
        ));
    };

    let mut references = items
        .iter()
        .filter_map(parse_weapon_reference_row)
        .collect::<Vec<_>>();

    references.sort_by(|left, right| {
        left.display_name
            .to_lowercase()
            .cmp(&right.display_name.to_lowercase())
            .then(
                left.category_label
                    .to_lowercase()
                    .cmp(&right.category_label.to_lowercase()),
            )
            .then(left.weapon_id.cmp(&right.weapon_id))
            .then(left.item_id.cmp(&right.item_id))
    });
    references.dedup_by(|left, right| left.item_id == right.item_id);

    Ok(references)
}

pub async fn resolve_weapon_name(
    service_id: &str,
    weapon_id: u32,
) -> Result<Option<String>, CensusError> {
    resolve_name_from_collection(
        service_id,
        "item",
        "item_id",
        weapon_id,
        &[
            "name.en",
            "name",
            "item_name",
            "item_name.en",
            "description.en",
        ],
    )
    .await
}

pub async fn resolve_character_name(
    service_id: &str,
    character_id: u64,
) -> Result<Option<String>, CensusError> {
    resolve_name_from_collection(
        service_id,
        "character",
        "character_id",
        character_id,
        &["name.first", "name.first_lower"],
    )
    .await
}

fn non_zero_vehicle_id(vehicle_id: u16) -> Option<u16> {
    (vehicle_id != 0).then_some(vehicle_id)
}

fn non_zero_weapon_id(weapon_id: u32) -> Option<u32> {
    (weapon_id != 0).then_some(weapon_id)
}

fn non_zero_character_id(character_id: u64) -> Option<u64> {
    (character_id != 0).then_some(character_id)
}

fn api_client(service_id: &str) -> ApiClient {
    ApiClient::new(ApiClientConfig {
        service_id: normalize_service_id(service_id),
        ..ApiClientConfig::default()
    })
}

fn normalize_service_id(service_id: &str) -> Option<String> {
    let normalized = service_id
        .trim()
        .trim_start_matches("s:")
        .trim()
        .to_string();
    (!normalized.is_empty()).then_some(normalized)
}

fn parse_weapon_reference_row(item: &Value) -> Option<WeaponReference> {
    let item_id = item["item_id"].as_str()?.parse::<i64>().ok()?;
    let weapon_id = item["weapon_id"].as_str()?.parse::<i64>().ok()?;
    let display_name = first_string_field(
        item,
        &[
            "item.name.en",
            "item.name",
            "item.item_name.en",
            "item.item_name",
            "weapon.name.en",
            "weapon.name",
        ],
    )?;
    let category_label = first_string_field(item, &["item.category.name.en", "item.category.name"])
        .unwrap_or_else(|| "Other".into());
    let faction = item["item"]["faction_id"]
        .as_str()
        .and_then(|value| value.parse::<Faction>().ok());
    let weapon_group_id = item["weapon"]["weapon_group_id"]
        .as_str()
        .and_then(|value| value.parse::<i64>().ok());

    Some(WeaponReference {
        item_id,
        weapon_id,
        display_name,
        category_label,
        faction,
        weapon_group_id,
    })
}

async fn resolve_name_from_collection(
    service_id: &str,
    collection: &str,
    id_field: &str,
    id: impl ToString,
    name_fields: &[&str],
) -> Result<Option<String>, CensusError> {
    if service_id.trim().is_empty() {
        return Ok(None);
    }

    let client = api_client(service_id);
    let response = client
        .get(collection)
        .filter(id_field, FilterType::EqualTo, id.to_string())
        .lang("en")
        .limit(1)
        .build()
        .await
        .map_err(|e| CensusError::Api(e.to_string()))?;

    let Some(item) = response.items.first() else {
        return Ok(None);
    };

    Ok(first_string_field(item, name_fields))
}

fn first_string_field(item: &Value, candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .find_map(|path| string_field(item, path))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn string_field<'a>(value: &'a Value, path: &str) -> Option<&'a str> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    current.as_str()
}

fn parse_non_zero_u32_field(item: &Value, candidates: &[&str]) -> Option<u32> {
    candidates
        .iter()
        .find_map(|path| string_field(item, path))
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value != 0)
}

/// Check if an event is a PlayerLogin for one of our tracked characters.
pub fn is_character_login(event: &Event, character_ids: &[CharacterID]) -> Option<CharacterID> {
    if let Event::PlayerLogin(login) = event {
        if character_ids.contains(&login.character_id) {
            return Some(login.character_id);
        }
    }
    None
}

/// Check if an event is a PlayerLogout for one of our tracked characters.
pub fn is_character_logout(event: &Event, character_ids: &[CharacterID]) -> Option<CharacterID> {
    if let Event::PlayerLogout(logout) = event {
        if character_ids.contains(&logout.character_id) {
            return Some(logout.character_id);
        }
    }
    None
}

#[derive(Debug, thiserror::Error)]
pub enum CensusError {
    #[error("character not found: {0}")]
    CharacterNotFound(String),
    #[error("Census API error: {0}")]
    Api(String),
    #[error("Census connection error: {0}")]
    Connection(String),
}

/// Events emitted by the Census event stream, translated for application use.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// One of our tracked characters logged in.
    Login {
        character_id: CharacterID,
    },
    /// One of our tracked characters logged out.
    Logout {
        character_id: CharacterID,
    },
    /// A classified gameplay event for one of our tracked characters.
    Classified {
        character_id: CharacterID,
        event: ClassifiedEvent,
    },
    Alert(AlertUpdate),
    /// The event stream disconnected; the stream will try to reconnect.
    Disconnected,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlertUpdate {
    pub alert_key: String,
    pub world_id: u32,
    pub zone_id: u32,
    pub instance_id: u32,
    pub metagame_event_id: u8,
    pub state_name: String,
    pub lifecycle: AlertLifecycle,
    pub timestamp: chrono::DateTime<Utc>,
    pub faction_nc: f32,
    pub faction_tr: f32,
    pub faction_vs: f32,
    pub winner_faction: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertLifecycle {
    Started,
    Updated,
    Ended,
}

/// Build a long-running [`Stream`] that connects to the Census event stream,
/// subscribes to login/logout/gameplay events for the given characters, and
/// emits [`StreamEvent`]s. Reconnects automatically on failure.
pub fn event_stream(
    service_id: String,
    character_ids: Vec<CharacterID>,
) -> impl Stream<Item = StreamEvent> + Send + 'static {
    iced::stream::channel(128, async move |mut output| {
        loop {
            let mut rx = match start_event_stream(&service_id, &character_ids).await {
                Ok(rx) => rx,
                Err(e) => {
                    tracing::error!("Census connect failed: {e}; retrying in 5s");
                    let _ = output.send(StreamEvent::Disconnected).await;
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };

            while let Some(event) = rx.recv().await {
                // Debug: log every raw event received from Census. The subscription
                // already filters to our character IDs, so everything here is
                // relevant to a tracked character.
                tracing::debug!("census raw event: {event}: {event:?}");
                // Login / logout first — these don't go through classify_event.
                if let Some(id) = is_character_login(&event, &character_ids) {
                    if output
                        .send(StreamEvent::Login { character_id: id })
                        .await
                        .is_err()
                    {
                        return;
                    }
                    continue;
                }
                if let Some(id) = is_character_logout(&event, &character_ids) {
                    if output
                        .send(StreamEvent::Logout { character_id: id })
                        .await
                        .is_err()
                    {
                        return;
                    }
                    continue;
                }
                // Gameplay events — try each configured character. At most one
                // will match (kills/deaths/etc. are attributed to one char).
                if let Some(alert) = classify_alert_update(&event) {
                    if output.send(StreamEvent::Alert(alert)).await.is_err() {
                        return;
                    }
                    continue;
                }
                for &cid in &character_ids {
                    if let Some(classified) = classify_event(&event, cid) {
                        tracing::info!(
                            "classified {:?} for char {cid} at {}",
                            classified.kind,
                            classified.timestamp,
                        );
                        let msg = StreamEvent::Classified {
                            character_id: cid,
                            event: classified,
                        };
                        if output.send(msg).await.is_err() {
                            return;
                        }
                        break;
                    }
                }
            }

            // Receiver closed — connection dropped. Try to reconnect.
            tracing::warn!("Census stream disconnected; reconnecting in 2s");
            let _ = output.send(StreamEvent::Disconnected).await;
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    })
}

pub fn classify_alert_update(event: &Event) -> Option<AlertUpdate> {
    let Event::MetagameEvent(metagame) = event else {
        return None;
    };

    let lifecycle = metagame_event_lifecycle(&metagame.metagame_event_state_name);
    let world_id = metagame.world_id as u32;
    let zone_id = metagame.zone_id as u32;
    Some(AlertUpdate {
        alert_key: format!("{world_id}:{zone_id}:{}", metagame.instance_id),
        world_id,
        zone_id,
        instance_id: metagame.instance_id,
        metagame_event_id: metagame.metagame_event_id,
        state_name: metagame.metagame_event_state_name.clone(),
        lifecycle,
        timestamp: metagame.timestamp,
        faction_nc: metagame.faction_nc,
        faction_tr: metagame.faction_tr,
        faction_vs: metagame.faction_vs,
        winner_faction: (lifecycle == AlertLifecycle::Ended)
            .then(|| {
                winning_faction_label(
                    metagame.faction_nc,
                    metagame.faction_tr,
                    metagame.faction_vs,
                )
            })
            .flatten(),
    })
}

fn metagame_event_lifecycle(state_name: &str) -> AlertLifecycle {
    let normalized = state_name.trim().to_lowercase();
    if normalized.contains("start") {
        AlertLifecycle::Started
    } else if normalized.contains("end") || normalized.contains("lock") {
        AlertLifecycle::Ended
    } else {
        AlertLifecycle::Updated
    }
}

fn winning_faction_label(faction_nc: f32, faction_tr: f32, faction_vs: f32) -> Option<String> {
    let mut scores = [
        (Faction::NC, faction_nc),
        (Faction::TR, faction_tr),
        (Faction::VS, faction_vs),
    ];
    scores.sort_by(|left, right| right.1.total_cmp(&left.1));
    let (winner, winner_score) = scores[0];
    let tied = scores
        .iter()
        .filter(|(_, score)| (*score - winner_score).abs() < f32::EPSILON)
        .count()
        > 1;
    if tied {
        None
    } else {
        Some(
            match winner {
                Faction::NC => "NC",
                Faction::TR => "TR",
                Faction::VS => "VS",
                _ => "Unknown",
            }
            .into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_service_id_accepts_prefixed_and_plain_values() {
        assert_eq!(normalize_service_id("example"), Some("example".into()));
        assert_eq!(normalize_service_id("s:example"), Some("example".into()));
        assert_eq!(
            normalize_service_id("  s:example  "),
            Some("example".into())
        );
        assert_eq!(normalize_service_id(""), None);
    }

    #[test]
    fn string_field_reads_nested_json_paths() {
        let value = serde_json::json!({
            "facility_name": "The Crown",
            "name": {
                "en": "Lightning"
            }
        });

        assert_eq!(string_field(&value, "facility_name"), Some("The Crown"));
        assert_eq!(string_field(&value, "name.en"), Some("Lightning"));
        assert_eq!(string_field(&value, "name.fr"), None);
    }

    #[test]
    fn parses_joined_weapon_reference_rows() {
        let value = serde_json::json!({
            "item_id": "3",
            "weapon_id": "3",
            "item": {
                "item_id": "3",
                "item_category_id": "8",
                "name": { "en": "AF-19 Mercenary" },
                "category": {
                    "name": { "en": "Carbine" }
                }
            },
            "weapon": {
                "weapon_id": "3",
                "weapon_group_id": "4"
            }
        });

        assert_eq!(
            parse_weapon_reference_row(&value),
            Some(WeaponReference {
                item_id: 3,
                weapon_id: 3,
                display_name: "AF-19 Mercenary".into(),
                category_label: "Carbine".into(),
                faction: None,
                weapon_group_id: Some(4),
            })
        );
    }

    #[test]
    fn highlightable_experience_ids_map_to_specific_event_kinds() {
        assert_eq!(classify_experience_event(7), Some(EventKind::Revive));
        assert_eq!(classify_experience_event(25), Some(EventKind::MultipleKill));
        assert_eq!(
            classify_experience_event(278),
            Some(EventKind::PriorityKill)
        );
        assert_eq!(
            classify_experience_event(556),
            Some(EventKind::ObjectivePulseDefend)
        );
        assert_eq!(classify_experience_event(593), Some(EventKind::BountyKill));
        assert_eq!(
            classify_experience_event(2133),
            Some(EventKind::CtfFlagCaptured)
        );
    }

    #[test]
    fn non_highlightable_experience_ids_are_ignored() {
        assert_eq!(classify_experience_event(4), None);
        assert_eq!(classify_experience_event(5), None);
        assert_eq!(classify_experience_event(34), None);
    }
}
