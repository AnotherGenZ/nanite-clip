use super::*;

pub(super) fn clips_badge(label: impl Into<String>, tone: BadgeTone) -> Element<'static, Message> {
    badge(label.into()).tone(tone).build().into()
}

pub(super) fn active_filter_count(app: &App) -> usize {
    let filters = &app.clips.filters;

    usize::from(!filters.search.trim().is_empty())
        + usize::from(filters.event_after_ts.is_some() || filters.event_before_ts.is_some())
        + usize::from(!filters.target.trim().is_empty())
        + usize::from(!filters.weapon.trim().is_empty())
        + usize::from(!filters.alert.trim().is_empty())
        + usize::from(!filters.tag.trim().is_empty())
        + usize::from(filters.favorites_only)
        + usize::from(filters.collection_id.is_some())
        + usize::from(filters.overlap_state != OverlapFilterState::All)
        + usize::from(!filters.profile.trim().is_empty())
        + usize::from(!filters.rule.trim().is_empty())
        + usize::from(!filters.character.trim().is_empty())
        + usize::from(!filters.server.trim().is_empty())
        + usize::from(!filters.continent.trim().is_empty())
        + usize::from(!filters.base.trim().is_empty())
}

pub(super) fn total_pages(app: &App) -> usize {
    let len = app.clips.history.len();
    let size = app.clips.history_page_size.max(1);
    if len == 0 { 1 } else { len.div_ceil(size) }
}

pub(in crate::app) fn rebuild_history(app: &mut App) {
    let mut filtered: Vec<ClipRecord> = app
        .clips
        .history_source
        .iter()
        .filter(|record| clip_matches_filters(app, record))
        .cloned()
        .collect();

    sort_clip_history(
        &mut filtered,
        app.clips.sort_column,
        app.clips.sort_descending,
        &app.clips.filters,
        &app.config,
        &app.runtime.lifecycle,
    );

    app.clips.history = filtered;
    app.clips.history_viewport = None;

    // Clamp page to valid range after filtering.
    let pages = total_pages(app);
    if app.clips.history_page >= pages {
        app.clips.history_page = pages.saturating_sub(1);
    }
}

fn sort_clip_history(
    records: &mut [ClipRecord],
    column: ClipSortColumn,
    descending: bool,
    filters: &crate::db::ClipFilters,
    config: &crate::config::Config,
    state: &AppState,
) {
    records.sort_by(|a, b| {
        if filters.collection_id.is_some() {
            return a
                .collection_sequence_index
                .cmp(&b.collection_sequence_index)
                .then(a.trigger_event_at.cmp(&b.trigger_event_at))
                .then(a.id.cmp(&b.id));
        }

        let favorite_order = b.favorited.cmp(&a.favorited);
        if favorite_order != std::cmp::Ordering::Equal {
            return favorite_order;
        }

        let order = match column {
            ClipSortColumn::When => a.trigger_event_at.cmp(&b.trigger_event_at),
            ClipSortColumn::Rule => rule_label_from(config, a).cmp(&rule_label_from(config, b)),
            ClipSortColumn::Character => {
                character_label_from(config, state, a).cmp(&character_label_from(config, state, b))
            }
            ClipSortColumn::Score => a.score.cmp(&b.score),
            ClipSortColumn::Duration => a.clip_duration_secs.cmp(&b.clip_duration_secs),
        };
        if descending { order.reverse() } else { order }
    });
}

pub(super) fn location_summary(record: &ClipRecord) -> String {
    record
        .facility_name
        .clone()
        .or_else(|| census::base_name(record.facility_id))
        .unwrap_or_else(|| "Unknown".into())
}

