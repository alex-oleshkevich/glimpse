use std::pin::Pin;
use std::sync::Arc;
#[cfg(feature = "testing")]
use std::sync::Mutex;

use futures_util::Stream;
#[cfg(feature = "testing")]
use futures_util::stream;
#[cfg(feature = "testing")]
use tokio::sync::mpsc;

/// Watching the compositor's clipboard and taking it back.
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

pub use glimpse_utils::is_sensitive;

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

/// The mock beside the declaration, so the whole history state machine is testable without a
/// compositor.
/// A clone shares the record, because the service takes its backend by `Arc` and a test still has
/// to read what was offered to it.
#[cfg(feature = "testing")]
#[derive(Clone, Default)]
pub struct FakeSelection {
    record: Arc<Mutex<Record>>,
}

#[cfg(feature = "testing")]
#[derive(Default)]
struct Record {
    /// One per live `events()` call. A dropped stream's sender is swept on the next send, so a
    /// source the runtime tore down costs nothing.
    feeds: Vec<mpsc::UnboundedSender<SelectionEvent>>,
    offered: Vec<Offer>,
    subscriptions: usize,
    failure: Option<String>,
}

#[cfg(feature = "testing")]
impl FakeSelection {
    pub fn capture(&self, mime: &str, data: &[u8]) {
        self.push(mime, data, false);
    }

    pub fn unavailable(&self, reason: &str) {
        self.emit(SelectionEvent::Unavailable(reason.to_owned()));
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

#[cfg(feature = "testing")]
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
mod tests;
