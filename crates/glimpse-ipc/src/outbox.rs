use std::collections::{HashMap, VecDeque};

use tokio_util::bytes::Bytes;

pub const MAX_BUFFERED_BYTES: usize = 1024 * 1024;
pub const MAX_SUBSCRIPTIONS: usize = 64;

/// One client's pending writes and the patterns it subscribed to.
///
/// Events coalesce per topic and responses do not, so a slow reader loses intermediate values —
/// lossless, because every event carries the whole value — while never losing a reply it is
/// blocking on.
pub(crate) struct Outbox {
    patterns: Vec<String>,
    order: VecDeque<Slot>,
    events: HashMap<String, PendingEvent>,
    bytes: usize,
    closed: bool,
}

struct PendingEvent {
    seq: u64,
    frame: Bytes,
}

enum Slot {
    Response(Bytes),
    Event(String),
}

impl Outbox {
    pub(crate) fn new() -> Self {
        Self {
            patterns: Vec::new(),
            order: VecDeque::new(),
            events: HashMap::new(),
            bytes: 0,
            closed: false,
        }
    }

    /// False when the client is already at the subscription cap. Subscribing twice to one pattern
    /// is idempotent rather than a second registration, or the client would receive every value
    /// twice.
    pub(crate) fn add_pattern(&mut self, pattern: &str) -> bool {
        if self.patterns.iter().any(|held| held == pattern) {
            return true;
        }
        if self.patterns.len() >= MAX_SUBSCRIPTIONS {
            return false;
        }
        self.patterns.push(pattern.to_owned());
        true
    }

    pub(crate) fn remove_pattern(&mut self, pattern: &str) {
        self.patterns.retain(|held| held != pattern);
    }

    pub(crate) fn wants(&self, topic: &str) -> bool {
        self.patterns
            .iter()
            .any(|pattern| crate::pattern::matches(pattern, topic))
    }

    pub(crate) fn push_response(&mut self, frame: Bytes) {
        self.bytes += frame.len();
        self.order.push_back(Slot::Response(frame));
        self.check_cap();
    }

    /// A lower `seq` than what is already queued is dropped rather than written. That is what makes
    /// a subscription snapshot and a value published while the subscription was being set up safe
    /// in either order — the newer one wins whichever arrives second.
    pub(crate) fn push_event(&mut self, topic: &str, seq: u64, frame: Bytes) {
        let len = frame.len();
        match self.events.get_mut(topic) {
            Some(pending) if pending.seq >= seq => return,
            Some(pending) => {
                self.bytes -= pending.frame.len();
                *pending = PendingEvent { seq, frame };
            }
            None => {
                self.events
                    .insert(topic.to_owned(), PendingEvent { seq, frame });
                self.order.push_back(Slot::Event(topic.to_owned()));
            }
        }
        self.bytes += len;
        self.check_cap();
    }

    pub(crate) fn pop(&mut self) -> Option<Bytes> {
        loop {
            let frame = match self.order.pop_front()? {
                Slot::Response(frame) => frame,
                // `order` holds exactly one entry per pending topic, so the map always has it.
                // Skipping rather than indexing keeps that invariant from becoming a panic if it
                // ever stops holding.
                Slot::Event(topic) => match self.events.remove(&topic) {
                    Some(pending) => pending.frame,
                    None => continue,
                },
            };
            self.bytes -= frame.len();
            return Some(frame);
        }
    }

    pub(crate) fn close(&mut self) {
        self.closed = true;
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closed
    }

    fn check_cap(&mut self) {
        if self.bytes > MAX_BUFFERED_BYTES {
            tracing::warn!(
                bytes = self.bytes,
                "client is over its buffered-byte cap, disconnecting"
            );
            self.closed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(text: &str) -> Bytes {
        Bytes::from(text.to_owned())
    }

    fn drain(outbox: &mut Outbox) -> Vec<String> {
        let mut written = Vec::new();
        while let Some(frame) = outbox.pop() {
            written.push(String::from_utf8(frame.to_vec()).expect("utf8"));
        }
        written
    }

    #[test]
    fn events_coalesce_per_topic_and_keep_first_appearance_order() {
        let mut outbox = Outbox::new();
        outbox.push_event("audio.volume", 1, frame("v1"));
        outbox.push_event("net.state", 1, frame("n1"));
        outbox.push_event("audio.volume", 2, frame("v2"));

        assert_eq!(drain(&mut outbox), ["v2", "n1"]);
    }

    #[test]
    fn a_value_older_than_the_queued_one_is_dropped() {
        let mut outbox = Outbox::new();
        outbox.push_event("audio.volume", 7, frame("live"));
        outbox.push_event("audio.volume", 3, frame("snapshot"));

        assert_eq!(drain(&mut outbox), ["live"]);
    }

    #[test]
    fn responses_keep_their_order_against_events() {
        let mut outbox = Outbox::new();
        outbox.push_response(frame("ack"));
        outbox.push_event("audio.volume", 1, frame("v1"));
        outbox.push_response(frame("result"));

        assert_eq!(drain(&mut outbox), ["ack", "v1", "result"]);
    }

    #[test]
    fn bytes_balance_across_push_and_pop() {
        let mut outbox = Outbox::new();
        outbox.push_response(frame("abc"));
        outbox.push_event("t", 1, frame("de"));
        outbox.push_event("t", 2, frame("fghi"));
        assert_eq!(outbox.bytes, 3 + 4);

        drain(&mut outbox);
        assert_eq!(outbox.bytes, 0);
    }

    #[test]
    fn exceeding_the_byte_cap_closes_the_client() {
        let mut outbox = Outbox::new();
        assert!(!outbox.is_closed());
        outbox.push_response(Bytes::from(vec![0; MAX_BUFFERED_BYTES + 1]));
        assert!(outbox.is_closed());
    }

    #[test]
    fn patterns_are_capped_and_deduplicated() {
        let mut outbox = Outbox::new();
        for index in 0..MAX_SUBSCRIPTIONS {
            assert!(outbox.add_pattern(&format!("topic.{index}")));
        }
        assert!(!outbox.add_pattern("one.too.many"));
        assert!(outbox.add_pattern("topic.0"), "a repeat is idempotent");
    }

    #[test]
    fn wants_follows_the_registered_patterns() {
        let mut outbox = Outbox::new();
        outbox.add_pattern("audio.*");
        assert!(outbox.wants("audio.volume"));
        assert!(!outbox.wants("net.state"));

        outbox.remove_pattern("audio.*");
        assert!(!outbox.wants("audio.volume"));
    }
}
