use futures_util::{StreamExt, stream};

use super::*;

#[test]
fn only_a_running_service_has_no_reason_it_is_unavailable() {
    assert_eq!(ServiceState::Running.unavailable_reason(), None);
    assert_eq!(
        ServiceState::Starting.unavailable_reason(),
        Some("starting")
    );
    assert_eq!(
        ServiceState::Degraded {
            reason: "another gamma client holds the outputs".to_owned()
        }
        .unavailable_reason(),
        Some("another gamma client holds the outputs")
    );
    assert_eq!(
        ServiceState::Stopped {
            reason: Some("name taken".to_owned())
        }
        .unavailable_reason(),
        Some("name taken")
    );
    assert_eq!(
        ServiceState::Stopped { reason: None }.unavailable_reason(),
        Some("stopped"),
        "a consumer asking why nothing is served must not be told nothing"
    );
}

struct Panicky;

impl Service for Panicky {
    const NAME: &'static str = "panicky";

    type Config = NoConfig;
    type State = ();
    type Command = tokio::sync::oneshot::Sender<()>;
    type Event = ();
    type Dependencies = ();
    type SubKey = ();

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    async fn start(
        _ctx: &Ctx<Self>,
        _config: Self::Config,
        _dependencies: Self::Dependencies,
    ) -> Self {
        Self
    }

    async fn handle(&mut self, _ctx: &Ctx<Self>, _input: Input<Self>) {
        panic!("the backend said something unrepeatable");
    }
}

#[tokio::test]
async fn a_panicking_handler_stops_its_service() {
    let (mut runtime, handle) = ServiceRuntime::<Panicky>::new(
        NoConfig,
        Buses::unavailable("no bus in tests"),
        CancellationToken::new(),
    );
    runtime
        .sender()
        .send(Input::Event(()))
        .await
        .expect("queued");
    runtime.run(()).await;

    assert!(matches!(
        &*handle.health().borrow(),
        ServiceState::Stopped { reason: Some(reason) } if reason.contains("unrepeatable")
    ));
}

#[tokio::test]
async fn a_command_queued_behind_a_panicking_handler_is_settled_rather_than_left_waiting() {
    let (mut runtime, handle) = ServiceRuntime::<Panicky>::new(
        NoConfig,
        Buses::unavailable("no bus in tests"),
        CancellationToken::new(),
    );
    runtime
        .sender()
        .send(Input::Event(()))
        .await
        .expect("queued");
    let (reply, answer) = tokio::sync::oneshot::channel();
    handle.command(reply).expect("queued");

    runtime.run(()).await;

    let settled = tokio::time::timeout(std::time::Duration::from_secs(1), answer)
        .await
        .expect("a caller is not left waiting on a service that already stopped");
    assert!(settled.is_err(), "the responder is dropped, not answered");
}

#[derive(Debug, Clone, PartialEq)]
struct Tuning(u8);

impl From<&glimpse_config::Config> for Tuning {
    fn from(_document: &glimpse_config::Config) -> Self {
        Self(0)
    }
}

struct Tunable {
    seen: u32,
}

impl Service for Tunable {
    const NAME: &'static str = "tunable";

    type Config = Tuning;
    type State = (u8, u32);
    type Command = ();
    type Event = ();
    type Dependencies = ();
    type SubKey = ();

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    async fn start(
        _ctx: &Ctx<Self>,
        _config: Self::Config,
        _dependencies: Self::Dependencies,
    ) -> Self {
        Self { seen: 0 }
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        if let Input::Config(Tuning(value)) = input {
            self.seen += 1;
            ctx.publisher().set((value, self.seen));
        }
    }
}

#[tokio::test]
async fn an_unchanged_configuration_never_reaches_the_handler() {
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<Tunable>::new(
        Tuning(1),
        Buses::unavailable("no bus in tests"),
        cancel.clone(),
    );
    let sender = runtime.sender();
    let mut state = handle.subscribe();
    let running = tokio::spawn(async move { runtime.run(()).await });

    sender.try_send(Input::Config(Tuning(1))).expect("queued");
    sender.try_send(Input::Config(Tuning(2))).expect("queued");
    state
        .wait_for(|state| *state == (2, 1))
        .await
        .expect("a reload matching the config it started on is not a change");

    sender.try_send(Input::Config(Tuning(2))).expect("queued");
    sender.try_send(Input::Config(Tuning(1))).expect("queued");
    state
        .wait_for(|state| *state == (1, 2))
        .await
        .expect("going back is a change, repeating is not");

    cancel.cancel();
    running.await.expect("joined");
}

