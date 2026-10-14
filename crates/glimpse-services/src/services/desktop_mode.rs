use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use futures_util::FutureExt as _;
use futures_util::future::BoxFuture;
use glimpse_config::AutoDark;
use glimpse_dbus::settings::{Mode, ModeStatus, SolarPhase, SolarStatus};
use tokio::{
    sync::{oneshot, watch},
    time::{Duration, timeout},
};

use crate::{
    CommandError, Ctx, Input, Publisher, Service, ServiceEndpoint, Sub, context::SourceGuard,
};

pub trait ModeWriter: Send + Sync {
    fn write(&self, mode: Option<Mode>) -> BoxFuture<'_, Result<(), String>>;
}

pub struct ModeDependencies {
    pub solar: watch::Receiver<Option<SolarStatus>>,
    pub observed: watch::Receiver<Result<Option<Mode>, String>>,
    pub writer: Box<dyn ModeWriter>,
}

type Reply = oneshot::Sender<Result<(), CommandError>>;

pub enum Command {
    Temporary(Mode, Reply),
    Resume(Reply),
}

pub enum Event {
    Solar,
    Observed(Result<Option<Mode>, String>),
    Written(u64, Result<(), String>, Option<Reply>),
}

#[derive(PartialEq, Eq, Hash)]
pub enum Watch {
    Solar,
    Observed,
}

pub type DesktopModeHandle = ServiceEndpoint<DesktopMode>;

impl ServiceEndpoint<DesktopMode> {
    pub async fn set_temporary(&self, mode: Mode) -> Result<(), CommandError> {
        self.call("desktop mode stopped", |reply| {
            Command::Temporary(mode, reply)
        })
        .await?
    }

    pub async fn resume(&self) -> Result<(), CommandError> {
        self.call("desktop mode stopped", Command::Resume).await?
    }
}

struct Write {
    revision: u64,
    mode: Option<Mode>,
    reply: Option<Reply>,
}

pub struct DesktopMode {
    state: Publisher<ModeStatus>,
    status: ModeStatus,
    solar: watch::Receiver<Option<SolarStatus>>,
    observed: watch::Receiver<Result<Option<Mode>, String>>,
    writer: Arc<dyn ModeWriter>,
    phase: Option<Mode>,
    baseline: Option<Option<Mode>>,
    last_write: Option<Option<Mode>>,
    failure: Option<String>,
    revision: u64,
    effect: Option<(SourceGuard, Option<Mode>)>,
    queued: Option<Write>,
}

impl Service for DesktopMode {
    const NAME: &'static str = "desktop_mode";
    type Config = AutoDark;
    type State = ModeStatus;
    type Command = Command;
    type Event = Event;
    type Dependencies = ModeDependencies;
    type SubKey = Watch;

    fn initial_state(config: &AutoDark) -> ModeStatus {
        ModeStatus {
            automatic: *config == AutoDark::Auto,
            requested: None,
            observed: None,
            temporary: None,
        }
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        vec![
            Sub::watch(
                Watch::Solar,
                self.solar.clone(),
                |_| Event::Solar,
                Event::Solar,
            ),
            Sub::watch(
                Watch::Observed,
                self.observed.clone(),
                Event::Observed,
                Event::Observed(Err("desktop mode observer stopped".into())),
            ),
        ]
    }

    async fn start(ctx: &Ctx<Self>, config: AutoDark, dependencies: ModeDependencies) -> Self {
        let mut service = Self {
            state: ctx.publisher(),
            status: Self::initial_state(&config),
            solar: dependencies.solar,
            observed: dependencies.observed,
            writer: Arc::from(dependencies.writer),
            phase: None,
            baseline: None,
            last_write: None,
            failure: None,
            revision: 0,
            effect: None,
            queued: None,
        };
        service.status.observed = service.observed.borrow().clone().ok().flatten();
        service.apply(ctx, None);
        service.publish(ctx);
        service
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Solar) => {
                let phase = self.latest_phase();
                if phase != self.phase {
                    self.phase = phase;
                    if self.status.automatic && self.status.temporary.is_none() {
                        self.apply(ctx, None);
                    }
                }
            }
            Input::Event(Event::Observed(observed)) => {
                self.status.observed = observed.ok().flatten();
            }
            Input::Event(Event::Written(revision, result, reply)) => {
                if let Some((_, mode)) = self.effect.take()
                    && result.is_ok()
                {
                    self.last_write = Some(mode);
                }
                let result = if revision == self.revision {
                    self.failure = result
                        .as_ref()
                        .err()
                        .map(|reason| glimpse_utils::clean(reason, 240));
                    result.map_err(CommandError::Unavailable)
                } else {
                    Err(CommandError::Unavailable(
                        "desktop mode request superseded".into(),
                    ))
                };
                self.publish(ctx);
                if let Some(reply) = reply {
                    let _ = reply.send(result);
                }
                if let Some(write) = self.queued.take() {
                    self.dispatch(ctx, write);
                }
                return;
            }
            Input::Command(Command::Temporary(mode, reply)) => {
                match self.observed.borrow().clone() {
                    Ok(observed) => {
                        self.baseline.get_or_insert(observed);
                    }
                    Err(reason) => {
                        let _ = reply.send(Err(CommandError::Unavailable(glimpse_utils::clean(
                            &reason, 240,
                        ))));
                        return;
                    }
                }
                self.status.temporary = Some(mode);
                self.apply(ctx, Some(reply));
            }
            Input::Command(Command::Resume(reply)) => {
                self.status.temporary = None;
                self.apply(ctx, Some(reply));
            }
            Input::Config(config) => {
                self.status.automatic = config == AutoDark::Auto;
                self.status.temporary = None;
                self.apply(ctx, None);
            }
        }
        self.publish(ctx);
    }

    async fn stop(self, _ctx: &Ctx<Self>) {
        if let Some(mode) = self.restore() {
            match timeout(Duration::from_secs(3), self.writer.write(mode)).await {
                Ok(Ok(())) => {}
                result => tracing::warn!(?result, "cannot release temporary desktop mode"),
            }
        }
    }
}

