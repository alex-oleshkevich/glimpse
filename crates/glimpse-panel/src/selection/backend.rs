use std::collections::HashMap;
use std::io::Read;
use std::os::fd::AsFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use glimpse_services::{Capture, Offer, SelectionEvent, is_sensitive};
use tokio::io::{Interest, unix::AsyncFd};
use tokio::sync::mpsc;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop};
use wayland_protocols::ext::data_control::v1::client as ext;
use wayland_protocols_wlr::data_control::v1::client as wlr;

use super::protocol::{DataOffer, Device, Manager, Source};

/// The largest selection that will be read off the compositor. Another application chooses this
/// content, so the cap is what stops one handing over something enormous; the service refuses
/// anything above its own configured cap a second time.
const MAX_READ_BYTES: u64 = 2 * 1024 * 1024;

/// How long a peer gets to answer `receive` before the read is abandoned. An application that
/// advertises a mime and then never writes would otherwise hold the pipe open for ever.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// How many paste targets may be served at once. A client that calls `receive` and never reads
/// parks a blocking thread until it gives up, and nothing limits how many times it may ask — so
/// without a ceiling another process can exhaust the pool the whole panel shares.
const MAX_CONCURRENT_SENDS: usize = 8;

const SETUP_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_DELAY: Duration = Duration::from_secs(2);

/// What the panel asks for, in order. The first the offer advertises is the one read.
const PREFERRED: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "STRING",
    "TEXT",
    "text/html",
    "image/png",
    "image/jpeg",
];

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

pub enum Request {
    Offer(Offer),
}

pub type Subscribers = Arc<Feeds>;

#[derive(Default)]
pub struct Feeds {
    live: std::sync::Mutex<Vec<mpsc::UnboundedSender<SelectionEvent>>>,
    wanted: tokio::sync::Notify,
}

impl Feeds {
    pub fn subscribe(&self) -> mpsc::UnboundedReceiver<SelectionEvent> {
        let (feed, events) = mpsc::unbounded_channel();
        self.held().push(feed);
        self.wanted.notify_one();
        events
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
}

pub fn announce(subscribers: &Subscribers, event: SelectionEvent) {
    let held = subscribers.held();
    for feed in held.iter().filter(|feed| !feed.is_closed()) {
        let _ = feed.send(event.clone());
    }
}

pub struct Backend {
    /// Mime lists accumulate on an offer before the `selection` event says which offer won.
    mimes: HashMap<u32, Vec<String>>,
    /// Filled by the `selection` handler and drained after the queue is dispatched, because the
    /// read needs the connection flushed first and a handler cannot flush.
    incoming: Option<(DataOffer, Vec<String>)>,
    /// What we currently offer, answered on every `send`.
    offering: Option<Offer>,
    source: Option<Source>,
    finished: bool,
    /// Paste targets currently being written to, against `MAX_CONCURRENT_SENDS`.
    sending: Arc<AtomicUsize>,
}

/// A failure that cannot resolve by waiting. A compositor does not grow a global mid-session, so
/// retrying "neither protocol" every two seconds would open a fresh connection and log a warning
/// for the life of the session on every desktop that implements neither.
#[derive(Debug)]
struct Unsupported(String);

enum Flow {
    Idle,
    Stop,
}

pub async fn run(
    subscribers: Subscribers,
    mut requests: mpsc::UnboundedReceiver<Request>,
    cancel: tokio_util::sync::CancellationToken,
) {
    // Survives a reconnect, so an entry the viewer restored is offered again rather than silently
    // lost: `Backend` is rebuilt per attempt and cannot hold it.
    let mut offering: Option<Offer> = None;
    let mut connecting: Option<tokio::task::JoinHandle<anyhow::Result<Ready>>> = None;

    loop {
        if offering.is_none() {
            tokio::select! {
                _ = cancel.cancelled() => return,
                () = subscribers.awaited() => {}
                request = requests.recv() => match request {
                    Some(Request::Offer(offer)) => offering = Some(offer),
                    None => return,
                },
            }
        }

        match serve(
            &subscribers,
            &mut requests,
            &cancel,
            &mut offering,
            &mut connecting,
        )
        .await
        {
            Ok(Flow::Idle) => {}
            Ok(Flow::Stop) => return,
            Err(error) => {
                if let Some(Unsupported(reason)) = error.downcast_ref::<Unsupported>() {
                    tracing::info!(
                        reason,
                        "clipboard capture is unavailable on this compositor"
                    );
                    announce(&subscribers, SelectionEvent::Unavailable(reason.clone()));
                    return;
                }
                tracing::warn!(%error, "clipboard wayland backend failed");
                announce(&subscribers, SelectionEvent::Unavailable(error.to_string()));
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(RETRY_DELAY) => {}
                }
            }
        }
    }
}

