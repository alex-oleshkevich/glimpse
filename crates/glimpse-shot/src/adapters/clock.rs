use std::time::Instant;

use glimpse_shot::ports::Clock;

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}
