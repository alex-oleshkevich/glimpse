use std::collections::VecDeque;

use chrono::{DateTime, Utc};

pub const MAX_LIMIT: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PickedColor {
    pub id: u32,
    pub rgb: [u8; 3],
    pub picked_at: DateTime<Utc>,
}

pub struct Palette {
    colors: VecDeque<PickedColor>,
    limit: usize,
    next_id: u32,
}

impl Palette {
    pub fn new(limit: usize) -> Self {
        Self {
            colors: VecDeque::new(),
            limit: limit.clamp(1, MAX_LIMIT),
            next_id: 1,
        }
    }

    pub fn colors(&self) -> Vec<PickedColor> {
        self.colors.iter().copied().collect()
    }

    pub fn get(&self, id: u32) -> Option<PickedColor> {
        self.colors.iter().find(|color| color.id == id).copied()
    }

    pub fn push(&mut self, rgb: [u8; 3], picked_at: DateTime<Utc>) -> PickedColor {
        let id = match self.colors.iter().position(|color| color.rgb == rgb) {
            Some(index) => self.colors.remove(index).map_or(0, |color| color.id),
            None => {
                let id = self.next_id;
                self.next_id = self.next_id.wrapping_add(1).max(1);
                id
            }
        };
        let color = PickedColor { id, rgb, picked_at };
        self.colors.push_front(color);
        self.colors.truncate(self.limit);
        color
    }

    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit.clamp(1, MAX_LIMIT);
        self.colors.truncate(self.limit);
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at(second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 23, 15, 0, second)
            .single()
            .unwrap()
    }

    fn rgbs(palette: &Palette) -> Vec<[u8; 3]> {
        palette.colors().iter().map(|color| color.rgb).collect()
    }

    #[test]
    fn the_newest_pick_comes_first() {
        let mut palette = Palette::new(8);
        palette.push([1, 1, 1], at(1));
        palette.push([2, 2, 2], at(2));

        assert_eq!(rgbs(&palette), [[2, 2, 2], [1, 1, 1]]);
    }

    #[test]
    fn picking_a_color_again_moves_it_to_the_front_and_keeps_its_id() {
        let mut palette = Palette::new(8);
        let first = palette.push([1, 1, 1], at(1));
        palette.push([2, 2, 2], at(2));
        let again = palette.push([1, 1, 1], at(3));

        assert_eq!(rgbs(&palette), [[1, 1, 1], [2, 2, 2]]);
        assert_eq!(again.id, first.id);
        assert_eq!(again.picked_at, at(3));
    }

    #[test]
    fn a_pick_beyond_the_limit_drops_the_oldest() {
        let mut palette = Palette::new(2);
        palette.push([1, 1, 1], at(1));
        palette.push([2, 2, 2], at(2));
        palette.push([3, 3, 3], at(3));

        assert_eq!(rgbs(&palette), [[3, 3, 3], [2, 2, 2]]);
    }

    #[test]
    fn a_dropped_color_is_not_found_and_its_id_is_not_reused() {
        let mut palette = Palette::new(1);
        let dropped = palette.push([1, 1, 1], at(1));
        let kept = palette.push([2, 2, 2], at(2));

        assert_eq!(palette.get(dropped.id), None);
        assert_eq!(palette.get(kept.id), Some(kept));
        assert_ne!(kept.id, dropped.id);
    }

    #[test]
    fn lowering_the_limit_trims_at_once_and_the_limit_is_clamped() {
        let mut palette = Palette::new(8);
        for value in 0..5 {
            palette.push([value; 3], at(u32::from(value)));
        }
        palette.set_limit(0);

        assert_eq!(rgbs(&palette), [[4, 4, 4]]);
        assert_eq!(Palette::new(1000).limit, MAX_LIMIT);
    }
}
