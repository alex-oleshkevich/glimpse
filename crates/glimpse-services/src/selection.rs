use std::pin::Pin;
use std::sync::{Arc, Mutex};

use futures_util::{Stream, stream};
use tokio::sync::mpsc;

/// Watching the compositor's clipboard and taking it back.
/// Declared here and implemented in `glimpse-panel`, because this crate is linked into every
/// binary and none of them may gain a Wayland dependency.
/// Synchronous on purpose, like `Gamma`: `set_selection` carries no reply, so the only failure
/// `offer` can report is that the backend has gone, and an `async` signature would be a promise
/// the protocol cannot keep.
pub trait Selection: Send + Sync + 'static {
    /// The source declared in `subscriptions`. **Every call must yield a fresh stream**: the
    /// runtime reconciles the declared set after each input, so disabling and re-enabling the
    /// clipboard tears the source down and builds it again. A backend that hands out its queue
    /// once leaves the clipboard silently deaf for the rest of the session.
    fn events(&self) -> Pin<Box<dyn Stream<Item = SelectionEvent> + Send>>;
    /// Become the owner of the selection, offering this content back to whoever asks.
    fn offer(&self, offer: Offer) -> Result<(), String>;
}

/// The mime an application sets beside its content to say the clipboard holds a secret. Matched
/// case-insensitively because it is a convention rather than a registered type, and the
/// applications that honour it do not agree on the spelling.
pub const SENSITIVE_HINT: &str = "x-kde-passwordManagerHint";

/// Whether an offer's mime list asks not to be remembered. This is the whole of the
/// password-manager rule and the one place it is decided; a backend classifies the offer with it
/// before reading any content, so a secret is never held even briefly.
pub fn is_sensitive(mimes: &[String]) -> bool {
    mimes
        .iter()
        .any(|mime| mime.trim().eq_ignore_ascii_case(SENSITIVE_HINT))
}

#[derive(Debug, Clone)]
pub enum SelectionEvent {
    Captured(Capture),
    /// The backend reached a data-control manager. Clears any previous `Unavailable`.
    Watching,
    /// The backend cannot watch, and says why in words a person can read.
    Unavailable(String),
}

/// One selection, already read and already capped by the backend. `mime` is the one representation
/// that was chosen off the offer; `sensitive` says the offer also advertised the password-manager
/// hint, which is decided against the whole mime list and so cannot be re-derived from here.
#[derive(Clone, PartialEq, Eq)]
pub struct Capture {
    pub mime: String,
    pub data: Arc<[u8]>,
    pub sensitive: bool,
}

/// Never the content. A clipboard routinely holds passwords, and one `tracing::debug!(?capture)`
/// in a future caller would write them to the journal.
impl std::fmt::Debug for Capture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Capture")
            .field("mime", &self.mime)
            .field("bytes", &self.data.len())
            .field("sensitive", &self.sensitive)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Offer {
    pub mime: String,
    pub data: Arc<[u8]>,
}

impl std::fmt::Debug for Offer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Offer")
            .field("mime", &self.mime)
            .field("bytes", &self.data.len())
            .finish()
    }
}

/// The null backend, for a compositor with neither data-control protocol. It watches nothing and
/// refuses a write plainly rather than pretending one landed, and it says so once through
/// `SelectionEvent::Unavailable` so the applet can explain itself.
pub struct UnavailableSelection {
    reason: String,
}

impl UnavailableSelection {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl Selection for UnavailableSelection {
    fn events(&self) -> Pin<Box<dyn Stream<Item = SelectionEvent> + Send>> {
        let reason = self.reason.clone();
        Box::pin(stream::once(
            async move { SelectionEvent::Unavailable(reason) },
        ))
    }

    fn offer(&self, _offer: Offer) -> Result<(), String> {
        Err(self.reason.clone())
    }
}

/// The mock beside the declaration, so the whole history state machine is testable without a
/// compositor. Not `#[cfg(test)]`: `glimpse-panel`'s own tests are a separate compilation unit.
/// A clone shares the record, because the service takes its backend by `Arc` and a test still has
/// to read what was offered to it.
#[derive(Clone, Default)]
pub struct FakeSelection {
    record: Arc<Mutex<Record>>,
}

#[derive(Default)]
struct Record {
    /// One per live `events()` call. A dropped stream's sender is swept on the next send, so a
    /// source the runtime tore down costs nothing.
    feeds: Vec<mpsc::UnboundedSender<SelectionEvent>>,
    offered: Vec<Offer>,
    subscriptions: usize,
    failure: Option<String>,
}

impl FakeSelection {
    pub fn capture(&self, mime: &str, data: &[u8]) {
        self.push(mime, data, false);
    }

