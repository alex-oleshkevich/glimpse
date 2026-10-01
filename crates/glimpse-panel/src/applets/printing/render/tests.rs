use super::*;

fn job_at(id: u32, printer_name: &str, state: JobState) -> PrintJob {
    PrintJob {
        id,
        name: "document.pdf".to_owned(),
        printer_name: printer_name.to_owned(),
        state,
        pages_completed: None,
        pages_total: None,
    }
}

fn printer_at(name: &str, state: PrinterState) -> ServicePrinter {
    ServicePrinter {
        name: name.to_owned(),
        make_model: "Office LaserJet".to_owned(),
        state,
        state_reasons: Vec::new(),
        state_message: String::new(),
        location: String::new(),
        accepting_jobs: true,
        color: false,
        duplex: false,
        media_ready: Vec::new(),
        resolution: String::new(),
        job_count: 0,
    }
}

fn no_pending() -> HashSet<u32> {
    HashSet::new()
}

#[test]
fn no_jobs_and_no_problems_shows_no_chip() {
    assert!(chip(&PrintingState::default()).is_none());
}

#[test]
fn a_processing_job_lights_the_printing_icon() {
    let state = PrintingState {
        printers: vec![printer_at("office", PrinterState::Idle)],
        jobs: vec![job_at(1, "office", JobState::Processing)],
    };
    assert_eq!(chip(&state).unwrap(), PRINTING);
    assert!(!attention(&state));
}

#[test]
fn a_queued_job_on_a_healthy_printer_shows_the_idle_icon() {
    let state = PrintingState {
        printers: vec![printer_at("office", PrinterState::Idle)],
        jobs: vec![job_at(1, "office", JobState::Pending)],
    };
    assert_eq!(chip(&state).unwrap(), IDLE);
}

#[test]
fn a_job_stuck_behind_a_stopped_printer_asks_for_attention_but_takes_no_colour() {
    let state = PrintingState {
        printers: vec![printer_at("office", PrinterState::Stopped)],
        jobs: vec![job_at(1, "office", JobState::Pending)],
    };
    assert_eq!(
        chip(&state).unwrap(),
        IDLE,
        "attention is carried by the indicator's own attention-dot styling, never by a \
             colour baked into the icon"
    );
    assert!(attention(&state));
}

#[test]
fn a_completed_job_does_not_light_the_chip() {
    let state = PrintingState {
        printers: vec![printer_at("office", PrinterState::Idle)],
        jobs: vec![job_at(1, "office", JobState::Completed)],
    };
    assert!(chip(&state).is_none());
    assert!(tooltip(&state, None, 8).is_none());
}

#[test]
fn the_tooltip_count_never_disagrees_with_what_the_popover_actually_shows() {
    let state = PrintingState {
        printers: Vec::new(),
        jobs: (0..20)
            .map(|id| job_at(id, "office", JobState::Processing))
            .collect(),
    };
    let cap_count = 8;

    let popover_count = jobs(&state, cap_count, &no_pending()).len();
    let told = tooltip(&state, None, cap_count).expect("active jobs exist");

    assert_eq!(
        told,
        format!("{popover_count} print jobs"),
        "a chip claiming a different total than the popover it opens is a lie the popover \
             cannot correct, since it has no overflow row of its own"
    );
}

#[test]
fn a_hostile_job_name_is_capped_before_it_reaches_the_widget() {
    let hostile = "Наушники ".repeat(20);
    let mut source = job_at(1, "office", JobState::Processing);
    source.name = hostile;

    let converted = jobs(
        &PrintingState {
            printers: Vec::new(),
            jobs: vec![source],
        },
        8,
        &no_pending(),
    );

    assert_eq!(converted.len(), 1);
    assert_eq!(converted[0].name.chars().count(), NAME_CAP + 1);
    assert!(converted[0].name.ends_with('…'));
}

#[test]
fn a_hostile_printer_name_is_capped_before_it_reaches_the_widget() {
    let hostile = "Наушники ".repeat(20);
    let mut source = printer_at("office", PrinterState::Idle);
    source.name = hostile;

    let converted = printers(&PrintingState {
        printers: vec![source],
        jobs: Vec::new(),
    });

    assert_eq!(converted.len(), 1);
    assert_eq!(converted[0].name.chars().count(), NAME_CAP + 1);
    assert!(converted[0].name.ends_with('…'));
    assert_eq!(
        converted[0].id, converted[0].name,
        "the id is a reconcile key, not a display string, but it still holds the same \
             sanitization boundary every field crossing into the widget must"
    );
}

