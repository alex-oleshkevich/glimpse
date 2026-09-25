use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use futures_util::{Stream, StreamExt as _, stream};
use glimpse_services::{Capture, Offer, Selection, SelectionEvent};
use glimpse_wayland::selection::{self, Entry, Owned, Watched};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// The aliases offered back beside a restored text entry, so an application asking for any common
/// spelling is answered. `wl-copy` does the same; without them a paste target that only knows
/// `STRING` gets nothing.
const TEXT_ALIASES: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "STRING",
    "TEXT",
];

enum Request {
    Offer(Offer),
}

type WatchStream = Pin<Box<dyn Stream<Item = Watched> + Send>>;
type Wanted = Box<dyn Fn() -> bool + Send + Sync>;
type Served = io::Result<Option<Owned>>;

pub struct WaylandSelection {
    subscribers: Arc<Feeds>,
    requests: mpsc::UnboundedSender<Request>,
    cancel: CancellationToken,
}

impl WaylandSelection {
    pub fn new() -> Self {
        let subscribers = Arc::new(Feeds::default());
        let (requests, inbox) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();

        tokio::spawn(run(
            Arc::clone(&subscribers),
            inbox,
            cancel.clone(),
            |wanted| Box::pin(selection::watch(wanted)),
            |entries, current: &dyn Fn() -> bool| Owned::serve(entries, current),
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
        let feed = self.subscribers.subscribe();
        Box::pin(stream::unfold(feed, |mut feed| async move {
            feed.events.recv().await.map(|event| (event, feed))
        }))
    }

    fn offer(&self, offer: Offer) -> Result<(), String> {
        self.requests
            .send(Request::Offer(offer))
            .map_err(|_| "the clipboard backend has stopped".to_owned())
    }
}

#[derive(Default)]
struct Feeds {
    live: std::sync::Mutex<Vec<mpsc::UnboundedSender<SelectionEvent>>>,
    wanted: tokio::sync::Notify,
    left: tokio::sync::Notify,
}

struct Feed {
    events: mpsc::UnboundedReceiver<SelectionEvent>,
    feeds: Arc<Feeds>,
}

impl Drop for Feed {
    fn drop(&mut self) {
        self.events.close();
        self.feeds.left.notify_one();
    }
}

impl Feeds {
    fn subscribe(self: &Arc<Self>) -> Feed {
        let (feed, events) = mpsc::unbounded_channel();
        self.held().push(feed);
        self.wanted.notify_one();
        Feed {
            events,
            feeds: Arc::clone(self),
        }
    }

    fn held(&self) -> std::sync::MutexGuard<'_, Vec<mpsc::UnboundedSender<SelectionEvent>>> {
        self.live
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn any(&self) -> bool {
        let mut held = self.held();
        held.retain(|feed| !feed.is_closed());
        !held.is_empty()
    }

    async fn awaited(&self) {
        while !self.any() {
            self.wanted.notified().await;
        }
    }

    async fn deserted(&self) {
        while self.any() {
            self.left.notified().await;
        }
    }

    fn announce(&self, event: SelectionEvent) {
        let held = self.held();
        for feed in held.iter().filter(|feed| !feed.is_closed()) {
            let _ = feed.send(event.clone());
        }
    }
}

async fn run<P>(
    subscribers: Arc<Feeds>,
    mut requests: mpsc::UnboundedReceiver<Request>,
    cancel: CancellationToken,
    watch: impl Fn(Wanted) -> WatchStream,
    publish: P,
) where
    P: Fn(Vec<Entry>, &dyn Fn() -> bool) -> Served + Send + Sync + 'static,
{
    let publish = Arc::new(publish);
    let generation = Arc::new(AtomicU64::new(0));
    let mut owned: Option<Owned> = None;
    let mut serving: Option<tokio::task::JoinHandle<Option<Owned>>> = None;
    let mut pending: Option<Offer> = None;
    let mut watching: Option<WatchStream> = None;
    let mut unsupported = false;

    let start = |offer: Offer| {
        let generation = Arc::clone(&generation);
        let mine = generation.load(Ordering::SeqCst);
        let cancel = cancel.clone();
        tokio::spawn(serve(offer, Arc::clone(&publish), move || {
            generation.load(Ordering::SeqCst) == mine && !cancel.is_cancelled()
        }))
    };

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                if let Some(serving) = serving {
                    serving.abort();
                }
                return;
            }
            () = subscribers.awaited(), if watching.is_none() && !unsupported => {
                let wanted = Arc::clone(&subscribers);
                watching = Some(watch(Box::new(move || wanted.any())));
            }
            () = subscribers.deserted(), if watching.is_some() => {
                tracing::info!("clipboard backend idle; releasing the compositor connection");
                watching = None;
            }
            request = requests.recv() => match request {
                Some(Request::Offer(offer)) => {
                    generation.fetch_add(1, Ordering::SeqCst);
                    match serving.is_some() {
                        true => pending = Some(offer),
                        false => serving = Some(start(offer)),
                    }
                }
                None => return,
            },
            served = finished(serving.as_mut()) => {
                serving = pending.take().map(start);
                if let Ok(Some(new)) = served {
                    owned = Some(new);
                }
            }
            () = ended(owned.as_ref()) => owned = None,
            event = next(watching.as_mut()) => match event {
                Some(event) => {
                    if let Some(event) = translated(event) {
                        subscribers.announce(event);
                    }
                }
                None => {
                    tracing::info!("the clipboard watch ended and is not reopened");
                    unsupported = true;
                    watching = None;
                }
            },
        }
    }
}