    pub fn capture_sensitive(&self, mime: &str, data: &[u8]) {
        self.push(mime, data, true);
    }

    pub fn unavailable(&self, reason: &str) {
        self.emit(SelectionEvent::Unavailable(reason.to_owned()));
    }

    pub fn watching(&self) {
        self.emit(SelectionEvent::Watching);
    }

    pub fn offered(&self) -> Vec<Offer> {
        self.record().offered.clone()
    }

    /// How many times the source has been built. The runtime rebuilds it whenever the declared
    /// set changes, so a backend that can only be subscribed once is a defect this can catch.
    pub fn subscriptions(&self) -> usize {
        self.record().subscriptions
    }

    pub fn fail(&self, reason: Option<&str>) {
        self.record().failure = reason.map(str::to_owned);
    }

    fn push(&self, mime: &str, data: &[u8], sensitive: bool) {
        self.emit(SelectionEvent::Captured(Capture {
            mime: mime.to_owned(),
            data: Arc::from(data),
            sensitive,
        }));
    }

    fn emit(&self, event: SelectionEvent) {
        let mut record = self.record();
        record.feeds.retain(|feed| !feed.is_closed());
        for feed in &record.feeds {
            let _ = feed.send(event.clone());
        }
    }

    /// Poisoning only means a test panicked while holding this; the record is still readable and
    /// the panic is the failure worth reporting, not a second one from here.
    fn record(&self) -> std::sync::MutexGuard<'_, Record> {
        self.record.lock().unwrap_or_else(|held| held.into_inner())
    }
}

impl Selection for FakeSelection {
    fn events(&self) -> Pin<Box<dyn Stream<Item = SelectionEvent> + Send>> {
        let (feed, events) = mpsc::unbounded_channel();
        let mut record = self.record();
        record.feeds.push(feed);
        record.subscriptions += 1;
        Box::pin(stream::unfold(events, |mut events| async move {
            events.recv().await.map(|event| (event, events))
        }))
    }

    fn offer(&self, offer: Offer) -> Result<(), String> {
        let mut record = self.record();
        match &record.failure {
            Some(reason) => Err(reason.clone()),
            None => {
                record.offered.push(offer);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mimes(list: &[&str]) -> Vec<String> {
        list.iter().map(|mime| (*mime).to_owned()).collect()
    }

    /// The spelling is a convention, not a registered type, and the applications that set it do
    /// not agree on its casing.
    #[test]
    fn the_password_hint_is_recognised_in_any_casing() {
        for spelling in [
            "x-kde-passwordManagerHint",
            "x-kde-passwordmanagerhint",
            "X-KDE-PASSWORDMANAGERHINT",
            "  x-kde-PasswordManagerHint  ",
        ] {
            assert!(
                is_sensitive(&mimes(&["text/plain", spelling])),
                "{spelling} must be recognised"
            );
        }
    }

    #[test]
    fn an_ordinary_offer_is_not_sensitive() {
        assert!(!is_sensitive(&mimes(&["text/plain", "UTF8_STRING"])));
        assert!(!is_sensitive(&[]));
        assert!(
            !is_sensitive(&mimes(&["x-kde-passwordManagerHintish"])),
            "a longer mime that merely starts the same is a different type"
        );
    }

    /// A torn-down source is rebuilt whenever the declared set changes. A backend that hands out
    /// its queue once leaves the clipboard deaf for the rest of the session.
    #[tokio::test]
    async fn every_subscription_gets_its_own_live_stream() {
        use futures_util::StreamExt as _;

        let selection = FakeSelection::default();
        let mut first = selection.events();
        selection.capture("text/plain", b"one");
        assert!(matches!(
            first.next().await,
            Some(SelectionEvent::Captured(_))
        ));

        drop(first);
        let mut second = selection.events();
        selection.capture("text/plain", b"two");

        assert!(
            matches!(second.next().await, Some(SelectionEvent::Captured(_))),
            "a rebuilt source must still receive captures"
        );
        assert_eq!(selection.subscriptions(), 2);
    }

    #[test]
    fn neither_content_nor_preview_reaches_a_debug_line() {
        let capture = Capture {
            mime: "text/plain".to_owned(),
            data: Arc::from(&b"hunter2"[..]),
            sensitive: false,
        };

        let rendered = format!("{capture:?}");

        assert!(!rendered.contains("hunter2"), "got {rendered}");
        assert!(rendered.contains("bytes: 7"), "got {rendered}");
    }
}