impl DesktopMode {
    fn latest_phase(&self) -> Option<Mode> {
        if self.solar.has_changed().is_err() {
            return None;
        }
        self.solar
            .borrow()
            .as_ref()
            .map(|status| match status.phase {
                SolarPhase::Day => Mode::Light,
                SolarPhase::Night => Mode::Dark,
            })
    }

    fn restore(&self) -> Option<Option<Mode>> {
        let baseline = self.baseline?;
        let observed = self.observed.borrow().as_ref().ok().copied()?;
        let pending = self.effect.as_ref().map(|(_, mode)| *mode);
        (Some(observed) == self.last_write
            || Some(observed) == pending
            || (pending.is_some() && observed == baseline))
            .then_some(baseline)
    }

    fn apply(&mut self, ctx: &Ctx<Self>, reply: Option<Reply>) {
        self.phase = self.latest_phase();
        self.revision += 1;
        self.failure = None;
        let target = if let Some(mode) = self.status.temporary {
            Some(Some(mode))
        } else if self.status.automatic {
            self.phase.map(Some)
        } else {
            self.restore()
        };
        if self.status.temporary.is_none() {
            self.baseline = None;
        }
        self.status.requested = target.flatten();
        if let Some(write) = self.queued.take()
            && let Some(reply) = write.reply
        {
            let _ = reply.send(Err(CommandError::Unavailable(
                "desktop mode request superseded".into(),
            )));
        }
        let Some(mode) = target else {
            self.publish(ctx);
            if let Some(reply) = reply {
                let result = if self.status.automatic {
                    Err(CommandError::Unavailable("no solar phase yet".into()))
                } else {
                    Ok(())
                };
                let _ = reply.send(result);
            }
            return;
        };
        let write = Write {
            revision: self.revision,
            mode,
            reply,
        };
        if self.effect.is_some() {
            self.queued = Some(write);
        } else {
            self.dispatch(ctx, write);
        }
    }

    fn dispatch(&mut self, ctx: &Ctx<Self>, write: Write) {
        let writer = self.writer.clone();
        let effect = ctx.spawn(move |_ctx| async move {
            let written = AssertUnwindSafe(async { writer.write(write.mode).await }).catch_unwind();
            let result = match timeout(Duration::from_secs(3), written).await {
                Ok(Ok(result)) => result,
                Ok(Err(_)) => Err("desktop mode writer panicked".into()),
                Err(_) => Err("desktop mode write timed out".into()),
            };
            Event::Written(write.revision, result, write.reply)
        });
        self.effect = Some((effect, write.mode));
    }

    fn publish(&self, ctx: &Ctx<Self>) {
        self.state.set(self.status.clone());
        let observed = self.observed.borrow();
        let reason = self
            .failure
            .as_deref()
            .or_else(|| {
                self.observed
                    .has_changed()
                    .is_err()
                    .then_some("desktop mode observer stopped")
            })
            .or_else(|| observed.as_ref().err().map(String::as_str))
            .or_else(|| {
                (self.status.automatic && self.status.temporary.is_none() && self.phase.is_none())
                    .then_some("no solar phase yet")
            });
        match reason {
            Some(reason) => ctx.degraded(glimpse_utils::clean(reason, 240)),
            None => ctx.running(),
        }
    }
}

#[cfg(test)]
mod tests;
