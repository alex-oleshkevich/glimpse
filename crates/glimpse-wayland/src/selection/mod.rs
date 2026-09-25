mod backend;
mod protocol;

use std::io::{self, Read as _};
use std::os::fd::AsFd;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::{Stream, stream};
use glimpse_utils::is_sensitive;
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use wayland_client::{Connection, EventQueue};

use backend::{Backend, Ready, SETUP_TIMEOUT};
use protocol::DataOffer;

/// The largest selection that will be read off the compositor. Another application chooses this
/// content, so the cap is what stops one handing over something enormous; the service refuses
/// anything above its own configured cap a second time.
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
    Incoming(Incoming),
    Unavailable(String),
}

#[derive(Debug, Clone)]
pub struct Incoming {
    pub mimes: Vec<String>,
    pub entry: Option<Entry>,
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
    let (wanted, mut skipped) = classify(&mimes);
    let mut entries = Vec::new();
    for mime in wanted {
        match read_before(&conn, &offer, &mime, deadline) {
            Ok(data) => entries.push(Entry { mime, data }),
            Err(reason) => skipped.push((mime, reason)),
        }
    }
    offer.destroy();
    conn.flush().map_err(io::Error::other)?;
    Ok(Snapshot { entries, skipped })
}

fn classify(mimes: &[String]) -> (Vec<String>, Vec<(String, SkipReason)>) {
    match is_sensitive(mimes) {
        true => (
            Vec::new(),
            mimes
                .iter()
                .map(|mime| (mime.clone(), SkipReason::Sensitive))
                .collect(),
        ),
        false => (mimes.to_vec(), Vec::new()),
    }
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
    let (mut reader, writer) = std::io::pipe().map_err(|_| SkipReason::Unreadable)?;
    offer.receive(mime.to_owned(), writer.as_fd());
    conn.flush().map_err(|_| SkipReason::Unreadable)?;
    drop(writer);

    let mut body = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let left = deadline
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or(SkipReason::TimedOut)?;
        let timeout = Timespec {
            tv_sec: left.as_secs() as i64,
            tv_nsec: i64::from(left.subsec_nanos()),
        };
        let mut fds = [PollFd::new(&reader, PollFlags::IN)];
        match poll(&mut fds, Some(&timeout)) {
            Ok(0) => return Err(SkipReason::TimedOut),
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => return Err(SkipReason::Unreadable),
        }
        match reader.read(&mut chunk) {
            Ok(0) => return Ok(Arc::from(body)),
            Ok(read) => body.extend_from_slice(&chunk[..read]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return Err(SkipReason::Unreadable),
        }
        if body.len() > MAX_READ_BYTES {
            return Err(SkipReason::TooLarge);
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
mod tests {
    use super::*;

    fn mimes(list: &[&str]) -> Vec<String> {
        list.iter().map(|mime| (*mime).to_owned()).collect()
    }

    #[test]
    fn a_sensitive_selection_reads_nothing_and_skips_every_mime() {
        let offered = mimes(&["text/plain", "x-kde-passwordManagerHint", "UTF8_STRING"]);

        let (wanted, skipped) = classify(&offered);

        assert!(wanted.is_empty(), "nothing may be read, got {wanted:?}");
        assert_eq!(
            skipped,
            offered
                .iter()
                .map(|mime| (mime.clone(), SkipReason::Sensitive))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn an_ordinary_selection_reads_every_offered_mime() {
        let offered = mimes(&["text/plain", "image/png"]);

        let (wanted, skipped) = classify(&offered);

        assert_eq!(wanted, offered);
        assert!(skipped.is_empty());
    }

    #[test]
    fn neither_content_reaches_a_debug_line() {
        let entry = Entry {
            mime: "text/plain".to_owned(),
            data: Arc::from(&b"hunter2"[..]),
        };

        let rendered = format!("{entry:?}");

        assert!(!rendered.contains("hunter2"), "got {rendered}");
        assert!(rendered.contains("bytes: 7"), "got {rendered}");
    }
}
