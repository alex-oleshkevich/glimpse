use super::*;

fn job_at(id: u32, printer_name: &str, state: JobState) -> PrintJob {
    PrintJob {
        id,
        name: "document.pdf".to_owned(),
        printer_name: printer_name.to_owned(),
        state,
        pages_completed: None,
        pages_total: None,
        sheets_completed: None,
        copies: None,
        sides: String::new(),
        color_mode: String::new(),
        media: String::new(),
        created: None,
        user: String::new(),
        host: String::new(),
        hold_until: String::new(),
        reason: String::new(),
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

fn viewer() -> Viewer {
    Viewer {
        user: "alex".to_owned(),
        now: DateTime::parse_from_rfc3339("2026-10-29T15:00:00+01:00").expect("a time"),
        twelve_hour: false,
    }
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

    let popover_count = jobs(&state, cap_count, &no_pending(), &viewer()).len();
    let told = tooltip(&state, Some("{count}"), cap_count).expect("active jobs exist");

    assert_eq!(
        told,
        popover_count.to_string(),
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
        &viewer(),
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
    assert_eq!(jobs(&state, 3, &no_pending(), &viewer()).len(), 3);
}

#[test]
fn pages_read_done_of_total_only_while_printing() {
    let mut source = job_at(1, "office", JobState::Processing);
    source.pages_completed = Some(3);
    source.pages_total = Some(12);
    assert_eq!(fact(source.clone(), "Pages").as_deref(), Some("3 of 12"));

    source.state = JobState::Pending;
    assert_eq!(fact(source.clone(), "Pages").as_deref(), Some("12"));

    source.state = JobState::Processing;
    source.pages_total = None;
    assert_eq!(
        fact(source, "Pages"),
        None,
        "a page count is never fabricated from one half of the pair"
    );
}

#[test]
fn the_job_facts_name_the_cups_keywords_and_drop_what_is_unknown() {
    let mut source = job_at(1, "office", JobState::Processing);
    source.copies = Some(3);
    source.sides = "two-sided-long-edge".to_owned();
    source.color_mode = "monochrome".to_owned();
    source.media = "iso_a4_210x297mm".to_owned();
    source.created = Some(
        DateTime::parse_from_rfc3339("2026-10-29T13:02:00Z")
            .expect("a time")
            .to_utc(),
    );
    assert_eq!(
        facts_of(source),
        [
            ("Printer", "office"),
            ("Status", "Printing"),
            ("Copies", "3"),
            ("Sides", "Two-sided"),
            ("Color", "Grayscale"),
            ("Paper", "A4"),
            ("Submitted", "14:02"),
        ]
        .map(|(label, value)| (label.to_owned(), value.to_owned()))
    );

    let mut plain = job_at(2, "office", JobState::Pending);
    plain.copies = Some(1);
    plain.sides = "something-new".to_owned();
    plain.color_mode = "auto".to_owned();
    assert_eq!(
        facts_of(plain).len(),
        2,
        "one copy, an unknown keyword and auto color are not facts"
    );
}

#[test]
fn paper_names_follow_the_pwg_media_name() {
    for (media, expected) in [
        ("iso_a4_210x297mm", "A4"),
        ("na_letter_8.5x11in", "Letter"),
        ("na_index-4x6_4x6in", "Index-4x6"),
        ("custom_min_3x5in", "Custom"),
        ("A4", "A4"),
        ("", ""),
    ] {
        assert_eq!(media_name(media), expected, "{media}");
    }
}

#[test]
fn a_job_from_another_day_names_the_day() {
    let mut source = job_at(1, "office", JobState::Pending);
    source.created = Some(
        DateTime::parse_from_rfc3339("2026-10-27T08:30:00Z")
            .expect("a time")
            .to_utc(),
    );
    assert_eq!(fact(source, "Submitted").as_deref(), Some("27 Oct, 09:30"));
}

#[test]
fn the_submitter_shows_only_when_it_is_someone_else_or_somewhere_else() {
    let mut mine = job_at(1, "office", JobState::Pending);
    mine.user = "alex".to_owned();
    mine.host = "localhost".to_owned();
    assert_eq!(fact(mine.clone(), "Submitted by"), None);
    assert_eq!(fact(mine, "From"), None);

    let mut theirs = job_at(2, "office", JobState::Pending);
    theirs.user = "maria".to_owned();
    theirs.host = "laptop.lan".to_owned();
    assert_eq!(
        fact(theirs.clone(), "Submitted by").as_deref(),
        Some("maria")
    );
    assert_eq!(fact(theirs, "From").as_deref(), Some("laptop.lan"));
}

#[test]
fn a_held_job_says_until_when() {
    let mut job = job_at(1, "office", JobState::Held);
    job.hold_until = "night".to_owned();
    assert_eq!(fact(job.clone(), "Held until").as_deref(), Some("Night"));

    job.hold_until = "18:00".to_owned();
    assert_eq!(
        fact(job.clone(), "Held until").as_deref(),
        Some("19:00"),
        "CUPS names the time in UTC"
    );

    job.hold_until = "indefinite".to_owned();
    assert_eq!(fact(job.clone(), "Held until"), None);

    job.hold_until = "night".to_owned();
    job.state = JobState::Pending;
    assert_eq!(
        fact(job, "Held until"),
        None,
        "a released job is no longer held"
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
fn the_job_actions_follow_the_job_state() {
    for (state, cancellable_expected, pausable_expected, resumable_expected, retryable_expected) in [
        (JobState::Pending, true, false, false, false),
        (JobState::Held, true, false, true, false),
        (JobState::Processing, true, true, false, false),
        (JobState::Stopped, true, false, false, true),
        (JobState::Completed, false, false, false, false),
        (JobState::Cancelled, false, false, false, false),
        (JobState::Aborted, false, false, false, false),
    ] {
        let converted = jobs(
            &PrintingState {
                printers: Vec::new(),
                jobs: vec![job_at(1, "office", state)],
            },
            8,
            &no_pending(),
            &viewer(),
        );
        assert_eq!(converted[0].cancellable, cancellable_expected, "{state:?}");
        assert_eq!(converted[0].pausable, pausable_expected, "{state:?}");
        assert_eq!(converted[0].resumable, resumable_expected, "{state:?}");
        assert_eq!(converted[0].retryable, retryable_expected, "{state:?}");
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

    let converted = jobs(&state, 8, &pending, &viewer());

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

fn row_of(job: PrintJob) -> PrintingJob {
    jobs(
        &PrintingState {
            printers: Vec::new(),
            jobs: vec![job],
        },
        8,
        &no_pending(),
        &viewer(),
    )
    .remove(0)
}

fn facts_of(job: PrintJob) -> Vec<(String, String)> {
    row_of(job)
        .facts
        .into_iter()
        .map(|fact| (fact.label, fact.value))
        .collect()
}

fn fact(job: PrintJob, label: &str) -> Option<String> {
    facts_of(job)
        .into_iter()
        .find(|(name, _)| name == label)
        .map(|(_, value)| value)
}

fn status_of(job: PrintJob) -> String {
    fact(job, "Status").expect("a status")
}

#[test]
fn a_stopped_job_says_why() {
    let mut job = job_at(1, "office", JobState::Stopped);
    job.reason = "Printer not connected".to_owned();
    job.pages_completed = Some(0);
    job.pages_total = Some(1);
    assert_eq!(
        fact(job.clone(), "Status").as_deref(),
        Some("Stopped: Printer not connected")
    );
    assert_eq!(fact(job, "Pages").as_deref(), Some("1"));
}

#[test]
fn a_running_job_keeps_its_plain_status_whatever_the_reason() {
    let mut job = job_at(1, "office", JobState::Pending);
    job.reason = "job-incoming".to_owned();
    assert_eq!(status_of(job), "Queued");
}

#[test]
fn a_hostile_job_reason_is_capped() {
    let mut job = job_at(1, "office", JobState::Held);
    job.reason = "ж".repeat(4096);
    let status = status_of(job);
    assert_eq!(status.chars().count(), "Held: ".len() + NAME_CAP + 1);
    assert!(status.ends_with('…'));
}

#[test]
fn the_default_tooltip_names_the_document_printing_and_drops_unknown_pages() {
    let mut printing = job_at(2, "office", JobState::Processing);
    printing.name = "invoice.pdf".to_owned();
    let state = PrintingState {
        printers: Vec::new(),
        jobs: vec![job_at(1, "office", JobState::Pending), printing],
    };
    assert_eq!(
        tooltip(&state, None, 8).as_deref(),
        Some("invoice.pdf\noffice")
    );
}

#[test]
fn the_default_tooltip_is_the_format_the_schema_documents() {
    assert_eq!(
        Some(default_tooltip().as_str()),
        Some(glimpse_config::PRINTING_TOOLTIP)
    );
}
