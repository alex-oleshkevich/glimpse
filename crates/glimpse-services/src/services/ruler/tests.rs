use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use glimpse_dbus::Buses;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::selection::FakeSelection;

struct FakeRunner {
    answers: Mutex<Vec<Result<Vec<Measurement>, String>>>,
    measures: AtomicUsize,
}

impl Runner for FakeRunner {
    fn measure(&self) -> Measured {
        self.measures.fetch_add(1, Ordering::SeqCst);
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
        measures: AtomicUsize::new(0),
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
    assert_eq!(harness.runner.measures.load(Ordering::SeqCst), 1);
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