pub(super) fn build_filter_options(app: &App) -> ClipFilterOptions {
    let mut profiles = std::collections::BTreeSet::new();
    let mut rules = std::collections::BTreeSet::new();
    let mut characters = std::collections::BTreeSet::new();
    let mut servers = std::collections::BTreeSet::new();
    let mut continents = std::collections::BTreeSet::new();
    let mut bases = std::collections::BTreeSet::new();
    let mut targets = std::collections::BTreeSet::new();
    let mut weapons = std::collections::BTreeSet::new();
    let mut alerts = std::collections::BTreeSet::new();
    let mut tags = std::collections::BTreeSet::new();

    for profile in &app.config.rule_profiles {
        profiles.insert(profile.name.clone());
    }
    for rule in &app.config.rule_definitions {
        rules.insert(rule.name.clone());
    }
    for character in &app.config.characters {
        characters.insert(character.name.clone());
    }

    for record in &app.clips.history_source {
        profiles.insert(profile_label(app, record));
        rules.insert(rule_label(app, record));
        characters.insert(character_label(app, record));
        servers.insert(server_label(record));
        continents.insert(continent_label(record));
        let base = location_summary(record);
        if base != "Unknown" {
            bases.insert(base);
        }
    }
    for target in &app.clips.filter_options.targets {
        targets.insert(target.clone());
    }
    for weapon in &app.clips.filter_options.weapons {
        weapons.insert(weapon.clone());
    }
    for alert in &app.clips.filter_options.alerts {
        alerts.insert(alert.clone());
    }
    for tag in &app.clips.filter_options.tags {
        tags.insert(tag.clone());
    }

    ClipFilterOptions {
        profiles: profiles
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect(),
        rules: rules
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect(),
        characters: characters
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect(),
        servers: servers
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect(),
        continents: continents
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect(),
        bases: bases.into_iter().collect(),
        targets: targets
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect(),
        weapons: weapons
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect(),
        alerts: alerts
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect(),
        tags: tags.into_iter().filter(|value| !value.is_empty()).collect(),
        collections: app.clips.filter_options.collections.clone(),
    }
}

fn clip_matches_filters(app: &App, record: &ClipRecord) -> bool {
    let profile = profile_label(app, record);
    let rule = rule_label(app, record);
    let character = character_label(app, record);
    let server = server_label(record);
    let continent = continent_label(record);
    let location = location_summary(record);

    exact_filter_matches(&app.clips.filters.profile, &profile)
        && exact_filter_matches(&app.clips.filters.rule, &rule)
        && exact_filter_matches(&app.clips.filters.character, &character)
        && exact_filter_matches(&app.clips.filters.server, &server)
        && exact_filter_matches(&app.clips.filters.continent, &continent)
        && exact_filter_matches(&app.clips.filters.base, &location)
        && tag_filter_matches(&app.clips.filters.tag, &record.tags)
        && quick_search_matches(
            &app.clips.filters.search,
            record,
            &profile,
            &rule,
            &character,
            &server,
            &continent,
            &location,
        )
}

fn exact_filter_matches(filter: &str, value: &str) -> bool {
    let filter = normalize(filter);
    filter.is_empty() || filter == normalize(value)
}

fn tag_filter_matches(filter: &str, tags: &[String]) -> bool {
    let filter = normalize(filter);
    filter.is_empty() || tags.iter().any(|tag| normalize(tag).contains(&filter))
}

#[allow(clippy::too_many_arguments)]
fn quick_search_matches(
    search: &str,
    record: &ClipRecord,
    profile: &str,
    rule: &str,
    character: &str,
    server: &str,
    continent: &str,
    location: &str,
) -> bool {
    let search = normalize(search);
    if search.is_empty() {
        return true;
    }

    let haystack = [
        profile.to_string(),
        rule.to_string(),
        character.to_string(),
        server.to_string(),
        continent.to_string(),
        location.to_string(),
        contribution_summary(record),
        record.profile_id.clone(),
        record.rule_id.clone(),
        record.character_id.to_string(),
        record.world_id.to_string(),
        record.zone_id.map(|id| id.to_string()).unwrap_or_default(),
        record
            .facility_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
        record.tags.join(" "),
    ]
    .join(" ");

    normalize(&haystack).contains(&search)
}

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

fn profile_label(app: &App, record: &ClipRecord) -> String {
    profile_label_from(&app.config, record)
}

fn profile_label_from(config: &crate::config::Config, record: &ClipRecord) -> String {
    config
        .rule_profiles
        .iter()
        .find(|profile| profile.id == record.profile_id)
        .map(|profile| profile.name.clone())
        .unwrap_or_else(|| record.profile_id.clone())
}

pub(super) fn rule_label(app: &App, record: &ClipRecord) -> String {
    rule_label_from(&app.config, record)
}

fn rule_label_from(config: &crate::config::Config, record: &ClipRecord) -> String {
    if record.origin == crate::db::ClipOrigin::Manual {
        return "Manual clip".into();
    }
    if record.origin == crate::db::ClipOrigin::Imported {
        return "Imported clip".into();
    }

    config
        .rule_definitions
        .iter()
        .find(|rule| rule.id == record.rule_id)
        .map(|rule| rule.name.clone())
        .unwrap_or_else(|| record.rule_id.clone())
}

pub(super) fn character_label(app: &App, record: &ClipRecord) -> String {
    character_label_from(&app.config, &app.runtime.lifecycle, record)
}

