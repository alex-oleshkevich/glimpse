use glimpse_dbus::Buses;
use tokio::sync::mpsc;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::selection::{Capture, FakeSelection};
use crate::service::ServiceState;

const TEXT: &str = "text/plain;charset=utf-8";

struct Harness {
    service: Clipboard,
    selection: FakeSelection,
    ctx: Ctx<Clipboard>,
    health: watch::Receiver<ServiceState>,
    state: watch::Receiver<ClipboardState>,
    _inbox: mpsc::Receiver<Input<Clipboard>>,
    _cancel: CancellationToken,
}

impl Harness {
    async fn feed(&mut self, input: Input<Clipboard>) {
        self.service.handle(&self.ctx, input).await;
    }

    async fn capture(&mut self, body: &str) {
        self.feed(Input::Event(SelectionEvent::Captured(Capture {
            mime: TEXT.to_owned(),
            data: Arc::from(body.as_bytes()),
        })))
        .await;
    }

    async fn restore(&mut self, id: ClipboardEntryId) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.feed(Input::Command(Command::Restore { id, reply }))
            .await;
        result.await.expect("the handler answers")
    }

    fn front(&self) -> ClipboardEntryId {
        self.state.borrow().entries[0].id
    }

    fn reason(&self) -> Option<String> {
        match &*self.health.borrow() {
            ServiceState::Degraded { reason } => Some(reason.clone()),
            _ => None,
        }
    }
}

fn config() -> Config {
    Config::from(&glimpse_config::Config::default())
}

async fn harness_with(
    selection: Arc<dyn Selection>,
    record: FakeSelection,
    config: Config,
) -> Harness {
    let (events, inbox) = mpsc::channel(32);
    let cancel = CancellationToken::new();
    let buses = Buses::unavailable("no bus in tests");
    let (health, health_rx) = watch::channel(ServiceState::Starting);
    let (published, state) = watch::channel(Clipboard::initial_state(&config));
    let ctx = Ctx::<Clipboard>::new(events, &cancel, published, health, buses);
    let service = Clipboard::start(
        &ctx,
        config,
        Dependencies {
            selection: Arc::clone(&selection),
        },
    )
    .await;

    Harness {
        service,
        selection: record,
        ctx,
        health: health_rx,
        state,
        _inbox: inbox,
        _cancel: cancel,
    }
}

async fn harness() -> Harness {
    let selection = FakeSelection::default();
    harness_with(Arc::new(selection.clone()), selection, config()).await
}

#[tokio::test]
async fn a_capture_reaches_the_published_state() {
    let mut harness = harness().await;

    harness.capture("hello").await;

    assert_eq!(harness.state.borrow().entries.len(), 1);
    assert_eq!(harness.state.borrow().entries[0].preview, "hello");
    assert_eq!(harness.state.borrow().total_bytes, 5);
}

#[tokio::test]
async fn copied_text_is_offered_as_plain_text() {
    let mut harness = harness().await;
    let (reply, result) = oneshot::channel();
    harness
        .feed(Input::Command(Command::CopyText {
            text: "rgb(30 136 229)".to_owned(),
            reply,
        }))
        .await;
    result
        .await
        .expect("the handler answers")
        .expect("the offer is accepted");

    let offered = harness.selection.offered();
    assert_eq!(offered[0].mime, PLAIN_TEXT);
    assert_eq!(&*offered[0].data, b"rgb(30 136 229)");
}

#[tokio::test]
async fn restoring_an_entry_hands_its_bytes_back_to_the_compositor() {
    let mut harness = harness().await;
    harness.capture("take me back").await;
    let id = harness.front();

    harness.restore(id).await.expect("the offer is accepted");

    let offered = harness.selection.offered();
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].mime, TEXT);
    assert_eq!(&*offered[0].data, b"take me back");
}

#[tokio::test]
async fn the_echo_of_our_own_offer_does_not_grow_the_history() {
    let mut harness = harness().await;
    harness.capture("first").await;
    harness.capture("second").await;
    let id = harness.front();
    harness.restore(id).await.expect("the offer is accepted");

    harness.capture("second").await;

    assert_eq!(harness.state.borrow().entries.len(), 2);
    assert_eq!(harness.state.borrow().entries[0].id, id);
}

#[tokio::test]
async fn restoring_an_entry_that_is_gone_says_so_rather_than_failing_silently() {
    let mut harness = harness().await;

    let refused = harness
        .restore(1234)
        .await
        .expect_err("there is no such id");

    assert!(matches!(refused, CommandError::InvalidArgument(_)));
}

