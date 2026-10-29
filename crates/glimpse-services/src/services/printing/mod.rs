mod call;
mod failure;
mod source;
mod subscription;

use std::time::Duration;

use tokio::sync::oneshot;
use tokio::time::{self, Instant};

use ipp::model::Operation;
use ipp::prelude::Uri;

pub use failure::Failure;
use failure::{Action, classify};

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, Service},
    subscription::Sub,
};

const DEFAULT_SERVER_URL: &str = "http://localhost:631/";
const RENEW_EVERY: Duration = Duration::from_secs(3500);
const FETCH_FLOOR: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq)]
pub struct Printer {
    pub name: String,
    pub make_model: String,
    pub device_uri: String,
    pub state: PrinterState,
    pub state_reasons: Vec<String>,
    pub state_message: String,
    pub location: String,
    pub accepting_jobs: bool,
    pub color: bool,
    pub duplex: bool,
    pub media_ready: Vec<String>,
    pub resolution: String,
    pub job_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrinterState {
    Idle,
    Processing,
    Stopped,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrintJob {
    pub id: u32,
    pub name: String,
    pub printer_name: String,
    pub state: JobState,
    pub pages_completed: Option<u32>,
    pub pages_total: Option<u32>,
    pub sheets_completed: Option<u32>,
    pub copies: Option<u32>,
    pub sides: String,
    pub color_mode: String,
    pub media: String,
    pub created: Option<chrono::DateTime<chrono::Utc>>,
    pub user: String,
    pub host: String,
    pub hold_until: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Pending,
    Held,
    Processing,
    Stopped,
    Completed,
    Cancelled,
    Aborted,
}

impl JobState {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Pending | Self::Processing | Self::Held)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrintingState {
    pub printers: Vec<Printer>,
    pub jobs: Vec<PrintJob>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    server: Result<Uri, String>,
}

impl From<&glimpse_config::Config> for Config {
    fn from(_: &glimpse_config::Config) -> Self {
        Self {
            server: match std::env::var("CUPS_SERVER") {
                Ok(value) => normalize_server(Some(&value)),
                Err(std::env::VarError::NotPresent) => normalize_server(None),
                Err(error) => Err(format!("invalid CUPS_SERVER: {error}")),
            },
        }
    }
}

impl Config {
    fn uri(&self) -> Result<Uri, String> {
        self.server.clone()
    }
}

fn normalize_server(value: Option<&str>) -> Result<Uri, String> {
    let value = value.unwrap_or(DEFAULT_SERVER_URL);
    let reason = || format!("invalid CUPS_SERVER: {value:?}");
    if value.is_empty() || value.trim() != value || value.starts_with('/') {
        return Err(reason());
    }
    let explicit = value.contains("://");
    let text = if explicit {
        value.to_owned()
    } else {
        format!("http://{value}")
    };
    let url = reqwest::Url::parse(&text).map_err(|_| reason())?;
    let original: Uri = text.parse().map_err(|_| reason())?;
    let scheme = match url.scheme() {
        "http" | "ipp" => "http",
        "https" | "ipps" => "https",
        _ => return Err(reason()),
    };
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || (!explicit && url.path() != "/")
    {
        return Err(reason());
    }
    let port = original
        .port_u16()
        .or_else(|| (!explicit || matches!(url.scheme(), "ipp" | "ipps")).then_some(631));
    let port = port.map(|port| format!(":{port}")).unwrap_or_default();
    format!(
        "{scheme}://{}{port}{}",
        url.host_str().ok_or_else(reason)?,
        url.path()
    )
    .parse()
    .map_err(|_| reason())
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Watch {
    Notifier(u64),
    Lease(u64),
}

type Reply = oneshot::Sender<Result<(), PrintingError>>;

#[derive(Debug, thiserror::Error)]
pub enum PrintingError {
    #[error("printing refused the command: {0:?}")]
    Failed(Failure),
    #[error(transparent)]
    Service(#[from] CommandError),
}

pub enum Command {
    CancelJob { id: u32, reply: Reply },
    PauseJob { id: u32, reply: Reply },
    ResumeJob { id: u32, reply: Reply },
    RetryJob { id: u32, reply: Reply },
    Refresh { reply: Reply },
}

pub enum Event {
    NotifierReady(u64, Result<(), String>),
    NotifierFailed(u64),
    Signal(u64, bool),
    LeaseDue(u64),
    Leased(u64, LeaseResult),
    FetchStarted(u64),
    Fetched(u64, Result<PrintingState, String>),
}

pub struct LeaseResult {
    id: Option<i32>,
    created: bool,
    error: Option<String>,
}

pub struct Printing {
    state: Publisher<PrintingState>,
    config: Config,
    generation: u64,
    subscription: Option<i32>,
    notifier_ready: bool,
    notifier_error: Option<String>,
    lease_error: Option<String>,
    fetch_error: Option<String>,
    leasing: bool,
    lease_pending: bool,
    fetching: bool,
    fetch_pending: bool,
    next_fetch: Instant,
    refresh_replies: Vec<Reply>,
    fetch_replies: Vec<Reply>,
}

pub type PrintingHandle = crate::ServiceEndpoint<Printing>;

impl crate::ServiceEndpoint<Printing> {
    pub async fn cancel_job(&self, id: u32) -> Result<(), PrintingError> {
        self.call("printing stopped before completing the command", |reply| {
            Command::CancelJob { id, reply }
        })
        .await?
    }

    pub async fn pause_job(&self, id: u32) -> Result<(), PrintingError> {
        self.call("printing stopped before completing the command", |reply| {
            Command::PauseJob { id, reply }
        })
        .await?
    }

    pub async fn resume_job(&self, id: u32) -> Result<(), PrintingError> {
        self.call("printing stopped before completing the command", |reply| {
            Command::ResumeJob { id, reply }
        })
        .await?
    }

    pub async fn retry_job(&self, id: u32) -> Result<(), PrintingError> {
        self.call("printing stopped before completing the command", |reply| {
            Command::RetryJob { id, reply }
        })
        .await?
    }

    pub async fn refresh(&self) -> Result<(), PrintingError> {
        self.call("printing stopped before completing the command", |reply| {
            Command::Refresh { reply }
        })
        .await?
    }
}

impl Service for Printing {
    const NAME: &'static str = "printing";
    type Config = Config;
    type State = PrintingState;
    type Command = Command;
    type Event = Event;
    type Dependencies = ();
    type SubKey = Watch;

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let generation = self.generation;
        let mut sources = vec![Sub::stream(Watch::Notifier(generation), move |ctx| {
            source::notifier(ctx, generation)
        })];
        if self.notifier_ready {
            sources.push(Sub::interval(
                Watch::Lease(generation),
                RENEW_EVERY,
                move |_| async move { Event::LeaseDue(generation) },
            ));
        }
        sources
    }

    async fn start(ctx: &Ctx<Self>, config: Self::Config, _: Self::Dependencies) -> Self {
        Self {
            state: ctx.publisher(),
            config,
            generation: 0,
            subscription: None,
            notifier_ready: false,
            notifier_error: None,
            lease_error: None,
            fetch_error: None,
            leasing: false,
            lease_pending: false,
            fetching: false,
            fetch_pending: false,
            next_fetch: Instant::now(),
            refresh_replies: Vec::new(),
            fetch_replies: Vec::new(),
        }
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Command(command) => self.dispatch(ctx, command).await,
            Input::Config(config) => {
                if config == self.config {
                    return;
                }
                if let (Ok(uri), Some(id)) = (self.config.uri(), self.subscription.take()) {
                    ctx.spawn_detached(move |_| async move {
                        let _ =
                            time::timeout(Duration::from_secs(2), subscription::cancel(&uri, id))
                                .await;
                    });
                }
                self.config = config;
                self.generation = self.generation.wrapping_add(1);
                self.notifier_ready = false;
                self.notifier_error = None;
                self.lease_error = None;
                self.fetch_error = None;
                self.leasing = false;
                self.lease_pending = false;
                self.fetching = false;
                self.refresh_replies.append(&mut self.fetch_replies);
                self.fetch_pending = !self.refresh_replies.is_empty();
                self.next_fetch = Instant::now();
            }
            Input::Event(Event::NotifierReady(generation, result))
                if generation == self.generation =>
            {
                self.notifier_ready = true;
                self.notifier_error = result.err();
                self.health(ctx);
            }
            Input::Event(Event::NotifierFailed(generation)) if generation == self.generation => {
                self.notifier_error = Some("CUPS notifier stream stopped".to_owned());
                self.health(ctx);
            }
            Input::Event(Event::LeaseDue(generation)) if generation == self.generation => {
                self.lease(ctx);
            }
            Input::Event(Event::Signal(generation, restart)) if generation == self.generation => {
                if restart {
                    self.lease(ctx);
                }
                self.fetch(ctx);
            }
            Input::Event(Event::Leased(generation, result)) if generation == self.generation => {
                self.leasing = false;
                self.subscription = result.id;
                self.lease_error = result.error;
                self.health(ctx);
                if result.created {
                    self.fetch_pending = true;
                }
                if self.lease_pending {
                    self.lease_pending = false;
                    self.lease(ctx);
                } else if self.fetch_pending {
                    self.fetch_pending = false;
                    self.fetch(ctx);
                }
            }
            Input::Event(Event::Fetched(generation, result)) if generation == self.generation => {
                self.fetching = false;
                match result {
                    Ok(state) => {
                        self.fetch_error = None;
                        self.state.set(state);
                    }
                    Err(reason) => {
                        self.fetch_error = Some(reason);
                        self.state.set(PrintingState::default());
                    }
                }
                self.health(ctx);
                for reply in self.fetch_replies.drain(..) {
                    let _ = reply.send(Ok(()));
                }
                if self.fetch_pending {
                    self.fetch_pending = false;
                    self.fetch(ctx);
                }
            }
            Input::Event(Event::FetchStarted(generation)) if generation == self.generation => {
                self.fetch_pending = false;
                self.fetch_replies.append(&mut self.refresh_replies);
            }
            Input::Event(_) => {}
        }
    }

    async fn stop(self, _: &Ctx<Self>) {
        if let (Ok(uri), Some(id)) = (self.config.uri(), self.subscription) {
            let _ = time::timeout(Duration::from_secs(2), subscription::cancel(&uri, id)).await;
        }
    }
}

impl Printing {
    fn health(&self, ctx: &Ctx<Self>) {
        if let Some(reason) = self
            .notifier_error
            .as_ref()
            .or(self.lease_error.as_ref())
            .or(self.fetch_error.as_ref())
        {
            ctx.degraded(reason.clone());
        } else if self.subscription.is_some() {
            ctx.running();
        }
    }

    fn lease(&mut self, ctx: &Ctx<Self>) {
        if !self.notifier_ready || self.leasing {
            self.lease_pending = true;
            return;
        }
        self.lease_pending = false;
        self.leasing = true;
        let config = self.config.clone();
        let id = self.subscription;
        let generation = self.generation;
        ctx.spawn_detached(move |ctx| async move {
            let result = source::lease(&config, id).await;
            let _ = ctx
                .events()
                .send(Input::Event(Event::Leased(generation, result)))
                .await;
        });
    }

    fn fetch(&mut self, ctx: &Ctx<Self>) {
        if !self.notifier_ready || self.leasing || self.fetching {
            self.fetch_pending = true;
            return;
        }
        self.fetching = true;
        self.fetch_replies.append(&mut self.refresh_replies);
        let at = self.next_fetch.max(Instant::now());
        self.next_fetch = at + FETCH_FLOOR;
        let config = self.config.clone();
        let generation = self.generation;
        ctx.spawn_detached(move |ctx| async move {
            time::sleep_until(at).await;
            let _ = ctx
                .events()
                .send(Input::Event(Event::FetchStarted(generation)))
                .await;
            let result = source::fetch(&config).await;
            let _ = ctx
                .events()
                .send(Input::Event(Event::Fetched(generation, result)))
                .await;
        });
    }

    async fn dispatch(&mut self, ctx: &Ctx<Self>, command: Command) {
        match command {
            Command::CancelJob { id, reply } => {
                self.job_action(ctx, Action::Cancel, Operation::CancelJob, id, reply)
            }
            Command::PauseJob { id, reply } => {
                self.job_action(ctx, Action::Pause, Operation::HoldJob, id, reply)
            }
            Command::ResumeJob { id, reply } => {
                self.job_action(ctx, Action::Resume, Operation::ReleaseJob, id, reply)
            }
            Command::RetryJob { id, reply } => {
                self.job_action(ctx, Action::Retry, Operation::RestartJob, id, reply)
            }
            Command::Refresh { reply } => self.refresh(ctx, reply),
        }
    }

    fn refresh(&mut self, ctx: &Ctx<Self>, reply: Reply) {
        self.refresh_replies.push(reply);
        self.fetch(ctx);
    }

    fn job_action(
        &self,
        ctx: &Ctx<Self>,
        action: Action,
        operation: Operation,
        id: u32,
        reply: Reply,
    ) {
        let uri = match self.config.uri() {
            Ok(uri) => uri,
            Err(reason) => {
                let _ = reply.send(Err(CommandError::Unavailable(reason).into()));
                return;
            }
        };
        let local = matches!(uri.host(), Some("localhost" | "127.0.0.1" | "[::1]"))
            && uri.port_u16() == Some(631);
        ctx.spawn_detached(move |_ctx| async move {
            let outcome = match call::perform(local, uri, operation, id).await {
                Ok(()) => Ok(()),
                Err(error) => classify(action, &error).map_err(PrintingError::Failed),
            };
            let _ = reply.send(outcome);
        });
    }
}

#[cfg(test)]
mod tests;