fn character_label_from(
    config: &crate::config::Config,
    state: &AppState,
    record: &ClipRecord,
) -> String {
    if record.character_id == 0 {
        return "Unassigned".into();
    }

    config
        .characters
        .iter()
        .find(|character| character.character_id == Some(record.character_id))
        .map(|character| character.name.clone())
        .or_else(|| match state {
            AppState::Monitoring {
                character_name,
                character_id,
            } if *character_id == record.character_id => Some(character_name.clone()),
            _ => None,
        })
        .unwrap_or_else(|| format!("Character {}", record.character_id))
}

pub(super) fn server_label(record: &ClipRecord) -> String {
    if record.world_id == 0 {
        return "Unknown".into();
    }
    census::world_name(record.world_id)
}

pub(super) fn continent_label(record: &ClipRecord) -> String {
    record
        .zone_name
        .clone()
        .unwrap_or_else(|| census::continent_name(record.zone_id))
}

pub(super) fn duration_label(record: &ClipRecord) -> String {
    format!("{} seconds", record.clip_duration_secs)
}

pub(super) fn overlap_label(record: &ClipRecord) -> String {
    match record.overlap_count {
        0 => "None".into(),
        1 => "1 clip".into(),
        count => format!("{count} clips"),
    }
}

pub(super) fn alert_label(record: &ClipRecord) -> String {
    match record.alert_count {
        0 => "None".into(),
        1 => "1 alert".into(),
        count => format!("{count} alerts"),
    }
}

pub(super) fn size_label(record: &ClipRecord) -> String {
    record
        .file_size_bytes
        .map(format_file_size)
        .unwrap_or_else(|| "\u{2013}".into())
}

pub(super) fn post_process_status_label(record: &ClipRecord) -> String {
    match record.post_process_status {
        crate::db::PostProcessStatus::NotRequired => "Audio: not required".into(),
        crate::db::PostProcessStatus::Pending => "Audio: pending".into(),
        crate::db::PostProcessStatus::Completed => "Audio: ok".into(),
        crate::db::PostProcessStatus::Failed => record
            .post_process_error
            .as_ref()
            .filter(|message| !message.trim().is_empty())
            .map(|message| format!("Audio failed | {message}"))
            .unwrap_or_else(|| "Audio failed".into()),
        crate::db::PostProcessStatus::Legacy => "Audio: legacy layout".into(),
    }
}

