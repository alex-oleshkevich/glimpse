mod backend;
mod protocol;

use std::pin::Pin;
use std::sync::Arc;

use futures_util::{Stream, stream};
use glimpse_services::{Offer, Selection, SelectionEvent};
use tokio::sync::mpsc;

use backend::{Feeds, Request, Subscribers};

pub struct WaylandSelection {
    subscribers: Subscribers,
    requests: mpsc::UnboundedSender<Request>,
    cancel: tokio_util::sync::CancellationToken,
}

impl WaylandSelection {
    pub fn new() -> Self {
        let subscribers: Subscribers = Arc::new(Feeds::default());
        let (requests, inbox) = mpsc::unbounded_channel();
        let cancel = tokio_util::sync::CancellationToken::new();

        tokio::spawn(backend::run(
            Arc::clone(&subscribers),
            inbox,
            cancel.clone(),
        ));

        Self {
            subscribers,
            requests,
            cancel,
        }
    }
}

impl Default for WaylandSelection {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for WaylandSelection {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl Selection for WaylandSelection {
    fn events(&self) -> Pin<Box<dyn Stream<Item = SelectionEvent> + Send>> {
        let events = self.subscribers.subscribe();
        Box::pin(stream::unfold(events, |mut events| async move {
            events.recv().await.map(|event| (event, events))
        }))
    }

    fn offer(&self, offer: Offer) -> Result<(), String> {
        self.requests
            .send(Request::Offer(offer))
            .map_err(|_| "the clipboard backend has stopped".to_owned())
    }
}
