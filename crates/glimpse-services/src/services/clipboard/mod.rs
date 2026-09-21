mod history;

use std::sync::Arc;

use chrono::Utc;
use tokio::sync::{oneshot, watch};

use crate::{
    ServiceState,
    context::Ctx,
    publisher::Publisher,
    selection::{Selection, SelectionEvent},
    service::{CommandError, Input, Service, ServiceEndpoint, ServiceError},
    subscription::Sub,
};

pub use history::{Entry as ClipboardEntry, EntryId as ClipboardEntryId, Kind as ClipboardKind};
use history::{History, Limits};

const KIB: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub enabled: bool,
    limits: Limits,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        let clipboard = &document.clipboard;
        // Clamped where the document meets the service, as `[mpris] art-max-kib` is. A zero cap
        // otherwise refuses every capture through a `debug` line nobody reads, leaving an empty
        // applet with nothing to explain it.
        Self {
            enabled: clipboard.enabled,
            limits: Limits {
                entries: clipboard.limit.clamp(1, 10_000),
                max_entry_bytes: usize::try_from(clipboard.max_entry_kib.clamp(4, 2_048))
                    .unwrap_or(usize::MAX)
                    .saturating_mul(KIB),
                max_total_bytes: usize::try_from(clipboard.max_total_kib.clamp(64, 1_048_576))
                    .unwrap_or(usize::MAX)
                    .saturating_mul(KIB),
                images: clipboard.capture_images,
            },
        }
    }
}

/// Newest first, pinned and unpinned in one list. `unavailable` carries the standing reason the
/// clipboard is not being watched, which the popover explains rather than leaving as a silence.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ClipboardState {
    pub entries: Vec<ClipboardEntry>,
    pub total_bytes: usize,
    pub unavailable: Option<String>,
}

