use chrono::Local;
use glimpse_config::{Applet as AppletConfig, AppletKind, ClockConfig};
use glimpse_widgets::IndicatorSpec;

use crate::applet::{Applet, Ctx, Input};

#[derive(Default)]
pub struct Clock {
    config: ClockConfig,
}

impl Applet for Clock {
    fn start() -> Self {
        Self::default()
    }

    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Clock(clock) = &config.kind else {
            return;
        };
        self.config = clock.clone();
    }

    fn handle(&mut self, _ctx: &Ctx, _input: &Input) {}

    fn indicators(&self) -> Vec<IndicatorSpec> {
        vec![IndicatorSpec {
            label: Some(Local::now().format(&self.config.label_format).to_string()),
            ..Default::default()
        }]
    }
}