/// Everything the setup produced, built entirely on the blocking pool.
struct Ready {
    conn: Connection,
    queue: wayland_client::EventQueue<Backend>,
    qh: QueueHandle<Backend>,
    manager: Manager,
    device: Device,
    backend: Backend,
}

async fn serve(
    subscribers: &Subscribers,
    requests: &mut mpsc::UnboundedReceiver<Request>,
    cancel: &tokio_util::sync::CancellationToken,
    offering: &mut Option<Offer>,
    connecting: &mut Option<tokio::task::JoinHandle<anyhow::Result<Ready>>>,
) -> anyhow::Result<Flow> {
    let Ready {
        conn,
        mut queue,
        qh,
        manager,
        device,
        mut backend,
    } = tokio::select! {
        _ = cancel.cancelled() => return Ok(Flow::Stop),
        setup = connect(connecting) => setup?,
    };

    tracing::info!(protocol = manager.name(), "clipboard backend watching");
    announce(subscribers, SelectionEvent::Watching);

    // An entry restored before the connection dropped is offered again, or the viewer pastes
    // something other than what they picked with nothing to say why.
    if let Some(held) = offering.clone() {
        backend.publish(&manager, &device, &qh, held);
    }

    conn.flush()?;
    let fd = conn.as_fd().try_clone_to_owned()?;
    let readable = AsyncFd::with_interest(fd, Interest::READABLE)?;

    loop {
        queue.dispatch_pending(&mut backend)?;
        offering.clone_from(&backend.offering);
        if let Some((offer, mimes)) = backend.incoming.take() {
            match subscribers.any() {
                true => read_selection(&conn, subscribers, offer, mimes)?,
                false => offer.destroy(),
            }
        }
        if backend.finished {
            anyhow::bail!("the compositor retired the data-control device");
        }
        conn.flush()?;

        if !subscribers.any() && offering.is_none() {
            tracing::info!("clipboard backend idle; releasing the compositor connection");
            return Ok(Flow::Idle);
        }

        tokio::select! {
            _ = cancel.cancelled() => return Ok(Flow::Stop),
            request = requests.recv() => match request {
                Some(Request::Offer(offer)) => {
                    *offering = Some(offer.clone());
                    backend.publish(&manager, &device, &qh, offer);
                    conn.flush()?;
                }
                None => return Ok(Flow::Stop),
            },
            guard = readable.readable() => {
                let mut guard = guard?;
                if let Some(read) = conn.prepare_read() {
                    read.read()?;
                }
                guard.clear_ready();
            }
        }
    }
}

/// The `JoinHandle` is carried across retries rather than dropped. Dropping it does **not** cancel
/// a blocking task, so a compositor that never answers would otherwise leak one pool thread every
/// `RETRY_DELAY` until the pool is exhausted and every other `spawn_blocking` in the panel stalls.
async fn connect(
    connecting: &mut Option<tokio::task::JoinHandle<anyhow::Result<Ready>>>,
) -> anyhow::Result<Ready> {
    let handle = connecting.get_or_insert_with(|| tokio::task::spawn_blocking(bind));
    let ready = tokio::time::timeout(SETUP_TIMEOUT, handle)
        .await
        .map_err(|_| anyhow::anyhow!("timed out reaching the compositor"))?;
    *connecting = None;
    ready.map_err(|error| anyhow::anyhow!("clipboard setup worker failed: {error}"))?
}

