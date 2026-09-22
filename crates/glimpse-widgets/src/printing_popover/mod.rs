mod imp;

use gettextrs::{gettext, ngettext};
use gtk4::{accessible, glib, prelude::*, subclass::prelude::*};

use crate::{Row, SplitRow, none_if_empty, reconcile};

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
        imp.jobs.set_empty(jobs.is_empty());
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
            |_| Row::new(),
            dress_printer,
        );
        imp.printers.set_visible(!printers.is_empty());
    }

    /// A job is a `$SplitRow` head over its own `Gtk.Revealer`, exactly as `BluetoothPopover`
    /// builds a device: the head carries the name and a spinner, the chevron opens the panel, and
    /// the pause/resume/cancel buttons live in the panel rather than in the row.
    ///
    /// They cannot live in the row. `Row` is a `Gtk.Button`, so a button placed inside it is a
    /// button inside a button — the outer gesture claims the press and the inner one never emits
    /// `clicked`. `SplitRow` exists for exactly this reason and keeps its own control a sibling.
    fn build_job_row(&self, job: &Job) -> gtk4::Box {
        let split = SplitRow::new();
        split.set_detail_icon("go-next-symbolic".to_owned());
        split.set_detail_tooltip(Some(gettext("Show what can be done with this job")));

        let holder = crate::drawer::holder(&split);

        let panel = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        panel.add_css_class("job-actions");
        let pause = action_button("media-playback-pause-symbolic", gettext("Pause this job"));
        let resume = action_button("media-playback-start-symbolic", gettext("Resume this job"));
        let cancel = action_button("window-close-symbolic", gettext("Cancel this job"));
        self.connect_action(&pause, &job.id, "paused");
        self.connect_action(&resume, &job.id, "resumed");
        self.connect_action(&cancel, &job.id, "cancelled");
        panel.append(&pause);
        panel.append(&resume);
        panel.append(&cancel);

        if let Some(drawer) = crate::drawer::panel(&holder) {
            drawer.set_child(Some(&panel));
            split.connect_details(move |_| crate::drawer::toggle(&drawer));
        }

        holder
    }

    fn connect_action(&self, button: &gtk4::Button, id: &str, signal: &'static str) {
        let key = id.to_owned();
        button.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>(signal, &[&key])
        ));
    }

    /// The head body activates nothing — every action is in the panel below — so the inner `Row`
    /// stays non-activatable and the chevron is the only target.
    fn dress_job(&self, holder: &gtk4::Box, job: &Job) {
        let Some(split) = crate::drawer::head::<SplitRow>(holder) else {
            return;
        };
        let row = split.row();
        row.set_title(none_if_empty(&job.name));
        row.set_subtitle(none_if_empty(&job_subtitle(job)));
        row.set_busy(job.busy && job.progress.is_none());
        row.set_activatable(false);

        let actionable = job.cancellable || job.pausable || job.resumable;
        split.detail().set_visible(actionable);

        let Some(drawer) = crate::drawer::panel(holder) else {
            return;
        };
        if !actionable {
            crate::drawer::set(&drawer, false);
        }
        let Some(panel) = drawer.child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let pause = panel.first_child();
        let resume = pause
            .as_ref()
            .and_then(gtk4::prelude::WidgetExt::next_sibling);
        let cancel = resume
            .as_ref()
            .and_then(gtk4::prelude::WidgetExt::next_sibling);
        if let Some(button) = pause.and_downcast::<gtk4::Button>() {
            button.set_visible(job.pausable);
        }
        if let Some(button) = resume.and_downcast::<gtk4::Button>() {
            button.set_visible(job.resumable);
        }
        if let Some(button) = cancel.and_downcast::<gtk4::Button>() {
            button.set_visible(job.cancellable);
        }
    }
}

fn action_button(icon: &str, tooltip: String) -> gtk4::Button {
    let button = gtk4::Button::from_icon_name(icon);
    button.set_has_frame(false);
    button.add_css_class("mute");
    button.set_valign(gtk4::Align::Center);
    button.set_tooltip_text(Some(&tooltip));
    button.update_property(&[accessible::Property::Label(&tooltip)]);
    button
}

fn summary_text(count: usize) -> String {
    if count == 0 {
        return gettext("No print jobs");
    }
    ngettext("{count} print job", "{count} print jobs", count as u32)
        .replace("{count}", &count.to_string())
}