struct Armable {
    armed: bool,
}

impl Service for Armable {
    const NAME: &'static str = "armable";

    type Config = NoConfig;
    type State = u8;
    type Command = ();
    type Event = u8;
    type Dependencies = ();
    type SubKey = ();

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        self.armed
            .then(|| {
                Sub::stream((), |_ctx| async {
                    stream::once(async { 7 }).chain(stream::pending())
                })
            })
            .into_iter()
            .collect()
    }

    async fn start(
        _ctx: &Ctx<Self>,
        _config: Self::Config,
        _dependencies: Self::Dependencies,
    ) -> Self {
        Self { armed: false }
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Command(()) => self.armed = true,
            Input::Event(value) => {
                ctx.publisher().set(value);
            }
            Input::Config(_) => {}
        }
    }
}

#[tokio::test]
async fn a_source_declared_by_a_handler_is_started_by_the_runtime() {
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<Armable>::new(
        NoConfig,
        Buses::unavailable("no bus in tests"),
        cancel.clone(),
    );
    runtime
        .sender()
        .send(Input::Command(()))
        .await
        .expect("queued");

    let running = tokio::spawn(async move { runtime.run(()).await });
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    assert_eq!(handle.snapshot(), 7);
    cancel.cancel();
    let _ = running.await;
    assert!(
        handle.command(()).is_err(),
        "a stopped service rejects commands"
    );
}

struct NeedsTheBus;

impl Service for NeedsTheBus {
    const NAME: &'static str = "needs-the-bus";

    type Config = NoConfig;
    type State = ();
    type Command = ();
    type Event = ();
    type Dependencies = ();
    type SubKey = ();

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    async fn start(
        ctx: &Ctx<Self>,
        _config: Self::Config,
        _dependencies: Self::Dependencies,
    ) -> Self {
        if let Err(reason) = ctx.system_bus() {
            ctx.degraded(format!("no system bus: {reason}"));
        }
        Self
    }

    async fn handle(&mut self, _ctx: &Ctx<Self>, _input: Input<Self>) {}
}

#[tokio::test]
async fn a_service_without_a_bus_degrades_and_keeps_running() {
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<NeedsTheBus>::new(
        NoConfig,
        Buses::unavailable("connect failed"),
        cancel.clone(),
    );
    let mut health = handle.health();
    let running = tokio::spawn(async move { runtime.run(()).await });

    health.changed().await.expect("health changes");
    assert!(matches!(
        &*health.borrow(),
        ServiceState::Degraded { reason } if reason.contains("connect failed")
    ));

    cancel.cancel();
    running.await.expect("runtime joins");
}

#[tokio::test]
async fn dropped_command_replies_keep_the_command_specific_reason() {
    let (mut runtime, handle) = ServiceRuntime::<crate::Heartbeat>::new(
        NoConfig,
        Buses::unavailable("no bus in tests"),
        CancellationToken::new(),
    );
    let mut reset = std::pin::pin!(handle.reset());
    assert!(futures_util::poll!(&mut reset).is_pending());
    drop(runtime.inbox.recv().await.expect("queued reset"));
    assert_eq!(
        reset.await,
        Err(CommandError::Unavailable(
            "`heartbeat` stopped before resetting".to_owned()
        ))
    );

    let mut interval = std::pin::pin!(handle.set_interval(100));
    assert!(futures_util::poll!(&mut interval).is_pending());
    drop(runtime.inbox.recv().await.expect("queued interval"));
    assert_eq!(
        interval.await,
        Err(CommandError::Unavailable(
            "`heartbeat` stopped before changing its interval".to_owned()
        ))
    );
}

#[tokio::test]
async fn a_full_or_closed_inbox_keeps_the_command_submission_error() {
    let (mut runtime, handle) = ServiceRuntime::<crate::Heartbeat>::new(
        NoConfig,
        Buses::unavailable("no bus in tests"),
        CancellationToken::new(),
    );
    for _ in 0..INBOX_SIZE {
        runtime
            .sender()
            .try_send(Input::Config(NoConfig))
            .expect("queued");
    }
    let rejected = Err(CommandError::Unavailable(
        "`heartbeat` is not accepting commands".to_owned(),
    ));
    assert_eq!(handle.reset().await, rejected);
    runtime.close_inbox();
    assert_eq!(handle.reset().await, rejected);
}
