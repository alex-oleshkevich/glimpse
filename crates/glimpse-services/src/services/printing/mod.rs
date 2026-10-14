mod call;
mod failure;
mod source;

use std::time::Duration;

use tokio::sync::oneshot;

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
    poll_active: Duration,
    poll_idle: Duration,
    server_url: Option<String>,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self {
            poll_active: Duration::from_secs(document.printing.poll_active),
            poll_idle: Duration::from_secs(document.printing.poll_idle),
            server_url: document.printing.server_url.clone(),
        }
    }
}

impl Config {
    fn uri(&self) -> Result<Uri, String> {
        self.server_url
            .as_deref()
            .unwrap_or(DEFAULT_SERVER_URL)
            .parse::<Uri>()
            .map_err(|error| format!("invalid printing server url: {error}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cadence {
    Active,
    Idle,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Watch {
    Poll(Cadence, Duration, u64),
    Notifier(u64),
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
    Polled(PrintingState),
    Unavailable(String),
}

pub struct Printing {
    state: Publisher<PrintingState>,
    config: Config,
    active: bool,
    generation: u64,
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
        let cadence = if self.active {
            Cadence::Active
        } else {
            Cadence::Idle
        };
        let period = if self.active {
            self.config.poll_active
        } else {
            self.config.poll_idle
        };
        let poll_config = self.config.clone();
        let notifier_config = self.config.clone();
        let floor = self.config.poll_active;
        let generation = self.generation;

        vec![
            Sub::interval(
                Watch::Poll(cadence, period, generation),
                period,
                move |_ctx| {
                    let config = poll_config.clone();
                    async move { source::fetch(&config).await }
                },
            ),
            Sub::stream(Watch::Notifier(generation), move |ctx| {
                source::notifier(ctx, notifier_config, floor)
            }),
        ]
    }

    async fn start(ctx: &Ctx<Self>, config: Self::Config, _: Self::Dependencies) -> Self {
        Self {
            state: ctx.publisher(),
            config,
            active: false,
            generation: 0,
        }
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Command(command) => self.dispatch(ctx, command).await,
            Input::Config(config) => {
                self.config = config;
                self.generation = self.generation.wrapping_add(1);
            }
            Input::Event(Event::Polled(state)) => {
                ctx.running();
                self.active = state.jobs.iter().any(|job| job.state.is_active());
                self.state.set(state);
            }
            Input::Event(Event::Unavailable(reason)) => self.note_unavailable(ctx, reason),
        }
    }
}

impl Printing {
    fn note_unavailable(&mut self, ctx: &Ctx<Self>, reason: String) {
        if !ctx.is_degraded() {
            tracing::debug!(reason = %reason, "printing: CUPS unreachable");
        }
        ctx.degraded(reason);
        self.active = false;
        self.state.set(PrintingState::default());
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

    fn refresh(&self, ctx: &Ctx<Self>, reply: Reply) {
        let config = self.config.clone();
        ctx.spawn_detached(move |ctx| async move {
            let event = source::fetch(&config).await;
            let sent = ctx.events().send(Input::Event(event)).await.is_ok();
            let _ = reply.send(if sent {
                Ok(())
            } else {
                Err(CommandError::Unavailable(
                    "printing stopped before the refresh completed".to_owned(),
                )
                .into())
            });
        });
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
