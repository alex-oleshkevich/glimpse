use std::time::{Duration, SystemTime};

use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{
    AudioDeviceId, AudioDirection, AudioHandle, CommandError, CompositorHandle, PrivacyHandle,
    PrivacyResource, PrivacyState,
};

use glimpse_widgets::{IndicatorSpec, PrivacyPopover, Severity};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::applet::popover::{PopoverHandle, Seat};
use crate::applet::{Applet, Ctx, Input, Opener, Report, report_failure, spawn_reported};
use crate::applets::audio;

use super::render;

const SECOND: Duration = Duration::from_secs(1);
const MINUTE: Duration = Duration::from_secs(60);

pub struct Privacy {
    state: PrivacyState,
    privacy: PrivacyHandle,
    compositor: CompositorHandle,
    audio: AudioHandle,
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
        self.refresh(&ctx.opener());
    }

    fn handle(&mut self, ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => self.state = self.privacy.snapshot(),
            Input::Tick => {}
            Input::Pointer(_) => return,
        }
        self.pace(ctx);
        self.refresh(&ctx.opener());
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = PrivacyPopover::new();

        shown.connect_stop_activated({
            let compositor = self.compositor.clone();
            let notifications = self.notifications.clone();
            let state = self.state.clone();
            let filters = self.filters;
            move |_, id| {
                for session in render::sessions_for(&state, filters, &id) {
                    stop_screencast(compositor.clone(), notifications.clone(), session);
                }
            }
        });

        shown.connect_mute_toggled({
            let audio = self.audio.clone();
            let notifications = self.notifications.clone();
            move |_, muted| {
                let Some(id) = default_input(&audio).map(|(id, _)| id) else {
                    return;
                };
                let audio = audio.clone();
                let report = Report {
                    notifications: notifications.clone(),
                    app_name: gettext("Privacy"),
                    icon: render::MICROPHONE.to_owned(),
                    summary: gettext("Could not change that setting"),
                };
                spawn_reported(
                    "audio.set_device_muted",
                    report,
                    audio::wording,
                    async move {
                        audio
                            .set_device_muted(AudioDirection::Input, id, muted)
                            .await
                    },
                );
            }
        });

        self.shown.set(Some(&shown));
        self.refresh(&seat.opener());
        Some(Box::new(shown))
    }
}

fn default_input(audio: &AudioHandle) -> Option<(AudioDeviceId, bool)> {
    audio
        .snapshot()
        .inputs
        .into_iter()
        .find(|device| device.default)
        .map(|device| (device.id, device.muted))
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
        audio: AudioHandle,
        notifications: NotificationsProviderHandle,
    ) -> Self {
        let state = privacy.snapshot();
        Self {
            state,
            privacy,
            compositor,
            audio,
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

    fn refresh(&mut self, opener: &Opener) {
        self.spec = self.chips();
        let Some(shown) = self.shown.upgrade() else {
            return;
        };
        match self.spec.is_empty() {
            true => opener.close_popover(),
            false => self.dress(&shown),
        }
    }

    fn dress(&self, shown: &PrivacyPopover) {
        let outputs = self
            .compositor
            .snapshot()
            .outputs
            .map(|outputs| outputs.outputs)
            .unwrap_or_default();
        let input = default_input(&self.audio);
        let muted = input.as_ref().is_some_and(|(_, muted)| *muted);
        shown.set_usages(&render::usages(&self.state, self.filters, &outputs, muted));
        shown.set_microphone_muted(
            input
                .filter(|_| render::uses_microphone(&self.state, self.filters))
                .map(|(_, muted)| muted),
        );
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
