use std::collections::HashSet;

use chrono::{DateTime, FixedOffset, NaiveTime, Utc};
use gettextrs::gettext;
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
    let active: Vec<&PrintJob> = state
        .jobs
        .iter()
        .filter(|job| job.state.is_active())
        .collect();
    let job = active
        .iter()
        .find(|job| job.state == JobState::Processing)
        .or_else(|| active.first())?;
    let count = active.len().min(cap_count).to_string();
    let document = glimpse_utils::clean(&job.name, NAME_CAP);
    let printer = glimpse_utils::clean(&job.printer_name, NAME_CAP);
    let pages = match (job.pages_completed, job.pages_total) {
        (Some(done), Some(total)) if total > 0 => gettext("page {done} of {total}")
            .replace("{done}", &done.to_string())
            .replace("{total}", &total.to_string()),
        _ => String::new(),
    };
    let format = format.map(str::to_owned).unwrap_or_else(default_tooltip);
    Some(crate::applets::tokens::render_lines(
        &format,
        |token| match token {
            "count" => Some(count.as_str()),
            "document" => Some(document.as_str()),
            "printer" => Some(printer.as_str()),
            "pages" => Some(pages.as_str()),
            _ => None,
        },
    ))
}

pub fn default_tooltip() -> String {
    gettext("{document}\n{printer}\n{pages}")
}

pub struct Viewer {
    pub user: String,
    pub now: DateTime<FixedOffset>,
    pub twelve_hour: bool,
}

impl Viewer {
    pub fn current() -> Self {
        Self {
            user: gtk4::glib::user_name().to_string_lossy().into_owned(),
            now: chrono::Local::now().fixed_offset(),
            twelve_hour: glimpse_config::twelve_hour(),
        }
    }
}

pub fn jobs(
    state: &PrintingState,
    cap_count: usize,
    pending: &HashSet<u32>,
    viewer: &Viewer,
) -> Vec<PrintingJob> {
    state
        .jobs
        .iter()
        .take(cap_count)
        .map(|job| job_row(job, pending, viewer))
        .collect()
}

fn job_row(job: &PrintJob, pending: &HashSet<u32>, viewer: &Viewer) -> PrintingJob {
    let in_flight = pending.contains(&job.id);
    PrintingJob {
        id: job.id.to_string(),
        name: glimpse_utils::clean(&job.name, NAME_CAP),
        facts: job_facts(job, viewer),
        busy: in_flight || job.state == JobState::Processing,
        cancellable: !in_flight && cancellable(job.state),
        pausable: !in_flight && job.state == JobState::Processing,
        resumable: !in_flight && job.state == JobState::Held,
        retryable: !in_flight && job.state == JobState::Stopped,
    }
}

fn job_facts(job: &PrintJob, viewer: &Viewer) -> Vec<Fact> {
    let user = glimpse_utils::clean(&job.user, NAME_CAP);
    let host = glimpse_utils::clean(&job.host, NAME_CAP);
    [
        (
            gettext("Printer"),
            glimpse_utils::clean(&job.printer_name, NAME_CAP),
        ),
        (gettext("Status"), job_status(job)),
        (gettext("Held until"), held_until(job, viewer)),
        (gettext("Pages"), pages(job)),
        (gettext("Sheets"), sheets(job)),
        (gettext("Copies"), copies(job)),
        (gettext("Sides"), sides(&job.sides)),
        (gettext("Color"), color_mode(&job.color_mode)),
        (gettext("Paper"), media_name(&job.media)),
        (gettext("Submitted"), submitted(job.created, viewer)),
        (
            gettext("Submitted by"),
            if user == viewer.user {
                String::new()
            } else {
                user
            },
        ),
        (
            gettext("From"),
            if is_local_host(&host) {
                String::new()
            } else {
                host
            },
        ),
    ]
    .into_iter()
    .filter(|(_, value)| !value.is_empty())
    .map(|(label, value)| Fact::new(label, value))
    .collect()
}

