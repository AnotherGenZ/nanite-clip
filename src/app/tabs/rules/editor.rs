use super::*;

pub(crate) fn weapon_lookup_options(
    entries: Vec<WeaponReferenceCacheEntry>,
) -> Vec<WeaponLookupOption> {
    let mut grouped = BTreeMap::<u32, Vec<WeaponReferenceCacheEntry>>::new();
    for entry in entries {
        let family_id = if entry.weapon_id != 0 {
            entry.weapon_id
        } else {
            entry.item_id
        };
        grouped.entry(family_id).or_default().push(entry);
    }

    let mut options = grouped
        .into_values()
        .map(|family| {
            let mut ids = family.iter().map(|entry| entry.item_id).collect::<Vec<_>>();
            ids.sort_unstable();
            ids.dedup();

            WeaponLookupOption {
                ids,
                label: weapon_family_display_name(&family),
                category_label: weapon_family_category_label(&family),
                faction: weapon_family_faction(&family),
            }
        })
        .collect::<Vec<_>>();

    options.sort_by(|left, right| {
        left.category_label
            .to_lowercase()
            .cmp(&right.category_label.to_lowercase())
            .then(left.label.to_lowercase().cmp(&right.label.to_lowercase()))
            .then(left.ids.cmp(&right.ids))
    });

    options
}

pub(crate) fn weapon_family_display_name(entries: &[WeaponReferenceCacheEntry]) -> String {
    entries
        .iter()
        .min_by(|left, right| {
            left.display_name
                .trim()
                .to_lowercase()
                .cmp(&right.display_name.trim().to_lowercase())
                .then(left.display_name.len().cmp(&right.display_name.len()))
                .then(left.item_id.cmp(&right.item_id))
        })
        .map(|entry| entry.display_name.trim().to_string())
        .unwrap_or_else(|| "Unknown weapon".into())
}

pub(crate) fn weapon_family_category_label(entries: &[WeaponReferenceCacheEntry]) -> String {
    let mut categories = entries
        .iter()
        .map(|entry| entry.category_label.trim().to_string())
        .filter(|label| !label.is_empty())
        .collect::<Vec<_>>();
    categories.sort_by(|left, right| {
        left.to_lowercase()
            .cmp(&right.to_lowercase())
            .then(left.cmp(right))
    });
    categories.dedup_by(|left, right| left.eq_ignore_ascii_case(right));

    if categories.len() <= 1 {
        return categories
            .into_iter()
            .next()
            .unwrap_or_else(|| "Other".into());
    }

    let mut platforms = categories
        .iter()
        .filter_map(|label| weapon_slot_platform_name(label))
        .collect::<Vec<_>>();
    platforms.sort();
    platforms.dedup();

    if platforms.len() == 1 {
        format!("{} weapon family", platforms[0])
    } else {
        categories
            .into_iter()
            .next()
            .unwrap_or_else(|| "Other".into())
    }
}

pub(crate) fn weapon_family_faction(entries: &[WeaponReferenceCacheEntry]) -> WeaponBrowseFaction {
    let mut factions = entries
        .iter()
        .filter_map(|entry| entry.faction)
        .map(WeaponBrowseFaction::from_faction)
        .filter(|faction| *faction != WeaponBrowseFaction::Other)
        .collect::<Vec<_>>();
    factions.sort();
    factions.dedup();

    match factions.as_slice() {
        [faction] => *faction,
        [] => WeaponBrowseFaction::Other,
        _ => WeaponBrowseFaction::Other,
    }
}

pub(crate) fn weapon_slot_platform_name(label: &str) -> Option<String> {
    let normalized = label.trim();
    let (platform, suffix) = normalized.split_once(' ')?;
    let suffix = suffix.to_ascii_lowercase();
    let slot_markers = [
        "left",
        "right",
        "rear",
        "front",
        "top",
        "tail",
        "nose",
        "belly",
        "wing",
        "primary",
        "secondary",
        "gunner",
        "turret",
        "bombard",
        "defense",
        "mount",
        "cannon",
        "weapon",
        "weapons",
    ];

    slot_markers
        .iter()
        .any(|marker| suffix.contains(marker))
        .then(|| platform.to_string())
}

pub(crate) fn weapon_browse_group_choices(
    options: &[WeaponLookupOption],
    current: Option<&WeaponLookupOption>,
) -> Vec<WeaponBrowseGroup> {
    let mut groups = vec![WeaponBrowseGroup::All];
    let mut labels = options
        .iter()
        .map(weapon_browse_group_for_option)
        .collect::<Vec<_>>();
    labels.sort();
    labels.dedup();
    groups.extend(labels);

    if let Some(current_option) = current {
        let current_group = weapon_browse_group_for_option(current_option);
        if !groups.contains(&current_group) {
            groups.push(current_group);
        }
    }

    groups
}

pub(crate) fn weapon_browse_category_choices(
    options: &[WeaponLookupOption],
    group: &WeaponBrowseGroup,
    current: Option<&WeaponLookupOption>,
) -> Vec<WeaponBrowseCategory> {
    let mut categories = vec![WeaponBrowseCategory::All];
    let mut labels = options
        .iter()
        .filter(|option| {
            *group == WeaponBrowseGroup::All || weapon_browse_group_for_option(option) == *group
        })
        .map(|option| WeaponBrowseCategory::Category(option.category_label.clone()))
        .collect::<Vec<_>>();
    labels.sort_by(|left, right| {
        left.label()
            .to_lowercase()
            .cmp(&right.label().to_lowercase())
            .then(left.label().cmp(right.label()))
    });
    labels.dedup();
    categories.extend(labels);

    if let Some(current_option) = current {
        let current_category =
            WeaponBrowseCategory::Category(current_option.category_label.clone());
        if !categories.contains(&current_category) {
            categories.push(current_category);
        }
    }

    categories
}

pub(crate) fn weapon_browse_faction_choices(
    options: &[WeaponLookupOption],
    group: &WeaponBrowseGroup,
    category: &WeaponBrowseCategory,
    current: Option<&WeaponLookupOption>,
) -> Vec<WeaponBrowseFaction> {
    let mut factions = vec![WeaponBrowseFaction::All];
    let mut labels = options
        .iter()
        .filter(|option| {
            (*group == WeaponBrowseGroup::All || weapon_browse_group_for_option(option) == *group)
                && (*category == WeaponBrowseCategory::All
                    || option.category_label.eq_ignore_ascii_case(category.label()))
        })
        .map(|option| option.faction)
        .collect::<Vec<_>>();
    labels.sort();
    labels.dedup();
    factions.extend(labels);

    if let Some(current_option) = current
        && !factions.contains(&current_option.faction)
    {
        factions.push(current_option.faction);
    }

    factions
}

