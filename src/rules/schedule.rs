use std::collections::BTreeSet;

use chrono::{DateTime, Datelike, Local, Timelike, Weekday};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleWeekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl ScheduleWeekday {
    pub const ALL: [Self; 7] = [
        Self::Monday,
        Self::Tuesday,
        Self::Wednesday,
        Self::Thursday,
        Self::Friday,
        Self::Saturday,
        Self::Sunday,
    ];

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Monday => "Mon",
            Self::Tuesday => "Tue",
            Self::Wednesday => "Wed",
            Self::Thursday => "Thu",
            Self::Friday => "Fri",
            Self::Saturday => "Sat",
            Self::Sunday => "Sun",
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::Monday => Self::Sunday,
            Self::Tuesday => Self::Monday,
            Self::Wednesday => Self::Tuesday,
            Self::Thursday => Self::Wednesday,
            Self::Friday => Self::Thursday,
            Self::Saturday => Self::Friday,
            Self::Sunday => Self::Saturday,
        }
    }

    pub(crate) fn from_chrono(value: Weekday) -> Self {
        match value {
            Weekday::Mon => Self::Monday,
            Weekday::Tue => Self::Tuesday,
            Weekday::Wed => Self::Wednesday,
            Weekday::Thu => Self::Thursday,
            Weekday::Fri => Self::Friday,
            Weekday::Sat => Self::Saturday,
            Weekday::Sun => Self::Sunday,
        }
    }

    fn from_cron_number(value: u32) -> Option<Self> {
        match value {
            0 | 7 => Some(Self::Sunday),
            1 => Some(Self::Monday),
            2 => Some(Self::Tuesday),
            3 => Some(Self::Wednesday),
            4 => Some(Self::Thursday),
            5 => Some(Self::Friday),
            6 => Some(Self::Saturday),
            _ => None,
        }
    }
}

impl std::fmt::Display for ScheduleWeekday {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.short_label())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalScheduleDefinition {
    pub weekdays: Vec<ScheduleWeekday>,
    pub start_hour: u8,
    pub start_minute: u8,
    pub end_hour: u8,
    pub end_minute: u8,
}

pub fn default_schedule_weekdays() -> Vec<ScheduleWeekday> {
    ScheduleWeekday::ALL.to_vec()
}

pub fn normalize_schedule_weekdays(weekdays: &mut Vec<ScheduleWeekday>) {
    weekdays.sort_unstable();
    weekdays.dedup();
}

pub fn summarize_local_schedule(
    weekdays: &[ScheduleWeekday],
    start_hour: u8,
    start_minute: u8,
    end_hour: u8,
    end_minute: u8,
) -> String {
    let day_summary = if weekdays.is_empty() {
        "No days selected".to_string()
    } else if weekdays.len() == ScheduleWeekday::ALL.len() {
        "Every day".to_string()
    } else {
        weekdays
            .iter()
            .map(|day| day.short_label())
            .collect::<Vec<_>>()
            .join(", ")
    };

    format!(
        "{day_summary} {}-{}",
        format_schedule_time(start_hour, start_minute),
        format_schedule_time(end_hour, end_minute)
    )
}

pub fn local_schedule_matches(
    local: DateTime<Local>,
    weekdays: &[ScheduleWeekday],
    start_hour: u8,
    start_minute: u8,
    end_hour: u8,
    end_minute: u8,
) -> bool {
    let current_day = ScheduleWeekday::from_chrono(local.weekday());
    let current_minutes = (local.hour() * 60 + local.minute()) as u16;
    let start_minutes = schedule_minutes(start_hour, start_minute);
    let end_minutes = schedule_minutes(end_hour, end_minute);

    let day_matches = if start_minutes > end_minutes && current_minutes < end_minutes {
        weekdays.contains(&current_day.previous())
    } else {
        weekdays.contains(&current_day)
    };

    day_matches && local_time_matches(local, start_hour, start_minute, end_hour, end_minute)
}