fn contribution_summary(record: &ClipRecord) -> String {
    if record.events.is_empty() {
        return "No contributions".into();
    }

    record
        .events
        .iter()
        .map(|event| {
            format!(
                "{} x{} = {}",
                event.event_kind, event.occurrences, event.points
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn short_duration_label(record: &ClipRecord) -> String {
    format!("{}s", record.clip_duration_secs)
}

pub(super) fn format_smart_timestamp(timestamp: chrono::DateTime<Utc>) -> String {
    let local = timestamp.with_timezone(&Local);
    let now = Local::now();
    if local.year() == now.year() {
        local.format("%m-%d %H:%M").to_string()
    } else {
        local.format("%Y-%m-%d %H:%M").to_string()
    }
}

pub(super) fn timeline_line(record: &ClipRecord, event: &crate::db::ClipRawEventRecord) -> String {
    let offset = event
        .event_at
        .signed_duration_since(record.clip_start_at)
        .num_milliseconds()
        .max(0);
    let seconds = offset as f64 / 1000.0;
    let target = event
        .other_character_name
        .clone()
        .unwrap_or_else(|| format_optional_id("Character", event.other_character_id));
    let weapon = event
        .attacker_weapon_name
        .clone()
        .unwrap_or_else(|| format_optional_id("Weapon", event.attacker_weapon_id.map(u64::from)));

    let mut parts = vec![format!("+{seconds:.1}s"), event.event_kind.clone()];
    if event.is_headshot {
        parts.push("headshot".into());
    }
    if event.other_character_id.is_some() {
        parts.push(format!("target {target}"));
    }
    if event.attacker_weapon_id.is_some() {
        parts.push(format!("weapon {weapon}"));
    }
    parts.join("  |  ")
}

fn format_optional_id(prefix: &str, value: Option<u64>) -> String {
    value
        .map(|id| format!("{prefix} #{id}"))
        .unwrap_or_else(|| "\u{2013}".into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlapFilterChoice {
    All,
    Overlapping,
    UniqueOnly,
}

impl OverlapFilterChoice {
    pub(super) const ALL: [Self; 3] = [Self::All, Self::Overlapping, Self::UniqueOnly];

    pub(super) fn from_state(state: OverlapFilterState) -> Self {
        match state {
            OverlapFilterState::All => Self::All,
            OverlapFilterState::Overlapping => Self::Overlapping,
            OverlapFilterState::UniqueOnly => Self::UniqueOnly,
        }
    }

    pub(super) fn into_state(self) -> OverlapFilterState {
        match self {
            Self::All => OverlapFilterState::All,
            Self::Overlapping => OverlapFilterState::Overlapping,
            Self::UniqueOnly => OverlapFilterState::UniqueOnly,
        }
    }
}

impl std::fmt::Display for OverlapFilterChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::All => "All clips",
            Self::Overlapping => "Only overlaps",
            Self::UniqueOnly => "Only unique",
        })
    }
}

pub(super) fn filter_pick_list_options(all_label: &str, values: &[String]) -> Vec<String> {
    let mut options = Vec::with_capacity(values.len() + 1);
    options.push(all_label.to_string());
    options.extend(values.iter().cloned());
    options
}

pub(super) fn selected_filter_option(current: &str, all_label: &str) -> String {
    if current.trim().is_empty() {
        all_label.to_string()
    } else {
        current.to_string()
    }
}

pub(super) fn filter_value_from_selection(selection: String, all_label: &str) -> String {
    if selection == all_label {
        String::new()
    } else {
        selection
    }
}

pub(in crate::app) fn format_timestamp(timestamp: chrono::DateTime<Utc>) -> String {
    timestamp
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

pub(super) fn format_file_size(size_bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

    let mut value = size_bytes as f64;
    let mut unit_index = 0usize;
    while value >= 1024.0 && unit_index < UNITS.len() - 1 {
        value /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{size_bytes} {}", UNITS[unit_index])
    } else {
        format!("{value:.1} {}", UNITS[unit_index])
    }
}

pub(super) fn format_duration_ms(duration_ms: i64) -> String {
    if duration_ms <= 0 {
        return "0.0s".into();
    }
    format!("{:.1}s", duration_ms as f64 / 1000.0)
}

// ---------------------------------------------------------------------------
// Date parsing helpers
// ---------------------------------------------------------------------------

pub(super) fn date_range_summary(app: &App) -> String {
    match app.clips.date_range_preset {
        DateRangePreset::AllTime => "All saved clips".into(),
        DateRangePreset::Custom => {
            let start = if app.clips.date_range_start.trim().is_empty() {
                "any start"
            } else {
                app.clips.date_range_start.trim()
            };
            let end = if app.clips.date_range_end.trim().is_empty() {
                "any end"
            } else {
                app.clips.date_range_end.trim()
            };
            format!("Custom: {start} \u{2192} {end}")
        }
        preset => format!("{preset}"),
    }
}

pub(super) fn parse_range_input(
    value: &str,
    is_end: bool,
) -> Result<Option<chrono::DateTime<Utc>>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    if let Ok(datetime) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Ok(Some(datetime.with_timezone(&Utc)));
    }

    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, format) {
            return local_naive_to_utc(naive).map(Some);
        }
    }

    if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        let naive = if is_end {
            date.and_hms_milli_opt(23, 59, 59, 999)
        } else {
            date.and_hms_opt(0, 0, 0)
        }
        .ok_or_else(|| format!("Invalid date value: {trimmed}"))?;

        return local_naive_to_utc(naive).map(Some);
    }

    Err(format!(
        "Invalid date/time `{trimmed}`. Use YYYY-MM-DD, YYYY-MM-DD HH:MM, YYYY-MM-DD HH:MM:SS, or RFC3339."
    ))
}

pub(super) fn calendar_seed_date(value: &str) -> Option<NaiveDate> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(datetime) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Some(datetime.with_timezone(&Local).date_naive());
    }

    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, format) {
            return Some(naive.date());
        }
    }

    NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").ok()
}

pub(in crate::app) fn today_local_date() -> NaiveDate {
    Local::now()
        .date_naive()
        .with_day(1)
        .unwrap_or_else(|| Local::now().date_naive())
}

pub(super) fn merge_calendar_date(existing: &str, date: NaiveDate) -> String {
    let trimmed = existing.trim();
    let date_text = date.format("%Y-%m-%d").to_string();

    if trimmed.is_empty() {
        return date_text;
    }

    if let Ok(datetime) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        let time = datetime.time();
        let offset = datetime.offset().to_string();
        return format!(
            "{}T{:02}:{:02}:{:02}{}",
            date_text,
            time.hour(),
            time.minute(),
            time.second(),
            offset
        );
    }

    if let Some((_, time)) = trimmed.split_once(' ') {
        return format!("{date_text} {}", time.trim());
    }

    date_text
}

