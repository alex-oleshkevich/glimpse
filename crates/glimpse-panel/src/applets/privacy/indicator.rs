use std::time::{Duration, SystemTime};

use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{
    CommandError, CompositorHandle, PrivacyHandle, PrivacyResource, PrivacyState,
};

use glimpse_widgets::{IndicatorSpec, PrivacyPopover, Severity};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::applet::popover::{PopoverHandle, Seat};
use crate::applet::{Applet, Ctx, Input, Report, report_failure};

use super::render;

const SECOND: Duration = Duration::from_secs(1);
const MINUTE: Duration = Duration::from_secs(60);

pub struct Privacy {
    state: PrivacyState,
    privacy: PrivacyHandle,
    compositor: CompositorHandle,
    notifications: NotificationsProviderHandle,
    filters: render::Filters,
    tooltip_format: Option<String>,
    spec: Vec<IndicatorSpec>,
    shown: glib::WeakRef<PrivacyPopover>,
    ticking: Option<bool>,
}

fn themed(name: &str) -> gio::Icon {
    gio::ThemedIcon::new(name).upcast()
}

impl Applet for Privacy {
    fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Privacy(cfg) = &config.kind else {
            return;
        };
        self.filters = render::Filters {
            camera: cfg.show_camera,
            microphone: cfg.show_microphone,
            screencast: cfg.show_screencast,
            location: cfg.show_location,
        };
        self.tooltip_format = config.common.tooltip_format.clone();
        self.pace(ctx);
        self.refresh();
    }

    fn handle(&mut self, ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => self.state = self.privacy.snapshot(),
            Input::Tick => {}
            Input::Pointer(_) => return,
        }
        self.pace(ctx);
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, _seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = PrivacyPopover::new();

        shown.connect_stop_activated({
            let compositor = self.compositor.clone();
            let notifications = self.notifications.clone();
            let state = self.state.clone();
            let filters = self.filters;
            move |_, id| {
                let Some(session) = render::session_for(&state, filters, &id) else {
                    return;
                };
                stop_screencast(compositor.clone(), notifications.clone(), session);
            }
        });

        self.shown.set(Some(&shown));
        self.refresh();
        Some(Box::new(shown))
    }
}

fn stop_screencast(
    compositor: CompositorHandle,
    notifications: NotificationsProviderHandle,
    session: u64,
) {
    relm4::spawn_local(async move {
        if let Err(error) = compositor.stop_screencast(session).await {
            let report = Report {
                notifications,
                app_name: gettext("Privacy"),
                icon: render::SCREEN.to_owned(),
                summary: gettext("Could not stop that screen share"),
            };
            report_failure(
                "compositor.stop_screencast",
                report,
                compositor_wording(&error),
                error,
            )
            .await;
        }
    });
}

fn compositor_wording(error: &CommandError) -> Option<String> {
    Some(match error {
        CommandError::InvalidArgument(_) => gettext("That share had already ended."),
        CommandError::Unavailable(_) => gettext("The compositor is unavailable."),
        CommandError::Unsupported(_) => gettext("That is not supported."),
        CommandError::LimitExceeded(_) => gettext("That could not be completed."),
        CommandError::Internal(_) => gettext("That did not work."),
    })
}

impl Privacy {
    pub fn start(
        privacy: PrivacyHandle,
        compositor: CompositorHandle,
        notifications: NotificationsProviderHandle,
    ) -> Self {
        let state = privacy.snapshot();
        Self {
            state,
            privacy,
            compositor,
            notifications,
            filters: render::Filters::default(),
            tooltip_format: None,
            spec: Vec::new(),
            shown: glib::WeakRef::new(),
            ticking: None,
        }
    }

    fn pace(&mut self, ctx: &Ctx) {
        let casting = render::screencast_since(&self.state, self.filters).is_some();
        if self.ticking != Some(casting) {
            ctx.interval(if casting { SECOND } else { MINUTE });
            self.ticking = Some(casting);
        }
    }

    fn refresh(&mut self) {
        self.spec = self.chips();
        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn dress(&self, shown: &PrivacyPopover) {
        shown.set_usages(&render::usages(&self.state, self.filters));
        shown.set_screen_shared(render::screen_shared(&self.state, self.filters).as_deref());
    }

    fn chips(&self) -> Vec<IndicatorSpec> {
        let now = SystemTime::now();
        render::kinds_in_use(&self.state, self.filters)
            .into_iter()
            .map(|kind| {
                let usages: Vec<_> = self
                    .state
                    .usages
                    .iter()
                    .filter(|usage| usage.kind == kind)
                    .collect();
                let since = (kind == PrivacyResource::Screen)
                    .then(|| render::screencast_since(&self.state, self.filters))
                    .flatten();
                IndicatorSpec {
                    icon: Some(themed(match since {
                        Some(_) => render::RECORDING,
                        None => render::icon(kind),
                    })),
                    label: since.map(|since| render::elapsed(since, now)),
                    tooltip: Some(render::tooltip(
                        kind,
                        &usages,
                        self.tooltip_format.as_deref(),
                    )),
                    severity: since.is_none().then_some(Severity::Warning),
                    class: since.map(|_| render::RECORDING_CLASS.to_owned()),
                    ..Default::default()
                }
            })
            .collect()
    }
}
