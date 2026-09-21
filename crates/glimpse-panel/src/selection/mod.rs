mod backend;
mod protocol;

use std::pin::Pin;
use std::sync::Arc;

use futures_util::{Stream, stream};
use glimpse_services::{Offer, Selection, SelectionEvent};
use tokio::sync::mpsc;

use backend::{Feeds, Request, Subscribers};

/// The panel's half of `trait Selection`: one Wayland connection, held by a task of its own, with
/// the compositor's data-control protocol on the other end. It lives here rather than in
/// `glimpse-services` because no service crate may bind a `wl_` object.
pub struct WaylandSelection {
    subscribers: Subscribers,
    requests: mpsc::UnboundedSender<Request>,
    cancel: tokio_util::sync::CancellationToken,
}

impl WaylandSelection {
    /// Returns immediately and touches no Wayland object: the connection is made inside the task,
    /// because this runs from `PanelServices::start_with_buses`, which is built on the GTK thread
    /// and again from tests on a current-thread runtime.
    pub fn new() -> Self {
        let subscribers: Subscribers = Arc::new(Feeds::default());
        let (requests, inbox) = mpsc::unbounded_channel();
        let cancel = tokio_util::sync::CancellationToken::new();

        relm4::spawn(backend::run(
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

impl Drop for WaylandSelection {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl Selection for WaylandSelection {
    fn events(&self) -> Pin<Box<dyn Stream<Item = SelectionEvent> + Send>> {
        // Subscribing is what brings the connection up: see `Feeds`.
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
