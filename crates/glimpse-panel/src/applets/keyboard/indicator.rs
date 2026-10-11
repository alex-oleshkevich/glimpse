use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_services::KeyboardHandle;
use glimpse_services::{KeyboardLayouts, LayoutRef};
use glimpse_widgets::{IndicatorSpec, KeyboardPopover};
use gtk4::glib;
use gtk4::prelude::Cast as _;

use crate::applet::popover::Seat;
use crate::applet::{Applet, Ctx, Direction, Input, Pointer, spawn_command};

use super::render;

pub struct Keyboard {
    layouts: Option<KeyboardLayouts>,
    keyboard: KeyboardHandle,
    tooltip_format: Option<String>,
    spec: Vec<IndicatorSpec>,
    shown: glib::WeakRef<KeyboardPopover>,
}

impl Applet for Keyboard {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Keyboard {} = &config.kind else {
            return;
        };
        self.tooltip_format = config.common.tooltip_format.clone();

        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        match input {
            Input::Pointer(Pointer::Scroll(direction)) => {
                let target = match direction {
                    Direction::Up | Direction::Left => LayoutRef::Prev,
                    Direction::Down | Direction::Right => LayoutRef::Next,
                };
                self.cycle(target);
                return;
            }
            Input::Woken => self.layouts = Some(self.keyboard.snapshot()),
            Input::Tick => {}
            Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<gtk4::Widget> {
        let shown = KeyboardPopover::new();
        let keyboard = self.keyboard.clone();
        let opener = seat.opener();
        shown.connect_activated(move |_, index| {
            opener.close_popover();
            let Ok(index) = u8::try_from(index) else {
                return;
            };
            let keyboard = keyboard.clone();
            spawn_command("keyboard.switch", async move {
                keyboard.switch(LayoutRef::Index { index }).await
            });
        });

        self.shown.set(Some(&shown));
        self.refresh();
        Some(shown.upcast())
    }
}

impl Keyboard {
    pub fn start(keyboard: KeyboardHandle) -> Self {
        Self {
            layouts: Some(keyboard.snapshot()),
            keyboard,
            tooltip_format: None,
            spec: Vec::new(),
            shown: glib::WeakRef::new(),
        }
    }

    fn cycle(&mut self, target: LayoutRef) {
        if !render::shown(self.layouts.as_ref()) {
            return;
        }
        if let Some(layouts) = self.layouts.as_mut()
            && let Some(next) = render::step(layouts, target)
        {
            layouts.current = Some(next);
        }
        let keyboard = self.keyboard.clone();
        spawn_command(
            "keyboard.switch",
            async move { keyboard.switch(target).await },
        );
        self.refresh();
    }

    fn refresh(&mut self) {
        self.spec = self.indicator().into_iter().collect();
        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn indicator(&self) -> Option<IndicatorSpec> {
        let layouts = self.layouts.as_ref()?;
        Some(IndicatorSpec {
            label: Some(render::badge(layouts)?.to_owned()),
            tooltip: render::tooltip(layouts, self.tooltip_format.as_deref()),
            ..Default::default()
        })
    }

    fn dress(&self, shown: &KeyboardPopover) {
        match self.layouts.as_ref() {
            Some(layouts) => shown.set_layouts(&render::rows(layouts)),
            None => shown.set_layouts(&[]),
        }
    }
}