pub fn legacy_cron_to_local_schedule(expression: &str) -> Result<LocalScheduleDefinition, String> {
    let fields: Vec<_> = expression.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(
            "local cron expression must have exactly 5 fields: minute hour day-of-month month day-of-week"
                .into(),
        );
    }
    if fields[2] != "*" || fields[3] != "*" {
        return Err(
            "legacy local cron migration only supports wildcard day-of-month and month fields"
                .into(),
        );
    }

    let hour_field = ScheduleField::parse(fields[1], ScheduleFieldKind::Hour)?;
    let day_of_week_field = ScheduleField::parse(fields[4], ScheduleFieldKind::DayOfWeek)?;

    let mut weekdays = if day_of_week_field.wildcard {
        default_schedule_weekdays()
    } else {
        day_of_week_field
            .allowed
            .iter()
            .filter_map(|value| ScheduleWeekday::from_cron_number(*value))
            .collect::<Vec<_>>()
    };
    normalize_schedule_weekdays(&mut weekdays);

    let hours = hour_field.allowed.iter().copied().collect::<Vec<_>>();
    let (start_hour, end_hour) = covering_hour_range(&hours)?;

    Ok(LocalScheduleDefinition {
        weekdays,
        start_hour,
        start_minute: 0,
        end_hour,
        end_minute: 0,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronSchedule {
    minute: ScheduleField,
    hour: ScheduleField,
    day_of_month: ScheduleField,
    month: ScheduleField,
    day_of_week: ScheduleField,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScheduleField {
    kind: ScheduleFieldKind,
    wildcard: bool,
    allowed: BTreeSet<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScheduleFieldKind {
    Minute,
    Hour,
    DayOfMonth,
    Month,
    DayOfWeek,
}

impl ScheduleFieldKind {
    fn label(self) -> &'static str {
        match self {
            Self::Minute => "minute",
            Self::Hour => "hour",
            Self::DayOfMonth => "day-of-month",
            Self::Month => "month",
            Self::DayOfWeek => "day-of-week",
        }
    }

    fn min(self) -> u32 {
        match self {
            Self::Minute => 0,
            Self::Hour => 0,
            Self::DayOfMonth => 1,
            Self::Month => 1,
            Self::DayOfWeek => 0,
        }
    }

    fn max(self) -> u32 {
        match self {
            Self::Minute => 59,
            Self::Hour => 23,
            Self::DayOfMonth => 31,
            Self::Month => 12,
            Self::DayOfWeek => 7,
        }
    }

    fn aliases(self) -> &'static [(&'static str, u32)] {
        match self {
            Self::Month => &[
                ("JAN", 1),
                ("FEB", 2),
                ("MAR", 3),
                ("APR", 4),
                ("MAY", 5),
                ("JUN", 6),
                ("JUL", 7),
                ("AUG", 8),
                ("SEP", 9),
                ("OCT", 10),
                ("NOV", 11),
                ("DEC", 12),
            ],
            Self::DayOfWeek => &[
                ("SUN", 0),
                ("MON", 1),
                ("TUE", 2),
                ("WED", 3),
                ("THU", 4),
                ("FRI", 5),
                ("SAT", 6),
            ],
            _ => &[],
        }
    }
}

impl ScheduleField {
    fn parse(input: &str, kind: ScheduleFieldKind) -> Result<Self, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(format!("{} field cannot be empty", kind.label()));
        }

        let mut field = Self {
            kind,
            wildcard: trimmed == "*",
            allowed: BTreeSet::new(),
        };

        if field.wildcard {
            for value in kind.min()..=kind.max() {
                field.allowed.insert(value);
            }
            return Ok(field);
        }

        for segment in trimmed.split(',') {
            field.add_segment(segment.trim())?;
        }

        if field.allowed.is_empty() {
            return Err(format!(
                "{} field must include at least one value",
                kind.label()
            ));
        }

        Ok(field)
    }

    fn add_segment(&mut self, segment: &str) -> Result<(), String> {
        if segment.is_empty() {
            return Err(format!(
                "{} field contains an empty list item",
                self.kind.label()
            ));
        }

        let (base, step) = if let Some((base, step)) = segment.split_once('/') {
            (base.trim(), Some(parse_step(step.trim(), self.kind)?))
        } else {
            (segment, None)
        };

        let values = self.expand_base(base)?;
        let step = step.unwrap_or(1) as usize;
        for (index, value) in values.into_iter().enumerate() {
            if index % step == 0 {
                self.allowed.insert(value);
            }
        }

        Ok(())
    }

    fn expand_base(&self, base: &str) -> Result<Vec<u32>, String> {
        if base == "*" {
            return Ok((self.kind.min()..=self.kind.max()).collect());
        }

        if let Some((start, end)) = base.split_once('-') {
            let start = self.parse_value(start.trim())?;
            let end = self.parse_value(end.trim())?;
            if start > end {
                return Err(format!(
                    "{} range `{base}` must be ascending",
                    self.kind.label()
                ));
            }

            return Ok((start..=end).collect());
        }

        Ok(vec![self.parse_value(base.trim())?])
    }

    fn parse_value(&self, value: &str) -> Result<u32, String> {
        if value.is_empty() {
            return Err(format!(
                "{} field contains an empty value",
                self.kind.label()
            ));
        }

        let upper = value.to_ascii_uppercase();
        if let Some((_, resolved)) = self
            .kind
            .aliases()
            .iter()
            .find(|(alias, _)| *alias == upper)
        {
            return Ok(*resolved);
        }

        let parsed = upper.parse::<u32>().map_err(|_| {
            format!(
                "invalid {} value `{value}` in local cron expression",
                self.kind.label()
            )
        })?;
        if parsed < self.kind.min() || parsed > self.kind.max() {
            return Err(format!(
                "{} value `{value}` must be between {} and {}",
                self.kind.label(),
                self.kind.min(),
                self.kind.max()
            ));
        }

        Ok(parsed)
    }

    fn matches(&self, value: u32) -> bool {
        self.allowed.contains(&value)
    }

    fn matches_weekday(&self, weekday: Weekday) -> bool {
        let numeric = weekday.num_days_from_sunday();
        self.allowed.contains(&numeric) || (numeric == 0 && self.allowed.contains(&7))
    }
}

