use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{AudioHandle, PrivacyError, PrivacyHandle, PrivacyState};

use glimpse_widgets::{IndicatorSpec, PrivacyPopover};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::applet::popover::{PopoverHandle, Seat};
use crate::applet::{Applet, Ctx, Input, Opener, Report, report_failure};

use super::render;

pub struct Privacy {
    state: PrivacyState,
    mic_muted: bool,
    privacy: PrivacyHandle,
    audio: AudioHandle,
    notifications: NotificationsProviderHandle,
    filters: render::Filters,
    tooltip_format: Option<String>,
    spec: Vec<IndicatorSpec>,
    pending: Rc<RefCell<HashSet<String>>>,
    shown: glib::WeakRef<PrivacyPopover>,
}

fn themed(name: &str) -> gio::Icon {
    gio::ThemedIcon::new(name).upcast()
}

fn mic_muted(audio: &AudioHandle) -> bool {
    audio
        .snapshot()
        .default_input()
        .is_some_and(|device| device.muted)
}

impl Applet for Privacy {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
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
        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => {
                self.state = self.privacy.snapshot();
                self.mic_muted = mic_muted(&self.audio);
            }
            Input::Tick | Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = PrivacyPopover::new();

        shown.connect_muted({
            let privacy = self.privacy.clone();
            let notifications = self.notifications.clone();
            let opener = seat.opener();
            let pending = Rc::clone(&self.pending);
            move |_, id| {
                let id = id.to_owned();
                if !pending.borrow_mut().insert(id.clone()) {
                    return;
                }
                opener.wake();
                let command = privacy.clone();
                act(
                    &notifications,
                    "privacy.mute_microphone",
                    gettext("Could not mute the microphone"),
                    &opener,
                    &pending,
                    id,
                    async move { command.mute_microphone().await },
                );
            }
        });

        shown.connect_stop_requested({
            let privacy = self.privacy.clone();
            let notifications = self.notifications.clone();
            let opener = seat.opener();
            let pending = Rc::clone(&self.pending);
            move |_, id| {
                let id = id.to_owned();
                if !pending.borrow_mut().insert(id.clone()) {
                    return;
                }
                opener.wake();
                let Some(session_id) = render::session_for(&privacy.snapshot(), &id) else {
                    tracing::debug!(
                        id = %id,
                        "stop-sharing requested for a cast that no longer exists"
                    );
                    pending.borrow_mut().remove(&id);
                    opener.wake();
                    return;
                };
                let command = privacy.clone();
                act(
                    &notifications,
                    "privacy.stop_screencast",
                    gettext("Could not stop screen sharing"),
                    &opener,
                    &pending,
                    id,
                    async move { command.stop_screencast(session_id).await },
                );
            }
        });

        self.shown.set(Some(&shown));
        self.refresh();
        Some(Box::new(shown))
    }
}

/// Runs one command to completion, whatever it decides: the id always leaves `pending` and the
/// popover always wakes, so a failure never leaves a row stuck busy. The service reaches its new
/// state reactively, through the audio or compositor subscription the command's own effect feeds,
/// so there is nothing here to refresh explicitly the way a one-shot service would need.
fn act(
    notifications: &NotificationsProviderHandle,
    operation: &'static str,
    summary: String,
    opener: &Opener,
    pending: &Rc<RefCell<HashSet<String>>>,
    id: String,
    command: impl std::future::Future<Output = Result<(), PrivacyError>> + 'static,
) {
    let report = Report {
        notifications: notifications.clone(),
        app_name: gettext("Privacy"),
        icon: render::HERO.to_owned(),
        summary,
    };
    let opener = opener.clone();
    let pending = Rc::clone(pending);
    relm4::spawn_local(async move {
        let outcome = command.await;
        pending.borrow_mut().remove(&id);
        if let Err(error) = outcome {
            report_failure(operation, report, wording(&error), error).await;
        }
        opener.wake();
    });
}

fn wording(error: &PrivacyError) -> Option<String> {
    match error {
        PrivacyError::Audio(_) => Some(gettext("The audio service could not complete that.")),
        PrivacyError::Service(error) => {
            crate::applet::wording(error, &gettext("Privacy tracking is unreachable."))
        }
    }
}

impl Privacy {
    pub fn start(
        privacy: PrivacyHandle,
        audio: AudioHandle,
        notifications: NotificationsProviderHandle,
    ) -> Self {
        let state = privacy.snapshot();
        let mic_muted_now = mic_muted(&audio);
        Self {
            state,
            mic_muted: mic_muted_now,
            privacy,
            audio,
            notifications,
            filters: render::Filters::default(),
            tooltip_format: None,
            spec: Vec::new(),
            pending: Rc::new(RefCell::new(HashSet::new())),
            shown: glib::WeakRef::new(),
        }
    }

    fn refresh(&mut self) {
        self.spec = self.chips();
        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn dress(&self, shown: &PrivacyPopover) {
        let pending = self.pending.borrow();
        shown.set_usages(&render::usages(
            &self.state,
            self.filters,
            self.mic_muted,
            &pending,
        ));
        shown.set_screen_shared(render::screen_shared(&self.state, self.filters).as_deref());
    }

    fn chips(&self) -> Vec<IndicatorSpec> {
        render::kinds_in_use(&self.state, self.filters)
            .into_iter()
            .map(|kind| {
                let usages: Vec<_> = self
                    .state
                    .usages
                    .iter()
                    .filter(|usage| usage.kind == kind)
                    .collect();
                IndicatorSpec {
                    icon: Some(themed(render::icon(kind))),
                    tooltip: Some(render::tooltip(
                        kind,
                        &usages,
                        self.tooltip_format.as_deref(),
                    )),
                    ..Default::default()
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wording_never_leaks_a_raw_failure_token() {
        let told = wording(&PrivacyError::Service(
            glimpse_services::CommandError::Unavailable("no default microphone".to_owned()),
        ))
        .expect("a failure always has words");
        assert!(!told.is_empty());
        assert!(!told.contains("no default microphone"));
    }
}
