#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    Command,
    Pointer,
    Popover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Reserve,
    Release,
    SlideIn,
    SlideOut,
    Linger,
    Settle,
    Region,
}

#[derive(Debug)]
pub struct Presence {
    pinned: bool,
    reserved: bool,
    visible: bool,
    holds: [bool; 3],
}

impl Presence {
    pub fn new(pinned: bool) -> Self {
        Self {
            pinned,
            reserved: pinned,
            visible: pinned,
            holds: [false; 3],
        }
    }

    pub fn pinned(&self) -> bool {
        self.pinned
    }

    pub fn reserved(&self) -> bool {
        self.reserved
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn held(&self, hold: Hold) -> bool {
        self.holds[hold as usize]
    }

    pub fn pin(&mut self, pinned: bool) -> Vec<Step> {
        if pinned == self.pinned {
            return Vec::new();
        }
        self.pinned = pinned;
        let mut steps = vec![Step::Settle];
        if pinned {
            self.reserved = true;
            steps.push(Step::Reserve);
            steps.extend(self.show());
        } else {
            self.holds = [false; 3];
            steps.extend(self.hide());
        }
        steps
    }

    pub fn hold(&mut self, hold: Hold, on: bool) -> Vec<Step> {
        if self.holds[hold as usize] == on {
            return Vec::new();
        }
        self.holds[hold as usize] = on;
        if on {
            let mut steps = vec![Step::Settle];
            steps.extend(self.show());
            return steps;
        }
        match self.idle() && self.visible {
            true => vec![Step::Linger],
            false => Vec::new(),
        }
    }

    pub fn expire(&mut self) -> Vec<Step> {
        match self.idle() && self.visible {
            true => self.hide(),
            false => Vec::new(),
        }
    }

    pub fn slid(&mut self) -> Vec<Step> {
        if self.visible {
            return Vec::new();
        }
        let mut steps = Vec::new();
        if self.reserved && !self.pinned {
            self.reserved = false;
            steps.push(Step::Release);
        }
        steps.push(Step::Region);
        steps
    }

    fn idle(&self) -> bool {
        !self.pinned && !self.holds.contains(&true)
    }

    fn show(&mut self) -> Vec<Step> {
        if self.visible {
            return Vec::new();
        }
        self.visible = true;
        vec![Step::SlideIn, Step::Region]
    }

    fn hide(&mut self) -> Vec<Step> {
        if !self.visible {
            return self.slid();
        }
        self.visible = false;
        vec![Step::SlideOut]
    }
}

#[cfg(test)]
mod tests;
