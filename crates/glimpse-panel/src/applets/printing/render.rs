use std::collections::HashSet;

use gettextrs::{gettext, ngettext};
use glimpse_config::PrintingShow;
use glimpse_services::{
    JobState, PrintJob, Printer as ServicePrinter, PrinterState, PrintingState,
};
use glimpse_widgets::{Fact, PrintingJob, PrintingPrinter};

pub const IDLE: &str = "printer-symbolic";
pub const PRINTING: &str = "printer-printing-symbolic";

pub const NAME_CAP: usize = 40;

pub fn chip(state: &PrintingState, show: PrintingShow) -> Option<&'static str> {
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
    match show {
        PrintingShow::Always if !state.printers.is_empty() => Some(IDLE),
        _ => None,
    }
}

pub fn attention(state: &PrintingState) -> bool {
    state.jobs.iter().any(|job| {
        job.state.is_active()
            && state.printers.iter().any(|printer| {
                printer.name == job.printer_name && printer.state == PrinterState::Stopped
            })
    })
}

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
        name: glimpse_utils::clean(&job.name, NAME_CAP),
        printer: glimpse_utils::clean(&job.printer_name, NAME_CAP),
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
    let problem = problem(printer);
    PrintingPrinter {
        id: glimpse_utils::clean(&printer.name, NAME_CAP),
        name: glimpse_utils::clean(&printer.name, NAME_CAP),
        facts: printer_facts(printer, problem.is_some()),
        problem: problem.unwrap_or_default(),
        network: is_network(&printer.device_uri),
    }
}

fn printer_facts(printer: &ServicePrinter, troubled: bool) -> Vec<Fact> {
    let model = glimpse_utils::clean(&printer.make_model, NAME_CAP);
    let location = glimpse_utils::clean(&printer.location, NAME_CAP);
    let busy = !troubled && printer.state != PrinterState::Idle;
    [
        (gettext("Model"), model),
        (gettext("Location"), location),
        (
            gettext("Status"),
            if busy {
                gettext("Printing")
            } else {
                String::new()
            },
        ),
    ]
    .into_iter()
    .filter(|(_, value)| !value.is_empty())
    .map(|(label, value)| Fact::new(label, value))
    .collect()
}

fn problem(printer: &ServicePrinter) -> Option<String> {
    if !printer.state_message.is_empty() {
        return Some(glimpse_utils::clean(&printer.state_message, NAME_CAP));
    }
    if !printer.accepting_jobs {
        return Some(gettext("Not accepting jobs"));
    }
    match printer.state {
        PrinterState::Stopped => Some(
            printer
                .state_reasons
                .first()
                .map(|reason| glimpse_utils::clean(reason, NAME_CAP))
                .filter(|reason| !reason.is_empty())
                .unwrap_or_else(|| gettext("Stopped")),
        ),
        PrinterState::Idle | PrinterState::Processing => None,
    }
}

fn is_network(device_uri: &str) -> bool {
    let Some((scheme, rest)) = device_uri.split_once(':') else {
        return false;
    };
    if scheme.eq_ignore_ascii_case("hp") {
        return rest.starts_with("/net/");
    }
    [
        "ipp",
        "ipps",
        "http",
        "https",
        "socket",
        "lpd",
        "dnssd",
        "implicitclass",
        "smb",
    ]
    .iter()
    .any(|network| scheme.eq_ignore_ascii_case(network))
}

#[cfg(test)]
mod tests;