#[test]
fn a_printer_carries_no_device_type_information_so_it_is_never_marked_network() {
    let converted = printers(&PrintingState {
        printers: vec![printer_at("office", PrinterState::Idle)],
        jobs: Vec::new(),
    });
    assert!(!converted[0].network);
}

#[test]
fn the_jobs_list_is_truncated_to_the_configured_cap() {
    let state = PrintingState {
        printers: Vec::new(),
        jobs: (0..10)
            .map(|id| job_at(id, "office", JobState::Pending))
            .collect(),
    };
    assert_eq!(jobs(&state, 3, &no_pending()).len(), 3);
}

#[test]
fn progress_is_only_some_when_both_page_counts_are_known() {
    let mut source = job_at(1, "office", JobState::Processing);
    source.pages_completed = Some(3);
    let converted = jobs(
        &PrintingState {
            printers: Vec::new(),
            jobs: vec![source],
        },
        8,
        &no_pending(),
    );
    assert_eq!(
        converted[0].progress, None,
        "a page count is never fabricated from one half of the pair"
    );
}

#[test]
fn a_printer_card_holds_only_its_location() {
    let bare = printer_at("Office", PrinterState::Idle);
    assert!(
        printer_facts(&bare).is_empty(),
        "a printer with no location has no card at all"
    );

    let mut full = printer_at("Office", PrinterState::Idle);
    full.location = "Study".to_owned();
    full.color = true;
    full.duplex = true;
    full.media_ready = vec!["A4".to_owned()];
    full.resolution = "600 dpi".to_owned();
    let details = printer_facts(&full);
    assert_eq!(
        details.len(),
        1,
        "specs are nothing a person acts on from a panel"
    );
    assert_eq!(details[0].label, "Location");
    assert_eq!(details[0].value, "Study");
}

#[test]
fn a_printer_problem_is_its_status_in_the_warning_colour() {
    let idle = printer_at("Office", PrinterState::Idle);
    let calm = printer(&idle);
    assert!(!calm.warning);

    let mut jammed = printer_at("Office", PrinterState::Idle);
    jammed.state_message = "Paper jam in tray 2".to_owned();
    let row = printer(&jammed);
    assert!(row.warning);
    assert_eq!(row.status, "Paper jam in tray 2");

    let mut refusing = printer_at("Office", PrinterState::Idle);
    refusing.accepting_jobs = false;
    let row = printer(&refusing);
    assert!(row.warning);
    assert_eq!(row.status, gettext("Not accepting jobs"));

    let stopped = printer(&printer_at("Office", PrinterState::Stopped));
    assert!(
        stopped.warning,
        "a stopped queue is a problem even with no message"
    );
}

#[test]
fn cancellable_pausable_resumable_follow_the_job_state() {
    for (state, cancellable_expected, pausable_expected, resumable_expected) in [
        (JobState::Pending, true, false, false),
        (JobState::Held, true, false, true),
        (JobState::Processing, true, true, false),
        (JobState::Stopped, true, false, false),
        (JobState::Completed, false, false, false),
        (JobState::Cancelled, false, false, false),
        (JobState::Aborted, false, false, false),
    ] {
        let converted = jobs(
            &PrintingState {
                printers: Vec::new(),
                jobs: vec![job_at(1, "office", state)],
            },
            8,
            &no_pending(),
        );
        assert_eq!(converted[0].cancellable, cancellable_expected, "{state:?}");
        assert_eq!(converted[0].pausable, pausable_expected, "{state:?}");
        assert_eq!(converted[0].resumable, resumable_expected, "{state:?}");
    }
}

#[test]
fn a_job_with_a_command_in_flight_is_busy_and_offers_no_action() {
    let state = PrintingState {
        printers: Vec::new(),
        jobs: vec![job_at(1, "office", JobState::Pending)],
    };
    let mut pending = HashSet::new();
    pending.insert(1);

    let converted = jobs(&state, 8, &pending);

    assert!(
        converted[0].busy,
        "a command in flight is a spinner, not a live button"
    );
    assert!(!converted[0].cancellable);
    assert!(!converted[0].pausable);
    assert!(!converted[0].resumable);
}
