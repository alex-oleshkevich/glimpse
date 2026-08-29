use glimpse_contracts::{HeartbeatReset, HeartbeatSetInterval, HeartbeatTick};
use glimpse_widgets::IndicatorSpec;
use gtk4::prelude::*;

use crate::applet::{Applet, Button, Ctx, Direction, Input, Pointer, payload};

const INDICATOR: &str = "count";
const ICON: &str = "emblem-synchronizing-symbolic";
const DEFAULT_PERIOD_MS: u64 = 1000;
const PERIOD_STEP_MS: u64 = 100;
const MIN_PERIOD_MS: u64 = 100;
const MAX_PERIOD_MS: u64 = 5000;

pub struct Heartbeat {
    count: Option<u64>,
    period_ms: u64,
    icon: gio::Icon,
}

impl Applet for Heartbeat {
    fn start(ctx: &Ctx) -> Self {
        ctx.subscribe::<HeartbeatTick>();
        Self {
            count: None,
            period_ms: DEFAULT_PERIOD_MS,
            icon: gio::ThemedIcon::new(ICON).upcast(),
        }
    }

    fn handle(&mut self, ctx: &Ctx, input: &Input) {
        match input {
            Input::Topic(event) => {
                if let Some(tick) = payload::<HeartbeatTick>(event) {
                    self.count = Some(tick.count);
                }
            }
            Input::Pointer {
                pointer: Pointer::Press(Button::Left),
                ..
            } => ctx.call::<HeartbeatReset>(HeartbeatReset {}),
            Input::Pointer {
                pointer: Pointer::Scroll(direction),
                ..
            } => self.retime(ctx, *direction),
            Input::Pointer { .. } => {}
        }
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        let Some(count) = self.count else {
            return Vec::new();
        };
        vec![IndicatorSpec {
            id: INDICATOR.to_owned(),
            icon: Some(self.icon.clone()),
            label: Some(count.to_string()),
            tooltip: Some(format!("every {} ms", self.period_ms)),
            ..Default::default()
        }]
    }
}

impl Heartbeat {
    fn retime(&mut self, ctx: &Ctx, direction: Direction) {
        let Some(period_ms) = stepped(self.period_ms, direction) else {
            return;
        };
        self.period_ms = period_ms;
        ctx.call::<HeartbeatSetInterval>(HeartbeatSetInterval { period_ms });
    }
}

fn stepped(period_ms: u64, direction: Direction) -> Option<u64> {
    let next = match direction {
        Direction::Up => period_ms.saturating_sub(PERIOD_STEP_MS),
        Direction::Down => period_ms.saturating_add(PERIOD_STEP_MS),
        Direction::Left | Direction::Right => return None,
    }
    .clamp(MIN_PERIOD_MS, MAX_PERIOD_MS);

    (next != period_ms).then_some(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrolling_walks_the_period_and_stops_at_the_ends() {
        assert_eq!(stepped(DEFAULT_PERIOD_MS, Direction::Up), Some(900));
        assert_eq!(stepped(DEFAULT_PERIOD_MS, Direction::Down), Some(1100));
        assert_eq!(stepped(MIN_PERIOD_MS, Direction::Up), None);
        assert_eq!(stepped(MAX_PERIOD_MS, Direction::Down), None);
    }

    #[test]
    fn a_horizontal_scroll_leaves_the_period_alone() {
        assert_eq!(stepped(DEFAULT_PERIOD_MS, Direction::Left), None);
        assert_eq!(stepped(DEFAULT_PERIOD_MS, Direction::Right), None);
    }
}
