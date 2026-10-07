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

fn job_row(popover: &PrintingPopover, id: &str) -> ExpanderRow {
    popover
        .imp()
        .job_held
        .borrow()
        .iter()
        .find(|(held, _)| held == id)
        .map(|(_, row)| row.clone())
        .expect("a held job")
}

fn children(parent: &impl IsA<gtk4::Widget>) -> Vec<gtk4::Widget> {
    let mut all = Vec::new();
    let mut child = parent.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        all.push(widget);
    }
    all
}

fn drawer_actions(row: &ExpanderRow) -> Vec<ActionRow> {
    children(&row.details())
        .into_iter()
        .filter_map(|widget| widget.downcast::<ActionRow>().ok())
        .collect()
}

fn titles(row: &ExpanderRow) -> Vec<Option<String>> {
    drawer_actions(row)
        .iter()
        .map(|action| action.upcast_ref::<Row>().title())
        .collect()
}

fn fact_values(parent: &impl IsA<gtk4::Widget>) -> Vec<(Option<String>, Option<String>)> {
    children(parent)
        .into_iter()
        .filter_map(|widget| widget.downcast::<Row>().ok())
        .map(|row| (row.title(), row.value()))
        .collect()
}

fn printer_widget(popover: &PrintingPopover, id: &str) -> gtk4::Widget {
    popover
        .imp()
        .printer_held
        .borrow()
        .iter()
        .find(|((held, _), _)| held == id)
        .map(|(_, widget)| widget.clone())
        .expect("a held printer")
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

    assert!(!imp.jobs.get_visible(), "no jobs is no section at all");
    assert!(!imp.footer.get_visible(), "no footer until one is named");

    popover.set_footer(Some("Printer settings"));
    assert!(imp.footer.get_visible());
    let opened = Rc::new(RefCell::new(0));
    popover.connect_footer_activated({
        let opened = Rc::clone(&opened);
        move |_| *opened.borrow_mut() += 1
    });
    imp.footer.emit_by_name::<()>("activated", &[]);
    assert_eq!(*opened.borrow(), 1);
    popover.set_footer(None);
    assert!(!imp.footer.get_visible());

    popover.set_jobs(&[job("1", Some((3, 12)))]);
    assert!(imp.jobs.get_visible());
    assert_eq!(
        imp.jobs.count().as_deref(),
        Some("1"),
        "the job count is the section's count"
    );
    let row = job_row(&popover, "1");
    assert_eq!(row.title().as_deref(), Some("specs-004-panel.pdf"));
    assert!(row.subtitle().is_none(), "a job row is one line");
    assert!(row.busy(), "a job in flight shows the spinner");
    assert!(row.icon_name().is_none(), "a job row carries no icon");
    assert!(
        imp.shell.icon_name().is_none(),
        "the popover header carries no icon"
    );

    let renders_after_first = imp.renders.get();
    popover.set_jobs(&[job("1", Some((3, 12)))]);
    assert_eq!(
        imp.renders.get(),
        renders_after_first,
        "an unchanged job list must never re-enter render_jobs at all"
    );
    assert_eq!(
        job_row(&popover, "1"),
        row,
        "an unchanged job list reuses its row rather than rebuilding it"
    );

    let drawer = children(&row.details());
    assert_eq!(drawer.len(), 4, "facts, Pause, Resume, Cancel");
    let facts = drawer[0]
        .clone()
        .downcast::<gtk4::Box>()
        .expect("the facts lead the drawer");
    assert_eq!(
        fact_values(&facts),
        [
            (
                Some("Printer".to_owned()),
                Some("HP LaserJet 400".to_owned())
            ),
            (Some("Status".to_owned()), Some("Page 3 of 12".to_owned())),
        ],
        "progress replaces the status word rather than sitting beside it"
    );
    assert_eq!(
        titles(&row),
        [
            Some("Pause".to_owned()),
            Some("Resume".to_owned()),
            Some("Cancel".to_owned())
        ],
        "info first, then Pause and Resume, then the destructive Cancel last"
    );
    let [pause, resume, cancel] =
        <[ActionRow; 3]>::try_from(drawer_actions(&row)).expect("three actions");
    assert!(!pause.get_visible() && !resume.get_visible() && cancel.get_visible());
    assert_eq!(
        cancel.upcast_ref::<Row>().tone(),
        RowTone::Destructive,
        "cancelling throws the job away, so it reads as destructive"
    );

    let cancelled = Rc::new(RefCell::new(Vec::new()));
    popover.connect_cancelled({
        let cancelled = Rc::clone(&cancelled);
        move |_, id| cancelled.borrow_mut().push(id.to_owned())
    });
    row.set_open(true);
    cancel.emit_by_name::<()>("activated", &[]);
    assert_eq!(*cancelled.borrow(), ["1".to_owned()]);
    popover.collapse("1");
    assert!(
        !row.open(),
        "an action that succeeded closes the drawer it was taken from"
    );

    popover.set_jobs(&[Job {
        pausable: true,
        ..job("2", None)
    }]);
    let processing = drawer_actions(&job_row(&popover, "2"));
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
    processing[0].emit_by_name::<()>("activated", &[]);
    assert_eq!(*paused.borrow(), ["2".to_owned()]);

    popover.set_jobs(&[Job {
        resumable: true,
        cancellable: false,
        busy: false,
        ..job("3", None)
    }]);
    let held_row = job_row(&popover, "3");
    assert!(!held_row.busy(), "a held job is not spinning");
    let held = drawer_actions(&held_row);
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
    held[1].emit_by_name::<()>("activated", &[]);
    assert_eq!(*resumed.borrow(), ["3".to_owned()]);

    popover.set_jobs(&[job("1", None), job("2", None), job("3", None)]);
    assert_eq!(imp.jobs.count().as_deref(), Some("3"));

    popover.set_jobs(&[]);
    assert!(!imp.jobs.get_visible());
    assert!(imp.job_rows.first_child().is_none());

    let printer_renders_before = imp.printer_renders.get();
    let kitchen = Printer {
        id: "kitchen".into(),
        name: "Kitchen".into(),
        problem: "Out of paper".into(),
        network: false,
        facts: vec![Fact::new("Location", "Ground floor")],
    };
    popover.set_printers(std::slice::from_ref(&kitchen));
    assert!(imp.printers.get_visible());
    assert_eq!(imp.printer_renders.get(), printer_renders_before + 1);
    let printer = printer_widget(&popover, "kitchen")
        .downcast::<ExpanderRow>()
        .expect("a printer with facts opens a drawer");
    assert_eq!(
        printer.subtitle().as_deref(),
        Some("Out of paper"),
        "a printer's problem is said on its row"
    );
    assert_eq!(printer.tone(), RowTone::Warning);
    assert_eq!(printer.icon_name().as_deref(), Some("printer-symbolic"));
    let card = printer
        .details()
        .first_child()
        .and_downcast::<gtk4::Box>()
        .expect("a list of facts");
    assert_eq!(fact_values(&card).len(), 1);

    let printer_renders_after = imp.printer_renders.get();
    popover.set_printers(std::slice::from_ref(&kitchen));
    assert_eq!(
        imp.printer_renders.get(),
        printer_renders_after,
        "an unchanged printer list must never re-enter render_printers at all"
    );

    popover.set_printers(&[Printer {
        network: true,
        ..kitchen.clone()
    }]);
    assert_eq!(
        printer.icon_name().as_deref(),
        Some("printer-network-symbolic"),
        "a network printer wears the network glyph"
    );

    popover.set_printers(&[Printer {
        facts: Vec::new(),
        problem: String::new(),
        ..kitchen
    }]);
    let plain = printer_widget(&popover, "kitchen")
        .downcast::<Row>()
        .expect("a printer with nothing to list is a plain row");
    assert!(
        plain.subtitle().is_none() && plain.tone() == RowTone::Normal,
        "no problem, no second line"
    );
    assert_eq!(
        imp.printer_rows.observe_children().n_items(),
        1,
        "the drawer row was replaced, not kept beside the plain one"
    );

    popover.set_printers(&[Printer {
        id: "kitchen".into(),
        name: "Kitchen".into(),
        problem: String::new(),
        network: false,
        facts: vec![Fact::new("Location", "Ground floor")],
    }]);
    printer_widget(&popover, "kitchen")
        .downcast::<ExpanderRow>()
        .expect("facts arriving turn the plain row back into a drawer");
    assert_eq!(imp.printer_rows.observe_children().n_items(), 1);

    popover.set_printers(&[]);
    assert!(!imp.printers.get_visible());
}
