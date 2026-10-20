mod backend;
mod protocol;

use std::io::{self, Read};
use std::os::fd::AsFd;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::{Stream, stream};
use glimpse_utils::is_sensitive;
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use wayland_client::{Connection, EventQueue};

use backend::{Backend, Ready, SETUP_TIMEOUT};
use protocol::DataOffer;

const MAX_READ_BYTES: usize = 2 * 1024 * 1024;
const SINCE_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone, PartialEq, Eq)]
pub struct Entry {
    pub mime: String,
    pub data: Arc<[u8]>,
}

impl std::fmt::Debug for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Entry")
            .field("mime", &self.mime)
            .field("bytes", &self.data.len())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    Sensitive,
    TooLarge,
    TimedOut,
    Unreadable,
}

#[derive(Debug, Default)]
pub struct Snapshot {
    pub entries: Vec<Entry>,
    pub skipped: Vec<(String, SkipReason)>,
}

#[derive(Debug, Clone)]
pub enum Watched {
    Watching,
    Incoming(Entry),
    Unavailable(String),
}

pub fn watch(wanted: impl Fn() -> bool + Send + Sync + 'static) -> impl Stream<Item = Watched> {
    let (events, received) = mpsc::unbounded_channel();
    tokio::spawn(backend::run(events, Box::new(wanted)));
    stream::unfold(received, |mut received| async move {
        received.recv().await.map(|event| (event, received))
    })
}

pub fn snapshot(budget: Duration) -> io::Result<Snapshot> {
    let deadline = Instant::now() + budget;
    let Ready {
        conn, mut backend, ..
    } = backend::bind(deadline)?;
    let Some((offer, mimes)) = backend.incoming.take() else {
        return Ok(Snapshot::default());
    };
    let snapshot = read_mimes(&mimes, |mime| read_before(&conn, &offer, mime, deadline));
    offer.destroy();
    conn.flush().map_err(io::Error::other)?;
    Ok(snapshot)
}

fn read_mimes(
    mimes: &[String],
    mut read: impl FnMut(&str) -> Result<Arc<[u8]>, SkipReason>,
) -> Snapshot {
    let sensitive = is_sensitive(mimes);
    let mut snapshot = Snapshot::default();
    let mut total = 0;
    for mime in mimes {
        let result = if sensitive {
            Err(SkipReason::Sensitive)
        } else if total == MAX_READ_BYTES {
            Err(SkipReason::TooLarge)
        } else {
            read(mime)
        };
        match result {
            Ok(data) if data.len() <= MAX_READ_BYTES - total => {
                total += data.len();
                snapshot.entries.push(Entry {
                    mime: mime.clone(),
                    data,
                });
            }
            Ok(_) | Err(SkipReason::TooLarge) => {
                total = MAX_READ_BYTES;
                snapshot.skipped.push((mime.clone(), SkipReason::TooLarge));
            }
            Err(reason) => snapshot.skipped.push((mime.clone(), reason)),
        }
    }
    snapshot
}

fn read_before(
    conn: &Connection,
    offer: &DataOffer,
    mime: &str,
    deadline: Instant,
) -> Result<Arc<[u8]>, SkipReason> {
    if Instant::now() >= deadline {
        return Err(SkipReason::TimedOut);
    }
    let (reader, writer) = std::io::pipe().map_err(|_| SkipReason::Unreadable)?;
    offer.receive(mime.to_owned(), writer.as_fd());
    conn.flush().map_err(|_| SkipReason::Unreadable)?;
    drop(writer);

    drain(reader, deadline)
}