async fn ended(owned: Option<&Owned>) {
    match owned {
        Some(owned) => owned.cancelled().await,
        None => std::future::pending().await,
    }
}

async fn finished(
    serving: Option<&mut tokio::task::JoinHandle<Option<Owned>>>,
) -> Result<Option<Owned>, tokio::task::JoinError> {
    match serving {
        Some(serving) => serving.await,
        None => std::future::pending().await,
    }
}

async fn next(watching: Option<&mut WatchStream>) -> Option<Watched> {
    match watching {
        Some(watching) => watching.next().await,
        None => std::future::pending().await,
    }
}

fn translated(event: Watched) -> Option<SelectionEvent> {
    match event {
        Watched::Watching => Some(SelectionEvent::Watching),
        Watched::Unavailable(reason) => Some(SelectionEvent::Unavailable(reason)),
        Watched::Incoming(incoming) => incoming.entry.map(|entry| {
            SelectionEvent::Captured(Capture {
                mime: entry.mime,
                data: entry.data,
                sensitive: false,
            })
        }),
    }
}

async fn serve<P>(
    offer: Offer,
    publish: Arc<P>,
    current: impl Fn() -> bool + Send + 'static,
) -> Option<Owned>
where
    P: Fn(Vec<Entry>, &dyn Fn() -> bool) -> Served + Send + Sync + 'static,
{
    let entries = entries(offer);
    match tokio::task::spawn_blocking(move || publish(entries, &current)).await {
        Ok(Ok(owned)) => owned,
        Ok(Err(error)) => {
            tracing::warn!(%error, "the clipboard entry could not be offered");
            None
        }
        Err(error) => {
            tracing::warn!(%error, "the clipboard offer worker failed");
            None
        }
    }
}

