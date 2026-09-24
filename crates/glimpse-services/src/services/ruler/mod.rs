mod history;
mod runner;

use std::sync::Arc;

use tokio::sync::{oneshot, watch};

use crate::{
    ServiceState,
    context::{Ctx, SourceGuard},
    publisher::Publisher,
    selection::{Offer, Selection},
    service::{CommandError, Input, Service, ServiceEndpoint, ServiceError},
};

use history::History;
pub use history::Measurement;
pub use runner::{Measured, ProcessRunner, Request as MeasureRequest, Runner};

const TEXT: &str = "text/plain;charset=utf-8";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    request: MeasureRequest,
    limit: usize,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        let ruler = &document.ruler;
        Self {
            request: MeasureRequest {
                lens_radius: ruler.lens_radius.clamp(40, 400),
                max_zoom: ruler.max_zoom.clamp(2, 64),
            },
            limit: ruler.history_limit,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RulerState {
    pub history: Vec<Measurement>,
    pub measuring: bool,
}

pub enum Command {
    Measure {
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    Copy {
        id: u32,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
}

pub enum Event {
    Measured(Result<Vec<Measurement>, String>),
}

pub struct Dependencies {
    pub selection: Arc<dyn Selection>,
    pub runner: Arc<dyn Runner>,
}

#[derive(Clone)]
pub struct RulerHandle(ServiceEndpoint<Ruler>);

impl RulerHandle {
    pub fn snapshot(&self) -> RulerState {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> watch::Receiver<RulerState> {
        self.0.subscribe()
    }

    pub fn health(&self) -> watch::Receiver<ServiceState> {
        self.0.health()
    }

    pub async fn measure(&self) -> Result<(), CommandError> {
        self.ask(|reply| Command::Measure { reply }, "measuring")
            .await
    }

    pub async fn copy(&self, id: u32) -> Result<(), CommandError> {
        self.ask(|reply| Command::Copy { id, reply }, "copying")
            .await
    }

    async fn ask(
        &self,
        build: impl FnOnce(oneshot::Sender<Result<(), CommandError>>) -> Command,
        doing: &str,
    ) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(build(reply))?;
        result
            .await
            .map_err(|_| CommandError::Unavailable(format!("the ruler stopped while {doing}")))?
    }
}

pub struct Ruler {
    state: Publisher<RulerState>,
    selection: Arc<dyn Selection>,
    runner: Arc<dyn Runner>,
    history: History,
    config: Config,
    pending: Option<(SourceGuard, oneshot::Sender<Result<(), CommandError>>)>,
}

impl Service for Ruler {
    const NAME: &'static str = "ruler";

    type Config = Config;
    type State = RulerState;
    type Handle = RulerHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = ();

    fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
        RulerHandle(endpoint)
    }

    fn initial_state(_config: &Self::Config) -> Self::State {
        RulerState {
            history: Vec::new(),
            measuring: false,
        }
    }

    async fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        dependencies: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        Ok(Self {
            state: ctx.publisher(),
            selection: dependencies.selection,
            runner: dependencies.runner,
            history: History::new(config.limit),
            config,
            pending: None,
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Command(Command::Measure { reply }) => {
                if self.pending.is_some() {
                    let _ = reply.send(Err(CommandError::LimitExceeded(
                        "a measurement is already in progress".to_owned(),
                    )));
                    return;
                }
                let runner = Arc::clone(&self.runner);
                let request = self.config.request;
                let guard =
                    ctx.spawn(
                        move |_ctx| async move { Event::Measured(runner.measure(request).await) },
                    );
                self.pending = Some((guard, reply));
            }
            Input::Command(Command::Copy { id, reply }) => {
                let _ = reply.send(self.copy(id));
            }
            Input::Event(Event::Measured(outcome)) => {
                let Some((_guard, reply)) = self.pending.take() else {
                    return;
                };
                let _ = reply.send(self.measured(outcome));
            }
            Input::Config(_) => return,
        }
        self.publish();
    }
}

impl Ruler {
    fn measured(&mut self, outcome: Result<Vec<Measurement>, String>) -> Result<(), CommandError> {
        let measurements = outcome.map_err(CommandError::Unavailable)?;
        self.history.push_all(measurements);
        Ok(())
    }

    fn copy(&self, id: u32) -> Result<(), CommandError> {
        let entry = self.history.get(id).ok_or_else(|| {
            CommandError::InvalidArgument("that measurement is no longer in history".to_owned())
        })?;
        self.offer(format!("{:.1}px", entry.distance))
    }

    fn offer(&self, text: String) -> Result<(), CommandError> {
        self.selection
            .offer(Offer {
                mime: TEXT.to_owned(),
                data: Arc::from(text.into_bytes()),
            })
            .map_err(CommandError::Unavailable)
    }

    fn publish(&self) {
        self.state.set(RulerState {
            history: self.history.entries(),
            measuring: self.pending.is_some(),
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use glimpse_dbus::Buses;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::selection::FakeSelection;

    struct FakeRunner {
        answers: Mutex<Vec<Result<Vec<Measurement>, String>>>,
        requests: Mutex<Vec<MeasureRequest>>,
    }

    impl Runner for FakeRunner {
        fn measure(&self, request: MeasureRequest) -> Measured {
            self.requests.lock().unwrap().push(request);
            let answer = self.answers.lock().unwrap().remove(0);
            Box::pin(async move { answer })
        }
    }

    struct Harness {
        service: Ruler,
        selection: FakeSelection,
        runner: Arc<FakeRunner>,
        ctx: Ctx<Ruler>,
        inbox: mpsc::Receiver<Input<Ruler>>,
        state: watch::Receiver<RulerState>,
        _cancel: CancellationToken,
    }

    fn document(limit: usize) -> glimpse_config::Config {
        let mut document = glimpse_config::Config::default();
        document.ruler.history_limit = limit;
        document
    }

    fn measurement(distance: f64) -> Measurement {
        Measurement {
            id: 0,
            from_x: 1,
            from_y: 2,
            to_x: 3,
            to_y: 4,
            dx: 2,
            dy: 2,
            distance,
            angle: 45.0,
        }
    }

    async fn harness(answers: Vec<Result<Vec<Measurement>, String>>) -> Harness {
        let config = Config::from(&document(8));
        let (events, inbox) = mpsc::channel(32);
        let cancel = CancellationToken::new();
        let (health, _health) = watch::channel(ServiceState::Starting);
        let (published, state) = watch::channel(Ruler::initial_state(&config));
        let ctx = Ctx::<Ruler>::new(
            events,
            &cancel,
            published,
            health,
            Buses::unavailable("no bus in tests"),
        );
        let selection = FakeSelection::default();
        let runner = Arc::new(FakeRunner {
            answers: Mutex::new(answers),
            requests: Mutex::new(Vec::new()),
        });
        let service = Ruler::start(
            &ctx,
            config,
            Dependencies {
                selection: Arc::new(selection.clone()),
                runner: runner.clone(),
            },
        )
        .await
        .unwrap();
        Harness {
            service,
            selection,
            runner,
            ctx,
            inbox,
            state,
            _cancel: cancel,
        }
    }

    impl Harness {
        async fn feed(&mut self, input: Input<Ruler>) {
            self.service.handle(&self.ctx, input).await;
        }

        async fn measure(&mut self) -> oneshot::Receiver<Result<(), CommandError>> {
            let (reply, result) = oneshot::channel();
            self.feed(Input::Command(Command::Measure { reply })).await;
            result
        }

        async fn settle(&mut self) {
            let input = self.inbox.recv().await.unwrap();
            self.feed(input).await;
        }

        fn offered(&self) -> Vec<String> {
            self.selection
                .offered()
                .into_iter()
                .map(|offer| String::from_utf8(offer.data.to_vec()).unwrap())
                .collect()
        }
    }

    #[tokio::test]
    async fn a_measurement_lands_in_history_in_confirmation_order_and_is_capped() {
        let mut harness = harness(vec![Ok(vec![measurement(1.0), measurement(2.0)])]).await;

        let result = harness.measure().await;
        assert!(harness.state.borrow().measuring);
        harness.settle().await;

        assert_eq!(result.await.unwrap(), Ok(()));
        let state = harness.state.borrow().clone();
        assert!(!state.measuring);
        assert_eq!(state.history.len(), 2);
        assert_eq!(state.history[0].distance, 2.0);
        assert_eq!(state.history[1].distance, 1.0);
        assert_eq!(
            harness.runner.requests.lock().unwrap()[0],
            MeasureRequest {
                lens_radius: 106,
                max_zoom: 30,
            }
        );
    }

    #[tokio::test]
    async fn a_cancelled_measurement_changes_nothing_and_is_not_an_error() {
        let mut harness = harness(vec![Ok(Vec::new())]).await;

        let result = harness.measure().await;
        harness.settle().await;

        assert_eq!(result.await.unwrap(), Ok(()));
        assert!(harness.state.borrow().history.is_empty());
        assert!(harness.offered().is_empty());
    }

    #[tokio::test]
    async fn a_failed_measurement_reports_what_the_ruler_said() {
        let mut harness = harness(vec![Err("no screencopy".to_owned())]).await;

        let result = harness.measure().await;
        harness.settle().await;

        assert_eq!(
            result.await.unwrap(),
            Err(CommandError::Unavailable("no screencopy".to_owned()))
        );
        assert!(!harness.state.borrow().measuring);
    }

    #[tokio::test]
    async fn a_second_measure_while_one_is_open_is_refused() {
        let mut harness = harness(vec![Ok(vec![measurement(1.0)])]).await;

        let _first = harness.measure().await;
        tokio::task::yield_now().await;
        let second = harness.measure().await;

        assert!(matches!(
            second.await.unwrap(),
            Err(CommandError::LimitExceeded(_))
        ));
        assert_eq!(harness.runner.requests.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_history_entry_copies_its_rendered_text_and_a_missing_one_is_refused() {
        let mut harness = harness(vec![Ok(vec![measurement(12.3)])]).await;
        let _measured = harness.measure().await;
        harness.settle().await;
        let id = harness.state.borrow().history[0].id;

        let (reply, result) = oneshot::channel();
        harness
            .feed(Input::Command(Command::Copy { id, reply }))
            .await;
        assert_eq!(result.await.unwrap(), Ok(()));
        assert_eq!(harness.offered(), ["12.3px"]);

        let (reply, result) = oneshot::channel();
        harness
            .feed(Input::Command(Command::Copy {
                id: id + 100,
                reply,
            }))
            .await;
        assert!(matches!(
            result.await.unwrap(),
            Err(CommandError::InvalidArgument(_))
        ));
    }
}
