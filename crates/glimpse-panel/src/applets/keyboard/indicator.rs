use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_contracts::{KeyboardLayouts, LayoutRef, Message, SwitchLayout};
use glimpse_widgets::{IndicatorSpec, KeyboardPopover};
use gtk4::glib;

use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Ctx, Direction, Input, Pointer, payload};

use super::render;

pub struct Keyboard {
    layouts: Option<KeyboardLayouts>,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    spec: Vec<IndicatorSpec>,
    shown: glib::WeakRef<KeyboardPopover>,
}

impl Applet for Keyboard {
    fn topics(&self) -> &'static [&'static str] {
        &[KeyboardLayouts::NAME]
    }

    fn start() -> Self {
        Self {
            layouts: None,
            tooltip_format: None,
            footer: None,
            spec: Vec::new(),
            shown: glib::WeakRef::new(),
        }
    }

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

    fn handle(&mut self, ctx: &Ctx, input: &Input) {
        match input {
            Input::Topic(event) => {
                let Some(layouts) = payload::<KeyboardLayouts>(event) else {
                    return;
                };
                self.layouts = Some(layouts);
            }
            Input::Pointer(Pointer::Scroll(direction)) => {
                let target = match direction {
                    Direction::Up | Direction::Left => LayoutRef::Prev,
                    Direction::Down | Direction::Right => LayoutRef::Next,
                };
                self.cycle(ctx, target);
                return;
            }
            Input::Tick | Input::Woken => {}
            Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = KeyboardPopover::new();
        let caller = seat.caller();
        shown.connect_activated(move |_, index| {
            let Ok(index) = u8::try_from(index) else {
                return;
            };
            caller.call::<SwitchLayout>(SwitchLayout {
                target: LayoutRef::Index { index },
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
    fn cycle(&mut self, ctx: &Ctx, target: LayoutRef) {
        if !render::shown(self.layouts.as_ref()) {
            return;
        }
        if let Some(layouts) = self.layouts.as_mut()
            && let Some(next) = render::step(layouts, target)
        {
            layouts.current = Some(next);
        }
        ctx.call::<SwitchLayout>(SwitchLayout { target });
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
