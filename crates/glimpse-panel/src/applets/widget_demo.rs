use glimpse_widgets::IndicatorSpec;
use gtk4::prelude::*;

use crate::applet::popover::{PopoverHandle, Seat};
use crate::applet::{Applet, Ctx, Input};

pub struct WidgetDemo;

impl Applet for WidgetDemo {
    fn handle(&mut self, _ctx: &Ctx, _input: &Input) {}

    fn indicators(&self) -> Vec<IndicatorSpec> {
        vec![IndicatorSpec {
            icon: Some(gio::ThemedIcon::new("applications-graphics-symbolic").upcast()),
            label: Some("int".into()),
            ..Default::default()
        }]
    }

    fn popover(&mut self, _seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        Some(Box::new(glimpse_widgets::WidgetDemoPopover::new()))
    }
}
