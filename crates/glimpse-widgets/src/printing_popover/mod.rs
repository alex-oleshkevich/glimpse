mod imp;

use gettextrs::{gettext, ngettext};
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Expandable, Row, none_if_empty, reconcile};

pub use imp::{Detail, Job, Printer};

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
        let Some(row) = holder.head::<Row>() else {
            return;
        };
        row.set_title(none_if_empty(&printer.name));
        row.set_subtitle(none_if_empty(&printer.status));
        crate::set_css_class(&row, WARNING, printer.warning);
        row.set_lead_icon(Some(match printer.network {
            true => "printer-network-symbolic",
            false => "printer-symbolic",
        }));
        let carded = !printer.details.is_empty();
        set_opens(&row, carded);
        if !carded {
            holder.set_details(None::<&gtk4::Widget>);
            return;
        }
        let card = holder.details::<gtk4::Box>().unwrap_or_else(|| {
            let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            holder.set_details(Some(&card));
            card
        });
        let mut lines = self.imp().printer_lines.borrow_mut();
        reconcile::by_key(
            &card,
            lines.entry(printer.id.clone()).or_default(),
            &printer.details,
            |detail| detail.label.clone(),
            |_| Row::new(),
            dress_detail,
        );
    }

    /// A job's whole row opens its card of Pause, Resume and Cancel, the same grammar as every other
    /// popover's card: the row body does nothing else, so it is the opener. A job that can do none
    /// of the three has no chevron and no card.
    fn build_job_row(&self, job: &Job) -> Expandable {
        let holder = Expandable::new(&opener_row(gettext("Show what can be done with this job")));
        let panel = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        panel.append(&self.build_action_row(&job.id, gettext("Pause"), "paused"));
        panel.append(&self.build_action_row(&job.id, gettext("Resume"), "resumed"));
        let cancel = self.build_action_row(&job.id, gettext("Cancel"), "cancelled");
        cancel.add_css_class(DESTRUCTIVE);
        panel.append(&cancel);
        holder.set_details(Some(&panel));
        holder
    }

    /// An action in the card is a `$Row`, the same as every other list entry in this shell — an
    /// icon button would be a second grammar for the same thing.
    fn build_action_row(&self, id: &str, label: String, signal: &'static str) -> Row {
        let row = Row::new();
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
        let Some(row) = holder.head::<Row>() else {
            return;
        };
        row.set_title(none_if_empty(&job.name));
        row.set_subtitle(none_if_empty(&job_subtitle(job)));
        row.set_busy(job.busy && job.progress.is_none());

        let actionable = job.cancellable || job.pausable || job.resumable;
        set_opens(&row, actionable);
        if !actionable {
            holder.set_expanded(false);
        }
        let Some(panel) = holder.details::<gtk4::Box>() else {
            return;
        };
        let shown = [job.pausable, job.resumable, job.cancellable];
        let mut child = panel.first_child();
        for visible in shown {
            let Some(row) = child else {
                break;
            };
            child = row.next_sibling();
            row.set_visible(visible);
        }
    }
}

const WARNING: &str = "printing-popover__printer--warning";
const DESTRUCTIVE: &str = "row--destructive";

fn opener_row(tooltip: String) -> Row {
    let row = Row::new();
    let chevron = gtk4::Image::from_icon_name("go-next-symbolic");
    chevron.set_accessible_role(gtk4::AccessibleRole::Presentation);
    chevron.add_css_class("drawer-chevron");
    chevron.set_tooltip_text(Some(&tooltip));
    row.set_trail(&chevron);
    row
}