pub enum Command {
    Restore {
        id: ClipboardEntryId,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    Remove {
        id: ClipboardEntryId,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    Pin {
        id: ClipboardEntryId,
        pinned: bool,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    ClearHistory {
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
}

#[derive(PartialEq, Eq, Hash)]
pub enum Watch {
    Selection,
}

pub struct Dependencies {
    pub selection: Arc<dyn Selection>,
}

#[derive(Clone)]
pub struct ClipboardHandle(ServiceEndpoint<Clipboard>);

impl ClipboardHandle {
    pub fn snapshot(&self) -> ClipboardState {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> watch::Receiver<ClipboardState> {
        self.0.subscribe()
    }

    pub fn health(&self) -> watch::Receiver<ServiceState> {
        self.0.health()
    }

    pub async fn restore(&self, id: ClipboardEntryId) -> Result<(), CommandError> {
        self.ask(|reply| Command::Restore { id, reply }, "restoring")
            .await
    }

    pub async fn remove(&self, id: ClipboardEntryId) -> Result<(), CommandError> {
        self.ask(|reply| Command::Remove { id, reply }, "forgetting an entry")
            .await
    }

    pub async fn pin(&self, id: ClipboardEntryId, pinned: bool) -> Result<(), CommandError> {
        self.ask(|reply| Command::Pin { id, pinned, reply }, "pinning")
            .await
    }

    pub async fn clear_history(&self) -> Result<(), CommandError> {
        self.ask(
            |reply| Command::ClearHistory { reply },
            "clearing the history",
        )
        .await
    }

    async fn ask(
        &self,
        build: impl FnOnce(oneshot::Sender<Result<(), CommandError>>) -> Command,
        doing: &str,
    ) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(build(reply))?;
        result.await.map_err(|_| {
            CommandError::Unavailable(format!("the clipboard stopped while {doing}"))
        })?
    }
}

pub struct Clipboard {
    state: Publisher<ClipboardState>,
    selection: Arc<dyn Selection>,
    history: History,
    config: Config,
    unavailable: Option<String>,
}

impl Service for Clipboard {
    const NAME: &'static str = "clipboard";

    type Config = Config;
    type State = ClipboardState;
    type Handle = ClipboardHandle;
    type Command = Command;
    type Event = SelectionEvent;
    type Dependencies = Dependencies;
    type SubKey = Watch;

    fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
        ClipboardHandle(endpoint)
    }

    fn initial_state(_config: &Self::Config) -> Self::State {
        ClipboardState::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        if !self.config.enabled {
            return Vec::new();
        }
        let selection = Arc::clone(&self.selection);
        vec![Sub::stream(Watch::Selection, move |_ctx| async move {
            selection.events()
        })]
    }

    async fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        dependencies: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        Ok(Self {
            state: ctx.publisher(),
            selection: dependencies.selection,
            history: History::new(config.limits),
            config,
            unavailable: None,
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(SelectionEvent::Captured(capture)) => {
                match self.history.insert(capture, Utc::now()) {
                    Ok(_) => {}
                    Err(refused) => tracing::debug!(?refused, "a selection was not recorded"),
                }
            }
            Input::Event(SelectionEvent::Watching) => {
                self.unavailable = None;
                ctx.running();
            }
            Input::Event(SelectionEvent::Unavailable(reason)) => {
                self.unavailable = Some(reason.clone());
                ctx.degraded(reason);
            }
            Input::Command(Command::Restore { id, reply }) => {
                let _ = reply.send(self.restore(id));
            }
            Input::Command(Command::Remove { id, reply }) => {
                self.history.remove(id);
                let _ = reply.send(Ok(()));
            }
            Input::Command(Command::Pin { id, pinned, reply }) => {
                self.history.pin(id, pinned);
                let _ = reply.send(Ok(()));
            }
            Input::Command(Command::ClearHistory { reply }) => {
                self.history.clear();
                let _ = reply.send(Ok(()));
            }
            Input::Config(config) => {
                self.history.set_limits(config.limits);
                self.config = config;
                if !self.config.enabled {
                    // Pins included: the schema promises the applet renders as if nothing had ever
                    // been copied, and a pinned token is exactly what turning this off is for.
                    self.history.forget_all();
                    // A complaint about a compositor we have stopped watching outlives its subject.
                    self.unavailable = None;
                    ctx.running();
                }
            }
        }
        self.publish();
    }
}

impl Clipboard {
    /// A refused restore is the viewer's own action failing, so it answers in words the applet can
    /// show. A missing id is not an error the viewer can act on — the row is already gone.
    fn restore(&self, id: ClipboardEntryId) -> Result<(), CommandError> {
        let Some(entry) = self.history.get(id) else {
            return Err(CommandError::InvalidArgument(
                "that entry is no longer in the history".to_owned(),
            ));
        };
        self.selection
            .offer(entry.offer())
            .map_err(CommandError::Unavailable)
    }

    fn publish(&self) {
        self.state.set(ClipboardState {
            entries: self.history.entries().to_vec(),
            total_bytes: self.history.total_bytes(),
            unavailable: self.unavailable.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use glimpse_dbus::Buses;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::selection::{Capture, FakeSelection, UnavailableSelection};
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
                sensitive: false,
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
        .await
        .expect("the service starts");

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
    async fn nothing_is_published_before_the_first_capture_arrives() {
        let harness = harness().await;

        assert!(harness.state.borrow().entries.is_empty());
        assert_eq!(harness.state.borrow().total_bytes, 0);
        assert!(harness.state.borrow().unavailable.is_none());
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

    /// The compositor announces our own `offer` straight back, and nothing can tell it apart from
    /// anyone else's. Dedup is the whole guard.
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
    async fn a_sensitive_capture_never_reaches_the_state() {
        let mut harness = harness().await;

        harness
            .feed(Input::Event(SelectionEvent::Captured(Capture {
                mime: TEXT.to_owned(),
                data: Arc::from(&b"hunter2"[..]),
                sensitive: true,
            })))
            .await;

        assert!(harness.state.borrow().entries.is_empty());
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
        let mut harness = harness_with(
            Arc::new(UnavailableSelection::new(reason)),
            FakeSelection::default(),
            config(),
        )
        .await;

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

    /// Pinning first is the whole point: `clear` deliberately keeps pins for the viewer's own
    /// Clear action, and reusing it here left a pinned token resident after the feature was
    /// switched off to be rid of exactly that.
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
    async fn an_enabled_clipboard_declares_exactly_one_source() {
        let harness = harness().await;

        assert_eq!(harness.service.subscriptions().len(), 1);
    }

    /// The projection is four assignments over three numeric fields of compatible types, so
    /// transposing two of them compiles and passes every other test in the crate. This is the one
    /// assertion that connects a written document to what the service actually does.
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

    /// A zero cap otherwise refuses every capture through a `debug` line nobody reads, leaving an
    /// empty applet with nothing to explain it.
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

    /// Health outlived its subject: a complaint about a compositor we have stopped watching kept
    /// the applet explaining a failure the viewer had just switched off.
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
}