/// Every blocking step of the setup, in one unit under `SETUP_TIMEOUT`: `connect_to_env`,
/// `registry_queue_init` and the `roundtrip` all wait on the compositor. `block_in_place` cannot be
/// used — `PanelServices::start_with_buses` is built from `#[tokio::test]`, whose runtime is
/// current-thread, where it panics — and leaving the roundtrip on an async worker parks a share of
/// the panel's runtime for as long as the compositor stays silent.
fn bind() -> anyhow::Result<Ready> {
    let conn = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<Backend>(&conn)?;
    let qh = queue.handle();

    // `ext` first: it is the standardised successor and a compositor offering both prefers it.
    let manager = globals
        .bind::<ext::ext_data_control_manager_v1::ExtDataControlManagerV1, _, _>(&qh, 1..=1, ())
        .map(Manager::Ext)
        .or_else(|_| {
            globals
                .bind::<wlr::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1, _, _>(
                    &qh,
                    1..=2,
                    (),
                )
                .map(Manager::Wlr)
        })
        .map_err(|_| {
            Unsupported(
                "this compositor offers neither ext-data-control-v1 nor \
                 wlr-data-control-unstable-v1, so the clipboard cannot be watched"
                    .to_owned(),
            )
        })?;
    let seat = globals.bind::<wl_seat::WlSeat, _, _>(&qh, 1..=9, ())?;

    let mut backend = Backend {
        mimes: HashMap::new(),
        incoming: None,
        offering: None,
        source: None,
        finished: false,
        sending: Arc::new(AtomicUsize::new(0)),
    };
    let device = manager.device(&seat, &qh);
    queue.roundtrip(&mut backend)?;

    Ok(Ready {
        conn,
        queue,
        qh,
        manager,
        device,
        backend,
    })
}

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Unsupported {}

/// The read runs after the queue is dispatched, because `receive` has to reach the compositor
/// before anything arrives on the pipe. **Our own copy of the write end is dropped after the
/// flush**: while it is open the reader never sees EOF and the read hangs until the cap.
fn read_selection(
    conn: &Connection,
    subscribers: &Subscribers,
    offer: DataOffer,
    mimes: Vec<String>,
) -> anyhow::Result<()> {
    let sensitive = is_sensitive(&mimes);
    let Some(mime) = choose(&mimes) else {
        offer.destroy();
        return Ok(());
    };

    if sensitive {
        // Not read at all. Classifying before the content moves is what keeps a password out of
        // this process entirely rather than out of the history afterwards.
        tracing::debug!(%mime, "a selection marked sensitive was not read");
        offer.destroy();
        return Ok(());
    }

    let (reader, writer) = std::io::pipe()?;
    offer.receive(mime.clone(), writer.as_fd());
    conn.flush()?;
    drop(writer);
    offer.destroy();

    let subscribers = Arc::clone(subscribers);
    tokio::spawn(async move {
        let read = tokio::task::spawn_blocking(move || {
            let mut body = Vec::new();
            // One byte past the cap, so an oversize selection can be told from one that merely
            // fills it. Reading exactly `MAX_READ_BYTES` would truncate silently, and a fragment
            // stored under the sender's mime pastes back as corrupt content — worse than nothing.
            reader
                .take(MAX_READ_BYTES + 1)
                .read_to_end(&mut body)
                .map(|_| body)
        });
        // An application that advertises a mime and then never writes would hold the pipe open for
        // ever. The bound is on waiting for it, not on the read itself: nothing can make a peer
        // answer, and the worker ends when the pipe is finally closed.
        let body = match tokio::time::timeout(READ_TIMEOUT, read).await {
            Ok(Ok(Ok(body))) => body,
            Ok(Ok(Err(error))) => {
                tracing::warn!(%error, %mime, "reading the selection failed");
                return;
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, %mime, "the selection reader panicked");
                return;
            }
            Err(_) => {
                tracing::warn!(%mime, "the application offering the selection never wrote it");
                return;
            }
        };
        if body.is_empty() {
            return;
        }
        if body.len() as u64 > MAX_READ_BYTES {
            tracing::debug!(
                %mime,
                bytes = body.len(),
                "a selection larger than the read cap was discarded rather than truncated"
            );
            return;
        }
        announce(
            &subscribers,
            SelectionEvent::Captured(Capture {
                mime,
                data: Arc::from(body.as_slice()),
                sensitive: false,
            }),
        );
    });
    Ok(())
}