impl CronSchedule {
    pub fn parse(expression: &str) -> Result<Self, String> {
        let fields: Vec<_> = expression.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(
                "local cron expression must have exactly 5 fields: minute hour day-of-month month day-of-week"
                    .into(),
            );
        }

        Ok(Self {
            minute: ScheduleField::parse(fields[0], ScheduleFieldKind::Minute)?,
            hour: ScheduleField::parse(fields[1], ScheduleFieldKind::Hour)?,
            day_of_month: ScheduleField::parse(fields[2], ScheduleFieldKind::DayOfMonth)?,
            month: ScheduleField::parse(fields[3], ScheduleFieldKind::Month)?,
            day_of_week: ScheduleField::parse(fields[4], ScheduleFieldKind::DayOfWeek)?,
        })
    }

    pub fn matches(&self, local: DateTime<Local>) -> bool {
        if !self.minute.matches(local.minute()) {
            return false;
        }
        if !self.hour.matches(local.hour()) {
            return false;
        }
        if !self.month.matches(local.month()) {
            return false;
        }

        let day_of_month_matches = self.day_of_month.matches(local.day());
        let day_of_week_matches = self.day_of_week.matches_weekday(local.weekday());

        match (self.day_of_month.wildcard, self.day_of_week.wildcard) {
            (true, true) => true,
            (true, false) => day_of_week_matches,
            (false, true) => day_of_month_matches,
            (false, false) => day_of_month_matches || day_of_week_matches,
        }
    }
}

pub fn validate_local_cron_expression(expression: &str) -> Result<(), String> {
    CronSchedule::parse(expression).map(|_| ())
}

pub fn format_schedule_time(hour: u8, minute: u8) -> String {
    format!("{hour:02}:{minute:02}")
}

fn local_time_matches(
    now: DateTime<Local>,
    start_hour: u8,
    start_minute: u8,
    end_hour: u8,
    end_minute: u8,
) -> bool {
    let current_minutes = (now.hour() * 60 + now.minute()) as u16;
    let start_minutes = schedule_minutes(start_hour, start_minute);
    let end_minutes = schedule_minutes(end_hour, end_minute);
    if start_minutes == 0 && end_minutes == 24 * 60 {
        return true;
    }
    if start_minutes == end_minutes {
        return current_slot_start(current_minutes) == start_minutes;
    }
    if start_minutes < end_minutes {
        current_minutes >= start_minutes && current_minutes < end_minutes
    } else {
        current_minutes >= start_minutes || current_minutes < end_minutes
    }
}

fn schedule_minutes(hour: u8, minute: u8) -> u16 {
    u16::from(hour) * 60 + u16::from(minute)
}

fn current_slot_start(total_minutes: u16) -> u16 {
    (total_minutes / 30) * 30
}

fn covering_hour_range(hours: &[u32]) -> Result<(u8, u8), String> {
    if hours.is_empty() {
        return Err("legacy local cron migration requires at least one hour".into());
    }
    let mut sorted = hours.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    if sorted.len() == 24 {
        return Ok((0, 24));
    }
    for &hour in &sorted {
        if hour > 23 {
            return Err(format!("hour value `{hour}` must be between 0 and 23"));
        }
    }

    let mut best_gap_start = 0u32;
    let mut best_gap_len = 0u32;
    for index in 0..sorted.len() {
        let current = sorted[index];
        let next = sorted[(index + 1) % sorted.len()];
        let gap_len = (next + 24 - current - 1) % 24;
        if gap_len > best_gap_len {
            best_gap_len = gap_len;
            best_gap_start = (current + 1) % 24;
        }
    }

    if best_gap_len == 0 {
        let start_hour = sorted[0] as u8;
        return Ok((start_hour, (start_hour + 1) % 24));
    }

    let start_hour = ((best_gap_start + best_gap_len) % 24) as u8;
    let end_hour = best_gap_start as u8;
    Ok((start_hour, end_hour))
}

