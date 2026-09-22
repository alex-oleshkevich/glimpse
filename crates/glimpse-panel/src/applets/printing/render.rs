use std::collections::HashSet;

use gettextrs::{gettext, ngettext};
use glimpse_services::{
    JobState, PrintJob, Printer as ServicePrinter, PrinterState, PrintingState,
};
use glimpse_widgets::{PrintingJob, PrintingPrinter};

pub const IDLE: &str = "printer-symbolic";
pub const PRINTING: &str = "printer-printing-symbolic";

pub const NAME_CAP: usize = 40;

pub fn cap(text: &str) -> String {
    glimpse_utils::clean(text, NAME_CAP)
}

/// An indicator is an icon and takes no colour of its own: `attention` is what asks the bar to
/// mark it, so the icon itself stays the same neutral glyph whether or not something needs
/// looking at.
pub fn chip(state: &PrintingState) -> Option<&'static str> {
    if attention(state) {
        return Some(IDLE);
    }
    if state
        .jobs
        .iter()
        .any(|job| job.state == JobState::Processing)
    {
        return Some(PRINTING);
    }
    if state.jobs.iter().any(|job| job.state.is_active()) {
        return Some(IDLE);
    }
    None
}

pub fn attention(state: &PrintingState) -> bool {
    state.jobs.iter().any(|job| {
        job.state.is_active()
            && state.printers.iter().any(|printer| {
                printer.name == job.printer_name && printer.state == PrinterState::Stopped
            })
    })
}

/// Counts exactly the jobs `jobs(state, cap)` hands the popover, so the chip's tooltip and the
/// popover's own hero subtitle can never disagree about how many jobs there are — the widget has
/// no overflow affordance to build a "true total" on top of, so the truncated count is the only
/// number both surfaces can honestly agree on.
pub fn tooltip(state: &PrintingState, format: Option<&str>, cap_count: usize) -> Option<String> {
    if !state.jobs.iter().any(|job| job.state.is_active()) {
        return None;
    }
    let shown = state.jobs.iter().take(cap_count).count();
    let status = status_summary(shown);
    let Some(format) = format else {
        return Some(status);
    };
    Some(crate::applets::tokens::render(
        format,
        |token| match token {
            "status" => Some(status.as_str()),
            _ => None,
        },
    ))
}

fn status_summary(shown: usize) -> String {
    ngettext("{count} print job", "{count} print jobs", shown as u32)
        .replace("{count}", &shown.to_string())
}

/// `pending` names the job ids a cancel/pause/resume command is already in flight for: their row
/// is shown busy and its buttons disabled, so a second click before the next poll cannot fire a
/// second command against a job that may already be gone.
pub fn jobs(state: &PrintingState, cap_count: usize, pending: &HashSet<u32>) -> Vec<PrintingJob> {
    state
        .jobs
        .iter()
        .take(cap_count)
        .map(|job| job_row(job, pending))
        .collect()
}

fn job_row(job: &PrintJob, pending: &HashSet<u32>) -> PrintingJob {
    let in_flight = pending.contains(&job.id);
    PrintingJob {
        id: job.id.to_string(),
        name: cap(&job.name),
        printer: cap(&job.printer_name),
        status: job_status(job),
        progress: job.pages_completed.zip(job.pages_total),
        busy: in_flight || job.state == JobState::Processing,
        cancellable: !in_flight && cancellable(job.state),
        pausable: !in_flight && job.state == JobState::Processing,
        resumable: !in_flight && job.state == JobState::Held,
    }
}

fn cancellable(state: JobState) -> bool {
    matches!(
        state,
        JobState::Pending | JobState::Held | JobState::Processing | JobState::Stopped
    )
}

fn job_status(job: &PrintJob) -> String {
    match job.state {
        JobState::Pending => gettext("Queued"),
        JobState::Held => gettext("Held"),
        JobState::Processing => gettext("Printing"),
        JobState::Stopped => gettext("Stopped"),
        JobState::Completed => gettext("Completed"),
        JobState::Cancelled => gettext("Cancelled"),
        JobState::Aborted => gettext("Aborted"),
    }
}

pub fn printers(state: &PrintingState) -> Vec<PrintingPrinter> {
    state.printers.iter().map(printer).collect()
}

fn printer(printer: &ServicePrinter) -> PrintingPrinter {
    PrintingPrinter {
        id: cap(&printer.name),
        name: cap(&printer.name),
        status: printer_status(printer),
        network: false,
    }
}

fn printer_status(printer: &ServicePrinter) -> String {
    match printer.state {
        PrinterState::Idle => gettext("Idle"),
        PrinterState::Processing => gettext("Printing"),
        PrinterState::Stopped => printer
            .state_reasons
            .first()
            .map(|reason| cap(reason))
            .filter(|reason| !reason.is_empty())
            .unwrap_or_else(|| gettext("Stopped")),
    }
}

#[cfg(test)]
mod tests {
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
}
