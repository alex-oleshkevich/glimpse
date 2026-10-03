mod imp;

use gettextrs::{gettext, ngettext};
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Expandable, Fact, FactList, LegacyRow, none_if_empty, reconcile};

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

    /// `Job::name`, `Job::printer` and `Job::status` are already-cleaned plain text: this widget
    /// performs no sanitization of its own, which is the panel applet's job before it calls this
    /// setter.
    pub fn set_jobs(&self, jobs: &[Job]) {
        let imp = self.imp();
        if imp.job_data.borrow().as_slice() == jobs {
            return;
        }
        imp.job_data.replace(jobs.to_vec());
        self.render_jobs();
    }

    /// `Printer::name` and `Printer::status` are already-cleaned plain text, for the same reason.
    pub fn set_printers(&self, printers: &[Printer]) {
        let imp = self.imp();
        if imp.printer_data.borrow().as_slice() == printers {
            return;
        }
        imp.printer_data.replace(printers.to_vec());
        self.render_printers();
    }

    /// Closes one job's card: what the applet calls once the action taken from it succeeded.
    pub fn collapse(&self, id: &str) {
        if let Some((_, holder)) = self
            .imp()
            .job_held
            .borrow()
            .iter()
            .find(|(held, _)| held == id)
        {
            holder.set_expanded(false);
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
            |row, job| self.dress_job(row, job),
        );
        imp.jobs.set_visible(!jobs.is_empty());
        imp.hero.set_subtitle(Some(summary_text(jobs.len())));
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
            |printer| printer.id.clone(),
            |_| self.build_printer_row(),
            |holder, printer| self.dress_printer(holder, printer),
        );
        imp.printers.set_visible(!printers.is_empty());
    }

    /// A printer's whole row opens its card of facts; a printer with none has no chevron and no
    /// card, and its row does nothing.
    fn build_printer_row(&self) -> Expandable {
        Expandable::new(&opener_row(gettext("Show this printer's details")))
    }

    fn dress_printer(&self, holder: &Expandable, printer: &Printer) {
        let Some(row) = holder.head::<LegacyRow>() else {
            return;
        };
        row.set_title(none_if_empty(&printer.name));
        row.set_lead_icon(Some(match printer.network {
            true => "printer-network-symbolic",
            false => "printer-symbolic",
        }));
        let problem = printer.warning.then_some(printer.status.as_str());
        row.set_subtitle(problem.and_then(none_if_empty));
        crate::set_css_class(&row, WARNING, printer.warning);
        let status = (!printer.warning && !printer.status.is_empty())
            .then(|| Fact::new(gettext("Status"), printer.status.clone()));
        let facts: Vec<Fact> = status
            .into_iter()
            .chain(printer.facts.iter().cloned())
            .collect();
        let carded = !facts.is_empty();
        set_opens(&row, carded);
        if !carded {
            holder.set_details(None::<&gtk4::Widget>);
            return;
        }
        let card = holder.details::<FactList>().unwrap_or_else(|| {
            let card = FactList::new();
            holder.set_details(Some(&card));
            card
        });
        card.set_facts(&facts);
    }

    /// A job's whole row opens its card, the same grammar as every other popover's card: first
    /// where and how it is printing, then Pause, Resume and Cancel. The row body does nothing else,
    /// so it is the opener.
    fn build_job_row(&self, job: &Job) -> Expandable {
        let holder = Expandable::new(&opener_row(gettext("Show what can be done with this job")));
        let panel = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        panel.append(&FactList::new());
        panel.append(&self.build_action_row(&job.id, gettext("Pause"), "paused"));
        panel.append(&self.build_action_row(&job.id, gettext("Resume"), "resumed"));
        let cancel = self.build_action_row(&job.id, gettext("Cancel"), "cancelled");
        cancel.add_css_class(crate::DESTRUCTIVE);
        panel.append(&cancel);
        holder.set_details(Some(&panel));
        holder
    }

    /// An action in the card is a `$LegacyRow`, the same as every other list entry in this shell — an
    /// icon button would be a second grammar for the same thing.
    fn build_action_row(&self, id: &str, label: String, signal: &'static str) -> LegacyRow {
        let row = LegacyRow::new();
        row.set_title(Some(label.as_str()));
        row.set_activatable(true);
        let key = id.to_owned();
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>(signal, &[&key])
        ));
        row
    }

    fn dress_job(&self, holder: &Expandable, job: &Job) {
        let Some(row) = holder.head::<LegacyRow>() else {
            return;
        };
        row.set_title(none_if_empty(&job.name));
        row.set_busy(job.busy && job.progress.is_none());
        set_opens(&row, true);

        let Some(panel) = holder.details::<gtk4::Box>() else {
            return;
        };
        let Some(facts) = panel.first_child().and_downcast::<FactList>() else {
            return;
        };
        facts.set_facts(&[
            Fact::new(gettext("Printer"), job.printer.clone()),
            Fact::new(gettext("Status"), job_status(job)),
        ]);
        let shown = [job.pausable, job.resumable, job.cancellable];
        let mut child = facts.next_sibling();
        for visible in shown {
            let Some(row) = child else {
                break;
            };
            child = row.next_sibling();
            row.set_visible(visible);
        }
    }
}

const WARNING: &str = "row--warning";

fn opener_row(tooltip: String) -> LegacyRow {
    let row = LegacyRow::new();
    let chevron = gtk4::Image::from_icon_name("go-next-symbolic");
    chevron.set_accessible_role(gtk4::AccessibleRole::Presentation);
    chevron.add_css_class("drawer-chevron");
    chevron.set_tooltip_text(Some(&tooltip));
    row.set_trail(&chevron);
    row
}

/// A row with a card to open is the opener; one without is a plain line of text with no chevron
/// and nothing under the pointer.
fn set_opens(row: &LegacyRow, opens: bool) {
    row.set_activatable(opens);
    if let Some(chevron) = row.trail()
        && chevron.get_visible() != opens
    {
        chevron.set_visible(opens);
    }
}

fn summary_text(count: usize) -> String {
    if count == 0 {
        return gettext("No print jobs");
    }
    ngettext("{count} print job", "{count} print jobs", count as u32)
        .replace("{count}", &count.to_string())
}

/// A job's status line reads its status word, or "Page {n} of {m}" while the driver reports page
/// progress — the progress text replaces the status word rather than sitting beside it.
fn job_status(job: &Job) -> String {
    match job.progress {
        Some((completed, total)) => progress_text(completed, total),
        None => job.status.clone(),
    }
}

fn progress_text(completed: u32, total: u32) -> String {
    gettext("Page {completed} of {total}")
        .replace("{completed}", &completed.to_string())
        .replace("{total}", &total.to_string())
}

#[cfg(test)]
mod tests;
