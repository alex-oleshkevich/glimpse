use std::time::Duration;

use serde::Serialize;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::services::framework::{Control, ServiceCommand, ServiceHandle};

pub type PrintingHandle = ServiceHandle<State, Command>;

// ── State types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct State {
    pub available: bool,
    pub printers: Vec<Printer>,
    pub jobs: Vec<PrintJob>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Printer {
    pub name: String,
    pub make_model: String,
    pub state: PrinterState,
    pub state_reasons: Vec<String>,
    pub job_count: u32,
    pub markers: Vec<InkMarker>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub enum PrinterState {
    #[default]
    Idle,
    Processing,
    Stopped,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PrintJob {
    pub id: u32,
    pub name: String,
    pub printer_name: String,
    pub state: JobState,
    pub pages_completed: Option<u32>,
    pub pages_total: Option<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub enum JobState {
    #[default]
    Pending,
    Held,
    Processing,
    Stopped,
    Completed,
    Cancelled,
    Aborted,
}

impl JobState {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Pending | Self::Processing | Self::Held)
    }

    pub fn to_string_reason(&self) -> String {
        match self {
            Self::Aborted => "aborted".into(),
            Self::Stopped => "stopped".into(),
            _ => "unknown".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct InkMarker {
    pub name: String,
    pub level: i32,
    pub kind: MarkerKind,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum MarkerKind {
    Toner,
    Ink,
    Other,
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Refresh,
    CancelJob { id: u32 },
    PauseJob  { id: u32 },
    ResumeJob { id: u32 },
}

// ── Service ──────────────────────────────────────────────────────────────────

const CUPS_URL: &str = "http://localhost:631/";
const POLL_ACTIVE: Duration = Duration::from_secs(2);
const POLL_IDLE: Duration = Duration::from_secs(30);

pub struct PrintingService {
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

impl PrintingService {
    pub fn new() -> (Self, PrintingHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(16);
        (
            Self { state_tx, command_rx },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        self.run_inner(cancel).await;
    }

    async fn run_inner(&mut self, cancel: CancellationToken) {
        tracing::debug!("printing service started");
        let mut first_failure_logged = false;

        loop {
            self.poll(&mut first_failure_logged).await;

            let interval = if self.state_tx.borrow().jobs.iter().any(|j| j.state.is_active()) {
                POLL_ACTIVE
            } else {
                POLL_IDLE
            };

            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(interval) => {}
                cmd = self.command_rx.recv() => match cmd {
                    Some(ServiceCommand::Command(Command::Refresh)) => {}
                    Some(ServiceCommand::Command(Command::CancelJob { id })) => {
                        self.job_action(id, JobAction::Cancel).await;
                        self.poll(&mut first_failure_logged).await;
                    }
                    Some(ServiceCommand::Command(Command::PauseJob { id })) => {
                        self.job_action(id, JobAction::Pause).await;
                        self.poll(&mut first_failure_logged).await;
                    }
                    Some(ServiceCommand::Command(Command::ResumeJob { id })) => {
                        self.job_action(id, JobAction::Resume).await;
                        self.poll(&mut first_failure_logged).await;
                    }
                    Some(ServiceCommand::Control(Control::Shutdown)) | None => break,
                    Some(ServiceCommand::Control(_)) => {}
                }
            }
        }
    }

    async fn poll(&mut self, first_failure_logged: &mut bool) {
        match fetch_state().await {
            Ok(state) => {
                *first_failure_logged = false;
                let _ = self.state_tx.send(state);
            }
            Err(_) => {
                if !*first_failure_logged {
                    tracing::debug!("printing: CUPS unavailable, will retry");
                    *first_failure_logged = true;
                }
                let _ = self.state_tx.send(State { available: false, ..State::default() });
            }
        }
    }

    async fn job_action(&self, id: u32, action: JobAction) {
        if let Err(e) = send_job_action(id, action).await {
            tracing::warn!(id, error = %e, "printing: job action failed");
        }
    }
}

#[derive(Clone, Copy)]
enum JobAction { Cancel, Pause, Resume }

// ── IPP client ───────────────────────────────────────────────────────────────

async fn fetch_state() -> anyhow::Result<State> {
    let (printers, jobs) = tokio::try_join!(fetch_printers(), fetch_jobs())?;
    Ok(State { available: true, printers, jobs })
}

async fn fetch_printers() -> anyhow::Result<Vec<Printer>> {
    use ipp::operation::cups::CupsGetPrinters;
    use ipp::prelude::*;

    let uri: Uri = CUPS_URL.parse()?;
    let client = AsyncIppClient::new(uri);
    let resp = client.send(CupsGetPrinters::new()).await?;

    if resp.header().status_code().is_success()
        || resp.header().status_code() == StatusCode::ClientErrorNotFound
    {
        return Ok(parse_printers(resp.attributes()));
    }

    anyhow::bail!("CUPS-Get-Printers failed: {:?}", resp.header().status_code());
}

async fn fetch_jobs() -> anyhow::Result<Vec<PrintJob>> {
    use ipp::prelude::*;

    let uri: Uri = CUPS_URL.parse()?;
    let op = IppOperationBuilder::get_jobs(uri.clone()).build()?;

    let client = AsyncIppClient::new(uri);
    let resp = client.send(op).await?;

    if resp.header().status_code().is_success()
        || resp.header().status_code() == StatusCode::ClientErrorNotFound
    {
        return Ok(parse_jobs(resp.attributes()));
    }

    anyhow::bail!("Get-Jobs failed: {:?}", resp.header().status_code());
}

async fn send_job_action(id: u32, action: JobAction) -> anyhow::Result<()> {
    use ipp::attribute::IppAttribute;
    use ipp::model::{DelimiterTag, IppVersion, Operation};
    use ipp::operation::{IppOperation as _, builder::IppOperationBuilder};
    use ipp::prelude::*;
    use ipp::value::IppValue;

    let cups_uri: Uri = CUPS_URL.parse()?;

    let req: IppRequestResponse = match action {
        JobAction::Cancel => {
            IppOperationBuilder::cancel_job(cups_uri.clone(), id as i32)
                .build()?
                .into_ipp_request()
        }
        JobAction::Pause => {
            let mut req = IppRequestResponse::new(
                IppVersion::v1_1(),
                Operation::HoldJob,
                Some(cups_uri.clone()),
            )?;
            req.attributes_mut().add(
                DelimiterTag::OperationAttributes,
                IppAttribute::new(
                    IppAttribute::JOB_ID.try_into().unwrap(),
                    IppValue::Integer(id as i32),
                ),
            );
            req
        }
        JobAction::Resume => {
            let mut req = IppRequestResponse::new(
                IppVersion::v1_1(),
                Operation::ReleaseJob,
                Some(cups_uri.clone()),
            )?;
            req.attributes_mut().add(
                DelimiterTag::OperationAttributes,
                IppAttribute::new(
                    IppAttribute::JOB_ID.try_into().unwrap(),
                    IppValue::Integer(id as i32),
                ),
            );
            req
        }
    };

    let client = AsyncIppClient::new(cups_uri);
    let resp = client.send(req).await?;

    if resp.header().status_code().is_success() {
        return Ok(());
    }
    anyhow::bail!("job action failed: {:?}", resp.header().status_code());
}

// ── Attribute parsing ────────────────────────────────────────────────────────

fn parse_printers(groups: &IppAttributes) -> Vec<Printer> {
    groups
        .groups_of(DelimiterTag::PrinterAttributes)
        .map(|group| {
            let name = str_attr(group, "printer-name").unwrap_or_default();
            let make_model = str_attr(group, "printer-make-and-model").unwrap_or_default();
            let state = match u32_attr(group, "printer-state") {
                Some(3) => PrinterState::Idle,
                Some(4) => PrinterState::Processing,
                Some(5) => PrinterState::Stopped,
                _ => PrinterState::Idle,
            };
            let state_reasons = str_list_attr(group, "printer-state-reasons")
                .into_iter()
                .filter(|r| r != "none")
                .collect();
            let job_count = u32_attr(group, "queued-job-count").unwrap_or(0);
            let markers = parse_markers(group);

            Printer { name, make_model, state, state_reasons, job_count, markers }
        })
        .collect()
}

fn parse_jobs(groups: &IppAttributes) -> Vec<PrintJob> {
    groups
        .groups_of(DelimiterTag::JobAttributes)
        .map(|group| {
            let id = u32_attr(group, "job-id").unwrap_or(0);
            let name = str_attr(group, "job-name").unwrap_or_else(|| format!("Job {id}"));
            let printer_uri = str_attr(group, "job-printer-uri").unwrap_or_default();
            let printer_name = printer_name_from_uri(&printer_uri);
            let state = match u32_attr(group, "job-state") {
                Some(3) => JobState::Pending,
                Some(4) => JobState::Held,
                Some(5) => JobState::Processing,
                Some(6) => JobState::Stopped,
                Some(7) => JobState::Cancelled,
                Some(8) => JobState::Aborted,
                Some(9) => JobState::Completed,
                _ => JobState::Pending,
            };
            let pages_completed = u32_attr(group, "job-impressions-completed");
            let pages_total = u32_attr(group, "job-impressions");

            PrintJob { id, name, printer_name, state, pages_completed, pages_total }
        })
        .filter(|j| j.state.is_active())
        .collect()
}

fn parse_markers(group: &IppAttributeGroup) -> Vec<InkMarker> {
    let names = str_list_attr(group, "marker-names");
    let levels = i32_list_attr(group, "marker-levels");
    let kinds = str_list_attr(group, "marker-types");

    if names.is_empty() { return vec![]; }

    names.into_iter().enumerate().map(|(i, name)| {
        let level = levels.get(i).copied().unwrap_or(-1);
        let kind = match kinds.get(i).map(String::as_str) {
            Some("toner") => MarkerKind::Toner,
            Some("ink")   => MarkerKind::Ink,
            _             => MarkerKind::Other,
        };
        InkMarker { name, level, kind }
    }).collect()
}

fn printer_name_from_uri(uri: &str) -> String {
    uri.rsplit('/').next().unwrap_or(uri).to_owned()
}

use ipp::attribute::{IppAttributeGroup, IppAttributes};
use ipp::model::DelimiterTag;

fn str_attr(group: &IppAttributeGroup, name: &str) -> Option<String> {
    use ipp::value::IppValue;
    match group.attributes().get(name)?.value() {
        IppValue::TextWithoutLanguage(s) | IppValue::OctetString(s) => Some(s.as_ref().to_owned()),
        IppValue::NameWithoutLanguage(s) => Some(s.as_ref().to_owned()),
        IppValue::Uri(s) => Some(s.as_ref().to_owned()),
        IppValue::Keyword(s) => Some(s.as_ref().to_owned()),
        _ => None,
    }
}

fn u32_attr(group: &IppAttributeGroup, name: &str) -> Option<u32> {
    use ipp::value::IppValue;
    match group.attributes().get(name)?.value() {
        IppValue::Enum(n) | IppValue::Integer(n) => Some(*n as u32),
        _ => None,
    }
}

fn i32_list_attr(group: &IppAttributeGroup, name: &str) -> Vec<i32> {
    use ipp::value::IppValue;
    let Some(attr) = group.attributes().get(name) else { return vec![] };
    match attr.value() {
        IppValue::Integer(n) => vec![*n],
        IppValue::Array(vals) => vals.iter().filter_map(|v| {
            if let IppValue::Integer(n) = v { Some(*n) } else { None }
        }).collect(),
        _ => vec![],
    }
}

fn str_list_attr(group: &IppAttributeGroup, name: &str) -> Vec<String> {
    use ipp::value::IppValue;
    let Some(attr) = group.attributes().get(name) else { return vec![] };
    fn extract(v: &IppValue) -> Option<String> {
        match v {
            IppValue::TextWithoutLanguage(s) | IppValue::OctetString(s) => Some(s.as_ref().to_owned()),
            IppValue::NameWithoutLanguage(s) => Some(s.as_ref().to_owned()),
            IppValue::Keyword(s) => Some(s.as_ref().to_owned()),
            IppValue::Uri(s) => Some(s.as_ref().to_owned()),
            _ => None,
        }
    }
    match attr.value() {
        IppValue::Array(vals) => vals.iter().filter_map(extract).collect(),
        single => extract(single).into_iter().collect(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_unavailable_and_empty() {
        let state = State::default();
        assert!(!state.available);
        assert!(state.printers.is_empty());
        assert!(state.jobs.is_empty());
    }

    #[test]
    fn job_state_active_identifies_workable_states() {
        assert!(JobState::Pending.is_active());
        assert!(JobState::Processing.is_active());
        assert!(JobState::Held.is_active());
        assert!(!JobState::Completed.is_active());
        assert!(!JobState::Cancelled.is_active());
        assert!(!JobState::Aborted.is_active());
        assert!(!JobState::Stopped.is_active());
    }

    #[test]
    fn printer_name_extracted_from_ipp_uri() {
        assert_eq!(
            printer_name_from_uri("ipp://localhost:631/printers/Office_Printer"),
            "Office_Printer"
        );
        assert_eq!(printer_name_from_uri("Office_Printer"), "Office_Printer");
    }
}
