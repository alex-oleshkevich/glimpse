mod history;

use std::sync::Arc;

use chrono::Utc;
use tokio::sync::oneshot;

use crate::{
    context::Ctx,
    publisher::Publisher,
    selection::{Offer, Selection, SelectionEvent},
    service::{CommandError, Input, Service, ServiceEndpoint},
    subscription::Sub,
};

pub use history::{Entry as ClipboardEntry, EntryId as ClipboardEntryId, Kind as ClipboardKind};
use history::{History, Limits};

const KIB: usize = 1024;
const PLAIN_TEXT: &str = "text/plain;charset=utf-8";

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
    CopyText {
        text: String,
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

pub type ClipboardHandle = ServiceEndpoint<Clipboard>;

impl ServiceEndpoint<Clipboard> {
    pub async fn restore(&self, id: ClipboardEntryId) -> Result<(), CommandError> {
        self.call("the clipboard stopped while restoring", |reply| {
            Command::Restore { id, reply }
        })
        .await?
    }

    pub async fn remove(&self, id: ClipboardEntryId) -> Result<(), CommandError> {
        self.call("the clipboard stopped while forgetting an entry", |reply| {
            Command::Remove { id, reply }
        })
        .await?
    }

    pub async fn pin(&self, id: ClipboardEntryId, pinned: bool) -> Result<(), CommandError> {
        self.call("the clipboard stopped while pinning", |reply| {
            Command::Pin { id, pinned, reply }
        })
        .await?
    }

    pub async fn clear_history(&self) -> Result<(), CommandError> {
        self.call(
            "the clipboard stopped while clearing the history",
            |reply| Command::ClearHistory { reply },
        )
        .await?
    }

    pub async fn copy_text(&self, text: String) -> Result<(), CommandError> {
        self.call("the clipboard stopped while copying text", |reply| {
            Command::CopyText { text, reply }
        })
        .await?
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
    type Command = Command;
    type Event = SelectionEvent;
    type Dependencies = Dependencies;
    type SubKey = Watch;

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
    ) -> Self {
        Self {
            state: ctx.publisher(),
            selection: dependencies.selection,
            history: History::new(config.limits),
            config,
            unavailable: None,
        }
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
            Input::Command(Command::CopyText { text, reply }) => {
                let offer = Offer {
                    mime: PLAIN_TEXT.to_owned(),
                    data: Arc::from(text.into_bytes()),
                };
                let _ = reply.send(
                    self.selection
                        .offer(offer)
                        .map_err(CommandError::Unavailable),
                );
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
mod tests;
