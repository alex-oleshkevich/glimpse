pub(crate) mod render;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use chrono::Utc;
use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_dbus::idle::{IdleProviderHandle, IdleProviderState};
use glimpse_widgets::{IdlePopover, IndicatorSpec, Severity};
use gtk4::{gio, glib, prelude::*};

use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Ctx, Input, spawn_command};

const MINUTE: Duration = Duration::from_secs(60);

pub struct Idle {
    idle: IdleProviderHandle,
    footer: Option<(String, Vec<String>)>,
    tooltip_format: Option<String>,
    state: IdleProviderState,
    known_ids: Vec<u64>,
    manual_hold: Rc<RefCell<Vec<u64>>>,
    icon: Option<(&'static str, gio::Icon)>,
    spec: Vec<IndicatorSpec>,
    shown: glib::WeakRef<IdlePopover>,
}

impl Applet for Idle {
    fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Idle {} = &config.kind else {
            return;
        };
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer = config
            .common
            .settings()
            .map(|(label, command)| (label.to_owned(), command.to_vec()));

        ctx.interval(MINUTE);
        self.sync();
        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => self.sync(),
            Input::Tick => {}
            _ => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, _seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = IdlePopover::new();

        shown.connect_hold_requested({
            let idle = self.idle.clone();
            move |popover, seconds| {
                popover.set_hold_active(true);
                let idle = idle.clone();
                spawn_command("idle.hold", async move { idle.hold(seconds).await });
            }
        });

        shown.connect_hold_toggled({
            let idle = self.idle.clone();
            let manual_hold = Rc::clone(&self.manual_hold);
            move |_, on| {
                if on {
                    let idle = idle.clone();
                    spawn_command("idle.hold", async move { idle.hold(0).await });
                    return;
                }
                let holds = manual_hold.borrow().clone();
                for id in holds {
                    let idle = idle.clone();
                    spawn_command("idle.release", async move { idle.release(id).await });
                }
            }
        });

        shown.connect_release_requested({
            let idle = self.idle.clone();
            move |_, id| {
                let idle = idle.clone();
                spawn_command("idle.release", async move { idle.release(id).await });
            }
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

impl Idle {
    pub fn start(idle: IdleProviderHandle) -> Self {
        let state = idle.snapshot();
        let known_ids = ids_of(&state);
        let manual_hold = Rc::new(RefCell::new(render::manual_hold_ids(&state.inhibitors)));
        Self {
            idle,
            footer: None,
            tooltip_format: None,
            state,
            known_ids,
            manual_hold,
            icon: None,
            spec: Vec::new(),
            shown: glib::WeakRef::new(),
        }
    }

    fn sync(&mut self) {
        let state = self.idle.snapshot();
        let adopted = render::newly_adopted_holds(&state.inhibitors, &self.known_ids);
        if !adopted.is_empty() {
            self.manual_hold.borrow_mut().extend(adopted);
        }
        self.manual_hold
            .borrow_mut()
            .retain(|id| state.inhibitors.iter().any(|record| record.id == *id));
        self.known_ids = ids_of(&state);
        self.state = state;
    }

    fn refresh(&mut self) {
        self.spec = self.indicator().into_iter().collect();

        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn indicator(&mut self) -> Option<IndicatorSpec> {
        if !self.state.available {
            return Some(IndicatorSpec {
                icon: Some(self.themed(render::ICON_IDLE)),
                tooltip: self.state.reason.clone(),
                severity: Some(Severity::Warning),
                ..Default::default()
            });
        }
        let active = !self.manual_hold.borrow().is_empty();
        let icon = self.themed(render::icon(active));
        Some(IndicatorSpec {
            icon: Some(icon),
            tooltip: render::tooltip(
                &self.state,
                &self.manual_hold.borrow(),
                self.tooltip_format.as_deref(),
            ),
            severity: active.then_some(Severity::Warning),
            ..Default::default()
        })
    }

    fn themed(&mut self, name: &'static str) -> gio::Icon {
        if self.icon.as_ref().is_none_or(|(held, _)| *held != name) {
            self.icon = Some((name, gio::ThemedIcon::new(name).upcast()));
        }
        match self.icon.as_ref() {
            Some((_, icon)) => icon.clone(),
            None => gio::ThemedIcon::new(name).upcast(),
        }
    }

    fn dress(&self, shown: &IdlePopover) {
        let manual_hold = self.manual_hold.borrow();
        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));
        shown.set_heading(
            render::ICON_IDLE,
            &gettext("Idle"),
            Some(&render::hero_subtitle(&self.state, &manual_hold)),
        );
        shown.set_hold_active(!manual_hold.is_empty());
        shown.set_inhibitors(&render::to_inhibitor_entries(
            &self.state.inhibitors,
            &manual_hold,
            Utc::now(),
        ));
    }
}

fn ids_of(state: &IdleProviderState) -> Vec<u64> {
    state.inhibitors.iter().map(|record| record.id).collect()
}

#[cfg(test)]
mod tests {
    use glimpse_dbus::idle::{
        IdleInhibitorRecord, IdleInhibitorSource, IdleProvider, InhibitionTargets, Login1Mode,
    };

    use super::*;

    #[test]
    fn external_inhibitors_leave_the_indicator_idle() {
        let provider = IdleProvider::unavailable("idle provider unavailable");
        let mut applet = Idle::start(provider.handle());
        applet.state.available = true;
        applet.state.reason = None;
        applet.state.inhibitors.push(IdleInhibitorRecord {
            id: 1,
            who: "Firefox".to_owned(),
            why: "Playing video".to_owned(),
            bus_name: ":1.7".to_owned(),
            process_name: "firefox".to_owned(),
            source: IdleInhibitorSource::screen_saver(1),
            targets: InhibitionTargets::idle_only(),
            can_release: false,
            added_at_unix: 0,
        });

        let indicator = applet.indicator().unwrap();

        assert_eq!(indicator.severity, None);
        assert_eq!(
            applet.icon.as_ref().map(|(name, _)| *name),
            Some(render::ICON_IDLE)
        );
    }

    #[test]
    fn manual_hold_activates_the_indicator() {
        let provider = IdleProvider::unavailable("idle provider unavailable");
        let mut applet = Idle::start(provider.handle());
        applet.state.available = true;
        applet.state.reason = None;
        applet.state.inhibitors.push(IdleInhibitorRecord {
            id: 1,
            who: "glimpse-idle".to_owned(),
            why: "Manual hold".to_owned(),
            bus_name: ":1.7".to_owned(),
            process_name: String::new(),
            source: IdleInhibitorSource::login1(4242, 1000, Login1Mode::Block),
            targets: InhibitionTargets::manual_hold(),
            can_release: true,
            added_at_unix: 0,
        });
        applet
            .manual_hold
            .borrow_mut()
            .extend(render::manual_hold_ids(&applet.state.inhibitors));

        let indicator = applet.indicator().unwrap();

        assert_eq!(indicator.severity, Some(Severity::Warning));
        assert_eq!(
            applet.icon.as_ref().map(|(name, _)| *name),
            Some(render::ICON_ACTIVE)
        );
    }
}
