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
mod tests;