pub(crate) fn weapon_filter_choices(
    options: &[WeaponLookupOption],
    current: &WeaponMatchFilter,
    group: &WeaponBrowseGroup,
    category: &WeaponBrowseCategory,
    faction: &WeaponBrowseFaction,
) -> Vec<WeaponFilterChoice> {
    let filtered_options = options
        .iter()
        .filter(|option| {
            (*group == WeaponBrowseGroup::All || weapon_browse_group_for_option(option) == *group)
                && (*category == WeaponBrowseCategory::All
                    || option.category_label.eq_ignore_ascii_case(category.label()))
                && (*faction == WeaponBrowseFaction::All || option.faction == *faction)
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut choices = Vec::with_capacity(filtered_options.len() + 1);
    choices.push(WeaponFilterChoice::Any);
    choices.extend(
        filtered_options
            .into_iter()
            .map(WeaponFilterChoice::Specific),
    );

    if let Some(current_option) = current_weapon_option(options, current)
        && !choices
            .iter()
            .any(|choice| matches!(choice, WeaponFilterChoice::Specific(option) if option == &current_option))
        {
            choices.push(WeaponFilterChoice::Specific(current_option));
        }

    choices
}

pub(crate) fn selected_weapon_choice(
    options: &[WeaponLookupOption],
    current: &WeaponMatchFilter,
) -> Option<WeaponFilterChoice> {
    Some(match current_weapon_option(options, current) {
        Some(option) => WeaponFilterChoice::Specific(option),
        None => WeaponFilterChoice::Any,
    })
}

pub(crate) fn current_weapon_option(
    options: &[WeaponLookupOption],
    current: &WeaponMatchFilter,
) -> Option<WeaponLookupOption> {
    if !current.weapon.ids.is_empty() {
        if let Some(option) = options
            .iter()
            .find(|option| option.ids == current.weapon.ids)
        {
            return Some(option.clone());
        }

        return Some(WeaponLookupOption {
            ids: current.weapon.ids.clone(),
            label: current
                .weapon
                .label
                .clone()
                .unwrap_or_else(|| format!("Weapon group ({})", current.weapon.ids.len())),
            category_label: "Other".into(),
            faction: WeaponBrowseFaction::Other,
        });
    }

    current.legacy_weapon_id.and_then(|weapon_id| {
        options
            .iter()
            .find(|option| option.ids.contains(&weapon_id))
            .cloned()
            .or_else(|| {
                Some(WeaponLookupOption {
                    ids: vec![weapon_id],
                    label: format!("Weapon #{weapon_id}"),
                    category_label: "Other".into(),
                    faction: WeaponBrowseFaction::Other,
                })
            })
    })
}

pub(crate) fn apply_weapon_filter_choice(
    filter: &mut WeaponMatchFilter,
    choice: WeaponFilterChoice,
) {
    match choice {
        WeaponFilterChoice::Any => *filter = WeaponMatchFilter::default(),
        WeaponFilterChoice::Specific(option) => {
            filter.weapon.label = Some(option.label);
            filter.weapon.ids = option.ids;
            filter.weapon.normalize();
            filter.legacy_weapon_id = None;
        }
    }
}

pub(crate) fn weapon_match_filter_mut(
    clause: &mut ScoredEventFilterClause,
) -> Option<&mut WeaponMatchFilter> {
    match clause {
        ScoredEventFilterClause::AttackerWeapon { weapon } => Some(weapon),
        _ => None,
    }
}

pub(crate) fn selected_weapon_browse_category(
    app: &App,
    path: &FilterClausePath,
    current_option: Option<&WeaponLookupOption>,
    group: &WeaponBrowseGroup,
) -> WeaponBrowseCategory {
    let key = WeaponBrowseKey::new(
        &path.rule_id,
        path.event_index,
        path.group_index,
        &path.clause_path,
    );
    let explicit_group = app.rules.weapon_browse_groups.get(&key).copied();

    app.rules
        .weapon_browse_categories
        .get(&key)
        .cloned()
        .filter(|category| {
            *category == WeaponBrowseCategory::All
                || options_have_weapon_category_in_group(
                    &app.rules.weapon_options,
                    group,
                    category.label(),
                )
        })
        .or_else(|| {
            current_option.and_then(|option| {
                let current_category =
                    WeaponBrowseCategory::Category(option.category_label.clone());
                ((*group == WeaponBrowseGroup::All
                    || weapon_browse_group_for_option(option) == *group)
                    && explicit_group.is_none_or(|selected_group| selected_group == *group))
                .then_some(current_category)
            })
        })
        .unwrap_or(WeaponBrowseCategory::All)
}

pub(crate) fn selected_weapon_browse_group(
    app: &App,
    path: &FilterClausePath,
    current_option: Option<&WeaponLookupOption>,
) -> WeaponBrowseGroup {
    app.rules
        .weapon_browse_groups
        .get(&WeaponBrowseKey::new(
            &path.rule_id,
            path.event_index,
            path.group_index,
            &path.clause_path,
        ))
        .copied()
        .or_else(|| current_option.map(weapon_browse_group_for_option))
        .unwrap_or(WeaponBrowseGroup::All)
}

pub(crate) fn selected_weapon_browse_faction(
    app: &App,
    path: &FilterClausePath,
    current_option: Option<&WeaponLookupOption>,
    group: &WeaponBrowseGroup,
    category: &WeaponBrowseCategory,
) -> WeaponBrowseFaction {
    let key = WeaponBrowseKey::new(
        &path.rule_id,
        path.event_index,
        path.group_index,
        &path.clause_path,
    );

    app.rules
        .weapon_browse_factions
        .get(&key)
        .copied()
        .filter(|faction| {
            *faction == WeaponBrowseFaction::All
                || options_have_weapon_faction_in_scope(
                    &app.rules.weapon_options,
                    group,
                    category,
                    faction,
                )
        })
        .or_else(|| {
            current_option.and_then(|option| {
                ((*group == WeaponBrowseGroup::All
                    || weapon_browse_group_for_option(option) == *group)
                    && (*category == WeaponBrowseCategory::All
                        || option.category_label.eq_ignore_ascii_case(category.label())))
                .then_some(option.faction)
            })
        })
        .unwrap_or(WeaponBrowseFaction::All)
}

pub(crate) fn set_weapon_browse_group(
    app: &mut App,
    key: WeaponBrowseKey,
    group: WeaponBrowseGroup,
) {
    if group == WeaponBrowseGroup::All {
        app.rules.weapon_browse_groups.remove(&key);
    } else {
        app.rules.weapon_browse_groups.insert(key, group);
    }
}

pub(crate) fn set_weapon_browse_category(
    app: &mut App,
    key: WeaponBrowseKey,
    category: WeaponBrowseCategory,
) {
    if category == WeaponBrowseCategory::All {
        app.rules.weapon_browse_categories.remove(&key);
    } else {
        app.rules.weapon_browse_categories.insert(key, category);
    }
}

pub(crate) fn set_weapon_browse_faction(
    app: &mut App,
    key: WeaponBrowseKey,
    faction: WeaponBrowseFaction,
) {
    if faction == WeaponBrowseFaction::All {
        app.rules.weapon_browse_factions.remove(&key);
    } else {
        app.rules.weapon_browse_factions.insert(key, faction);
    }
}

pub(crate) fn options_have_weapon_category_in_group(
    options: &[WeaponLookupOption],
    group: &WeaponBrowseGroup,
    category_label: &str,
) -> bool {
    options.iter().any(|option| {
        (*group == WeaponBrowseGroup::All || weapon_browse_group_for_option(option) == *group)
            && option.category_label.eq_ignore_ascii_case(category_label)
    })
}

pub(crate) fn options_have_weapon_faction_in_scope(
    options: &[WeaponLookupOption],
    group: &WeaponBrowseGroup,
    category: &WeaponBrowseCategory,
    faction: &WeaponBrowseFaction,
) -> bool {
    options.iter().any(|option| {
        (*group == WeaponBrowseGroup::All || weapon_browse_group_for_option(option) == *group)
            && (*category == WeaponBrowseCategory::All
                || option.category_label.eq_ignore_ascii_case(category.label()))
            && option.faction == *faction
    })
}

pub(crate) fn weapon_browse_group_for_option(option: &WeaponLookupOption) -> WeaponBrowseGroup {
    weapon_browse_group_for_category_label(&option.category_label)
}

pub(crate) fn weapon_browse_group_for_category_label(label: &str) -> WeaponBrowseGroup {
    let normalized = label.trim().to_ascii_lowercase();

    if matches_any_weapon_category(
        &normalized,
        &[
            "knife",
            "pistol",
            "shotgun",
            "smg",
            "lmg",
            "assault rifle",
            "carbine",
            "sniper rifle",
            "scout rifle",
            "rocket launcher",
            "heavy weapon",
            "battle rifle",
            "crossbow",
            "heavy crossbow",
            "amphibious rifle",
            "amphibious sidearm",
            "anti-materiel rifle",
            "grenade",
            "explosive",
        ],
    ) {
        return WeaponBrowseGroup::Infantry;
    }

    if normalized.contains("max") {
        return WeaponBrowseGroup::Max;
    }

    if contains_any_weapon_category(
        &normalized,
        &[
            "galaxy",
            "liberator",
            "mosquito",
            "reaver",
            "scythe",
            "valkyrie",
            "dervish",
            "aerial combat",
        ],
    ) {
        return WeaponBrowseGroup::AirVehicles;
    }

    if contains_any_weapon_category(&normalized, &["corsair"]) {
        return WeaponBrowseGroup::NavalVehicles;
    }

    if contains_any_weapon_category(&normalized, &["bastion", "colossus"]) {
        return WeaponBrowseGroup::HeavyPlatforms;
    }

    if contains_any_weapon_category(
        &normalized,
        &[
            "flash",
            "harasser",
            "lightning",
            "magrider",
            "prowler",
            "sunderer",
            "vanguard",
            "ant",
            "javelin",
            "chimera",
            "deliverer",
        ],
    ) {
        return WeaponBrowseGroup::GroundVehicles;
    }

    WeaponBrowseGroup::Other
}

pub(crate) fn matches_any_weapon_category(normalized: &str, categories: &[&str]) -> bool {
    categories.contains(&normalized)
}

pub(crate) fn contains_any_weapon_category(normalized: &str, categories: &[&str]) -> bool {
    categories
        .iter()
        .any(|category| normalized.contains(category))
}

pub(crate) fn vehicle_lookup_options(entries: Vec<(i64, String)>) -> Vec<LookupOption> {
    let mut grouped = BTreeMap::<String, LookupOption>::new();
    for (id, label) in entries {
        let normalized = label.trim().to_lowercase();
        let entry = grouped.entry(normalized).or_insert_with(|| LookupOption {
            ids: Vec::new(),
            label: label.clone(),
        });
        entry.ids.push(id as u16);
        entry.ids.sort_unstable();
        entry.ids.dedup();
    }

    grouped.into_values().collect()
}

pub(crate) fn vehicle_browse_category_choices(
    options: &[LookupOption],
    current: Option<&LookupOption>,
) -> Vec<VehicleBrowseCategory> {
    let mut choices = vec![VehicleBrowseCategory::All];
    for category in VehicleBrowseCategory::browse_categories() {
        if options
            .iter()
            .any(|option| vehicle_browse_category_for_option(option) == category)
        {
            choices.push(category);
        }
    }

    if let Some(current_option) = current {
        let current_category = vehicle_browse_category_for_option(current_option);
        if !choices.contains(&current_category) {
            choices.push(current_category);
        }
    }

    choices
}

pub(crate) fn vehicle_filter_choices(
    options: &[LookupOption],
    current: &VehicleMatchFilter,
    category: VehicleBrowseCategory,
) -> Vec<VehicleFilterChoice> {
    let filtered_options = options
        .iter()
        .filter(|option| {
            category == VehicleBrowseCategory::All
                || vehicle_browse_category_for_option(option) == category
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut choices = Vec::with_capacity(filtered_options.len() + 1);
    choices.push(VehicleFilterChoice::Any);
    choices.extend(
        filtered_options
            .into_iter()
            .map(VehicleFilterChoice::Specific),
    );

    if let Some(current_option) = current_vehicle_option(options, current)
        && !choices
            .iter()
            .any(|choice| matches!(choice, VehicleFilterChoice::Specific(option) if option == &current_option))
        {
            choices.push(VehicleFilterChoice::Specific(current_option));
        }

    choices
}

pub(crate) fn selected_vehicle_choice(
    options: &[LookupOption],
    current: &VehicleMatchFilter,
) -> Option<VehicleFilterChoice> {
    Some(match current_vehicle_option(options, current) {
        Some(option) => VehicleFilterChoice::Specific(option),
        None => VehicleFilterChoice::Any,
    })
}

pub(crate) fn current_vehicle_option(
    options: &[LookupOption],
    current: &VehicleMatchFilter,
) -> Option<LookupOption> {
    if !current.vehicle.ids.is_empty() {
        if let Some(option) = options
            .iter()
            .find(|option| option.ids == current.vehicle.ids)
        {
            return Some(option.clone());
        }

        return Some(LookupOption {
            ids: current.vehicle.ids.clone(),
            label: current
                .vehicle
                .label
                .clone()
                .unwrap_or_else(|| format!("Vehicle group ({})", current.vehicle.ids.len())),
        });
    }

    current.legacy_vehicle_id.and_then(|vehicle_id| {
        options
            .iter()
            .find(|option| option.ids.contains(&vehicle_id))
            .cloned()
            .or_else(|| {
                Some(LookupOption {
                    ids: vec![vehicle_id],
                    label: format!("Vehicle #{vehicle_id}"),
                })
            })
    })
}

pub(crate) fn apply_vehicle_filter_choice(
    filter: &mut VehicleMatchFilter,
    choice: VehicleFilterChoice,
) {
    match choice {
        VehicleFilterChoice::Any => *filter = VehicleMatchFilter::default(),
        VehicleFilterChoice::Specific(option) => {
            filter.vehicle.label = Some(option.label);
            filter.vehicle.ids = option.ids;
            filter.vehicle.normalize();
            filter.legacy_vehicle_id = None;
        }
    }
}

pub(crate) fn vehicle_match_filter_mut(
    clause: &mut ScoredEventFilterClause,
) -> Option<&mut VehicleMatchFilter> {
    match clause {
        ScoredEventFilterClause::AttackerVehicle { vehicle }
        | ScoredEventFilterClause::DestroyedVehicle { vehicle } => Some(vehicle),
        _ => None,
    }
}

pub(crate) fn selected_vehicle_browse_category(
    app: &App,
    path: &FilterClausePath,
    current_option: Option<&LookupOption>,
) -> VehicleBrowseCategory {
    app.rules
        .vehicle_browse_categories
        .get(&VehicleBrowseKey::new(
            &path.rule_id,
            path.event_index,
            path.group_index,
            &path.clause_path,
        ))
        .copied()
        .or_else(|| current_option.map(vehicle_browse_category_for_option))
        .unwrap_or(VehicleBrowseCategory::All)
}

pub(crate) fn set_vehicle_browse_category(
    app: &mut App,
    key: VehicleBrowseKey,
    category: VehicleBrowseCategory,
) {
    if category == VehicleBrowseCategory::All {
        app.rules.vehicle_browse_categories.remove(&key);
    } else {
        app.rules.vehicle_browse_categories.insert(key, category);
    }
}

pub(crate) fn vehicle_browse_category_for_option(option: &LookupOption) -> VehicleBrowseCategory {
    let normalized = option.label.trim().to_ascii_lowercase();

    if matches_any_vehicle_name(
        &normalized,
        &[
            "galaxy",
            "liberator",
            "mosquito",
            "reaver",
            "scythe",
            "valkyrie",
            "dervish",
            "bastion",
            "lodestar",
        ],
    ) {
        return VehicleBrowseCategory::Air;
    }

    if matches_any_vehicle_name(
        &normalized,
        &[
            "ant",
            "chimera",
            "colossus",
            "deliverer",
            "flash",
            "harasser",
            "javelin",
            "lightning",
            "magrider",
            "prowler",
            "sunderer",
            "vanguard",
        ],
    ) {
        return VehicleBrowseCategory::Ground;
    }

    if matches_any_vehicle_name(&normalized, &["recon drone"]) {
        return VehicleBrowseCategory::Infantry;
    }

    if normalized.contains("mana") {
        return VehicleBrowseCategory::Infantry;
    }

    if matches_any_vehicle_name(
        &normalized,
        &[
            "spear phalanx turret",
            "aspis phalanx turret",
            "xiphos phalanx turret",
        ],
    ) || normalized.contains("spitfire")
        || normalized.contains("hardlight")
        || normalized.contains("turret")
    {
        return VehicleBrowseCategory::Construction;
    }

    VehicleBrowseCategory::Other
}

fn matches_any_vehicle_name(normalized: &str, names: &[&str]) -> bool {
    names.contains(&normalized)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LookupOption {
    pub ids: Vec<u16>,
    pub label: String,
}

impl std::fmt::Display for LookupOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WeaponLookupOption {
    pub ids: Vec<u32>,
    pub label: String,
    pub category_label: String,
    pub faction: WeaponBrowseFaction,
}

impl std::fmt::Display for WeaponLookupOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum VehicleBrowseCategory {
    All,
    Ground,
    Air,
    Infantry,
    Construction,
    Other,
}

impl VehicleBrowseCategory {
    const fn browse_categories() -> [Self; 5] {
        [
            Self::Ground,
            Self::Air,
            Self::Infantry,
            Self::Construction,
            Self::Other,
        ]
    }
}

impl std::fmt::Display for VehicleBrowseCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All => f.write_str("All vehicle types"),
            Self::Ground => f.write_str("Ground vehicles"),
            Self::Air => f.write_str("Air vehicles"),
            Self::Infantry => f.write_str("Infantry"),
            Self::Construction => f.write_str("Construction"),
            Self::Other => f.write_str("Other"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct VehicleBrowseKey {
    pub rule_id: String,
    pub event_index: usize,
    pub group_index: usize,
    pub clause_path: Vec<usize>,
}

impl VehicleBrowseKey {
    pub(crate) fn new(
        rule_id: &str,
        event_index: usize,
        group_index: usize,
        clause_path: &[usize],
    ) -> Self {
        Self {
            rule_id: rule_id.to_string(),
            event_index,
            group_index,
            clause_path: clause_path.to_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum WeaponBrowseCategory {
    All,
    Category(String),
}

impl WeaponBrowseCategory {
    fn label(&self) -> &str {
        match self {
            Self::All => "All weapon categories",
            Self::Category(label) => label,
        }
    }
}

impl std::fmt::Display for WeaponBrowseCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum WeaponBrowseGroup {
    All,
    Infantry,
    Max,
    GroundVehicles,
    AirVehicles,
    NavalVehicles,
    HeavyPlatforms,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum WeaponBrowseFaction {
    All,
    VS,
    NC,
    TR,
    NS,
    Other,
}

impl std::fmt::Display for WeaponBrowseFaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All => f.write_str("All factions"),
            Self::VS => f.write_str("VS"),
            Self::NC => f.write_str("NC"),
            Self::TR => f.write_str("TR"),
            Self::NS => f.write_str("NS"),
            Self::Other => f.write_str("Other"),
        }
    }
}

impl WeaponBrowseFaction {
    fn from_faction(faction: Faction) -> Self {
        match faction {
            Faction::VS => Self::VS,
            Faction::NC => Self::NC,
            Faction::TR => Self::TR,
            Faction::NS => Self::NS,
            Faction::Unknown => Self::Other,
        }
    }
}

impl std::fmt::Display for WeaponBrowseGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All => f.write_str("All weapon groups"),
            Self::Infantry => f.write_str("Infantry"),
            Self::Max => f.write_str("MAX"),
            Self::GroundVehicles => f.write_str("Ground vehicles"),
            Self::AirVehicles => f.write_str("Air vehicles"),
            Self::NavalVehicles => f.write_str("Naval vehicles"),
            Self::HeavyPlatforms => f.write_str("Heavy platforms"),
            Self::Other => f.write_str("Other"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct WeaponBrowseKey {
    pub rule_id: String,
    pub event_index: usize,
    pub group_index: usize,
    pub clause_path: Vec<usize>,
}

impl WeaponBrowseKey {
    pub(crate) fn new(
        rule_id: &str,
        event_index: usize,
        group_index: usize,
        clause_path: &[usize],
    ) -> Self {
        Self {
            rule_id: rule_id.to_string(),
            event_index,
            group_index,
            clause_path: clause_path.to_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuleDragState {
    ScoredEvent(ScoredEventDragState),
    FilterClause(FilterClauseDragState),
}

impl RuleDragState {
    pub(crate) fn rule_id(&self) -> &str {
        match self {
            Self::ScoredEvent(drag) => &drag.rule_id,
            Self::FilterClause(drag) => &drag.list_path.rule_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScoredEventDragState {
    pub rule_id: String,
    pub source_index: usize,
    pub target_index: usize,
    pub list_len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FilterClausePath {
    pub rule_id: String,
    pub event_index: usize,
    pub group_index: usize,
    pub clause_path: Vec<usize>,
}

impl FilterClausePath {
    pub(crate) fn new(
        rule_id: &str,
        event_index: usize,
        group_index: usize,
        clause_path: Vec<usize>,
    ) -> Self {
        Self {
            rule_id: rule_id.to_string(),
            event_index,
            group_index,
            clause_path,
        }
    }

    pub(crate) fn is_first_in_group(&self) -> bool {
        self.clause_path.first() == Some(&0) && self.clause_path.len() == 1
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FilterClauseListPath {
    pub rule_id: String,
    pub event_index: usize,
    pub group_index: usize,
    pub parent_clause_path: Vec<usize>,
}

impl FilterClauseListPath {
    pub(crate) fn root(rule_id: &str, event_index: usize, group_index: usize) -> Self {
        Self {
            rule_id: rule_id.to_string(),
            event_index,
            group_index,
            parent_clause_path: Vec::new(),
        }
    }

    pub(crate) fn from_clause_path(path: &FilterClausePath) -> Option<Self> {
        let mut parent_clause_path = path.clause_path.clone();
        parent_clause_path.pop()?;
        Some(Self {
            rule_id: path.rule_id.clone(),
            event_index: path.event_index,
            group_index: path.group_index,
            parent_clause_path,
        })
    }

    pub(crate) fn child_clause_path(&self, index: usize) -> FilterClausePath {
        let mut clause_path = self.parent_clause_path.clone();
        clause_path.push(index);
        FilterClausePath::new(
            &self.rule_id,
            self.event_index,
            self.group_index,
            clause_path,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FilterClauseDragState {
    pub source_path: FilterClausePath,
    pub list_path: FilterClauseListPath,
    pub target_index: usize,
    pub list_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FilterTextField {
    TargetCharacter,
    TargetOutfit,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FilterTextDraftKey {
    pub path: FilterClausePath,
    pub field: FilterTextField,
}

impl FilterTextDraftKey {
    pub(crate) fn new(path: FilterClausePath, field: FilterTextField) -> Self {
        Self { path, field }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ResolvedFilterReference {
    Character(CharacterReferenceFilter),
    Outfit(OutfitReferenceFilter),
}

impl ResolvedFilterReference {
    pub(crate) fn display_value(&self) -> &str {
        match self {
            Self::Character(reference) => reference.name.as_deref().unwrap_or_default(),
            Self::Outfit(reference) => reference.tag.as_deref().unwrap_or_default(),
        }
    }

    pub(crate) fn success_message(&self) -> String {
        match self {
            Self::Character(reference) => format!(
                "Resolved {} to character id {}.",
                reference.name.as_deref().unwrap_or("character"),
                reference.character_id.unwrap_or_default()
            ),
            Self::Outfit(reference) => format!(
                "Resolved [{}] to outfit id {}.",
                reference.tag.as_deref().unwrap_or("outfit"),
                reference.outfit_id.unwrap_or_default()
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FilterClauseKindChoice {
    TargetCharacter,
    TargetOutfit,
    AttackerVehicle,
    AttackerWeapon,
    DestroyedVehicle,
    AnyOf,
}

impl FilterClauseKindChoice {
    pub(crate) const ALL: [Self; 6] = [
        Self::TargetCharacter,
        Self::TargetOutfit,
        Self::AttackerVehicle,
        Self::AttackerWeapon,
        Self::DestroyedVehicle,
        Self::AnyOf,
    ];

    pub(crate) const NESTED_ALL: [Self; 6] = Self::ALL;

    pub(crate) fn from_clause(clause: &ScoredEventFilterClause) -> Self {
        match clause {
            ScoredEventFilterClause::TargetCharacter { .. } => Self::TargetCharacter,
            ScoredEventFilterClause::TargetOutfit { .. } => Self::TargetOutfit,
            ScoredEventFilterClause::AttackerVehicle { .. } => Self::AttackerVehicle,
            ScoredEventFilterClause::AttackerWeapon { .. } => Self::AttackerWeapon,
            ScoredEventFilterClause::DestroyedVehicle { .. } => Self::DestroyedVehicle,
            ScoredEventFilterClause::Any { .. } => Self::AnyOf,
        }
    }

    pub(crate) fn default_clause(self) -> ScoredEventFilterClause {
        match self {
            Self::TargetCharacter => ScoredEventFilterClause::TargetCharacter {
                target: CharacterReferenceFilter::default(),
            },
            Self::TargetOutfit => ScoredEventFilterClause::TargetOutfit {
                outfit: OutfitReferenceFilter::default(),
            },
            Self::AttackerVehicle => ScoredEventFilterClause::AttackerVehicle {
                vehicle: VehicleMatchFilter::default(),
            },
            Self::AttackerWeapon => ScoredEventFilterClause::AttackerWeapon {
                weapon: WeaponMatchFilter::default(),
            },
            Self::DestroyedVehicle => ScoredEventFilterClause::DestroyedVehicle {
                vehicle: VehicleMatchFilter::default(),
            },
            Self::AnyOf => ScoredEventFilterClause::Any {
                clauses: vec![Self::TargetCharacter.default_clause()],
            },
        }
    }
}

impl std::fmt::Display for FilterClauseKindChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TargetCharacter => f.write_str("Target character"),
            Self::TargetOutfit => f.write_str("Target outfit"),
            Self::AttackerVehicle => f.write_str("Attacker vehicle"),
            Self::AttackerWeapon => f.write_str("Attacker weapon"),
            Self::DestroyedVehicle => f.write_str("Destroyed vehicle"),
            Self::AnyOf => f.write_str("Any of (OR)"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VehicleFilterChoice {
    Any,
    Specific(LookupOption),
}

impl VehicleFilterChoice {}

impl std::fmt::Display for VehicleFilterChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Any => f.write_str("Any vehicle"),
            Self::Specific(option) => option.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WeaponFilterChoice {
    Any,
    Specific(WeaponLookupOption),
}

impl std::fmt::Display for WeaponFilterChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Any => f.write_str("Any weapon"),
            Self::Specific(option) => option.fmt(f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn import_decision_shake_offset_returns_zero_without_shake_state() {
        assert_eq!(import_decision_shake_offset(None), 0.0);
    }

    #[test]
    fn import_decision_shake_offset_expires_after_animation_window() {
        let started_at = Instant::now() - Duration::from_secs(1);

        assert_eq!(import_decision_shake_offset(Some(started_at)), 0.0);
    }

    #[test]
    fn import_decision_shake_offset_starts_with_visible_nudge() {
        let started_at = Instant::now();

        assert!(import_decision_shake_offset(Some(started_at)).abs() >= 18.0);
    }

    #[test]
    fn vehicle_lookup_options_collapse_same_name_variants() {
        let options = vehicle_lookup_options(vec![
            (5, "Sunderer".into()),
            (7, "Lightning".into()),
            (6, "sunderer".into()),
        ]);

        assert_eq!(
            options,
            vec![
                LookupOption {
                    ids: vec![7],
                    label: "Lightning".into(),
                },
                LookupOption {
                    ids: vec![5, 6],
                    label: "Sunderer".into(),
                },
            ]
        );
    }

    #[test]
    fn vehicle_browse_categories_group_known_vehicle_families() {
        assert_eq!(
            vehicle_browse_category_for_option(&LookupOption {
                ids: vec![1],
                label: "Sunderer".into(),
            }),
            VehicleBrowseCategory::Ground
        );
        assert_eq!(
            vehicle_browse_category_for_option(&LookupOption {
                ids: vec![2],
                label: "Scythe".into(),
            }),
            VehicleBrowseCategory::Air
        );
        assert_eq!(
            vehicle_browse_category_for_option(&LookupOption {
                ids: vec![3],
                label: "MANA Anti-Vehicle Turret".into(),
            }),
            VehicleBrowseCategory::Infantry
        );
        assert_eq!(
            vehicle_browse_category_for_option(&LookupOption {
                ids: vec![4],
                label: "Spear Phalanx Turret".into(),
            }),
            VehicleBrowseCategory::Construction
        );
        assert_eq!(
            vehicle_browse_category_for_option(&LookupOption {
                ids: vec![5],
                label: "Recon Drone".into(),
            }),
            VehicleBrowseCategory::Infantry
        );
    }

    #[test]
    fn vehicle_filter_choices_respect_browse_category() {
        let options = vec![
            LookupOption {
                ids: vec![1],
                label: "Sunderer".into(),
            },
            LookupOption {
                ids: vec![2],
                label: "Scythe".into(),
            },
        ];

        let choices = vehicle_filter_choices(
            &options,
            &VehicleMatchFilter::default(),
            VehicleBrowseCategory::Air,
        );

        assert_eq!(
            choices,
            vec![
                VehicleFilterChoice::Any,
                VehicleFilterChoice::Specific(LookupOption {
                    ids: vec![2],
                    label: "Scythe".into(),
                }),
            ]
        );
    }

    #[test]
    fn vehicle_filter_choices_keep_current_selection_visible_outside_category() {
        let options = vec![
            LookupOption {
                ids: vec![1],
                label: "Sunderer".into(),
            },
            LookupOption {
                ids: vec![2],
                label: "Scythe".into(),
            },
        ];
        let current = VehicleMatchFilter {
            vehicle: crate::rules::VehicleVariantFilter {
                label: Some("Sunderer".into()),
                ids: vec![1],
            },
            legacy_vehicle_id: None,
        };

        let choices = vehicle_filter_choices(&options, &current, VehicleBrowseCategory::Air);

        assert_eq!(
            choices,
            vec![
                VehicleFilterChoice::Any,
                VehicleFilterChoice::Specific(LookupOption {
                    ids: vec![2],
                    label: "Scythe".into(),
                }),
                VehicleFilterChoice::Specific(LookupOption {
                    ids: vec![1],
                    label: "Sunderer".into(),
                }),
            ]
        );
    }

    #[test]
    fn weapon_lookup_options_do_not_merge_same_name_different_weapon_families() {
        let options = weapon_lookup_options(vec![
            WeaponReferenceCacheEntry {
                item_id: 180,
                weapon_id: 80,
                display_name: "Gauss Rifle".into(),
                category_label: "Assault Rifle".into(),
                faction: Some(Faction::NC),
                weapon_group_id: None,
            },
            WeaponReferenceCacheEntry {
                item_id: 1180,
                weapon_id: 1080,
                display_name: "Gauss Rifle".into(),
                category_label: "Assault Rifle".into(),
                faction: Some(Faction::TR),
                weapon_group_id: None,
            },
            WeaponReferenceCacheEntry {
                item_id: 281,
                weapon_id: 81,
                display_name: "Bishop".into(),
                category_label: "Battle Rifle".into(),
                faction: Some(Faction::NS),
                weapon_group_id: None,
            },
        ]);

        assert_eq!(
            options,
            vec![
                WeaponLookupOption {
                    ids: vec![180],
                    label: "Gauss Rifle".into(),
                    category_label: "Assault Rifle".into(),
                    faction: WeaponBrowseFaction::NC,
                },
                WeaponLookupOption {
                    ids: vec![1180],
                    label: "Gauss Rifle".into(),
                    category_label: "Assault Rifle".into(),
                    faction: WeaponBrowseFaction::TR,
                },
                WeaponLookupOption {
                    ids: vec![281],
                    label: "Bishop".into(),
                    category_label: "Battle Rifle".into(),
                    faction: WeaponBrowseFaction::NS,
                },
            ]
        );
    }

    #[test]
    fn weapon_lookup_options_collapse_cross_slot_variants_by_weapon_id() {
        let options = weapon_lookup_options(vec![
            WeaponReferenceCacheEntry {
                item_id: 401,
                weapon_id: 90,
                display_name: "Drake".into(),
                category_label: "Galaxy Left Weapon".into(),
                faction: Some(Faction::TR),
                weapon_group_id: None,
            },
            WeaponReferenceCacheEntry {
                item_id: 402,
                weapon_id: 90,
                display_name: "Drake".into(),
                category_label: "Galaxy Right Weapon".into(),
                faction: Some(Faction::TR),
                weapon_group_id: None,
            },
            WeaponReferenceCacheEntry {
                item_id: 403,
                weapon_id: 90,
                display_name: "Drake".into(),
                category_label: "Galaxy Tail Weapon".into(),
                faction: Some(Faction::TR),
                weapon_group_id: None,
            },
        ]);

        assert_eq!(
            options,
            vec![WeaponLookupOption {
                ids: vec![401, 402, 403],
                label: "Drake".into(),
                category_label: "Galaxy weapon family".into(),
                faction: WeaponBrowseFaction::TR,
            }]
        );
    }

    #[test]
    fn weapon_browse_group_mapping_classifies_major_platforms() {
        assert_eq!(
            weapon_browse_group_for_category_label("Assault Rifle"),
            WeaponBrowseGroup::Infantry
        );
        assert_eq!(
            weapon_browse_group_for_category_label("AV MAX (Left)"),
            WeaponBrowseGroup::Max
        );
        assert_eq!(
            weapon_browse_group_for_category_label("Galaxy Left Weapon"),
            WeaponBrowseGroup::AirVehicles
        );
        assert_eq!(
            weapon_browse_group_for_category_label("Sunderer Rear Gunner"),
            WeaponBrowseGroup::GroundVehicles
        );
        assert_eq!(
            weapon_browse_group_for_category_label("Corsair Front Turret"),
            WeaponBrowseGroup::NavalVehicles
        );
        assert_eq!(
            weapon_browse_group_for_category_label("Bastion Bombard"),
            WeaponBrowseGroup::HeavyPlatforms
        );
    }

    #[test]
    fn weapon_filter_choices_respect_browse_category() {
        let options = vec![
            WeaponLookupOption {
                ids: vec![180],
                label: "Gauss Rifle".into(),
                category_label: "Assault Rifle".into(),
                faction: WeaponBrowseFaction::NC,
            },
            WeaponLookupOption {
                ids: vec![281],
                label: "Bishop".into(),
                category_label: "Battle Rifle".into(),
                faction: WeaponBrowseFaction::NS,
            },
        ];

        let choices = weapon_filter_choices(
            &options,
            &WeaponMatchFilter::default(),
            &WeaponBrowseGroup::Infantry,
            &WeaponBrowseCategory::Category("Battle Rifle".into()),
            &WeaponBrowseFaction::NS,
        );

        assert_eq!(
            choices,
            vec![
                WeaponFilterChoice::Any,
                WeaponFilterChoice::Specific(WeaponLookupOption {
                    ids: vec![281],
                    label: "Bishop".into(),
                    category_label: "Battle Rifle".into(),
                    faction: WeaponBrowseFaction::NS,
                }),
            ]
        );
    }

    #[test]
    fn weapon_filter_choices_include_family_under_synthesized_slot_category() {
        let options = vec![WeaponLookupOption {
            ids: vec![401, 402, 403],
            label: "Drake".into(),
            category_label: "Galaxy weapon family".into(),
            faction: WeaponBrowseFaction::TR,
        }];

        let choices = weapon_filter_choices(
            &options,
            &WeaponMatchFilter::default(),
            &WeaponBrowseGroup::AirVehicles,
            &WeaponBrowseCategory::Category("Galaxy weapon family".into()),
            &WeaponBrowseFaction::TR,
        );

        assert_eq!(
            choices,
            vec![
                WeaponFilterChoice::Any,
                WeaponFilterChoice::Specific(WeaponLookupOption {
                    ids: vec![401, 402, 403],
                    label: "Drake".into(),
                    category_label: "Galaxy weapon family".into(),
                    faction: WeaponBrowseFaction::TR,
                }),
            ]
        );
    }

    #[test]
    fn weapon_filter_choices_keep_current_selection_visible_outside_category() {
        let options = vec![
            WeaponLookupOption {
                ids: vec![180],
                label: "Gauss Rifle".into(),
                category_label: "Assault Rifle".into(),
                faction: WeaponBrowseFaction::NC,
            },
            WeaponLookupOption {
                ids: vec![281],
                label: "Bishop".into(),
                category_label: "Battle Rifle".into(),
                faction: WeaponBrowseFaction::NS,
            },
        ];
        let current = WeaponMatchFilter {
            weapon: crate::rules::WeaponVariantFilter {
                label: Some("Gauss Rifle".into()),
                ids: vec![180],
            },
            legacy_weapon_id: None,
        };

        let choices = weapon_filter_choices(
            &options,
            &current,
            &WeaponBrowseGroup::Infantry,
            &WeaponBrowseCategory::Category("Battle Rifle".into()),
            &WeaponBrowseFaction::All,
        );

        assert_eq!(
            choices,
            vec![
                WeaponFilterChoice::Any,
                WeaponFilterChoice::Specific(WeaponLookupOption {
                    ids: vec![281],
                    label: "Bishop".into(),
                    category_label: "Battle Rifle".into(),
                    faction: WeaponBrowseFaction::NS,
                }),
                WeaponFilterChoice::Specific(WeaponLookupOption {
                    ids: vec![180],
                    label: "Gauss Rifle".into(),
                    category_label: "Assault Rifle".into(),
                    faction: WeaponBrowseFaction::NC,
                }),
            ]
        );
    }

    #[test]
    fn auto_switch_choice_maps_schedule_variants_to_local_schedule() {
        assert_eq!(
            AutoSwitchConditionChoice::from_condition(&AutoSwitchCondition::LocalSchedule {
                weekdays: default_schedule_weekdays(),
                start_hour: 18,
                start_minute: 0,
                end_hour: 23,
                end_minute: 0,
            }),
            AutoSwitchConditionChoice::LocalSchedule
        );
        assert_eq!(
            AutoSwitchConditionChoice::from_condition(&AutoSwitchCondition::LocalTimeRange {
                start_hour: 18,
                end_hour: 23,
            }),
            AutoSwitchConditionChoice::LocalSchedule
        );
    }

    #[test]
    fn auto_switch_choice_maps_character_variants_to_active_character() {
        assert_eq!(
            AutoSwitchConditionChoice::from_condition(&AutoSwitchCondition::ActiveCharacter {
                character_ids: vec![42],
                character_id: None,
            }),
            AutoSwitchConditionChoice::ActiveCharacter
        );
        assert_eq!(
            AutoSwitchConditionChoice::from_condition(&AutoSwitchCondition::OnEvent {
                event: EventKind::FacilityCapture,
            }),
            AutoSwitchConditionChoice::ActiveCharacter
        );
    }

    #[test]
    fn wrap_schedule_slot_supports_schedule_end_time_24_00() {
        assert_eq!(wrap_schedule_slot(48, 1, 48), 0);
        assert_eq!(wrap_schedule_slot(0, -1, 48), 48);
    }

    #[test]
    fn slot_time_round_trips_half_hour_schedule_values() {
        assert_eq!(slot_time(schedule_slot(19, 30)), (19, 30));
        assert_eq!(slot_time(schedule_slot(24, 0)), (24, 0));
    }

    #[test]
    fn reorder_destination_index_matches_drop_slots() {
        assert_eq!(reorder_destination_index(1, 0, 4), Some(0));
        assert_eq!(reorder_destination_index(1, 3, 4), Some(2));
        assert_eq!(reorder_destination_index(1, 4, 4), Some(3));
        assert_eq!(reorder_destination_index(1, 1, 4), None);
        assert_eq!(reorder_destination_index(1, 2, 4), None);
    }

    #[test]
    fn reorder_index_updates_peer_positions() {
        let reordered = (0..4)
            .map(|index| reorder_index(index, 1, 4, 4))
            .collect::<Vec<_>>();
        assert_eq!(reordered, vec![0, 3, 1, 2]);
    }

    #[test]
    fn remap_clause_key_updates_only_matching_clause_lists() {
        let list_path = FilterClauseListPath {
            rule_id: "rule_1".into(),
            event_index: 2,
            group_index: 1,
            parent_clause_path: vec![3],
        };
        let mut rule_id = "rule_1".to_string();
        let mut event_index = 2;
        let mut group_index = 1;
        let mut clause_path = vec![3, 0, 4];

        remap_clause_key(
            &mut rule_id,
            &mut event_index,
            &mut group_index,
            &mut clause_path,
            &list_path,
            0,
            2,
            3,
        );

        assert_eq!(clause_path, vec![3, 1, 4]);

        let mut other_clause_path = vec![4, 0];
        remap_clause_key(
            &mut rule_id,
            &mut event_index,
            &mut group_index,
            &mut other_clause_path,
            &list_path,
            0,
            2,
            3,
        );
        assert_eq!(other_clause_path, vec![4, 0]);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassFilterChoice {
    Any,
    Infiltrator,
    LightAssault,
    Medic,
    Engineer,
    HeavyAssault,
    Max,
}

impl ClassFilterChoice {
    pub(crate) const ALL: [Self; 7] = [
        Self::Any,
        Self::Infiltrator,
        Self::LightAssault,
        Self::Medic,
        Self::Engineer,
        Self::HeavyAssault,
        Self::Max,
    ];

    pub(crate) fn from_option(value: Option<CharacterClass>) -> Self {
        match value {
            None => Self::Any,
            Some(CharacterClass::Infiltrator) => Self::Infiltrator,
            Some(CharacterClass::LightAssault) => Self::LightAssault,
            Some(CharacterClass::Medic) => Self::Medic,
            Some(CharacterClass::Engineer) => Self::Engineer,
            Some(CharacterClass::HeavyAssault) => Self::HeavyAssault,
            Some(CharacterClass::Max) => Self::Max,
        }
    }

    pub(crate) fn into_option(self) -> Option<CharacterClass> {
        match self {
            Self::Any => None,
            Self::Infiltrator => Some(CharacterClass::Infiltrator),
            Self::LightAssault => Some(CharacterClass::LightAssault),
            Self::Medic => Some(CharacterClass::Medic),
            Self::Engineer => Some(CharacterClass::Engineer),
            Self::HeavyAssault => Some(CharacterClass::HeavyAssault),
            Self::Max => Some(CharacterClass::Max),
        }
    }
}

impl std::fmt::Display for ClassFilterChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Any => "Any",
            Self::Infiltrator => "Infiltrator",
            Self::LightAssault => "Light Assault",
            Self::Medic => "Combat Medic",
            Self::Engineer => "Engineer",
            Self::HeavyAssault => "Heavy Assault",
            Self::Max => "MAX",
        };
        f.write_str(label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutoSwitchConditionChoice {
    LocalSchedule,
    ActiveCharacter,
}

impl AutoSwitchConditionChoice {
    pub(crate) const ALL: [Self; 2] = [Self::LocalSchedule, Self::ActiveCharacter];

    pub(crate) fn from_condition(condition: &AutoSwitchCondition) -> Self {
        match condition {
            AutoSwitchCondition::LocalSchedule { .. }
            | AutoSwitchCondition::LocalTimeRange { .. }
            | AutoSwitchCondition::LocalCron { .. } => Self::LocalSchedule,
            AutoSwitchCondition::ActiveCharacter { .. } | AutoSwitchCondition::OnEvent { .. } => {
                Self::ActiveCharacter
            }
        }
    }
}

impl std::fmt::Display for AutoSwitchConditionChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::LocalSchedule => "Local schedule",
            Self::ActiveCharacter => "Active character",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProfileOption {
    pub(crate) id: String,
    pub(crate) name: String,
}

impl ProfileOption {
    pub(crate) fn from_profile(profile: &RuleProfile) -> Self {
        Self {
            id: profile.id.clone(),
            name: profile.name.clone(),
        }
    }
}

impl std::fmt::Display for ProfileOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CharacterOption {
    pub(crate) id: u64,
    pub(crate) name: String,
}

impl std::fmt::Display for CharacterOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}
