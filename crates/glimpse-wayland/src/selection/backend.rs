use std::collections::HashMap;
use std::io::{self, Write as _};
use std::os::fd::{AsFd, AsRawFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use futures_util::future::BoxFuture;
use glimpse_utils::is_sensitive;
use rustix::event::PollFlags;
use rustix::fs::{FileType, Mode, OFlags, fstat, open};
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
use super::{Entry, Watched, drain, wait};
use crate::wire::{self, Synced};

const READ_TIMEOUT: Duration = Duration::from_secs(5);

const MAX_CONCURRENT_SENDS: usize = 8;
static SENDING: AtomicUsize = AtomicUsize::new(0);
const MAX_MIMES: usize = 64;
const MAX_MIME_BYTES: usize = 256;

pub(super) const SETUP_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_DELAY: Duration = Duration::from_secs(2);

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
    mimes: HashMap<u32, Vec<String>>,
    pub(super) incoming: Option<(DataOffer, Vec<String>)>,
    pub(super) serving: Option<Arc<[Entry]>>,
    source: Option<Source>,
    finished: bool,
    selection_changed: bool,
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
    let mut reading = None;

    loop {
        queue
            .dispatch_pending(&mut backend)
            .map_err(io::Error::other)?;
        if std::mem::take(&mut backend.selection_changed) {
            reading.take();
        }
        if let Some((offer, mimes)) = backend.incoming.take() {
            let wanted = !events.is_closed() && wanted();
            reading = read_selection(&conn, events, offer, mimes, wanted)?;
        }
        if backend.finished {
            return Err(io::Error::other(
                "the compositor retired the data-control device",
            ));
        }
        conn.flush().map_err(io::Error::other)?;

        tokio::select! {
            () = events.closed() => return Ok(()),
            _ = async { reading.as_mut()?.await; Some(()) }, if reading.is_some() => {
                reading.take();
            }
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
        selection_changed: false,
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

fn read_selection(
    conn: &Connection,
    events: &mpsc::UnboundedSender<Watched>,
    offer: DataOffer,
    mimes: Vec<String>,
    wanted: bool,
) -> io::Result<Option<BoxFuture<'static, ()>>> {
    let Some(mime) = verdict(&mimes, wanted) else {
        offer.destroy();
        return Ok(None);
    };

    let (reader, writer) = std::io::pipe()?;
    offer.receive(mime.clone(), writer.as_fd());
    conn.flush().map_err(io::Error::other)?;
    drop(writer);
    offer.destroy();

    Ok(Some(read_entry(reader, mime, events.clone())))
}

fn read_entry(
    reader: std::io::PipeReader,
    mime: String,
    events: mpsc::UnboundedSender<Watched>,
) -> BoxFuture<'static, ()> {
    let deadline = Instant::now() + READ_TIMEOUT;
    Box::pin(async move {
        match tokio::task::spawn_blocking(move || drain(reader, deadline)).await {
            Ok(Ok(data)) if !data.is_empty() => {
                let _ = events.send(Watched::Incoming(Entry { mime, data }));
            }
            Ok(Ok(_)) => {}
            Ok(Err(reason)) => tracing::debug!(?reason, %mime, "reading the selection failed"),
            Err(error) => tracing::warn!(%error, %mime, "the selection reader panicked"),
        }
    })
}

fn verdict(mimes: &[String], wanted: bool) -> Option<String> {
    if !wanted || is_sensitive(mimes) {
        return None;
    }
    choose(mimes)
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
        self.selection_changed = true;
        if let Some((displaced, _)) = self.incoming.take() {
            displaced.destroy();
        }
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
        self.incoming = Some((offer, mimes));
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
        let Some(sending) = Sending::start() else {
            tracing::warn!(%mime, "too many paste targets are already being served");
            return;
        };

        let mime = mime.to_owned();
        let deadline = Instant::now() + READ_TIMEOUT;
        tokio::task::spawn_blocking(move || {
            let _sending = sending;
            if let Err(error) = write_before(fd, &data, deadline) {
                tracing::debug!(%error, %mime, "a paste target went away mid-write");
            }
        });
    }
}

struct Sending;

impl Sending {
    fn start() -> Option<Self> {
        SENDING
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |live| {
                (live < MAX_CONCURRENT_SENDS).then_some(live + 1)
            })
            .ok()
            .map(|_| Self)
    }
}

impl Drop for Sending {
    fn drop(&mut self) {
        SENDING.fetch_sub(1, Ordering::Relaxed);
    }
}

fn write_before(fd: std::os::fd::OwnedFd, mut data: &[u8], deadline: Instant) -> io::Result<()> {
    if FileType::from_raw_mode(fstat(&fd)?.st_mode) != FileType::Fifo {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut file = std::fs::File::from(open(
        format!("/proc/self/fd/{}", fd.as_raw_fd()),
        OFlags::WRONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?);
    while !data.is_empty() {
        wait(&file, PollFlags::OUT, deadline)?;
        match file.write(data) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(written) => data = &data[written..],
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                ) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn keep_mime(mimes: &mut Vec<String>, mut mime: String) {
    let sensitive = is_sensitive(std::slice::from_ref(&mime));
    if sensitive {
        mime = mime.trim().to_owned();
    }
    if mime.len() > MAX_MIME_BYTES || (mimes.len() == MAX_MIMES && !sensitive) {
        return;
    }
    if mimes.len() == MAX_MIMES {
        mimes.pop();
    }
    mimes.push(mime);
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
                    keep_mime(
                        state.mimes.entry(proxy.id().protocol_id()).or_default(),
                        mime_type,
                    );
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