fn choose(mimes: &[String]) -> Option<String> {
    PREFERRED
        .iter()
        .find(|wanted| {
            mimes
                .iter()
                .any(|offered| offered.eq_ignore_ascii_case(wanted))
        })
        .map(|wanted| (*wanted).to_string())
}

impl Backend {
    fn publish(
        &mut self,
        manager: &Manager,
        device: &Device,
        qh: &QueueHandle<Self>,
        offer: Offer,
    ) {
        if let Some(previous) = self.source.take() {
            previous.destroy();
        }
        let source = manager.source(qh);
        // Text is offered under every spelling a paste target might ask for, as `wl-copy` does;
        // anything else is offered as itself. Membership of the alias list is the test, not a guess
        // at the shape of the string.
        let text = TEXT_ALIASES
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case(&offer.mime));
        let mut advertised: Vec<String> = match text {
            true => TEXT_ALIASES.iter().map(|m| (*m).to_string()).collect(),
            false => vec![offer.mime.clone()],
        };
        if !advertised.contains(&offer.mime) {
            advertised.push(offer.mime.clone());
        }
        for mime in advertised {
            source.offer(mime);
        }
        device.set_selection(&source);
        self.offering = Some(offer);
        self.source = Some(source);
    }

    fn selected(&mut self, offer: Option<DataOffer>) {
        let Some(offer) = offer else {
            self.mimes.clear();
            return;
        };
        let mimes = self.mimes.remove(&offer.key()).unwrap_or_default();
        // Two copies inside one dispatch batch: the offer being displaced is still ours to destroy.
        if let Some((displaced, _)) = self.incoming.replace((offer, mimes)) {
            displaced.destroy();
        }
    }

    fn answer(&self, mime: &str, fd: std::os::fd::OwnedFd) {
        let Some(offering) = self.offering.clone() else {
            return;
        };
        let live = Arc::clone(&self.sending);
        if live.fetch_add(1, Ordering::Relaxed) >= MAX_CONCURRENT_SENDS {
            live.fetch_sub(1, Ordering::Relaxed);
            tracing::warn!(%mime, "too many paste targets are already being served");
            return;
        }

        let mime = mime.to_owned();
        // Written on the blocking pool: a paste target that reads slowly would otherwise stall the
        // whole Wayland queue, and with it every other selection the panel is watching.
        tokio::task::spawn_blocking(move || {
            use std::io::Write as _;
            let mut file = std::fs::File::from(fd);
            if let Err(error) = file.write_all(&offering.data) {
                tracing::debug!(%error, %mime, "a paste target went away mid-write");
            }
            live.fetch_sub(1, Ordering::Relaxed);
        });
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Backend {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(Backend: ignore wl_seat::WlSeat);

macro_rules! device_dispatch {
    ($device:path, $events:path, $offer:path, $wrap:path) => {
        impl Dispatch<$device, ()> for Backend {
            fn event(
                state: &mut Self,
                _proxy: &$device,
                event: $events,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
                use wayland_client::Proxy as _;
                use $events as Event;
                match event {
                    Event::DataOffer { id } => {
                        state.mimes.insert(id.id().protocol_id(), Vec::new());
                    }
                    Event::Selection { id } => state.selected(id.map($wrap)),
                    // The protocol announces `data_offer` before `selection` OR
                    // `primary_selection`, so a mouse highlight lands here. The panel does not
                    // remember the primary selection, but the offer is still ours to destroy —
                    // ignoring it leaks a map entry and a compositor resource per highlight.
                    Event::PrimarySelection { id: Some(id) } => {
                        let offer = $wrap(id);
                        state.mimes.remove(&offer.key());
                        offer.destroy();
                    }
                    Event::Finished => state.finished = true,
                    _ => {}
                }
            }

            wayland_client::event_created_child!(Backend, $device, [
                0 => ($offer, ()),
            ]);
        }
    };
}

device_dispatch!(
    ext::ext_data_control_device_v1::ExtDataControlDeviceV1,
    ext::ext_data_control_device_v1::Event,
    ext::ext_data_control_offer_v1::ExtDataControlOfferV1,
    DataOffer::Ext
);
device_dispatch!(
    wlr::zwlr_data_control_device_v1::ZwlrDataControlDeviceV1,
    wlr::zwlr_data_control_device_v1::Event,
    wlr::zwlr_data_control_offer_v1::ZwlrDataControlOfferV1,
    DataOffer::Wlr
);

macro_rules! offer_dispatch {
    ($offer:path, $events:path) => {
        impl Dispatch<$offer, ()> for Backend {
            fn event(
                state: &mut Self,
                proxy: &$offer,
                event: $events,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
                use wayland_client::Proxy as _;
                use $events as Event;
                if let Event::Offer { mime_type } = event {
                    state
                        .mimes
                        .entry(proxy.id().protocol_id())
                        .or_default()
                        .push(mime_type);
                }
            }
        }
    };
}

offer_dispatch!(
    ext::ext_data_control_offer_v1::ExtDataControlOfferV1,
    ext::ext_data_control_offer_v1::Event
);
offer_dispatch!(
    wlr::zwlr_data_control_offer_v1::ZwlrDataControlOfferV1,
    wlr::zwlr_data_control_offer_v1::Event
);

macro_rules! source_dispatch {
    ($source:path, $events:path) => {
        impl Dispatch<$source, ()> for Backend {
            fn event(
                state: &mut Self,
                _proxy: &$source,
                event: $events,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
                use $events as Event;
                match event {
                    Event::Send { mime_type, fd } => state.answer(&mime_type, fd),
                    // Another client took the selection; ours is dead and must not be reused.
                    Event::Cancelled => {
                        if let Some(source) = state.source.take() {
                            source.destroy();
                        }
                        state.offering = None;
                    }
                    _ => {}
                }
            }
        }
    };
}

source_dispatch!(
    ext::ext_data_control_source_v1::ExtDataControlSourceV1,
    ext::ext_data_control_source_v1::Event
);
source_dispatch!(
    wlr::zwlr_data_control_source_v1::ZwlrDataControlSourceV1,
    wlr::zwlr_data_control_source_v1::Event
);

#[cfg(test)]
mod tests {
    use super::*;

    fn mimes(list: &[&str]) -> Vec<String> {
        list.iter().map(|mime| (*mime).to_owned()).collect()
    }

    #[test]
    fn the_ladder_prefers_utf8_text_over_every_other_spelling() {
        let chosen = choose(&mimes(&[
            "STRING",
            "text/plain",
            "text/plain;charset=utf-8",
        ]));

        assert_eq!(chosen.as_deref(), Some("text/plain;charset=utf-8"));
    }

    #[test]
    fn an_offer_of_only_an_image_is_read_as_an_image() {
        assert_eq!(choose(&mimes(&["image/png"])).as_deref(), Some("image/png"));
    }

    #[test]
    fn text_is_preferred_to_an_image_offered_beside_it() {
        assert_eq!(
            choose(&mimes(&["image/png", "text/plain"])).as_deref(),
            Some("text/plain")
        );
    }

    #[test]
    fn an_offer_of_nothing_we_understand_is_not_read() {
        assert_eq!(choose(&mimes(&["application/x-private"])), None);
        assert_eq!(choose(&[]), None);
    }

    /// X11 atoms arrive from some toolkits in their own casing, and mime types are case-insensitive.
    #[test]
    fn a_mime_is_matched_without_regard_to_casing() {
        assert_eq!(
            choose(&mimes(&["TEXT/PLAIN"])).as_deref(),
            Some("text/plain")
        );
    }
}