/// A row with a card to open is the opener; one without is a plain line of text with no chevron
/// and nothing under the pointer.
fn set_opens(row: &Row, opens: bool) {
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

fn dress_detail(row: &Row, detail: &Detail) {
    row.set_activatable(false);
    row.set_title(none_if_empty(&detail.label));
    row.set_value(none_if_empty(&detail.value));
}

/// The second line reads "{printer} · {status}" normally, and "{printer} · Page {n} of {m}" while
/// the driver reports page progress — the progress text replaces the status word rather than
/// sitting beside it, so a job's trailing area holds only its action buttons.
fn job_subtitle(job: &Job) -> String {
    let status = match job.progress {
        Some((completed, total)) => progress_text(completed, total),
        None => job.status.clone(),
    };
    gettext("{printer} · {status}")
        .replace("{printer}", &job.printer)
        .replace("{status}", &status)
}

fn progress_text(completed: u32, total: u32) -> String {
    gettext("Page {completed} of {total}")
        .replace("{completed}", &completed.to_string())
        .replace("{total}", &total.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn job(id: &str, progress: Option<(u32, u32)>) -> Job {
        Job {
            id: id.to_owned(),
            name: "specs-004-panel.pdf".into(),
            printer: "HP LaserJet 400".into(),
            status: "Printing".into(),
            progress,
            busy: true,
            cancellable: true,
            pausable: false,
            resumable: false,
        }
    }

    fn job_holder(popover: &PrintingPopover, id: &str) -> Expandable {
        popover
            .imp()
            .job_held
            .borrow()
            .iter()
            .find(|(held, _)| held == id)
            .map(|(_, holder)| holder.clone())
            .expect("a held job")
    }

    fn actions(holder: &Expandable) -> Vec<Row> {
        let panel = holder
            .details::<gtk4::Box>()
            .expect("a job carries its actions");
        let mut rows = Vec::new();
        let mut child = panel.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Ok(row) = widget.downcast::<Row>() {
                rows.push(row);
            }
        }
        rows
    }

    #[test]
    #[ignore = "needs a display"]
    fn printing_popover_states() {
        if gtk4::init().is_err() {
            return;
        }
        crate::register_resources().expect("resources");

        let popover = PrintingPopover::new();
        let imp = popover.imp();

        assert!(
            !imp.jobs.get_visible(),
            "no jobs is no section at all; the header already says so"
        );
        assert_eq!(imp.hero.title().as_deref(), Some("Printing"));
        assert_eq!(imp.hero.subtitle().as_deref(), Some("No print jobs"));

        popover.set_jobs(&[job("1", Some((3, 12)))]);
        assert!(imp.jobs.get_visible());
        assert_eq!(imp.hero.subtitle().as_deref(), Some("1 print job"));
        let holder = job_holder(&popover, "1");
        let row = holder.head::<Row>().expect("a job row");
        assert_eq!(
            row.subtitle().as_deref(),
            Some("HP LaserJet 400 · Page 3 of 12"),
            "progress replaces the status word on the second line rather than sitting beside it"
        );
        assert!(
            row.activatable(),
            "a job with something to do opens its card from the whole row"
        );

        let renders_after_first = imp.renders.get();
        popover.set_jobs(&[job("1", Some((3, 12)))]);
        assert_eq!(
            imp.renders.get(),
            renders_after_first,
            "an unchanged job list must never re-enter render_jobs at all"
        );
        assert_eq!(
            job_holder(&popover, "1"),
            holder,
            "an unchanged job list reuses its row rather than rebuilding it"
        );

        row.emit_clicked();
        assert!(holder.expanded(), "the row opens the job's card");
        let [pause, resume, cancel] =
            <[Row; 3]>::try_from(actions(&holder)).expect("three actions");
        assert!(!pause.get_visible() && !resume.get_visible() && cancel.get_visible());
        assert!(
            cancel.has_css_class("row--destructive"),
            "cancelling throws the job away, so it reads as destructive"
        );

        let cancelled = Rc::new(RefCell::new(Vec::new()));
        popover.connect_cancelled({
            let cancelled = Rc::clone(&cancelled);
            move |_, id| cancelled.borrow_mut().push(id.to_owned())
        });
        cancel.emit_clicked();
        assert_eq!(*cancelled.borrow(), ["1".to_owned()]);
        popover.collapse("1");
        assert!(
            !holder.expanded(),
            "an action that succeeded closes the card it was taken from"
        );

        popover.set_jobs(&[Job {
            pausable: true,
            ..job("2", None)
        }]);
        let processing = actions(&job_holder(&popover, "2"));
        assert!(processing[0].get_visible(), "a pausable job shows Pause");
        assert!(
            !processing[1].get_visible(),
            "a job that is not resumable hides Resume"
        );
        let paused = Rc::new(RefCell::new(Vec::new()));
        popover.connect_paused({
            let paused = Rc::clone(&paused);
            move |_, id| paused.borrow_mut().push(id.to_owned())
        });
        processing[0].emit_clicked();
        assert_eq!(*paused.borrow(), ["2".to_owned()]);

        popover.set_jobs(&[Job {
            resumable: true,
            cancellable: false,
            ..job("3", None)
        }]);
        let held = actions(&job_holder(&popover, "3"));
        assert!(!held[0].get_visible(), "not pausable while held");
        assert!(held[1].get_visible(), "a resumable job shows Resume");
        assert!(
            !held[2].get_visible(),
            "a job that is not cancellable hides Cancel"
        );
        let resumed = Rc::new(RefCell::new(Vec::new()));
        popover.connect_resumed({
            let resumed = Rc::clone(&resumed);
            move |_, id| resumed.borrow_mut().push(id.to_owned())
        });
        held[1].emit_clicked();
        assert_eq!(*resumed.borrow(), ["3".to_owned()]);

        popover.set_jobs(&[Job {
            cancellable: false,
            ..job("4", None)
        }]);
        let stuck = job_holder(&popover, "4");
        let stuck_row = stuck.head::<Row>().expect("a job row");
        assert!(
            !stuck_row.activatable(),
            "a job with nothing to do is a line of text, with no card to open"
        );

        popover.set_jobs(&[
            Job {
                pausable: true,
                ..job("1", Some((3, 12)))
            },
            Job { ..job("2", None) },
            Job {
                resumable: true,
                cancellable: false,
                ..job("3", None)
            },
        ]);
        assert_eq!(
            imp.hero.subtitle().as_deref(),
            Some("3 print jobs"),
            "the summary counts every job without claiming they are all active"
        );

        popover.set_jobs(&[]);
        assert!(!imp.jobs.get_visible());
        assert!(imp.job_rows.first_child().is_none());
        assert_eq!(imp.hero.subtitle().as_deref(), Some("No print jobs"));

        let printer_renders_before = imp.printer_renders.get();
        let kitchen = Printer {
            id: "kitchen".into(),
            name: "Kitchen".into(),
            status: "Out of paper".into(),
            warning: true,
            network: true,
            details: vec![Detail {
                label: "Location".into(),
                value: "Ground floor".into(),
            }],
        };
        popover.set_printers(std::slice::from_ref(&kitchen));
        assert!(imp.printers.get_visible());
        assert_eq!(imp.printer_renders.get(), printer_renders_before + 1);
        let printer = imp
            .printer_held
            .borrow()
            .first()
            .map(|(_, holder)| holder.clone())
            .expect("a printer");
        let printer_row = printer.head::<Row>().expect("a printer row");
        assert!(
            printer_row.has_css_class("printing-popover__printer--warning"),
            "a printer's problem is said on its row, in the warning colour"
        );
        printer_row.emit_clicked();
        assert!(
            printer.expanded(),
            "a printer with a location opens its card"
        );

        let printer_renders_after = imp.printer_renders.get();
        popover.set_printers(std::slice::from_ref(&kitchen));
        assert_eq!(
            imp.printer_renders.get(),
            printer_renders_after,
            "an unchanged printer list must never re-enter render_printers at all"
        );

        popover.set_printers(&[Printer {
            details: Vec::new(),
            warning: false,
            status: "Idle".into(),
            ..kitchen
        }]);
        assert!(
            !printer.expanded() && !printer_row.activatable(),
            "a printer with nothing to show has no card, and its row does nothing"
        );

        popover.set_printers(&[]);
        assert!(!imp.printers.get_visible());
    }
}
