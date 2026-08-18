use glimpse_core::services::printing::{JobState, PrintJob, PrinterState, State};

pub fn has_errors(state: &State) -> bool {
    state
        .printers
        .iter()
        .any(|p| p.state_reasons.iter().any(|r| !is_informational_reason(r)))
}

/// IPP/CUPS convention: reason keywords carrying "-warning"/"-report"
/// severity (or the two well-known bare supply-level reasons) are
/// informational, not blocking errors - a low toner warning shouldn't pin
/// the error icon and force the applet visible forever in auto mode.
fn is_informational_reason(reason: &str) -> bool {
    reason.ends_with("-warning")
        || reason.ends_with("-report")
        || matches!(reason, "media-low" | "toner-low")
}

pub fn label(state: &State) -> String {
    if state.jobs.is_empty() {
        String::new()
    } else {
        state.jobs.len().to_string()
    }
}

pub fn tooltip(state: &State) -> String {
    let errors: Vec<String> = state
        .printers
        .iter()
        .flat_map(|p| p.state_reasons.iter().map(|r| state_reason_text(r)))
        .collect();

    match (state.jobs.len(), errors.as_slice()) {
        (0, []) => "No print jobs".into(),
        (0, reasons) => reasons.join(", "),
        (1, []) => "1 job active".into(),
        (n, []) => format!("{n} jobs active"),
        (1, reasons) => format!("1 job active — {}", reasons.join(", ")),
        (n, reasons) => format!("{n} jobs active — {}", reasons.join(", ")),
    }
}

pub fn job_state_text(state: &JobState) -> &'static str {
    match state {
        JobState::Pending => "Queued",
        JobState::Held => "Paused",
        JobState::Processing => "Printing",
        JobState::Stopped => "Stopped",
        JobState::Completed => "Completed",
        JobState::Cancelled => "Cancelled",
        JobState::Aborted => "Failed",
    }
}

pub fn printer_state_text(state: &PrinterState) -> &'static str {
    match state {
        PrinterState::Idle => "Ready",
        PrinterState::Processing => "Printing",
        PrinterState::Stopped => "Stopped",
    }
}

pub fn state_reason_text(reason: &str) -> String {
    match reason {
        "paper-jam" => "Paper jam".into(),
        "media-empty" => "Out of paper".into(),
        "media-low" => "Paper running low".into(),
        "toner-empty" => "Out of toner".into(),
        "marker-supply-empty-error" => "Out of toner".into(),
        "toner-low" => "Toner running low".into(),
        "marker-supply-low-report" => "Toner running low".into(),
        "cover-open" => "Cover open".into(),
        "door-open" => "Door open".into(),
        "offline" => "Offline".into(),
        "offline-report" => "Offline".into(),
        "shutdown" => "Offline".into(),
        other => title_case(other),
    }
}

pub fn page_progress(job: &PrintJob) -> Option<String> {
    let completed = job.pages_completed?;
    let total = job.pages_total?;
    Some(format!("Page {completed} of {total}"))
}

fn title_case(s: &str) -> String {
    s.replace('-', " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::printing::{PrintJob, State};

    fn make_job(id: u32, state: JobState) -> PrintJob {
        PrintJob {
            id,
            name: "Test.pdf".into(),
            printer_name: "Printer".into(),
            state,
            pages_completed: None,
            pages_total: None,
        }
    }

    fn make_printer(state_reasons: Vec<&str>) -> glimpse_core::services::printing::Printer {
        glimpse_core::services::printing::Printer {
            name: "Printer".into(),
            make_model: "Test".into(),
            state: glimpse_core::services::printing::PrinterState::Idle,
            state_reasons: state_reasons.into_iter().map(String::from).collect(),
            job_count: 0,
            markers: vec![],
        }
    }

    #[test]
    fn has_errors_ignores_informational_supply_level_reasons() {
        let state = State {
            available: true,
            jobs: vec![],
            printers: vec![make_printer(vec!["media-low", "toner-low"])],
        };
        assert!(!has_errors(&state));
    }

    #[test]
    fn has_errors_ignores_report_and_warning_suffixed_reasons() {
        let state = State {
            available: true,
            jobs: vec![],
            printers: vec![make_printer(vec![
                "offline-report",
                "marker-supply-low-report",
            ])],
        };
        assert!(!has_errors(&state));
    }

    #[test]
    fn has_errors_true_for_a_genuine_error_reason() {
        let state = State {
            available: true,
            jobs: vec![],
            printers: vec![make_printer(vec!["paper-jam"])],
        };
        assert!(has_errors(&state));
    }

    #[test]
    fn has_errors_true_when_mixed_with_an_informational_reason() {
        let state = State {
            available: true,
            jobs: vec![],
            printers: vec![make_printer(vec!["media-low", "paper-jam"])],
        };
        assert!(has_errors(&state));
    }

    #[test]
    fn label_empty_when_no_jobs() {
        assert_eq!(label(&State::default()), "");
    }

    #[test]
    fn label_shows_count_when_jobs_present() {
        let state = State {
            available: true,
            jobs: vec![
                make_job(1, JobState::Processing),
                make_job(2, JobState::Pending),
            ],
            printers: vec![],
        };
        assert_eq!(label(&state), "2");
    }

    #[test]
    fn tooltip_singular_and_plural() {
        assert_eq!(tooltip(&State::default()), "No print jobs");
        let one = State {
            jobs: vec![make_job(1, JobState::Processing)],
            available: true,
            printers: vec![],
        };
        assert_eq!(tooltip(&one), "1 job active");
        let two = State {
            jobs: vec![
                make_job(1, JobState::Processing),
                make_job(2, JobState::Pending),
            ],
            available: true,
            printers: vec![],
        };
        assert_eq!(tooltip(&two), "2 jobs active");
    }

    #[test]
    fn state_reason_maps_known_reasons() {
        assert_eq!(state_reason_text("paper-jam"), "Paper jam");
        assert_eq!(state_reason_text("media-empty"), "Out of paper");
        assert_eq!(state_reason_text("toner-empty"), "Out of toner");
        assert_eq!(
            state_reason_text("marker-supply-empty-error"),
            "Out of toner"
        );
        assert_eq!(state_reason_text("cover-open"), "Cover open");
        assert_eq!(state_reason_text("offline"), "Offline");
        assert_eq!(state_reason_text("offline-report"), "Offline");
    }

    #[test]
    fn state_reason_title_cases_unknown() {
        assert_eq!(state_reason_text("my-custom-reason"), "My Custom Reason");
    }

    #[test]
    fn page_progress_returns_none_without_page_data() {
        let job = make_job(1, JobState::Processing);
        assert_eq!(page_progress(&job), None);
    }

    #[test]
    fn page_progress_formats_when_both_present() {
        let job = PrintJob {
            pages_completed: Some(3),
            pages_total: Some(12),
            ..make_job(1, JobState::Processing)
        };
        assert_eq!(page_progress(&job), Some("Page 3 of 12".into()));
    }

    #[test]
    fn page_progress_returns_none_when_only_one_field_present() {
        let job = PrintJob {
            pages_completed: Some(3),
            pages_total: None,
            ..make_job(1, JobState::Processing)
        };
        assert_eq!(page_progress(&job), None);
    }
}
