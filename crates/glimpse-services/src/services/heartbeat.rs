use glimpse_contracts::{
    Command as _, HeartbeatInterval, HeartbeatReset, HeartbeatSetInterval, HeartbeatTick, Message,
};
use glimpse_ipc::{CallError, ErrorCode};
use serde_json::Value;
use tokio::time;

use crate::{
    broker::Responder,
    context::{Ctx, SourceGuard},
    publisher::Publisher,
    service::{Input, Service, ServiceError, decode_args, unknown_command},
};

const DEFAULT_PERIOD_MS: u64 = 1000;
const MIN_PERIOD_MS: u64 = 10;
const MAX_PERIOD_MS: u64 = 60_000;

pub enum Event {
    Tick,
}

#[derive(Debug)]
pub enum Command {
    Reset,
    SetInterval { period_ms: u64 },
}

/// A development fixture: the one service that publishes on its own, so `get`, `topics` and `watch`
/// have something live to show before any real service works, and the one that answers commands, so
/// `call` and `methods` do too. The counter is what makes it visible — an unchanging payload would
/// be swallowed by the publisher's equality gate and nothing would arrive after the first tick.
pub struct Heartbeat {
    tick: Publisher<HeartbeatTick>,
    count: u64,
    period_ms: u64,
    timer: SourceGuard,
}

impl Service for Heartbeat {
    const NAME: &'static str = "heartbeat";
    const TOPICS: &'static [&'static str] = &[HeartbeatTick::NAME];
    const METHODS: &'static [&'static str] = &[HeartbeatReset::NAME, HeartbeatSetInterval::NAME];

    type Config = ();
    type Command = Command;
    type Event = Event;

    fn decode(method: &str, args: Value) -> Result<Self::Command, CallError> {
        match method {
            HeartbeatReset::NAME => Ok(Command::Reset),
            HeartbeatSetInterval::NAME => {
                let HeartbeatSetInterval { period_ms } = decode_args(args)?;
                Ok(Command::SetInterval { period_ms })
            }
            _ => Err(unknown_command(Self::NAME, method)),
        }
    }

    async fn start(ctx: &Ctx<Self>, _config: Self::Config) -> Result<Self, ServiceError> {
        tracing::debug!("starting heartbeat service");
        let period = time::Duration::from_millis(DEFAULT_PERIOD_MS);
        Ok(Self {
            count: 0,
            period_ms: DEFAULT_PERIOD_MS,
            tick: ctx.publisher::<HeartbeatTick>(),
            timer: ctx.interval(period, |_ctx| async { Event::Tick }),
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Tick) => {
                self.count += 1;
                self.tick.set(HeartbeatTick { count: self.count });
            }
            Input::Command(Command::Reset, responder) => {
                self.count = 0;
                self.tick.set(HeartbeatTick { count: 0 });
                responder.ok(());
            }
            Input::Command(Command::SetInterval { period_ms }, responder) => {
                self.set_interval(ctx, period_ms, responder);
            }
            Input::Config(()) => {}
        }
    }

    fn peek_config(_config: &glimpse_config::Config) -> Self::Config {}
}

impl Heartbeat {
    fn set_interval(&mut self, ctx: &Ctx<Self>, period_ms: u64, responder: Responder) {
        if !(MIN_PERIOD_MS..=MAX_PERIOD_MS).contains(&period_ms) {
            return responder.fail(CallError::new(
                ErrorCode::InvalidArgs,
                format!("period_ms must be {MIN_PERIOD_MS}..={MAX_PERIOD_MS}, got {period_ms}"),
            ));
        }

        let previous_ms = std::mem::replace(&mut self.period_ms, period_ms);
        let period = time::Duration::from_millis(period_ms);
        // Assigning drops the old guard, which aborts the task behind it; there is no separate
        // cancellation to remember.
        self.timer = ctx.interval(period, |_ctx| async { Event::Tick });
        responder.ok(HeartbeatInterval { previous_ms });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glimpse_dbus::Buses;
    use tokio::sync::oneshot;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{BrokerHandle, MockBroker, service::ServiceRuntime};

    /// Drives one command through the whole path a `call` takes inside the daemon: the sender's
    /// non-blocking dispatch, the serial handler, and the responder the handler answers.
    async fn call(command: Command) -> Result<Value, CallError> {
        let broker: Arc<dyn BrokerHandle> = Arc::new(MockBroker::default());
        let cancel = CancellationToken::new();
        let mut runtime = ServiceRuntime::<Heartbeat>::new(
            broker,
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );

        let (reply, answer) = oneshot::channel();
        runtime.sender().dispatch(command, Responder::new(reply));

        let running = tokio::spawn(async move { runtime.run(()).await });
        let outcome = answer.await.expect("the handler answered");
        cancel.cancel();
        let _ = running.await;
        outcome
    }

    #[tokio::test]
    async fn set_interval_reports_the_period_it_replaced() {
        let value = call(Command::SetInterval { period_ms: 250 })
            .await
            .expect("accepted");
        assert_eq!(value["previous_ms"], DEFAULT_PERIOD_MS);
    }

    /// A rejected period must not be retryable: retrying the same argument cannot start working.
    #[tokio::test]
    async fn set_interval_refuses_a_period_outside_the_supported_range() {
        let error = call(Command::SetInterval { period_ms: 0 })
            .await
            .expect_err("refused");
        assert_eq!(error.code, ErrorCode::InvalidArgs);
        assert!(!error.retryable);
    }

    #[test]
    fn a_mistyped_argument_is_refused_as_an_argument_not_as_a_missing_command() {
        let error = Heartbeat::decode(
            HeartbeatSetInterval::NAME,
            serde_json::json!({ "period_ms": "fast" }),
        )
        .expect_err("refused");
        assert_eq!(error.code, ErrorCode::InvalidArgs);
    }

    #[test]
    fn a_name_the_service_does_not_declare_is_refused() {
        let error = Heartbeat::decode("heartbeat.explode", Value::Null).expect_err("refused");
        assert_eq!(error.code, ErrorCode::UnknownCommand);
    }
}