fn pages(job: &PrintJob) -> String {
    match (job.pages_completed, job.pages_total) {
        (Some(done), Some(total)) if total > 0 && job.state == JobState::Processing => {
            gettext("{done} of {total}")
                .replace("{done}", &done.to_string())
                .replace("{total}", &total.to_string())
        }
        (_, Some(total)) if total > 0 => total.to_string(),
        _ => String::new(),
    }
}

fn sheets(job: &PrintJob) -> String {
    match job.sheets_completed {
        Some(sheets) if sheets > 0 && Some(sheets) != job.pages_completed => sheets.to_string(),
        _ => String::new(),
    }
}

fn copies(job: &PrintJob) -> String {
    match job.copies {
        Some(copies) if copies > 1 => copies.to_string(),
        _ => String::new(),
    }
}

fn sides(keyword: &str) -> String {
    match keyword {
        "one-sided" => gettext("One-sided"),
        "two-sided-long-edge" => gettext("Two-sided"),
        "two-sided-short-edge" => gettext("Two-sided, short edge"),
        _ => String::new(),
    }
}

fn color_mode(keyword: &str) -> String {
    match keyword {
        "color" => gettext("Full color"),
        "monochrome" | "process-monochrome" | "auto-monochrome" => gettext("Grayscale"),
        "bi-level" | "process-bi-level" => gettext("Black and white"),
        _ => String::new(),
    }
}

fn media_name(media: &str) -> String {
    let parts: Vec<&str> = media.split('_').collect();
    let name = match parts.as_slice() {
        [] | [""] => return String::new(),
        ["custom", ..] => return gettext("Custom"),
        [class, name @ .., _size] if !name.is_empty() => {
            let name = name.join("_");
            match *class {
                "iso" | "jis" | "jpn" | "prc" | "roc" => name.to_uppercase(),
                _ => capitalized(&name),
            }
        }
        _ => media.to_owned(),
    };
    glimpse_utils::clean(&name, NAME_CAP)
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn submitted(created: Option<DateTime<Utc>>, viewer: &Viewer) -> String {
    let Some(created) = created else {
        return String::new();
    };
    let local = created.with_timezone(&viewer.now.timezone());
    let clock = glimpse_config::clock(viewer.twelve_hour);
    let time = crate::applets::strftime(&local, clock).unwrap_or_default();
    if local.date_naive() == viewer.now.date_naive() {
        return time;
    }
    let date = crate::applets::strftime(&local, "%-d %b").unwrap_or_default();
    gettext("{date}, {time}")
        .replace("{date}", &date)
        .replace("{time}", &time)
}

fn held_until(job: &PrintJob, viewer: &Viewer) -> String {
    if job.state != JobState::Held {
        return String::new();
    }
    match job.hold_until.as_str() {
        "day-time" => gettext("Daytime"),
        "evening" => gettext("Evening"),
        "night" => gettext("Night"),
        "weekend" => gettext("Weekend"),
        "second-shift" => gettext("Second shift"),
        "third-shift" => gettext("Third shift"),
        other => NaiveTime::parse_from_str(other, "%H:%M:%S")
            .or_else(|_| NaiveTime::parse_from_str(other, "%H:%M"))
            .ok()
            .and_then(|time| {
                let utc = viewer.now.with_timezone(&Utc).date_naive().and_time(time);
                let local = utc.and_utc().with_timezone(&viewer.now.timezone());
                crate::applets::strftime(&local, glimpse_config::clock(viewer.twelve_hour))
            })
            .unwrap_or_default(),
    }
}

fn is_local_host(host: &str) -> bool {
    matches!(host, "" | "localhost" | "127.0.0.1" | "::1")
}

fn cancellable(state: JobState) -> bool {
    matches!(
        state,
        JobState::Pending | JobState::Held | JobState::Processing | JobState::Stopped
    )
}

fn job_status(job: &PrintJob) -> String {
    let status = state_name(job.state);
    let reason = glimpse_utils::clean(&job.reason, NAME_CAP);
    match job.state {
        JobState::Held | JobState::Stopped | JobState::Aborted if !reason.is_empty() => {
            gettext("{status}: {reason}")
                .replace("{status}", &status)
                .replace("{reason}", &reason)
        }
        _ => status,
    }
}

fn state_name(state: JobState) -> String {
    match state {
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
