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
        device_uri: String::new(),
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
    assert!(chip(&PrintingState::default(), PrintingShow::Jobs).is_none());
}

#[test]
fn a_processing_job_lights_the_printing_icon() {
    let state = PrintingState {
        printers: vec![printer_at("office", PrinterState::Idle)],
        jobs: vec![job_at(1, "office", JobState::Processing)],
    };
    assert_eq!(chip(&state, PrintingShow::Jobs).unwrap(), PRINTING);
    assert!(!attention(&state));
}

#[test]
fn a_queued_job_on_a_healthy_printer_shows_the_idle_icon() {
    let state = PrintingState {
        printers: vec![printer_at("office", PrinterState::Idle)],
        jobs: vec![job_at(1, "office", JobState::Pending)],
    };
    assert_eq!(chip(&state, PrintingShow::Jobs).unwrap(), IDLE);
}

#[test]
fn a_job_stuck_behind_a_stopped_printer_asks_for_attention_but_takes_no_colour() {
    let state = PrintingState {
        printers: vec![printer_at("office", PrinterState::Stopped)],
        jobs: vec![job_at(1, "office", JobState::Pending)],
    };
    assert_eq!(
        chip(&state, PrintingShow::Jobs).unwrap(),
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
    assert!(chip(&state, PrintingShow::Jobs).is_none());
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
fn the_device_uri_scheme_decides_whether_a_printer_is_network() {
    for (uri, network) in [
        ("ipp://192.168.1.40/ipp/print", true),
        ("ipps://kitchen.local:631/ipp/print", true),
        ("http://printer.local/ipp", true),
        ("https://printer.local/ipp", true),
        ("socket://192.168.1.40:9100", true),
        ("lpd://192.168.1.40/queue", true),
        ("dnssd://Kitchen._ipp._tcp.local/", true),
        ("IPP://upper.case/", true),
        ("implicitclass://Kitchen/", true),
        ("smb://server/queue", true),
        ("hp:/net/LaserJet?ip=192.168.1.40", true),
        ("usb://HP/LaserJet?serial=1", false),
        ("hp:/usb/LaserJet?serial=1", false),
        ("parallel:/dev/lp0", false),
        ("serial:/dev/ttyS0", false),
        ("file:///dev/null", false),
        ("ippfoo://not.a.scheme/", false),
        ("", false),
        ("no-scheme-at-all", false),
    ] {
        assert_eq!(is_network(uri), network, "{uri}");
    }
}

#[test]
fn a_printer_is_network_by_its_device_uri() {
    let mut source = printer_at("office", PrinterState::Idle);
    source.device_uri = "ipp://office.local/ipp/print".to_owned();
    let converted = printers(&PrintingState {
        printers: vec![source],
        jobs: Vec::new(),
    });
    assert!(converted[0].network);
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

fn labels(printer: &PrintingPrinter) -> Vec<(String, String)> {
    printer
        .facts
        .iter()
        .map(|fact| (fact.label.clone(), fact.value.clone()))
        .collect()
}

#[test]
fn a_printer_with_no_model_or_location_has_no_facts() {
    let mut bare = printer_at("Office", PrinterState::Idle);
    bare.make_model = String::new();
    assert!(printer(&bare).facts.is_empty());
}

#[test]
fn a_printer_lists_its_model_then_its_location() {
    let mut full = printer_at("Office", PrinterState::Idle);
    full.location = "Study".to_owned();
    full.color = true;
    full.resolution = "600 dpi".to_owned();
    assert_eq!(
        labels(&printer(&full)),
        [
            (gettext("Model"), "Office LaserJet".to_owned()),
            (gettext("Location"), "Study".to_owned()),
        ],
        "specs are nothing a person acts on from a panel"
    );
}

#[test]
fn status_is_a_fact_only_for_a_printer_that_is_not_idle() {
    let mut bare = printer_at("Office", PrinterState::Idle);
    bare.make_model = String::new();
    assert!(printer(&bare).facts.is_empty(), "idle says nothing");

    bare.state = PrinterState::Processing;
    assert_eq!(
        labels(&printer(&bare)),
        [(gettext("Status"), gettext("Printing"))],
        "a printer at work says so, last"
    );

    let mut full = printer_at("Office", PrinterState::Processing);
    full.location = "Study".to_owned();
    let facts = labels(&printer(&full));
    assert_eq!(facts.last().map(|fact| fact.0.as_str()), Some("Status"));

    let mut stopped = printer_at("Office", PrinterState::Stopped);
    stopped.make_model = String::new();
    assert!(
        printer(&stopped).facts.is_empty(),
        "a problem is the row's second line, never repeated as a fact"
    );
}

#[test]
fn a_printer_problem_is_its_second_line() {
    let calm = printer(&printer_at("Office", PrinterState::Idle));
    assert!(calm.problem.is_empty());

    let mut jammed = printer_at("Office", PrinterState::Idle);
    jammed.state_message = "Paper jam in tray 2".to_owned();
    assert_eq!(printer(&jammed).problem, "Paper jam in tray 2");

    let mut refusing = printer_at("Office", PrinterState::Idle);
    refusing.accepting_jobs = false;
    assert_eq!(printer(&refusing).problem, gettext("Not accepting jobs"));

    let stopped = printer(&printer_at("Office", PrinterState::Stopped));
    assert_eq!(
        stopped.problem,
        gettext("Stopped"),
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

fn idle_state(jobs: Vec<PrintJob>) -> PrintingState {
    PrintingState {
        printers: vec![printer_at("office", PrinterState::Idle)],
        jobs,
    }
}

#[test]
fn always_shows_the_idle_icon_for_an_idle_printer() {
    let state = idle_state(Vec::new());
    assert_eq!(chip(&state, PrintingShow::Always), Some(IDLE));
    assert_eq!(chip(&state, PrintingShow::Jobs), None);
}

#[test]
fn always_hides_the_chip_with_no_printers() {
    let state = PrintingState::default();
    assert_eq!(chip(&state, PrintingShow::Always), None);
    assert_eq!(chip(&state, PrintingShow::Jobs), None);
}

#[test]
fn always_keeps_the_job_icons() {
    let queued = idle_state(vec![job_at(1, "office", JobState::Pending)]);
    assert_eq!(chip(&queued, PrintingShow::Always), Some(IDLE));
    let processing = idle_state(vec![job_at(1, "office", JobState::Processing)]);
    assert_eq!(chip(&processing, PrintingShow::Always), Some(PRINTING));
    assert_eq!(chip(&processing, PrintingShow::Jobs), Some(PRINTING));
}

#[test]
fn a_completed_job_leaves_always_on_the_idle_icon() {
    let state = idle_state(vec![job_at(1, "office", JobState::Completed)]);
    assert_eq!(chip(&state, PrintingShow::Always), Some(IDLE));
    assert_eq!(chip(&state, PrintingShow::Jobs), None);
}
