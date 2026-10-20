use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::KeyboardHandle;
use glimpse_services::{KeyboardLayouts, LayoutRef};
use glimpse_widgets::{IndicatorSpec, KeyboardPopover};
use gtk4::glib;
use gtk4::prelude::Cast as _;

use crate::applet::popover::Seat;
use crate::applet::{Applet, Ctx, Direction, Input, Pointer, Report, spawn_command, tell, wording};

use super::render;

pub struct Keyboard {
    layouts: Option<KeyboardLayouts>,
    keyboard: KeyboardHandle,
    notifications: NotificationsProviderHandle,
    tooltip_format: Option<String>,
    spec: Vec<IndicatorSpec>,
    shown: glib::WeakRef<KeyboardPopover>,
}

impl Applet for Keyboard {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Keyboard { .. } = &config.kind else {
            return;
        };
        self.tooltip_format = config.kind.tooltip_format().map(str::to_owned);

        self.refresh();
    }

    fn handle(&mut self, ctx: &Ctx, input: &Input) {
        match input {
            Input::Pointer(Pointer::Scroll(direction)) => {
                let target = match direction {
                    Direction::Up | Direction::Left => LayoutRef::Prev,
                    Direction::Down | Direction::Right => LayoutRef::Next,
                };
                self.cycle(ctx, target);
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
    pub fn start(keyboard: KeyboardHandle, notifications: NotificationsProviderHandle) -> Self {
        Self {
            layouts: Some(keyboard.snapshot()),
            keyboard,
            notifications,
            tooltip_format: None,
            spec: Vec::new(),
            shown: glib::WeakRef::new(),
        }
    }

    fn cycle(&mut self, ctx: &Ctx, target: LayoutRef) {
        if !render::shown(self.layouts.as_ref()) {
            return;
        }
        if let Some(layouts) = self.layouts.as_mut()
            && let Some(next) = render::step(layouts, target)
        {
            layouts.current = Some(next);
        }
        let keyboard = self.keyboard.clone();
        tell(
            Report {
                notifications: self.notifications.clone(),
                app_name: gettext("Keyboard"),
                icon: "input-keyboard-symbolic".to_owned(),
                summary: gettext("Could not switch the keyboard layout"),
            },
            ctx.opener(),
            "keyboard.switch",
            async move { keyboard.switch(target).await },
            gettext("The keyboard layout service is unavailable."),
            wording,
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

#[cfg(test)]
mod tests;
