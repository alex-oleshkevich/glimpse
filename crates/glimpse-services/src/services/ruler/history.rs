use std::collections::VecDeque;

pub const MAX_LIMIT: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    pub id: u32,
    pub from_x: u32,
    pub from_y: u32,
    pub to_x: u32,
    pub to_y: u32,
    pub dx: i64,
    pub dy: i64,
    pub distance: f64,
    pub angle: f64,
}

pub struct History {
    entries: VecDeque<Measurement>,
    limit: usize,
    next_id: u32,
}

impl History {
    pub fn new(limit: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            limit: limit.clamp(1, MAX_LIMIT),
            next_id: 1,
        }
    }

    pub fn entries(&self) -> Vec<Measurement> {
        self.entries.iter().copied().collect()
    }

    pub fn get(&self, id: u32) -> Option<Measurement> {
        self.entries.iter().find(|entry| entry.id == id).copied()
    }

    /// Assigns each confirmed measurement a fresh id, in the order it arrives, and pushes it to
    /// the front — so after a whole batch the deque reads newest-confirmed-first. No dedup, unlike
    /// `Palette`: every call strictly appends.
    pub fn push_all(&mut self, measurements: Vec<Measurement>) -> Vec<Measurement> {
        for measurement in measurements {
            let id = self.next_id;
            self.next_id = self.next_id.wrapping_add(1).max(1);
            self.entries.push_front(Measurement { id, ..measurement });
        }
        self.entries.truncate(self.limit);
        self.entries()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measurement(distance: f64) -> Measurement {
        Measurement {
            id: 0,
            from_x: 0,
            from_y: 0,
            to_x: 0,
            to_y: 0,
            dx: 0,
            dy: 0,
            distance,
            angle: 0.0,
        }
    }

    fn distances(history: &History) -> Vec<f64> {
        history
            .entries()
            .iter()
            .map(|entry| entry.distance)
            .collect()
    }

    #[test]
    fn a_batch_lands_newest_confirmed_first() {
        let mut history = History::new(8);
        history.push_all(vec![measurement(1.0), measurement(2.0)]);

        assert_eq!(distances(&history), [2.0, 1.0]);
    }

    #[test]
    fn a_batch_beyond_the_limit_drops_the_oldest() {
        let mut history = History::new(2);
        history.push_all(vec![measurement(1.0)]);
        history.push_all(vec![measurement(2.0), measurement(3.0)]);

        assert_eq!(distances(&history), [3.0, 2.0]);
    }

    #[test]
    fn every_entry_gets_a_fresh_id_and_repeats_are_not_deduplicated() {
        let mut history = History::new(8);
        history.push_all(vec![measurement(1.0), measurement(1.0)]);

        let entries = history.entries();
        assert_eq!(entries.len(), 2);
        assert_ne!(entries[0].id, entries[1].id);
    }

    #[test]
    fn an_id_finds_its_entry_and_a_dropped_one_does_not() {
        let mut history = History::new(1);
        history.push_all(vec![measurement(1.0)]);
        let dropped = history.entries()[0].id;
        history.push_all(vec![measurement(2.0)]);
        let kept = history.entries()[0].id;

        assert_eq!(history.get(dropped), None);
        assert_eq!(history.get(kept), Some(history.entries()[0]));
    }

    #[test]
    fn the_limit_is_clamped() {
        assert_eq!(History::new(0).limit, 1);
        assert_eq!(History::new(1000).limit, MAX_LIMIT);
    }
}
