use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{PrintingError, PrintingHandle, PrintingState};

use glimpse_widgets::{IndicatorSpec, PrintingPopover, Severity};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::applet::popover::{PopoverHandle, Seat};
use crate::applet::{Applet, Ctx, Input, Opener, Report, report_failure};

use super::render;

pub struct Printing {
    state: PrintingState,
    printing: PrintingHandle,
    notifications: NotificationsProviderHandle,
    tooltip_format: Option<String>,
    spec: Vec<IndicatorSpec>,
    jobs: usize,
    pending: Rc<RefCell<HashSet<u32>>>,
    shown: glib::WeakRef<PrintingPopover>,
}

fn themed(name: &str) -> gio::Icon {
    gio::ThemedIcon::new(name).upcast()
}

impl Applet for Printing {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Printing(cfg) = &config.kind else {
            return;
        };
        self.jobs = cfg.jobs;
        self.tooltip_format = config.common.tooltip_format.clone();
        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => self.state = self.printing.snapshot(),
            Input::Tick | Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = PrintingPopover::new();

        shown.connect_cancelled({
            let printing = self.printing.clone();
            let notifications = self.notifications.clone();
            let opener = seat.opener();
            let pending = Rc::clone(&self.pending);
            let popover = shown.downgrade();
            move |_, id| {
                let Ok(id) = id.parse::<u32>() else {
                    return;
                };
                if !pending.borrow_mut().insert(id) {
                    return;
                }
                opener.wake();
                let command = printing.clone();
                act(
                    &notifications,
                    "printing.cancel_job",
                    gettext("Could not cancel the print job"),
                    &opener,
                    &popover,
                    &pending,
                    id,
                    printing.clone(),
                    async move { command.cancel_job(id).await },
                );
            }
        });

        shown.connect_paused({
            let printing = self.printing.clone();
            let notifications = self.notifications.clone();
            let opener = seat.opener();
            let pending = Rc::clone(&self.pending);
            let popover = shown.downgrade();
            move |_, id| {
                let Ok(id) = id.parse::<u32>() else {
                    return;
                };
                if !pending.borrow_mut().insert(id) {
                    return;
                }
                opener.wake();
                let command = printing.clone();
                act(
                    &notifications,
                    "printing.pause_job",
                    gettext("Could not pause the print job"),
                    &opener,
                    &popover,
                    &pending,
                    id,
                    printing.clone(),
                    async move { command.pause_job(id).await },
                );
            }
        });

        shown.connect_resumed({
            let printing = self.printing.clone();
            let notifications = self.notifications.clone();
            let opener = seat.opener();
            let pending = Rc::clone(&self.pending);
            let popover = shown.downgrade();
            move |_, id| {
                let Ok(id) = id.parse::<u32>() else {
                    return;
                };
                if !pending.borrow_mut().insert(id) {
                    return;
                }
                opener.wake();
                let command = printing.clone();
                act(
                    &notifications,
                    "printing.resume_job",
                    gettext("Could not resume the print job"),
                    &opener,
                    &popover,
                    &pending,
                    id,
                    printing.clone(),
                    async move { command.resume_job(id).await },
                );
            }
        });

        self.shown.set(Some(&shown));
        self.refresh();
        Some(Box::new(shown))
    }
}

/// Runs one job command to completion, whatever it decides: the id always leaves `pending` and
/// the popover always wakes, so a failure never leaves a row stuck busy. A success calls
/// `refresh()` so the state the next render reads is current rather than whatever the last poll
/// happened to see, shortening the window in which the row could otherwise be double-clicked.
#[allow(clippy::too_many_arguments)]
fn act(
    notifications: &NotificationsProviderHandle,
    operation: &'static str,
    summary: String,
    opener: &Opener,
    popover: &glib::WeakRef<PrintingPopover>,
    pending: &Rc<RefCell<HashSet<u32>>>,
    id: u32,
    printing: PrintingHandle,
    command: impl std::future::Future<Output = Result<(), PrintingError>> + 'static,
) {
    let report = Report {
        notifications: notifications.clone(),
        app_name: gettext("Printing"),
        icon: render::IDLE.to_owned(),
        summary,
    };
    let opener = opener.clone();
    let popover = popover.clone();
    let pending = Rc::clone(pending);
    relm4::spawn_local(async move {
        let outcome = command.await;
        pending.borrow_mut().remove(&id);
        match outcome {
            Ok(()) => {
                if let Some(popover) = popover.upgrade() {
                    popover.collapse(&id.to_string());
                }
                let _ = printing.refresh().await;
            }
            Err(error) => report_failure(operation, report, wording(&error), error).await,
        }
        opener.wake();
    });
}

fn wording(error: &PrintingError) -> Option<String> {
    match error {
        PrintingError::Failed(_) => Some(gettext("The print server refused that.")),
        PrintingError::Service(error) => {
            crate::applet::wording(error, &gettext("The print server is unreachable."))
        }
    }
}

impl Printing {
    pub fn start(printing: PrintingHandle, notifications: NotificationsProviderHandle) -> Self {
        let state = printing.snapshot();
        Self {
            state,
            printing,
            notifications,
            tooltip_format: None,
            spec: Vec::new(),
            jobs: 8,
            pending: Rc::new(RefCell::new(HashSet::new())),
            shown: glib::WeakRef::new(),
        }
    }

    fn refresh(&mut self) {
        self.spec = self.indicator().into_iter().collect();
        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn dress(&self, shown: &PrintingPopover) {
        let pending = self.pending.borrow();
        shown.set_jobs(&render::jobs(&self.state, self.jobs, &pending));
        shown.set_printers(&render::printers(&self.state));
    }

    fn indicator(&self) -> Option<IndicatorSpec> {
        let attention = render::attention(&self.state);
        Some(IndicatorSpec {
            icon: Some(themed(render::chip(&self.state)?)),
            attention,
            severity: attention.then_some(Severity::Warning),
            tooltip: render::tooltip(&self.state, self.tooltip_format.as_deref(), self.jobs),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wording_never_leaks_a_raw_failure_token() {
        let told = wording(&PrintingError::Failed(
            glimpse_services::PrintingFailure::Refused,
        ))
        .expect("a failure always has words");
        assert!(!told.is_empty());
    }
}