fn entries(offer: Offer) -> Vec<Entry> {
    let text = TEXT_ALIASES
        .iter()
        .any(|alias| alias.eq_ignore_ascii_case(&offer.mime));
    let mut mimes: Vec<String> = match text {
        true => TEXT_ALIASES.iter().map(|mime| (*mime).to_owned()).collect(),
        false => Vec::new(),
    };
    if !mimes.contains(&offer.mime) {
        mimes.push(offer.mime.clone());
    }
    mimes
        .into_iter()
        .map(|mime| Entry {
            mime,
            data: Arc::clone(&offer.data),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Watch {
        streams: Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<Watched>>>>,
        wanted: Arc<std::sync::Mutex<Option<Wanted>>>,
    }

    impl Watch {
        fn new() -> (Self, impl Fn(Wanted) -> WatchStream) {
            let streams: Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<Watched>>>> =
                Arc::default();
            let wanted: Arc<std::sync::Mutex<Option<Wanted>>> = Arc::default();
            let opened = Arc::clone(&streams);
            let asked = Arc::clone(&wanted);
            let factory = move |wanted: Wanted| -> WatchStream {
                let (feed, events) = mpsc::unbounded_channel();
                opened.lock().unwrap().push(feed);
                *asked.lock().unwrap() = Some(wanted);
                Box::pin(stream::unfold(events, |mut events| async move {
                    events.recv().await.map(|event| (event, events))
                }))
            };
            (Self { streams, wanted }, factory)
        }

        fn wanted(&self) -> bool {
            self.wanted
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|wanted| wanted())
        }

        fn opened(&self) -> usize {
            self.streams.lock().unwrap().len()
        }

        fn open(&self) -> bool {
            self.streams
                .lock()
                .unwrap()
                .last()
                .is_some_and(|feed| !feed.is_closed())
        }
    }

    fn unused(_: Vec<Entry>, _: &dyn Fn() -> bool) -> Served {
        Ok(None)
    }

    #[derive(Default)]
    struct Publisher {
        published: std::sync::Mutex<Vec<Vec<u8>>>,
        open: std::sync::Mutex<bool>,
        opened: std::sync::Condvar,
        setting_up: std::sync::atomic::AtomicUsize,
        most_at_once: std::sync::atomic::AtomicUsize,
    }

    type Publish = Box<dyn Fn(Vec<Entry>, &dyn Fn() -> bool) -> Served + Send + Sync>;

    impl Publisher {
        fn new() -> (Arc<Self>, Publish) {
            let publisher = Arc::new(Self::default());
            let shared = Arc::clone(&publisher);
            let publish = move |entries: Vec<Entry>, current: &dyn Fn() -> bool| -> Served {
                let at_once = shared.setting_up.fetch_add(1, Ordering::SeqCst) + 1;
                shared.most_at_once.fetch_max(at_once, Ordering::SeqCst);
                let open = shared.open.lock().unwrap();
                drop(shared.opened.wait_while(open, |open| !*open).unwrap());
                if current() {
                    shared
                        .published
                        .lock()
                        .unwrap()
                        .push(entries[0].data.to_vec());
                }
                shared.setting_up.fetch_sub(1, Ordering::SeqCst);
                Ok(None)
            };
            (publisher, Box::new(publish))
        }

        fn release(&self) {
            *self.open.lock().unwrap() = true;
            self.opened.notify_all();
        }

        fn published(&self) -> Vec<Vec<u8>> {
            self.published.lock().unwrap().clone()
        }
    }

    fn offer(data: &[u8]) -> Request {
        Request::Offer(Offer {
            mime: "image/png".to_owned(),
            data: Arc::from(data),
        })
    }

    async fn settled() {
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    #[tokio::test]
    async fn the_connection_follows_the_first_subscriber_and_leaves_with_the_last() {
        let subscribers = Arc::new(Feeds::default());
        let (_requests, inbox) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let (watch, factory) = Watch::new();
        tokio::spawn(run(
            Arc::clone(&subscribers),
            inbox,
            cancel.clone(),
            factory,
            unused,
        ));

        settled().await;
        assert_eq!(
            watch.opened(),
            0,
            "no subscriber, so nothing may be watched"
        );

        let events = subscribers.subscribe();
        settled().await;
        assert!(watch.open(), "the first subscriber opens the watch");

        drop(events);
        settled().await;
        assert!(
            !watch.open(),
            "the last subscriber leaving releases the watch"
        );

        let _again = subscribers.subscribe();
        settled().await;
        assert_eq!(watch.opened(), 2, "a returning subscriber opens it again");
        assert!(watch.open());
        cancel.cancel();
    }

    #[tokio::test]
    async fn nothing_is_wanted_the_moment_the_last_subscriber_leaves() {
        let subscribers = Arc::new(Feeds::default());
        let (_requests, inbox) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let (watch, factory) = Watch::new();
        tokio::spawn(run(
            Arc::clone(&subscribers),
            inbox,
            cancel.clone(),
            factory,
            unused,
        ));

        let events = subscribers.subscribe();
        settled().await;
        assert!(watch.wanted(), "a subscriber wants each selection");

        drop(events);
        assert!(
            !watch.wanted(),
            "a selection arriving before the watch is released must not be read"
        );
        cancel.cancel();
    }

    #[tokio::test]
    async fn an_offer_superseded_during_setup_never_publishes() {
        let (requests, inbox) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let (_watch, factory) = Watch::new();
        let (publisher, publish) = Publisher::new();
        tokio::spawn(run(Arc::default(), inbox, cancel.clone(), factory, publish));

        requests.send(offer(b"first")).unwrap();
        settled().await;
        requests.send(offer(b"second")).unwrap();
        settled().await;
        publisher.release();
        settled().await;

        assert_eq!(publisher.published(), vec![b"second".to_vec()]);
        cancel.cancel();
    }

    #[tokio::test]
    async fn offers_are_set_up_one_at_a_time_and_only_the_latest_waiting_one_publishes() {
        let (requests, inbox) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let (_watch, factory) = Watch::new();
        let (publisher, publish) = Publisher::new();
        tokio::spawn(run(Arc::default(), inbox, cancel.clone(), factory, publish));

        for data in [&b"first"[..], b"second", b"third"] {
            requests.send(offer(data)).unwrap();
            settled().await;
        }
        publisher.release();
        settled().await;

        assert_eq!(publisher.published(), vec![b"third".to_vec()]);
        assert_eq!(
            publisher.most_at_once.load(Ordering::SeqCst),
            1,
            "a second setup started while one was in flight"
        );
        cancel.cancel();
    }

    #[tokio::test]
    async fn an_offer_in_setup_when_cancelled_never_publishes() {
        let (requests, inbox) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let (_watch, factory) = Watch::new();
        let (publisher, publish) = Publisher::new();
        tokio::spawn(run(Arc::default(), inbox, cancel.clone(), factory, publish));

        requests.send(offer(b"first")).unwrap();
        settled().await;
        cancel.cancel();
        settled().await;
        publisher.release();
        settled().await;

        assert!(publisher.published().is_empty());
    }
}
