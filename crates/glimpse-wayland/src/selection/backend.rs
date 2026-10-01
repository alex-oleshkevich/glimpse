use std::collections::HashMap;
use std::io::{self, Read};
use std::os::fd::AsFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use glimpse_utils::is_sensitive;
use tokio::io::{Interest, unix::AsyncFd};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use wayland_client::protocol::{wl_callback, wl_seat};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, delegate_dispatch, delegate_noop,
};
use wayland_protocols::ext::data_control::v1::client as ext;
use wayland_protocols_wlr::data_control::v1::client as wlr;

use super::protocol::{DataOffer, Device, Manager, Source};
use super::{Entry, Incoming, MAX_READ_BYTES, Watched};
use crate::wire::{self, Synced};

/// How long a peer gets to answer `receive` before the read is abandoned. An application that
/// advertises a mime and then never writes would otherwise hold the pipe open for ever.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// How many paste targets may be served at once. A client that calls `receive` and never reads
/// parks a blocking thread until it gives up, and nothing limits how many times it may ask — so
/// without a ceiling another process can exhaust the pool the whole panel shares.
const MAX_CONCURRENT_SENDS: usize = 8;

pub(super) const SETUP_TIMEOUT: Duration = Duration::from_secs(5);
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

pub(super) struct Backend {
    /// Mime lists accumulate on an offer before the `selection` event says which offer won.
    mimes: HashMap<u32, Vec<String>>,
    /// Filled by the `selection` handler and drained after the queue is dispatched, because the
    /// read needs the connection flushed first and a handler cannot flush.
    pub(super) incoming: Option<(DataOffer, Vec<String>)>,
    /// What we currently offer, answered on every `send`.
    pub(super) serving: Option<Arc<[Entry]>>,
    source: Option<Source>,
    finished: bool,
    /// Paste targets currently being written to, against `MAX_CONCURRENT_SENDS`.
    sending: Arc<AtomicUsize>,
    pub(super) changes: Option<Vec<Vec<String>>>,
}

pub(super) type Wanted = Box<dyn Fn() -> bool + Send + Sync>;

pub(super) async fn run(events: mpsc::UnboundedSender<Watched>, wanted: Wanted) {
    let mut connecting: Option<tokio::task::JoinHandle<io::Result<Ready>>> = None;

    loop {
        let Err(error) = watch(&events, &mut connecting, &wanted).await else {
            return;
        };
        if error.kind() == io::ErrorKind::Unsupported {
            let reason = error.to_string();
            tracing::info!(
                reason,
                "clipboard capture is unavailable on this compositor"
            );
            let _ = events.send(Watched::Unavailable(reason));
            return;
        }
        tracing::warn!(%error, "clipboard wayland backend failed");
        let _ = events.send(Watched::Unavailable(error.to_string()));
        tokio::select! {
            () = events.closed() => return,
            _ = tokio::time::sleep(RETRY_DELAY) => {}
        }
    }
}

/// Everything the setup produced, built entirely on the blocking pool.
pub(super) struct Ready {
    pub(super) conn: Connection,
    pub(super) queue: EventQueue<Backend>,
    pub(super) qh: QueueHandle<Backend>,
    pub(super) manager: Manager,
    pub(super) device: Device,
    pub(super) backend: Backend,
}

