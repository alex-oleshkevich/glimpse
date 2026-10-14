mod imp;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{ActionRow, ExpanderRow, Fact, Row, RowTone, none_if_empty, reconcile};

pub use imp::{Job, Printer};

glib::wrapper! {
    pub struct PrintingPopover(ObjectSubclass<imp::PrintingPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for PrintingPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl PrintingPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_jobs(&self, jobs: &[Job]) {
        let imp = self.imp();
        if imp.job_data.borrow().as_slice() == jobs {
            return;
        }
        imp.job_data.replace(jobs.to_vec());
        self.render_jobs();
    }

    pub fn set_printers(&self, printers: &[Printer]) {
        let imp = self.imp();
        if imp.printer_data.borrow().as_slice() == printers {
            return;
        }
        imp.printer_data.replace(printers.to_vec());
        self.render_printers();
    }

    pub fn collapse(&self, id: &str) {
        if let Some((_, holder)) = self
            .imp()
            .job_held
            .borrow()
            .iter()
            .find(|(held, _)| held == id)
        {
            holder.set_open(false);
        }
    }

    pub fn connect_cancelled<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "cancelled",
            false,
            glib::closure_local!(move |popover: Self, id: String| f(&popover, &id)),
        )
    }

    pub fn connect_paused<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "paused",
            false,
            glib::closure_local!(move |popover: Self, id: String| f(&popover, &id)),
        )
    }

    pub fn connect_resumed<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "resumed",
            false,
            glib::closure_local!(move |popover: Self, id: String| f(&popover, &id)),
        )
    }

    pub fn connect_retried<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "retried",
            false,
            glib::closure_local!(move |popover: Self, id: String| f(&popover, &id)),
        )
    }

    fn render_jobs(&self) {
        let imp = self.imp();
        #[cfg(test)]
        imp.renders.set(imp.renders.get() + 1);
        let jobs = imp.job_data.borrow();
        reconcile::by_key(
            &*imp.job_rows,
            &mut imp.job_held.borrow_mut(),
            &jobs,
            |job| job.id.clone(),
            |job| self.build_job_row(job),
            dress_job,
        );
        imp.jobs.set_visible(!jobs.is_empty());
        imp.jobs.set_count(Some(jobs.len().to_string().as_str()));
    }

    fn render_printers(&self) {
        let imp = self.imp();
        #[cfg(test)]
        imp.printer_renders.set(imp.printer_renders.get() + 1);
        let printers = imp.printer_data.borrow();
        reconcile::by_key(
            &*imp.printer_rows,
            &mut imp.printer_held.borrow_mut(),
            &printers,
            |printer| (printer.id.clone(), !printer.facts.is_empty()),
            |printer| build_printer_row(!printer.facts.is_empty()),
            dress_printer,
        );
        imp.printers.set_visible(!printers.is_empty());
    }

    fn build_job_row(&self, job: &Job) -> ExpanderRow {
        let row = ExpanderRow::new();
        let details = row.details();
        details.append(&gtk4::Box::new(gtk4::Orientation::Vertical, 0));
        details.append(&self.build_action(&job.id, gettext("Pause"), "paused"));
        details.append(&self.build_action(&job.id, gettext("Resume"), "resumed"));
        details.append(&self.build_action(&job.id, gettext("Retry"), "retried"));
        let cancel = self.build_action(&job.id, gettext("Cancel"), "cancelled");
        cancel.upcast_ref::<Row>().set_tone(RowTone::Destructive);
        details.append(&cancel);
        row
    }

    fn build_action(&self, id: &str, label: String, signal: &'static str) -> ActionRow {
        let row = ActionRow::new();
        row.upcast_ref::<Row>().set_title(Some(label.as_str()));
        let key = id.to_owned();
        row.connect_activated(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>(signal, &[&key])
        ));
        row
    }
}

fn dress_job(row: &ExpanderRow, job: &Job) {
    row.set_title(none_if_empty(&job.name));
    row.set_busy(job.busy);
    let details = row.details();
    let Some(facts) = details.first_child().and_downcast::<gtk4::Box>() else {
        return;
    };
    crate::set_facts(
        &facts,
        &[
            Fact::new(gettext("Printer"), job.printer.clone()),
            Fact::new(gettext("Status"), job_status(job)),
        ],
    );
    let shown = [job.pausable, job.resumable, job.retryable, job.cancellable];
    let mut child = facts.next_sibling();
    for visible in shown {
        let Some(action) = child else {
            break;
        };
        child = action.next_sibling();
        if action.get_visible() != visible {
            action.set_visible(visible);
        }
    }
}

fn build_printer_row(carded: bool) -> gtk4::Widget {
    match carded {
        true => {
            let row = ExpanderRow::new();
            row.details()
                .append(&gtk4::Box::new(gtk4::Orientation::Vertical, 0));
            row.upcast()
        }
        false => Row::new().upcast(),
    }
}

fn dress_printer(widget: &gtk4::Widget, printer: &Printer) {
    let icon = Some(match printer.network {
        true => NETWORK_ICON,
        false => LOCAL_ICON,
    });
    let problem = none_if_empty(&printer.problem);
    let tone = match problem {
        Some(_) => RowTone::Warning,
        None => RowTone::Normal,
    };
    if let Some(row) = widget.downcast_ref::<ExpanderRow>() {
        row.set_title(none_if_empty(&printer.name));
        row.set_icon_name(icon);
        row.set_subtitle(problem);
        row.set_tone(tone);
        if let Some(facts) = row.details().first_child().and_downcast::<gtk4::Box>() {
            crate::set_facts(&facts, &printer.facts);
        }
    } else if let Some(row) = widget.downcast_ref::<Row>() {
        row.set_title(none_if_empty(&printer.name));
        row.set_icon_name(icon);
        row.set_subtitle(problem);
        row.set_tone(tone);
    }
}

const NETWORK_ICON: &str = "printer-network-symbolic";
const LOCAL_ICON: &str = "printer-symbolic";

fn job_status(job: &Job) -> String {
    match job.progress {
        Some((current, total)) => gettext("Page {current} of {total}")
            .replace("{current}", &current.to_string())
            .replace("{total}", &total.to_string()),
        None => job.status.clone(),
    }
}

#[cfg(test)]
mod tests;
