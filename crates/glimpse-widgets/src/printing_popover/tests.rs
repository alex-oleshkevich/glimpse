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
    rows_in(
        &holder
            .details::<gtk4::Box>()
            .expect("a job carries its actions"),
    )
}

fn rows_in(parent: &impl IsA<gtk4::Widget>) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut child = parent.first_child();
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
    assert!(row.subtitle().is_none(), "a job row is one line");
    assert!(row.activatable(), "a job opens its card from the whole row");

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
    let [pause, resume, cancel] = <[Row; 3]>::try_from(actions(&holder)).expect("three actions");
    assert!(!pause.get_visible() && !resume.get_visible() && cancel.get_visible());
    let facts = holder
        .details::<gtk4::Box>()
        .and_then(|panel| panel.first_child())
        .and_downcast::<FactList>()
        .expect("the facts lead the card");
    let [printed_on, status] = <[Row; 2]>::try_from(rows_in(&facts)).expect("two facts");
    assert_eq!(printed_on.value().as_deref(), Some("HP LaserJet 400"));
    assert_eq!(
        status.value().as_deref(),
        Some("Page 3 of 12"),
        "progress replaces the status word rather than sitting beside it"
    );
    assert!(!status.activatable(), "a fact in the card is static");
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
        stuck_row.activatable(),
        "a job with nothing to do still opens the card that says where and how it is"
    );
    assert!(actions(&stuck)[..3].iter().all(|row| !row.get_visible()));

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
        facts: vec![Fact::new("Location", "Ground floor")],
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
    assert_eq!(
        printer_row.subtitle().as_deref(),
        Some("Out of paper"),
        "a printer's problem is said on its row"
    );
    assert!(
        printer_row.has_css_class("row--warning"),
        "in the warning colour"
    );
    let card = printer.details::<FactList>().expect("a card of facts");
    assert_eq!(
        rows_in(&card).len(),
        1,
        "the card lists the location and does not repeat the problem"
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
        facts: Vec::new(),
        warning: false,
        status: String::new(),
        ..kitchen
    }]);
    assert!(
        printer_row.subtitle().is_none(),
        "no problem, no second line"
    );
    assert!(
        !printer.expanded() && !printer_row.activatable(),
        "a printer with nothing to show has no card, and its row does nothing"
    );

    popover.set_printers(&[]);
    assert!(!imp.printers.get_visible());
}