async fn watch(
    events: &mpsc::UnboundedSender<Watched>,
    connecting: &mut Option<tokio::task::JoinHandle<io::Result<Ready>>>,
    wanted: &Wanted,
) -> io::Result<()> {
    let Ready {
        conn,
        mut queue,
        manager,
        mut backend,
        ..
    } = tokio::select! {
        () = events.closed() => return Ok(()),
        setup = connect(connecting) => setup?,
    };

    tracing::info!(protocol = manager.name(), "clipboard backend watching");
    let _ = events.send(Watched::Watching);

    conn.flush().map_err(io::Error::other)?;
    let fd = conn.as_fd().try_clone_to_owned()?;
    let readable = AsyncFd::with_interest(fd, Interest::READABLE)?;

    loop {
        queue
            .dispatch_pending(&mut backend)
            .map_err(io::Error::other)?;
        if let Some((offer, mimes)) = backend.incoming.take() {
            let wanted = !events.is_closed() && wanted();
            read_selection(&conn, events, offer, mimes, wanted)?;
        }
        if backend.finished {
            return Err(io::Error::other(
                "the compositor retired the data-control device",
            ));
        }
        conn.flush().map_err(io::Error::other)?;

        tokio::select! {
            () = events.closed() => return Ok(()),
            guard = readable.readable() => {
                let mut guard = guard?;
                if let Some(read) = conn.prepare_read() {
                    read.read().map_err(io::Error::other)?;
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
    connecting: &mut Option<tokio::task::JoinHandle<io::Result<Ready>>>,
) -> io::Result<Ready> {
    let handle = connecting.get_or_insert_with(|| {
        tokio::task::spawn_blocking(|| bind(Instant::now() + SETUP_TIMEOUT))
    });
    let ready = tokio::time::timeout(SETUP_TIMEOUT, handle)
        .await
        .map_err(|_| {
            io::Error::new(io::ErrorKind::TimedOut, "timed out reaching the compositor")
        })?;
    *connecting = None;
    ready.map_err(|error| io::Error::other(format!("clipboard setup worker failed: {error}")))?
}

pub(super) fn bind(deadline: Instant) -> io::Result<Ready> {
    let (conn, globals) = wire::connect(deadline)?;
    let mut queue = conn.new_event_queue::<Backend>();
    let qh = queue.handle();

    // `ext` first: it is the standardised successor and a compositor offering both prefers it.
    let manager = globals
        .bind::<ext::ext_data_control_manager_v1::ExtDataControlManagerV1, _, _>(&qh, 1..=1, ())
        .map(Manager::Ext)
        .or_else(|| {
            globals
                .bind::<wlr::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1, _, _>(
                    &qh,
                    1..=2,
                    (),
                )
                .map(Manager::Wlr)
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Unsupported,
                "this compositor offers neither ext-data-control-v1 nor \
                 wlr-data-control-unstable-v1, so the clipboard cannot be watched",
            )
        })?;
    let seat = globals
        .bind::<wl_seat::WlSeat, _, _>(&qh, 1..=9, ())
        .ok_or_else(|| io::Error::other("the compositor offers no wl_seat"))?;

    let mut backend = Backend {
        mimes: HashMap::new(),
        incoming: None,
        serving: None,
        source: None,
        finished: false,
        sending: Arc::new(AtomicUsize::new(0)),
        changes: None,
    };
    let device = manager.device(&seat, &qh);
    wire::roundtrip(&conn, &mut queue, &mut backend, deadline)?;

    Ok(Ready {
        conn,
        queue,
        qh,
        manager,
        device,
        backend,
    })
}

/// The read runs after the queue is dispatched, because `receive` has to reach the compositor
/// before anything arrives on the pipe. **Our own copy of the write end is dropped after the
/// flush**: while it is open the reader never sees EOF and the read hangs until the cap.
fn read_selection(
    conn: &Connection,
    events: &mpsc::UnboundedSender<Watched>,
    offer: DataOffer,
    mimes: Vec<String>,
    wanted: bool,
) -> io::Result<()> {
    let unread = |mimes| {
        let _ = events.send(Watched::Incoming(Incoming { mimes, entry: None }));
    };
    let mime = match verdict(&mimes, wanted) {
        Verdict::Unwanted => {
            offer.destroy();
            return Ok(());
        }
        Verdict::Unread => {
            offer.destroy();
            unread(mimes);
            return Ok(());
        }
        Verdict::Sensitive => {
            // Not read at all. Classifying before the content moves is what keeps a password out of
            // this process entirely rather than out of the history afterwards.
            tracing::debug!("a selection marked sensitive was not read");
            offer.destroy();
            unread(mimes);
            return Ok(());
        }
        Verdict::Read(mime) => mime,
    };

    let (reader, writer) = std::io::pipe()?;
    offer.receive(mime.clone(), writer.as_fd());
    conn.flush().map_err(io::Error::other)?;
    drop(writer);
    offer.destroy();

    let events = events.clone();
    tokio::spawn(async move {
        let read = tokio::task::spawn_blocking(move || {
            let mut body = Vec::new();
            // One byte past the cap, so an oversize selection can be told from one that merely
            // fills it. Reading exactly `MAX_READ_BYTES` would truncate silently, and a fragment
            // stored under the sender's mime pastes back as corrupt content — worse than nothing.
            reader
                .take(MAX_READ_BYTES as u64 + 1)
                .read_to_end(&mut body)
                .map(|_| body)
        });
        // An application that advertises a mime and then never writes would hold the pipe open for
        // ever. The bound is on waiting for it, not on the read itself: nothing can make a peer
        // answer, and the worker ends when the pipe is finally closed.
        let entry = match tokio::time::timeout(READ_TIMEOUT, read).await {
            Ok(Ok(Ok(body))) if body.len() > MAX_READ_BYTES => {
                tracing::debug!(
                    %mime,
                    bytes = body.len(),
                    "a selection larger than the read cap was discarded rather than truncated"
                );
                None
            }
            Ok(Ok(Ok(body))) => (!body.is_empty()).then(|| Entry {
                mime,
                data: Arc::from(body.as_slice()),
            }),
            Ok(Ok(Err(error))) => {
                tracing::warn!(%error, %mime, "reading the selection failed");
                None
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, %mime, "the selection reader panicked");
                None
            }
            Err(_) => {
                tracing::warn!(%mime, "the application offering the selection never wrote it");
                None
            }
        };
        let _ = events.send(Watched::Incoming(Incoming { mimes, entry }));
    });
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Unwanted,
    Unread,
    Sensitive,
    Read(String),
}

fn verdict(mimes: &[String], wanted: bool) -> Verdict {
    if !wanted {
        return Verdict::Unwanted;
    }
    match choose(mimes) {
        None => Verdict::Unread,
        Some(_) if is_sensitive(mimes) => Verdict::Sensitive,
        Some(mime) => Verdict::Read(mime),
    }
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

pub(super) async fn own(ready: Ready, stop: CancellationToken, ended: CancellationToken) {
    let _ended = ended.drop_guard();
    let Ready {
        conn,
        mut queue,
        mut backend,
        ..
    } = ready;
    if let Err(error) = hold(&conn, &mut queue, &mut backend, &stop).await {
        tracing::warn!(%error, "serving the selection failed");
    }
    if let Some(source) = backend.source.take() {
        source.destroy();
        let _ = conn.flush();
    }
}

async fn hold(
    conn: &Connection,
    queue: &mut EventQueue<Backend>,
    backend: &mut Backend,
    stop: &CancellationToken,
) -> io::Result<()> {
    let fd = conn.as_fd().try_clone_to_owned()?;
    let readable = AsyncFd::with_interest(fd, Interest::READABLE)?;
    loop {
        queue.dispatch_pending(backend).map_err(io::Error::other)?;
        if let Some((offer, _)) = backend.incoming.take() {
            offer.destroy();
        }
        if backend.finished {
            return Err(io::Error::other(
                "the compositor retired the data-control device",
            ));
        }
        if backend.serving.is_none() {
            return Ok(());
        }
        conn.flush().map_err(io::Error::other)?;

        tokio::select! {
            () = stop.cancelled() => return Ok(()),
            guard = readable.readable() => {
                let mut guard = guard?;
                if let Some(read) = conn.prepare_read() {
                    read.read().map_err(io::Error::other)?;
                }
                guard.clear_ready();
            }
        }
    }
}

impl Backend {
    pub(super) fn publish(
        &mut self,
        manager: &Manager,
        device: &Device,
        qh: &QueueHandle<Self>,
        entries: Vec<Entry>,
    ) {
        if let Some(previous) = self.source.take() {
            previous.destroy();
        }
        let source = manager.source(qh);
        for entry in &entries {
            source.offer(entry.mime.clone());
        }
        device.set_selection(&source);
        self.serving = Some(Arc::from(entries));
        self.source = Some(source);
    }

    fn selected(&mut self, offer: Option<DataOffer>) {
        let Some(offer) = offer else {
            self.mimes.clear();
            if let Some(changes) = &mut self.changes {
                changes.push(Vec::new());
            }
            return;
        };
        let mimes = self.mimes.remove(&offer.key()).unwrap_or_default();
        if let Some(changes) = &mut self.changes {
            changes.push(mimes.clone());
        }
        // Two copies inside one dispatch batch: the offer being displaced is still ours to destroy.
        if let Some((displaced, _)) = self.incoming.replace((offer, mimes)) {
            displaced.destroy();
        }
    }

    fn answer(&self, mime: &str, fd: std::os::fd::OwnedFd) {
        let Some(data) = self.serving.as_ref().and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.mime.eq_ignore_ascii_case(mime))
                .map(|entry| Arc::clone(&entry.data))
        }) else {
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
            if let Err(error) = file.write_all(&data) {
                tracing::debug!(%error, %mime, "a paste target went away mid-write");
            }
            live.fetch_sub(1, Ordering::Relaxed);
        });
    }
}

delegate_noop!(Backend: ignore wl_seat::WlSeat);
delegate_dispatch!(Backend: [wl_callback::WlCallback: Synced] => Synced);

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
                        state.serving = None;
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
mod tests;
