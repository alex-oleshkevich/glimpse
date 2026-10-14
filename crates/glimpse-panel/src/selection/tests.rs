use super::*;

struct Watch {
    streams: Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<Watched>>>>,
    wanted: Arc<std::sync::Mutex<Option<Wanted>>>,
}

impl Watch {
    fn new() -> (Self, impl Fn(Wanted) -> WatchStream) {
        let streams: Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<Watched>>>> = Arc::default();
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
}

async fn settled_when(done: impl Fn() -> bool) {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while !done() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the selection worker reaches the expected state");
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
    settled_when(|| publisher.setting_up.load(Ordering::SeqCst) == 1).await;
    requests.send(offer(b"second")).unwrap();
    settled().await;
    publisher.release();
    settled_when(|| !publisher.published().is_empty()).await;

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
    settled_when(|| !publisher.published().is_empty()).await;

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
    settled_when(|| publisher.setting_up.load(Ordering::SeqCst) == 1).await;
    cancel.cancel();
    settled().await;
    publisher.release();
    settled_when(|| publisher.setting_up.load(Ordering::SeqCst) == 0).await;

    assert!(publisher.published().is_empty());
}