fn drain(mut reader: impl Read + AsFd, deadline: Instant) -> Result<Arc<[u8]>, SkipReason> {
    fcntl_getfl(&reader)
        .and_then(|flags| fcntl_setfl(&reader, flags | OFlags::NONBLOCK))
        .map_err(|_| SkipReason::Unreadable)?;
    let mut body = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        wait(&reader, PollFlags::IN, deadline).map_err(|error| match error.kind() {
            io::ErrorKind::TimedOut => SkipReason::TimedOut,
            _ => SkipReason::Unreadable,
        })?;
        match reader.read(&mut chunk) {
            Ok(0) => return Ok(Arc::from(body)),
            Ok(read) => body.extend_from_slice(&chunk[..read]),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                ) => {}
            Err(_) => return Err(SkipReason::Unreadable),
        }
        if body.len() > MAX_READ_BYTES {
            return Err(SkipReason::TooLarge);
        }
    }
}

fn wait(fd: &impl AsFd, flags: PollFlags, deadline: Instant) -> io::Result<()> {
    loop {
        let left = deadline
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or(io::ErrorKind::TimedOut)?;
        let timeout = Timespec {
            tv_sec: left.as_secs() as i64,
            tv_nsec: i64::from(left.subsec_nanos()),
        };
        match poll(&mut [PollFd::new(fd, flags)], Some(&timeout)) {
            Ok(0) => return Err(io::ErrorKind::TimedOut.into()),
            Ok(_) => return Ok(()),
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

struct Listening {
    conn: Connection,
    queue: EventQueue<Backend>,
    backend: Backend,
}

pub struct Changes {
    listening: Mutex<Listening>,
}

impl Changes {
    pub fn start() -> io::Result<Changes> {
        let Ready {
            conn,
            queue,
            mut backend,
            ..
        } = backend::bind(Instant::now() + SETUP_TIMEOUT)?;
        if let Some((offer, _)) = backend.incoming.take() {
            offer.destroy();
        }
        backend.changes = Some(Vec::new());
        conn.flush().map_err(io::Error::other)?;
        Ok(Changes {
            listening: Mutex::new(Listening {
                conn,
                queue,
                backend,
            }),
        })
    }

    pub fn since(&self) -> Vec<Vec<String>> {
        let mut held = self
            .listening
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Listening {
            conn,
            queue,
            backend,
        } = &mut *held;
        if let Err(error) =
            crate::wire::roundtrip(conn, queue, backend, Instant::now() + SINCE_TIMEOUT)
        {
            tracing::warn!(?error, "selection changes may be incomplete");
        }
        if let Some((offer, _)) = backend.incoming.take() {
            offer.destroy();
            let _ = conn.flush();
        }
        backend.changes.clone().unwrap_or_default()
    }
}

pub struct Owned {
    stop: CancellationToken,
    ended: CancellationToken,
}

impl Owned {
    pub fn serve(entries: Vec<Entry>, current: impl FnOnce() -> bool) -> io::Result<Option<Owned>> {
        let runtime = tokio::runtime::Handle::try_current().map_err(io::Error::other)?;
        let mut ready = backend::bind(Instant::now() + SETUP_TIMEOUT)?;
        if let Some((offer, _)) = ready.backend.incoming.take() {
            offer.destroy();
        }
        if !current() {
            return Ok(None);
        }
        ready
            .backend
            .publish(&ready.manager, &ready.device, &ready.qh, entries);
        crate::wire::roundtrip(
            &ready.conn,
            &mut ready.queue,
            &mut ready.backend,
            Instant::now() + SETUP_TIMEOUT,
        )
        .map_err(|stall| {
            io::Error::other(format!(
                "the compositor did not take the selection: {stall:?}"
            ))
        })?;
        if ready.backend.serving.is_none() {
            return Err(io::Error::other(
                "the selection was replaced as soon as it was set",
            ));
        }
        let stop = CancellationToken::new();
        let ended = CancellationToken::new();
        runtime.spawn(backend::own(ready, stop.clone(), ended.clone()));
        Ok(Some(Owned { stop, ended }))
    }

    pub async fn cancelled(&self) {
        self.ended.cancelled().await;
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

#[cfg(test)]
mod tests;