#[tokio::test]
async fn a_compositor_without_data_control_degrades_and_says_why_in_the_state() {
    let reason = "the compositor does not offer a data-control protocol";
    let selection = FakeSelection::default();
    selection.fail(Some(reason));
    let mut harness = harness_with(Arc::new(selection.clone()), selection, config()).await;

    harness
        .feed(Input::Event(SelectionEvent::Unavailable(reason.to_owned())))
        .await;

    assert_eq!(harness.reason().as_deref(), Some(reason));
    assert_eq!(harness.state.borrow().unavailable.as_deref(), Some(reason));
    assert!(
        harness.service.restore(1).is_err(),
        "a backend that cannot watch cannot offer either"
    );
}

#[tokio::test]
async fn reaching_the_compositor_clears_an_earlier_complaint() {
    let mut harness = harness().await;
    harness
        .feed(Input::Event(SelectionEvent::Unavailable("gone".to_owned())))
        .await;

    harness.feed(Input::Event(SelectionEvent::Watching)).await;

    assert!(harness.reason().is_none());
    assert!(harness.state.borrow().unavailable.is_none());
}

#[tokio::test]
async fn pinning_survives_a_clear_and_unpinning_gives_it_back() {
    let mut harness = harness().await;
    harness.capture("keep me").await;
    let id = harness.front();
    harness.capture("forget me").await;

    let (reply, result) = oneshot::channel();
    harness
        .feed(Input::Command(Command::Pin {
            id,
            pinned: true,
            reply,
        }))
        .await;
    result.await.expect("answers").expect("pins");

    let (reply, result) = oneshot::channel();
    harness
        .feed(Input::Command(Command::ClearHistory { reply }))
        .await;
    result.await.expect("answers").expect("clears");

    assert_eq!(harness.state.borrow().entries.len(), 1);
    assert!(harness.state.borrow().entries[0].pinned);

    let (reply, result) = oneshot::channel();
    harness
        .feed(Input::Command(Command::Pin {
            id,
            pinned: false,
            reply,
        }))
        .await;
    result.await.expect("answers").expect("unpins");

    assert!(!harness.state.borrow().entries[0].pinned);
}

#[tokio::test]
async fn a_document_that_turns_the_clipboard_off_forgets_what_was_held() {
    let mut harness = harness().await;
    harness.capture("held").await;
    let id = harness.front();
    let (reply, result) = oneshot::channel();
    harness
        .feed(Input::Command(Command::Pin {
            id,
            pinned: true,
            reply,
        }))
        .await;
    result.await.expect("answers").expect("pins");

    let mut off = config();
    off.enabled = false;
    harness.feed(Input::Config(off)).await;

    assert!(harness.state.borrow().entries.is_empty());
    assert!(
        harness.service.subscriptions().is_empty(),
        "a disabled clipboard opens no connection"
    );
}

#[tokio::test]
async fn a_configured_limit_is_what_the_service_keeps() {
    let mut document = glimpse_config::Config::default();
    document.clipboard.limit = 3;
    let mut harness = harness_with(
        Arc::new(FakeSelection::default()),
        FakeSelection::default(),
        Config::from(&document),
    )
    .await;

    for body in ["one", "two", "three", "four"] {
        harness.capture(body).await;
    }

    let previews: Vec<_> = harness
        .state
        .borrow()
        .entries
        .iter()
        .map(|entry| entry.preview.clone())
        .collect();
    assert_eq!(previews, ["four", "three", "two"]);
}

#[test]
fn the_document_limits_are_clamped_where_they_meet_the_service() {
    let mut document = glimpse_config::Config::default();
    document.clipboard.limit = 0;
    document.clipboard.max_entry_kib = 0;
    document.clipboard.max_total_kib = 0;
    let floor = Config::from(&document);

    assert_eq!(floor.limits.entries, 1);
    assert_eq!(floor.limits.max_entry_bytes, 4 * KIB);
    assert_eq!(floor.limits.max_total_bytes, 64 * KIB);

    document.clipboard.max_entry_kib = u32::MAX;
    document.clipboard.max_total_kib = u32::MAX;
    let ceiling = Config::from(&document);

    assert_eq!(ceiling.limits.max_entry_bytes, 2_048 * KIB);
    assert_eq!(ceiling.limits.max_total_bytes, 1_048_576 * KIB);
}

#[tokio::test]
async fn disabling_the_clipboard_retires_an_earlier_complaint() {
    let mut harness = harness().await;
    harness
        .feed(Input::Event(SelectionEvent::Unavailable("gone".to_owned())))
        .await;

    let mut off = config();
    off.enabled = false;
    harness.feed(Input::Config(off)).await;

    assert!(harness.reason().is_none());
    assert!(harness.state.borrow().unavailable.is_none());
}
