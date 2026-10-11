use glimpse_widgets::IndicatorSpec;
use gtk4::prelude::*;

use crate::applet::popover::Seat;
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

    fn popover(&mut self, _seat: &Seat) -> Option<gtk4::Widget> {
        Some(glimpse_widgets::WidgetDemoPopover::new().upcast())
    }
}
