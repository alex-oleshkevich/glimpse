use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_contracts::{KeyboardLayouts, LayoutRef};
use glimpse_services::KeyboardHandle;
use glimpse_widgets::{IndicatorSpec, KeyboardPopover};
use gtk4::glib;

use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Ctx, Direction, Input, Pointer, spawn_command};

use super::render;

pub struct Keyboard {
    layouts: Option<KeyboardLayouts>,
    keyboard: KeyboardHandle,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    spec: Vec<IndicatorSpec>,
    shown: glib::WeakRef<KeyboardPopover>,
}

impl Applet for Keyboard {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Keyboard {} = &config.kind else {
            return;
        };
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer = config
            .common
            .settings()
            .map(|(label, command)| (label.to_owned(), command.to_vec()));
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
            _ => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, _seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = KeyboardPopover::new();
        let keyboard = self.keyboard.clone();
        shown.connect_activated(move |_, index| {
            let Ok(index) = u8::try_from(index) else {
                return;
            };
            let keyboard = keyboard.clone();
            spawn_command("keyboard.switch", async move {
                keyboard.switch(LayoutRef::Index { index }).await
            });
        });
        if let Some((_, command)) = &self.footer {
            let command = command.clone();
            shown.connect_footer_activated(move |_| run(&command));
        }
        self.shown.set(Some(&shown));
        self.refresh();
        Some(Box::new(shown))
    }
}

impl Keyboard {
    pub fn start(keyboard: KeyboardHandle) -> Self {
        Self {
            layouts: Some(keyboard.snapshot()),
            keyboard,
            tooltip_format: None,
            footer: None,
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
        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));
        match self.layouts.as_ref() {
            Some(layouts) => shown.set_layouts(&render::rows(layouts)),
            None => shown.set_layouts(&[]),
        }
    }
}
