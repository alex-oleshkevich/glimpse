pub(crate) mod render;

use std::time::Duration;

use chrono::Utc;
use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_dbus::idle::{IdleProviderHandle, IdleProviderState};
use glimpse_widgets::{IdlePopover, IndicatorSpec, Severity};
use gtk4::{gio, glib, prelude::*};

use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Ctx, Input, Opener};

const MINUTE: Duration = Duration::from_secs(60);

pub struct Idle {
    idle: IdleProviderHandle,
    footer: Option<(String, Vec<String>)>,
    tooltip_format: Option<String>,
    state: IdleProviderState,
    manual_hold: Vec<u64>,
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

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = IdlePopover::new();

        shown.connect_hold_requested({
            let idle = self.idle.clone();
            let opener = seat.opener();
            move |popover, seconds| {
                popover.set_hold_active(true);
                let idle = idle.clone();
                settling("idle.hold", opener.clone(), async move {
                    idle.hold(seconds).await
                });
            }
        });

        shown.connect_hold_toggled({
            let idle = self.idle.clone();
            let opener = seat.opener();
            move |_, on| {
                if on {
                    let idle = idle.clone();
                    settling(
                        "idle.hold",
                        opener.clone(),
                        async move { idle.hold(0).await },
                    );
                    return;
                }
                for id in render::manual_hold_ids(&idle.snapshot().inhibitors) {
                    let idle = idle.clone();
                    settling("idle.release", opener.clone(), async move {
                        idle.release(id).await
                    });
                }
            }
        });

        shown.connect_release_requested({
            let idle = self.idle.clone();
            let opener = seat.opener();
            move |_, id| {
                let idle = idle.clone();
                settling("idle.release", opener.clone(), async move {
                    idle.release(id).await
                });
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
        let manual_hold = render::manual_hold_ids(&state.inhibitors);
        Self {
            idle,
            footer: None,
            tooltip_format: None,
            state,
            manual_hold,
            icon: None,
            spec: Vec::new(),
            shown: glib::WeakRef::new(),
        }
    }

    fn sync(&mut self) {
        self.state = self.idle.snapshot();
        self.manual_hold = render::manual_hold_ids(&self.state.inhibitors);
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
        let active = !self.manual_hold.is_empty();
        let icon = self.themed(render::icon(active));
        Some(IndicatorSpec {
            icon: Some(icon),
            tooltip: render::tooltip(
                &self.state,
                &self.manual_hold,
                self.tooltip_format.as_deref(),
            ),
            severity: active.then_some(Severity::Warning),
            ..Default::default()
        })
    }

    fn themed(&mut self, name: &'static str) -> gio::Icon {
        match &self.icon {
            Some((held, icon)) if *held == name => icon.clone(),
            _ => {
                let icon: gio::Icon = gio::ThemedIcon::new(name).upcast();
                self.icon = Some((name, icon.clone()));
                icon
            }
        }
    }

    fn dress(&self, shown: &IdlePopover) {
        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));
        shown.set_heading(
            render::ICON_IDLE,
            &gettext("Idle"),
            Some(&render::hero_subtitle(&self.state, &self.manual_hold)),
        );
        shown.set_hold_active(!self.manual_hold.is_empty());
        shown.set_inhibitors(&render::to_inhibitor_entries(
            &self.state.inhibitors,
            &self.manual_hold,
            Utc::now(),
        ));
    }
}

fn settling<F, T, E>(operation: &'static str, opener: Opener, future: F)
where
    F: std::future::Future<Output = Result<T, E>> + 'static,
    T: 'static,
    E: std::fmt::Display + 'static,
{
    relm4::spawn_local(async move {
        if let Err(error) = future.await {
            tracing::warn!(operation, %error, "service command failed");
            opener.wake();
        }
    });
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
        applet.manual_hold = render::manual_hold_ids(&applet.state.inhibitors);

        let indicator = applet.indicator().unwrap();

        assert_eq!(indicator.severity, Some(Severity::Warning));
        assert_eq!(
            applet.icon.as_ref().map(|(name, _)| *name),
            Some(render::ICON_ACTIVE)
        );
    }
}