fn parse_step(raw: &str, kind: ScheduleFieldKind) -> Result<u32, String> {
    let step = raw
        .parse::<u32>()
        .map_err(|_| format!("{} step `{raw}` must be a positive integer", kind.label()))?;
    if step == 0 {
        return Err(format!("{} step must be at least 1", kind.label()));
    }
    Ok(step)
}

#[cfg(test)]
mod tests {
    use super::{
        CronSchedule, ScheduleWeekday, legacy_cron_to_local_schedule, local_schedule_matches,
        validate_local_cron_expression,
    };
    use chrono::{Local, TimeZone};

    fn local_datetime(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
    ) -> chrono::DateTime<Local> {
        Local
            .with_ymd_and_hms(year, month, day, hour, minute, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn cron_matches_weekday_ranges_and_steps() {
        let schedule = CronSchedule::parse("*/15 18-23 * * MON-FRI").unwrap();

        assert!(schedule.matches(local_datetime(2026, 4, 8, 18, 30)));
        assert!(!schedule.matches(local_datetime(2026, 4, 11, 18, 30)));
        assert!(!schedule.matches(local_datetime(2026, 4, 8, 18, 31)));
    }

    #[test]
    fn cron_supports_lists_and_named_months() {
        let schedule = CronSchedule::parse("0 20 1 JAN,MAR,DEC *").unwrap();

        assert!(schedule.matches(local_datetime(2026, 3, 1, 20, 0)));
        assert!(!schedule.matches(local_datetime(2026, 4, 1, 20, 0)));
    }

    #[test]
    fn cron_uses_standard_dom_and_dow_or_matching() {
        let schedule = CronSchedule::parse("0 19 15 * MON").unwrap();

        assert!(schedule.matches(local_datetime(2026, 6, 15, 19, 0)));
        assert!(schedule.matches(local_datetime(2026, 6, 8, 19, 0)));
        assert!(!schedule.matches(local_datetime(2026, 6, 10, 19, 0)));
    }

    #[test]
    fn invalid_cron_requires_five_fields() {
        let error = validate_local_cron_expression("0 19 * *").unwrap_err();

        assert!(error.contains("exactly 5 fields"));
    }

    #[test]
    fn invalid_cron_rejects_zero_step() {
        let error = validate_local_cron_expression("*/0 19 * * *").unwrap_err();

        assert!(error.contains("step must be at least 1"));
    }

    #[test]
    fn local_schedule_matches_selected_weekday_and_time_range() {
        assert!(local_schedule_matches(
            local_datetime(2026, 4, 8, 19, 0),
            &[ScheduleWeekday::Wednesday],
            18,
            0,
            23,
            0,
        ));
        assert!(!local_schedule_matches(
            local_datetime(2026, 4, 9, 19, 0),
            &[ScheduleWeekday::Wednesday],
            18,
            0,
            23,
            0,
        ));
    }

    #[test]
    fn local_schedule_matches_half_hour_windows() {
        assert!(!local_schedule_matches(
            local_datetime(2026, 4, 8, 19, 29),
            &[ScheduleWeekday::Wednesday],
            19,
            30,
            20,
            30,
        ));
        assert!(local_schedule_matches(
            local_datetime(2026, 4, 8, 19, 30),
            &[ScheduleWeekday::Wednesday],
            19,
            30,
            20,
            30,
        ));
        assert!(!local_schedule_matches(
            local_datetime(2026, 4, 8, 20, 30),
            &[ScheduleWeekday::Wednesday],
            19,
            30,
            20,
            30,
        ));
    }

    #[test]
    fn overnight_schedule_matches_after_midnight_for_previous_selected_day() {
        assert!(local_schedule_matches(
            local_datetime(2026, 4, 10, 23, 30),
            &[ScheduleWeekday::Friday],
            23,
            30,
            2,
            0,
        ));
        assert!(local_schedule_matches(
            local_datetime(2026, 4, 11, 1, 30),
            &[ScheduleWeekday::Friday],
            23,
            30,
            2,
            0,
        ));
    }

    #[test]
    fn overnight_schedule_does_not_match_after_midnight_for_same_calendar_day_only() {
        assert!(!local_schedule_matches(
            local_datetime(2026, 4, 11, 1, 30),
            &[ScheduleWeekday::Saturday],
            23,
            30,
            2,
            0,
        ));
    }

    #[test]
    fn legacy_cron_migration_maps_weekdays_and_hour_to_schedule() {
        let schedule = legacy_cron_to_local_schedule("0 19 * * fri,sat").unwrap();

        assert_eq!(
            schedule.weekdays,
            vec![ScheduleWeekday::Friday, ScheduleWeekday::Saturday]
        );
        assert_eq!(schedule.start_hour, 19);
        assert_eq!(schedule.start_minute, 0);
        assert_eq!(schedule.end_hour, 20);
        assert_eq!(schedule.end_minute, 0);
    }
}
