use std::{
    collections::{BTreeMap, HashMap},
    time::{SystemTime, UNIX_EPOCH},
};

use glimpse_contracts::{Message, ServiceState, SystemServices, SystemTopics, TopicReport};
use glimpse_ipc::{CallError, ErrorCode, Event, Subscribed, pattern};
use serde_json::Value;

/// Latest value per topic, who owns each topic, and how every service is doing. This is the whole
/// of the daemon's memory: topics are state cells, so a value and a `seq` is all there is to keep.
#[derive(Default)]
pub struct Store {
    cells: HashMap<String, Cell>,
    owners: BTreeMap<String, Option<&'static str>>,
    states: BTreeMap<&'static str, ServiceState>,
}

struct Cell {
    data: Value,
    seq: u64,
    ts: u64,
}

impl Store {
    /// The broker's own topics belong to no service and are therefore never stale.
    pub fn new() -> Self {
        let mut store = Self::default();
        store.owners.insert(SystemTopics::NAME.to_owned(), None);
        store.owners.insert(SystemServices::NAME.to_owned(), None);
        store
    }

    pub fn declare(&mut self, service: &'static str, topics: &'static [&'static str]) {
        self.states.insert(service, ServiceState::Starting);
        for topic in topics {
            self.owners.insert((*topic).to_owned(), Some(service));
        }
    }

    /// `None` when no service declared the topic. Publishing one that was never declared is a bug
    /// in that service's `TOPICS`, not a value to route: nothing could `get` it and `system.topics`
    /// would not list it. Dynamic claims — `tray.item.{id}` — arrive with the tray.
    /// The flag says this was the topic's first value, which is the only thing that changes
    /// `system.topics` — every later value leaves the registry report identical.
    pub fn publish(&mut self, topic: &str, data: Value) -> Option<(Event, bool)> {
        if !self.owners.contains_key(topic) {
            tracing::error!(topic, "a service published a topic it never declared");
            return None;
        }

        let previous = self.cells.get(topic);
        let first = previous.is_none();
        let seq = previous.map_or(1, |cell| cell.seq + 1);
        let ts = now_ms();
        self.cells.insert(topic.to_owned(), Cell { data, seq, ts });

        self.event(topic).map(|event| (event, first))
    }

    pub fn get(&self, topic: &str) -> Result<Option<Event>, CallError> {
        if !self.owners.contains_key(topic) {
            return Err(CallError::new(
                ErrorCode::UnknownTopic,
                format!("no service declares `{topic}`"),
            ));
        }
        Ok(self.event(topic))
    }

    /// `matched` counts declared topics, not valued ones — a subscription to a service that has not
    /// published yet still matched something, and the client needs to be told so.
    pub fn matching(&self, pattern: &str) -> Subscribed {
        let topics: Vec<&String> = self
            .owners
            .keys()
            .filter(|topic| pattern::matches(pattern, topic))
            .collect();

        Subscribed {
            matched: topics.len(),
            snapshot: topics
                .into_iter()
                .filter_map(|topic| self.event(topic))
                .collect(),
        }
    }

    /// Returns the topics whose `stale` flag the transition changed, so the caller can republish
    /// them. A `Running` ↔ `Degraded` move changes nothing about staleness and yields none.
    pub fn set_state(&mut self, service: &'static str, state: ServiceState) -> Vec<String> {
        let was_stale = self.states.get(service).is_some_and(ServiceState::is_stale);
        let now_stale = state.is_stale();
        self.states.insert(service, state);

        if was_stale == now_stale {
            return Vec::new();
        }

        self.owners
            .iter()
            .filter(|(topic, owner)| **owner == Some(service) && self.cells.contains_key(*topic))
            .map(|(topic, _)| topic.clone())
            .collect()
    }

    /// Re-stamps a topic that already has a value, so a `stale` flip reaches clients already
    /// subscribed rather than only those that reconnect.
    pub fn restamp(&mut self, topic: &str) -> Option<Event> {
        {
            let cell = self.cells.get_mut(topic)?;
            cell.seq += 1;
            cell.ts = now_ms();
        }
        self.event(topic)
    }

    pub fn services(&self) -> SystemServices {
        SystemServices {
            services: self
                .states
                .iter()
                .map(|(name, state)| ((*name).to_owned(), state.clone()))
                .collect(),
        }
    }

    pub fn topics(&self) -> SystemTopics {
        SystemTopics {
            topics: self
                .owners
                .iter()
                .map(|(topic, owner)| {
                    (
                        topic.clone(),
                        TopicReport {
                            service: owner.map(str::to_owned),
                            has_value: self.cells.contains_key(topic),
                        },
                    )
                })
                .collect(),
        }
    }

    /// `None` for a topic that has no value yet, which is a different answer from an unknown one.
    /// Indexing here instead would be a panic in the broker, and a panic in the broker takes every
    /// client's connection with it.
    fn event(&self, topic: &str) -> Option<Event> {
        let cell = self.cells.get(topic)?;
        Some(Event {
            topic: topic.to_owned(),
            seq: cell.seq,
            ts: cell.ts,
            stale: self.is_stale(topic),
            data: cell.data.clone(),
        })
    }

    fn is_stale(&self, topic: &str) -> bool {
        self.owners
            .get(topic)
            .and_then(|owner| owner.as_ref())
            .and_then(|service| self.states.get(service))
            .is_some_and(ServiceState::is_stale)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let mut store = Store::new();
        store.declare("audio", &["audio.volume", "audio.mute"]);
        store
    }

    fn value(n: u64) -> Value {
        serde_json::json!({ "n": n })
    }

    #[test]
    fn seq_counts_per_topic_and_ts_is_stamped() {
        let mut store = store();

        let (first, was_first) = store.publish("audio.volume", value(1)).expect("declared");
        assert_eq!(first.seq, 1);
        assert!(was_first);
        assert!(first.ts > 0, "the broker stamps its own publish time");

        let (second, was_first) = store.publish("audio.volume", value(2)).expect("declared");
        assert_eq!(second.seq, 2);
        assert!(!was_first, "only the first value changes system.topics");

        let (other, _) = store.publish("audio.mute", value(1)).expect("declared");
        assert_eq!(other.seq, 1, "seq is per topic, not a delivery counter");
    }

    #[test]
    fn an_undeclared_topic_is_refused_rather_than_routed() {
        let mut store = store();
        assert!(store.publish("audio.balance", value(1)).is_none());
        assert!(store.get("audio.balance").is_err());
    }

    #[test]
    fn get_tells_unknown_apart_from_declared_and_empty() {
        let mut store = store();

        assert!(store.get("nothing.real").is_err(), "unknown topic");
        assert_eq!(
            store.get("audio.volume").expect("declared"),
            None,
            "declared with no value is a different answer"
        );

        store.publish("audio.volume", value(1));
        assert!(store.get("audio.volume").expect("declared").is_some());
    }

    #[test]
    fn degraded_keeps_publishing_and_is_never_stale() {
        let mut store = store();
        store.set_state("audio", ServiceState::Running);
        store.publish("audio.volume", value(1));
        assert!(
            !store
                .get("audio.volume")
                .expect("declared")
                .expect("valued")
                .stale
        );

        let restamp = store.set_state(
            "audio",
            ServiceState::Degraded {
                reason: "no sink".into(),
            },
        );
        assert!(
            restamp.is_empty(),
            "Running to Degraded does not cross the stale boundary, so nothing is republished"
        );
        assert!(
            !store
                .get("audio.volume")
                .expect("declared")
                .expect("valued")
                .stale,
            "a degraded service is still running and its values are current"
        );
    }

    #[test]
    fn leaving_running_marks_the_service_topics_stale() {
        let mut store = store();
        store.set_state("audio", ServiceState::Running);
        store.publish("audio.volume", value(1));

        let restamp = store.set_state("audio", ServiceState::Stopped { reason: None });
        assert_eq!(restamp, ["audio.volume"], "only topics that have a value");

        let before = store
            .get("audio.volume")
            .expect("declared")
            .expect("valued");
        let after = store.restamp("audio.volume").expect("valued");
        assert!(after.stale);
        assert!(
            after.seq > before.seq,
            "a flip is a new value, so it takes a seq"
        );
    }

    #[test]
    fn the_brokers_own_topics_belong_to_nobody_and_never_go_stale() {
        let mut store = store();
        store.publish(SystemServices::NAME, value(1));
        store.set_state("audio", ServiceState::Stopped { reason: None });

        let event = store
            .get(SystemServices::NAME)
            .expect("declared")
            .expect("valued");
        assert!(!event.stale);
    }

    #[test]
    fn matching_counts_declared_topics_and_snapshots_only_valued_ones() {
        let mut store = store();
        store.publish("audio.volume", value(1));

        let found = store.matching("audio.*");
        assert_eq!(found.matched, 2, "both declared topics matched");
        assert_eq!(found.snapshot.len(), 1, "only one of them has a value");
        assert_eq!(found.snapshot[0].topic, "audio.volume");

        assert_eq!(store.matching("nothing.*").matched, 0);
    }

    #[test]
    fn the_registry_reports_ownership_and_presence() {
        let mut store = store();
        store.publish("audio.volume", value(1));

        let topics = store.topics();
        let volume = &topics.topics["audio.volume"];
        assert_eq!(volume.service.as_deref(), Some("audio"));
        assert!(volume.has_value);
        assert!(!topics.topics["audio.mute"].has_value);
        assert_eq!(topics.topics[SystemTopics::NAME].service, None);
    }
}