fn dress_printer(row: &Row, printer: &Printer) {
    row.set_activatable(false);
    row.set_title(none_if_empty(&printer.name));
    row.set_subtitle(none_if_empty(&printer.status));
    let icon = match printer.network {
        true => "printer-network-symbolic",
        false => "printer-symbolic",
    };
    row.set_lead_icon(Some(icon));
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

    fn trail_button(row: &Row, index: usize) -> gtk4::Button {
        let actions = row
            .trail()
            .and_downcast::<gtk4::Box>()
            .expect("actions box");
        let mut child = actions.first_child();
        for _ in 0..index {
            child = child.and_then(|widget| widget.next_sibling());
        }
        child.and_downcast::<gtk4::Button>().expect("trail button")
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
            imp.empty_jobs.is_visible(),
            "an untouched popover shows the empty state"
        );
        assert!(!imp.job_rows.is_visible());
        assert_eq!(imp.hero.subtitle().as_deref(), Some("No print jobs"));

        popover.set_jobs(&[job("1", Some((3, 12)))]);
        assert!(!imp.empty_jobs.is_visible());
        assert!(imp.job_rows.is_visible());
        assert_eq!(imp.hero.subtitle().as_deref(), Some("1 print job"));
        let row = imp
            .job_rows
            .first_child()
            .and_downcast::<Row>()
            .expect("job row");
        assert_eq!(
            row.subtitle().as_deref(),
            Some("HP LaserJet 400 · Page 3 of 12"),
            "progress replaces the status word on the second line rather than sitting beside it"
        );
        assert_eq!(
            row.value(),
            None,
            "the value slot is not used for job rows; actions live in the trail instead"
        );
        assert!(
            row.activatable(),
            "a row carrying a cancel button must stay targetable, or the button is unreachable"
        );

        let renders_after_first = imp.renders.get();
        popover.set_jobs(&[job("1", Some((3, 12)))]);
        assert_eq!(
            imp.renders.get(),
            renders_after_first,
            "an unchanged job list must never re-enter render_jobs at all"
        );
        let same = imp
            .job_rows
            .first_child()
            .and_downcast::<Row>()
            .expect("job row");
        assert_eq!(
            row, same,
            "an unchanged job list reuses its row rather than rebuilding it"
        );

        let cancelled = Rc::new(RefCell::new(Vec::new()));
        popover.connect_cancelled({
            let cancelled = Rc::clone(&cancelled);
            move |_, id| cancelled.borrow_mut().push(id.to_owned())
        });
        trail_button(&row, 2).emit_clicked();
        assert_eq!(
            *cancelled.borrow(),
            ["1".to_owned()],
            "clicking the trailing button must reach it and report the row's own id"
        );
        assert!(
            row.activatable(),
            "cancelling must not leave the row unable to report a future click"
        );

        popover.set_jobs(&[Job {
            pausable: true,
            ..job("2", None)
        }]);
        let processing = imp
            .job_rows
            .first_child()
            .and_downcast::<Row>()
            .expect("job row");
        assert!(
            trail_button(&processing, 0).is_visible(),
            "a pausable job shows Pause"
        );
        assert!(
            !trail_button(&processing, 1).is_visible(),
            "a job that is not resumable hides Resume"
        );

        let paused = Rc::new(RefCell::new(Vec::new()));
        popover.connect_paused({
            let paused = Rc::clone(&paused);
            move |_, id| paused.borrow_mut().push(id.to_owned())
        });
        trail_button(&processing, 0).emit_clicked();
        assert_eq!(*paused.borrow(), ["2".to_owned()]);

        popover.set_jobs(&[Job {
            resumable: true,
            cancellable: false,
            ..job("3", None)
        }]);
        let held = imp
            .job_rows
            .first_child()
            .and_downcast::<Row>()
            .expect("job row");
        assert!(
            !trail_button(&held, 0).is_visible(),
            "not pausable while held"
        );
        assert!(
            trail_button(&held, 1).is_visible(),
            "a resumable job shows Resume"
        );
        assert!(
            !trail_button(&held, 2).is_visible(),
            "a job that is not cancellable hides Cancel"
        );

        let resumed = Rc::new(RefCell::new(Vec::new()));
        popover.connect_resumed({
            let resumed = Rc::clone(&resumed);
            move |_, id| resumed.borrow_mut().push(id.to_owned())
        });
        trail_button(&held, 1).emit_clicked();
        assert_eq!(*resumed.borrow(), ["3".to_owned()]);

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
        assert!(imp.empty_jobs.is_visible());
        assert!(imp.job_rows.first_child().is_none());
        assert_eq!(imp.hero.subtitle().as_deref(), Some("No print jobs"));

        let printer_renders_before = imp.printer_renders.get();
        popover.set_printers(&[Printer {
            id: "kitchen".into(),
            name: "Kitchen".into(),
            status: "Idle".into(),
            network: true,
        }]);
        assert!(imp.printers.get_visible());
        assert_eq!(imp.printer_renders.get(), printer_renders_before + 1);

        let printer_renders_after = imp.printer_renders.get();
        popover.set_printers(&[Printer {
            id: "kitchen".into(),
            name: "Kitchen".into(),
            status: "Idle".into(),
            network: true,
        }]);
        assert_eq!(
            imp.printer_renders.get(),
            printer_renders_after,
            "an unchanged printer list must never re-enter render_printers at all"
        );

        popover.set_printers(&[]);
        assert!(!imp.printers.get_visible());
    }
}
