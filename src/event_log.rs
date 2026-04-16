use std::collections::VecDeque;

use chrono::{DateTime, Duration, Utc};

use crate::rules::ClassifiedEvent;

#[derive(Debug, Clone)]
pub struct EventLog {
    retention: Duration,
    events: VecDeque<ClassifiedEvent>,
}

impl EventLog {
    pub fn new(retention_secs: u32) -> Self {
        Self {
            retention: Duration::seconds(i64::from(retention_secs.max(1))),
            events: VecDeque::new(),
        }
    }

    pub fn set_retention_secs(&mut self, retention_secs: u32) {
        self.retention = Duration::seconds(i64::from(retention_secs.max(1)));
    }

    pub fn append(&mut self, event: ClassifiedEvent) {
        let timestamp = event.timestamp;
        self.events.push_back(event);
        self.prune(timestamp);
    }

    pub fn prune(&mut self, now: DateTime<Utc>) {
        let cutoff = now - self.retention;
        while self
            .events
            .front()
            .is_some_and(|event| event.timestamp < cutoff)
        {
            self.events.pop_front();
        }
    }

    pub fn query_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<ClassifiedEvent> {
        if end < start {
            return Vec::new();
        }

        self.events
            .iter()
            .filter(|event| event.timestamp >= start && event.timestamp <= end)
            .cloned()
            .collect()
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{ClassifiedEvent, EventKind};
    use chrono::TimeZone;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_700_000_000 + secs, 0).unwrap()
    }

    fn event(kind: EventKind, secs: i64) -> ClassifiedEvent {
        ClassifiedEvent {
            kind,
            timestamp: ts(secs),
            world_id: 17,
            zone_id: Some(2),
            facility_id: Some(1_234),
            actor_character_id: Some(10),
            other_character_id: Some(20),
            other_character_outfit_id: None,
            characters_killed: 1,
            attacker_weapon_id: Some(80),
            attacker_vehicle_id: Some(4),
            vehicle_killed_id: Some(5),
            is_headshot: kind == EventKind::Headshot,
            actor_class: None,
            experience_id: None,
        }
    }

    #[test]
    fn prune_removes_expired_events() {
        let mut log = EventLog::new(10);
        log.append(event(EventKind::Kill, 0));
        log.append(event(EventKind::Headshot, 5));
        log.prune(ts(12));

        let events = log.query_range(ts(-100), ts(100));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::Headshot);
    }

    #[test]
    fn empty_range_query_returns_nothing() {
        let mut log = EventLog::new(10);
        log.append(event(EventKind::Kill, 0));

        assert!(log.query_range(ts(10), ts(5)).is_empty());
        assert!(log.query_range(ts(1), ts(2)).is_empty());
    }

    #[test]
    fn query_range_is_inclusive_on_exact_boundaries() {
        let mut log = EventLog::new(30);
        log.append(event(EventKind::Kill, 0));
        log.append(event(EventKind::Headshot, 10));
        log.append(event(EventKind::Revive, 20));

        let events = log.query_range(ts(10), ts(20));
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, EventKind::Headshot);
        assert_eq!(events[1].kind, EventKind::Revive);
    }
}