pub(super) fn local_day_start(date: NaiveDate) -> Option<chrono::DateTime<Utc>> {
    local_naive_to_utc(date.and_hms_opt(0, 0, 0)?).ok()
}

fn local_naive_to_utc(naive: NaiveDateTime) -> Result<chrono::DateTime<Utc>, String> {
    Local
        .from_local_datetime(&naive)
        .single()
        .or_else(|| Local.from_local_datetime(&naive).earliest())
        .map(|value| value.with_timezone(&Utc))
        .ok_or_else(|| format!("Local date/time `{}` is ambiguous or invalid.", naive))
}

// ---------------------------------------------------------------------------
// Keyboard subscription router
// ---------------------------------------------------------------------------

pub(in crate::app) fn subscription_event_handler(
    event: iced::Event,
    status: iced::event::Status,
) -> Option<Message> {
    let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event else {
        return None;
    };

    use iced::keyboard::Key;
    use iced::keyboard::key::Named;

    let captured = matches!(status, iced::event::Status::Captured);

    let action = match (&key, captured) {
        (Key::Named(Named::ArrowDown), _) => KeyNav::SelectNext,
        (Key::Named(Named::ArrowUp), _) => KeyNav::SelectPrevious,
        (Key::Named(Named::PageDown), false) => KeyNav::NextPage,
        (Key::Named(Named::PageUp), false) => KeyNav::PreviousPage,
        (Key::Named(Named::Home), false) => KeyNav::First,
        (Key::Named(Named::End), false) => KeyNav::Last,
        (Key::Named(Named::Enter), true) => KeyNav::SubmitActiveOrganizationInput,
        (Key::Named(Named::Enter), false) => KeyNav::OpenSelected,
        (Key::Named(Named::Delete), false) => KeyNav::DeleteSelected,
        (Key::Named(Named::Escape), _) => KeyNav::Escape,
        (Key::Character(c), false) if c.as_str() == " " => KeyNav::ToggleMontageSelected,
        _ => return None,
    };

    Some(Message::KeyNav(action))
}

#[cfg(test)]
mod tests {
    use super::{HistoryViewportState, history_page_row_is_visible, history_scroll_ratio};

    #[test]
    fn history_scroll_ratio_returns_none_for_empty_pages() {
        assert_eq!(history_scroll_ratio(0, 0), None);
    }

    #[test]
    fn history_scroll_ratio_keeps_single_row_pages_at_top() {
        assert_eq!(history_scroll_ratio(0, 1), Some(0.0));
    }

    #[test]
    fn history_scroll_ratio_maps_first_middle_and_last_rows() {
        assert_eq!(history_scroll_ratio(0, 5), Some(0.0));
        assert_eq!(history_scroll_ratio(2, 5), Some(0.5));
        assert_eq!(history_scroll_ratio(4, 5), Some(1.0));
    }

    #[test]
    fn history_scroll_ratio_clamps_rows_past_the_end() {
        assert_eq!(history_scroll_ratio(9, 5), Some(1.0));
    }

    #[test]
    fn visible_history_row_does_not_require_scroll() {
        let viewport = HistoryViewportState {
            offset_y: 64.0,
            viewport_width: 960.0,
            viewport_height: 96.0,
            content_height: 238.0,
        };

        assert!(history_page_row_is_visible(viewport, 2, 7));
    }

    #[test]
    fn row_above_viewport_requires_scroll() {
        let viewport = HistoryViewportState {
            offset_y: 96.0,
            viewport_width: 960.0,
            viewport_height: 96.0,
            content_height: 238.0,
        };

        assert!(!history_page_row_is_visible(viewport, 1, 7));
    }

    #[test]
    fn row_below_viewport_requires_scroll() {
        let viewport = HistoryViewportState {
            offset_y: 0.0,
            viewport_width: 960.0,
            viewport_height: 96.0,
            content_height: 238.0,
        };

        assert!(!history_page_row_is_visible(viewport, 4, 7));
    }

    #[test]
    fn non_scrollable_history_always_keeps_rows_visible() {
        let viewport = HistoryViewportState {
            offset_y: 0.0,
            viewport_width: 960.0,
            viewport_height: 220.0,
            content_height: 180.0,
        };

        assert!(history_page_row_is_visible(viewport, 3, 5));
    }
}
