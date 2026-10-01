use std::collections::VecDeque;

pub const MAX_LIMIT: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PickedColor {
    pub id: u32,
    pub rgb: [u8; 3],
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

    pub fn push(&mut self, rgb: [u8; 3]) -> PickedColor {
        let id = match self.colors.iter().position(|color| color.rgb == rgb) {
            Some(index) => self.colors.remove(index).map_or(0, |color| color.id),
            None => {
                let id = self.next_id;
                self.next_id = self.next_id.wrapping_add(1).max(1);
                id
            }
        };
        let color = PickedColor { id, rgb };
        self.colors.push_front(color);
        self.colors.truncate(self.limit);
        color
    }
}

#[cfg(test)]
mod tests;
