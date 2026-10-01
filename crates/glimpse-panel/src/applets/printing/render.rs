use std::collections::HashSet;

use gettextrs::{gettext, ngettext};
use glimpse_services::{
    JobState, PrintJob, Printer as ServicePrinter, PrinterState, PrintingState,
};
use glimpse_widgets::{Fact, PrintingJob, PrintingPrinter};

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
        warning: troubled(printer),
        network: false,
        facts: printer_facts(printer),
    }
}

/// A printer's card holds only where it is: the specs a queue reports — colour, sides, paper,
/// resolution — are nothing a person acts on from a panel, and its status leads the card as a
/// static line. A printer with neither has no card at all.
fn printer_facts(printer: &ServicePrinter) -> Vec<Fact> {
    match printer.location.is_empty() {
        true => Vec::new(),
        false => vec![Fact::new(gettext("Location"), printer.location.clone())],
    }
}

fn troubled(printer: &ServicePrinter) -> bool {
    printer.state == PrinterState::Stopped
        || !printer.accepting_jobs
        || !printer.state_message.is_empty()
}

/// The row's second line: the printer's own message when it reports one, since that is the
/// problem a person opened the popover about, then a refusal to take jobs, then its state.
fn printer_status(printer: &ServicePrinter) -> String {
    if !printer.state_message.is_empty() {
        return cap(&printer.state_message);
    }
    if !printer.accepting_jobs {
        return gettext("Not accepting jobs");
    }
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
mod tests;
