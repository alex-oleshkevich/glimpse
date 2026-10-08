use std::sync::Mutex;

use glimpse_dbus::Buses;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::selection::FakeSelection;

struct FakePicker {
    answers: Mutex<Vec<Result<Option<[u8; 3]>, String>>>,
    requests: Mutex<usize>,
}

impl Picker for FakePicker {
    fn pick(&self) -> Picked {
        *self.requests.lock().unwrap() += 1;
        let answer = self.answers.lock().unwrap().remove(0);
        Box::pin(async move { answer })
    }
}

struct Harness {
    service: ColorPicker,
    selection: FakeSelection,
    picker: Arc<FakePicker>,
    ctx: Ctx<ColorPicker>,
    inbox: mpsc::Receiver<Input<ColorPicker>>,
    state: watch::Receiver<ColorPickerState>,
    _cancel: CancellationToken,
}

fn document(format: glimpse_config::ColorFormat, limit: usize) -> glimpse_config::Config {
    let mut document = glimpse_config::Config::default();
    document.color_picker.format = format;
    document.color_picker.limit = limit;
    document
}

async fn harness(answers: Vec<Result<Option<[u8; 3]>, String>>) -> Harness {
    let config = Config::from(&document(ColorFormat::Hex, 8));
    let (events, inbox) = mpsc::channel(32);
    let cancel = CancellationToken::new();
    let (health, _health) = watch::channel(ServiceState::Starting);
    let (published, state) = watch::channel(ColorPicker::initial_state(&config));
    let ctx = Ctx::<ColorPicker>::new(
        events,
        &cancel,
        published,
        health,
        Buses::unavailable("no bus in tests"),
    );
    let selection = FakeSelection::default();
    let picker = Arc::new(FakePicker {
        answers: Mutex::new(answers),
        requests: Mutex::new(0),
    });
    let service = ColorPicker::start(
        &ctx,
        config,
        Dependencies {
            selection: Arc::new(selection.clone()),
            picker: picker.clone(),
        },
    )
    .await
    .unwrap();
    Harness {
        service,
        selection,
        picker,
        ctx,
        inbox,
        state,
        _cancel: cancel,
    }
}

impl Harness {
    async fn feed(&mut self, input: Input<ColorPicker>) {
        self.service.handle(&self.ctx, input).await;
    }

    async fn pick(&mut self) -> oneshot::Receiver<Result<(), CommandError>> {
        let (reply, result) = oneshot::channel();
        self.feed(Input::Command(Command::Pick { reply })).await;
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
async fn a_pick_lands_in_the_palette_and_on_the_clipboard_in_the_configured_format() {
    let mut harness = harness(vec![Ok(Some([224, 86, 63]))]).await;

    let result = harness.pick().await;
    assert!(harness.state.borrow().picking);
    harness.settle().await;

    assert_eq!(result.await.unwrap(), Ok(()));
    let state = harness.state.borrow().clone();
    assert!(!state.picking);
    assert_eq!(state.colors.len(), 1);
    assert_eq!(state.colors[0].rgb, [224, 86, 63]);
    assert_eq!(harness.offered(), ["#E0563F"]);
    assert_eq!(*harness.picker.requests.lock().unwrap(), 1);
}

#[tokio::test]
async fn a_cancelled_pick_changes_nothing_and_is_not_an_error() {
    let mut harness = harness(vec![Ok(None)]).await;

    let result = harness.pick().await;
    harness.settle().await;

    assert_eq!(result.await.unwrap(), Ok(()));
    assert!(harness.state.borrow().colors.is_empty());
    assert!(harness.offered().is_empty());
}

#[tokio::test]
async fn a_failed_pick_reports_what_the_picker_said() {
    let mut harness = harness(vec![Err("no screencopy".to_owned())]).await;

    let result = harness.pick().await;
    harness.settle().await;

    assert_eq!(
        result.await.unwrap(),
        Err(CommandError::Unavailable("no screencopy".to_owned()))
    );
    assert!(!harness.state.borrow().picking);
}

#[tokio::test]
async fn a_second_pick_while_one_is_open_is_refused() {
    let mut harness = harness(vec![Ok(Some([1, 2, 3]))]).await;

    let _first = harness.pick().await;
    tokio::task::yield_now().await;
    let second = harness.pick().await;

    assert!(matches!(
        second.await.unwrap(),
        Err(CommandError::LimitExceeded(_))
    ));
    assert_eq!(*harness.picker.requests.lock().unwrap(), 1);
}

#[tokio::test]
async fn a_palette_color_copies_in_any_format_and_a_missing_one_is_refused() {
    let mut harness = harness(vec![Ok(Some([224, 86, 63]))]).await;
    let _picked = harness.pick().await;
    harness.settle().await;
    let id = harness.state.borrow().colors[0].id;

    let (reply, result) = oneshot::channel();
    harness
        .feed(Input::Command(Command::Copy {
            id,
            format: ColorFormat::Rgb,
            reply,
        }))
        .await;
    assert_eq!(result.await.unwrap(), Ok(()));
    assert_eq!(harness.offered(), ["#E0563F", "rgb(224 86 63)"]);

    let (reply, result) = oneshot::channel();
    harness
        .feed(Input::Command(Command::Copy {
            id: id + 100,
            format: ColorFormat::Hex,
            reply,
        }))
        .await;
    assert!(matches!(
        result.await.unwrap(),
        Err(CommandError::InvalidArgument(_))
    ));
}
