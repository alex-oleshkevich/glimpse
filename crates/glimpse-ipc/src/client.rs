use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use tokio::{
    net::UnixStream,
    sync::{Notify, OwnedSemaphorePermit, Semaphore, mpsc, oneshot, watch},
    time,
};
use tokio_util::codec::Framed;

use std::io;

use crate::{
    codec::{CodecError, FrameCodec},
    frame::{Body, CallError, ErrorCode, Event, Frame},
    pattern,
};

/// The one cap on outstanding requests. A permit is taken before a request is queued and released
/// when its reply settles, so nothing can be waiting on the daemon without holding one — there is
/// no second queue for a request to sit in and no call that blocks instead of being refused.
const MAX_INFLIGHT: usize = 32;
const REQUEST_TIMEOUT: time::Duration = time::Duration::from_secs(5);
const BACKOFF_MIN: time::Duration = time::Duration::from_millis(250);
const BACKOFF_MAX: time::Duration = time::Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("no daemon listening at {path}")]
    NotListening {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the daemon did not complete the handshake")]
    Handshake,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Reconnecting {
        attempt: u32,
        next_in: time::Duration,
    },
}
type Wire = Framed<UnixStream, FrameCodec>;

/// Cheap to clone: every handle addresses the one connection task through the same channel.
#[derive(Clone)]
pub struct Client {
    requests: mpsc::Sender<Request>,
    inflight: Arc<Semaphore>,
    state: watch::Receiver<ConnectionState>,
}

impl Client {
    pub async fn connect(socket: &Path) -> Result<Self, ConnectError> {
        Ok(Self::start(socket, Some(dial(socket).await?)))
    }

    pub async fn open(socket: &Path) -> Self {
        Self::start(socket, dial(socket).await.ok())
    }

    fn start(socket: &Path, wire: Option<Wire>) -> Self {
        let (requests, inbox) = mpsc::channel(MAX_INFLIGHT);
        let (state, watcher) = watch::channel(match wire {
            Some(_) => ConnectionState::Connected,
            None => ConnectionState::Connecting,
        });

        tokio::spawn(
            Connection {
                socket: socket.to_owned(),
                inbox,
                state,
                subscriptions: Vec::new(),
                pending: HashMap::new(),
                next_id: 0,
            }
            .run(wire),
        );

        Self {
            requests,
            inflight: Arc::new(Semaphore::new(MAX_INFLIGHT)),
            state: watcher,
        }
    }

    pub fn watch_state(&self) -> watch::Receiver<ConnectionState> {
        self.state.clone()
    }

    pub async fn get(&self, topic: &str) -> Result<Option<Event>, CallError> {
        let (reply, answer) = oneshot::channel();
        self.ask(
            |permit| Request::Ask {
                body: Body::Get {
                    topic: topic.to_owned(),
                },
                pending: Pending::Get(reply),
                permit,
            },
            answer,
        )
        .await
    }

    pub async fn call(&self, command: &str, args: Value) -> Result<Value, CallError> {
        let (reply, answer) = oneshot::channel();
        self.ask(
            |permit| Request::Ask {
                body: Body::Call {
                    command: command.to_owned(),
                    args,
                },
                pending: Pending::Call(reply),
                permit,
            },
            answer,
        )
        .await
    }

    pub async fn subscribe(&self, pattern: &str) -> Result<Subscription, CallError> {
        let mailbox = Mailbox::new();
        let watched = Arc::downgrade(&mailbox);
        let (reply, answer) = oneshot::channel();

        let matched = self
            .ask(
                |permit| Request::Watch {
                    pattern: pattern.to_owned(),
                    mailbox: watched,
                    reply,
                    permit,
                },
                answer,
            )
            .await?;

        tracing::debug!(pattern, matched, "subscribed");
        Ok(Subscription {
            pattern: pattern.to_owned(),
            matched,
            mailbox,
            requests: self.requests.downgrade(),
        })
    }

    /// The permit is taken here and travels with the request, so the slot is held for exactly as
    /// long as the daemon owes an answer.
    async fn ask<T>(
        &self,
        build: impl FnOnce(OwnedSemaphorePermit) -> Request,
        answer: oneshot::Receiver<Result<T, CallError>>,
    ) -> Result<T, CallError> {
        let permit = Arc::clone(&self.inflight)
            .try_acquire_owned()
            .map_err(|_| {
                CallError::new(
                    ErrorCode::LimitExceeded,
                    "too many requests are already in flight",
                )
            })?;

        self.requests
            .send(build(permit))
            .await
            .map_err(|_| gone())?;
        answer.await.map_err(|_| gone())?
    }
}

fn gone() -> CallError {
    CallError::new(
        ErrorCode::Unavailable,
        "the connection to the daemon has stopped",
    )
}

/// One subscription's pending events, newest-wins per topic.
///
/// A bounded channel drops the value it is handed once it is full, which is the *newest* one — a
/// subscriber that falls behind then renders an old value and never catches up. Coalescing per
/// topic drops the intermediate values instead, which costs nothing because every event carries the
/// whole one.
struct Mailbox {
    queued: Mutex<Queued>,
    ready: Notify,
}

#[derive(Default)]
struct Queued {
    order: VecDeque<String>,
    events: HashMap<String, Event>,
    closed: bool,
}

impl Mailbox {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            queued: Mutex::new(Queued::default()),
            ready: Notify::new(),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Queued> {
        self.queued
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// A value older than the one already queued is dropped rather than replacing it, so a
    /// subscription snapshot and a value published while the subscribe was in flight are safe in
    /// either order — the newer one wins whichever arrives second.
    fn offer(&self, event: Event) {
        {
            let mut queued = self.lock();
            match queued.events.get(&event.topic).map(|pending| pending.seq) {
                Some(seq) if seq >= event.seq => return,
                Some(_) => {}
                None => queued.order.push_back(event.topic.clone()),
            }
            queued.events.insert(event.topic.clone(), event);
        }
        self.ready.notify_one();
    }

    fn take(&self) -> Option<Event> {
        let mut queued = self.lock();
        loop {
            // `order` holds one entry per queued topic, so the map always has it. Skipping rather
            // than indexing keeps that invariant from becoming a panic if it ever stops holding.
            let topic = queued.order.pop_front()?;
            if let Some(event) = queued.events.remove(&topic) {
                return Some(event);
            }
        }
    }

    fn close(&self) {
        self.lock().closed = true;
        self.ready.notify_one();
    }

    fn is_closed(&self) -> bool {
        self.lock().closed
    }
}

/// Survives a reconnect: a daemon that goes away does not end the subscription, and the next value
/// after one is a fresh snapshot. `None` means the connection stopped, which happens when the
/// caller drops the last `Client`.
pub struct Subscription {
    pattern: String,
    matched: usize,
    mailbox: Arc<Mailbox>,
    /// Weak, so a subscription cannot hold the connection open after the last `Client` is gone.
    /// Dropping the client is what ends the connection, and ending it is what makes `next` return
    /// `None` — a strong sender here would keep the task alive with nobody left to drive it.
    requests: mpsc::WeakSender<Request>,
}

impl Subscription {
    /// How many declared topics the pattern matched when it was registered. Zero is not an error —
    /// a topic can be declared later — but it is what a typo looks like, and a caller that never
    /// reports it turns one into a silent wait.
    pub fn matched(&self) -> usize {
        self.matched
    }

    pub async fn next(&mut self) -> Option<Event> {
        loop {
            if let Some(event) = self.mailbox.take() {
                return Some(event);
            }
            if self.mailbox.is_closed() {
                return None;
            }
            // `notify_one` stores a permit, so an offer between the take above and this await
            // wakes us rather than being lost.
            self.mailbox.ready.notified().await;
        }
    }
}

impl Drop for Subscription {
    /// Releasing the pattern is what stops a client that subscribes and drops in a loop — a popover
    /// opening and closing — from walking into the daemon's per-connection subscription cap holding
    /// patterns nothing reads. `try_send` because `Drop` cannot await; a full channel only defers
    /// the release to the next prune, which every incoming event drives.
    fn drop(&mut self) {
        let Some(requests) = self.requests.upgrade() else {
            return;
        };
        let pattern = std::mem::take(&mut self.pattern);
        let _ = requests.try_send(Request::Unwatch { pattern });
    }
}

async fn dial(socket: &Path) -> Result<Wire, ConnectError> {
    let stream =
        UnixStream::connect(socket)
            .await
            .map_err(|source| ConnectError::NotListening {
                path: socket.to_owned(),
                source,
            })?;
    let mut wire = Framed::new(stream, FrameCodec::default());

    let hello = Frame {
        id: None,
        body: Body::Hello {},
    };
    if let Err(error) = wire.send(hello).await {
        tracing::warn!(%error, "the handshake could not be written");
        return Err(ConnectError::Handshake);
    }

    match time::timeout(REQUEST_TIMEOUT, wire.next()).await {
        Ok(Some(Ok(Frame {
            body: Body::HelloAck { daemon_version },
            ..
        }))) => {
            tracing::debug!(daemon_version, "connected");
            Ok(wire)
        }
        // Anything else means the socket is not a glimpse daemon, which is the failure `--socket`
        // makes reachable and the reason the ack is worth one frame at connect.
        Ok(answer) => {
            tracing::warn!(?answer, "expected hello_ack");
            Err(ConnectError::Handshake)
        }
        Err(_) => {
            tracing::warn!(
                ?socket,
                "the daemon accepted but did not answer the handshake"
            );
            Err(ConnectError::Handshake)
        }
    }
}

fn backoff(attempt: u32) -> time::Duration {
    BACKOFF_MIN
        .saturating_mul(1 << attempt.min(6))
        .min(BACKOFF_MAX)
}

enum Request {
    Ask {
        body: Body,
        pending: Pending,
        permit: OwnedSemaphorePermit,
    },
    Watch {
        pattern: String,
        mailbox: Weak<Mailbox>,
        reply: oneshot::Sender<Result<usize, CallError>>,
        permit: OwnedSemaphorePermit,
    },
    /// A dropped `Subscription`. Nothing waits on it, so it holds no in-flight slot.
    Unwatch { pattern: String },
}

impl Request {
    fn fail(self, error: CallError) {
        match self {
            Self::Ask { pending, .. } => pending.fail(error),
            Self::Watch { reply, .. } => {
                let _ = reply.send(Err(error));
            }
            // Nothing to release: a connection that is gone holds no patterns.
            Self::Unwatch { .. } => {}
        }
    }
}

enum Pending {
    Get(oneshot::Sender<Result<Option<Event>, CallError>>),
    Call(oneshot::Sender<Result<Value, CallError>>),
    Subscribe(oneshot::Sender<Result<usize, CallError>>),
}

impl Pending {
    fn settle(self, body: Body) {
        match (self, body) {
            (Self::Get(reply), Body::GetResult(status)) => {
                let _ = reply.send(status.into());
            }
            (Self::Call(reply), Body::CallResult(status)) => {
                let _ = reply.send(status.into());
            }
            (Self::Subscribe(reply), Body::SubscribeAck(status)) => {
                let _ = reply.send(status.into());
            }
            (pending, body) => {
                tracing::warn!(?body, "the daemon answered with a frame of the wrong kind");
                pending.fail(CallError::new(ErrorCode::Internal, "mismatched reply"));
            }
        }
    }

    fn fail(self, error: CallError) {
        match self {
            Self::Get(reply) => {
                let _ = reply.send(Err(error));
            }
            Self::Call(reply) => {
                let _ = reply.send(Err(error));
            }
            Self::Subscribe(reply) => {
                let _ = reply.send(Err(error));
            }
        }
    }
}

struct Waiting {
    pending: Pending,
    deadline: time::Instant,
    /// Held for exactly as long as the daemon owes an answer; dropping it frees the slot.
    _permit: OwnedSemaphorePermit,
}

enum Step {
    Request(Option<Request>),
    Frame(Option<Result<Frame, CodecError>>),
    Expire,
}

enum Stop {
    CallerGone,
    Lost,
}

struct Connection {
    socket: PathBuf,
    inbox: mpsc::Receiver<Request>,
    state: watch::Sender<ConnectionState>,
    /// Weak, so a dropped `Subscription` is what releases its pattern: the connection never keeps
    /// a subscription alive on behalf of a caller that has stopped reading it.
    subscriptions: Vec<(String, Weak<Mailbox>)>,
    pending: HashMap<u64, Waiting>,
    next_id: u64,
}

impl Connection {
    async fn run(mut self, first: Option<Wire>) {
        let mut wire = first;
        let mut attempt = 0;

        loop {
            match wire {
                Some(connected) => {
                    if attempt > 0 {
                        tracing::info!(socket = ?self.socket, attempt, "connected to the daemon");
                    }
                    attempt = 0;
                    self.state.send_replace(ConnectionState::Connected);
                    if let Stop::CallerGone = self.session(connected).await {
                        break;
                    }
                    tracing::warn!(socket = ?self.socket, "the connection to the daemon was lost");
                    self.fail_pending("the connection to the daemon was lost");
                }
                None => self.fail_pending("the daemon is not reachable"),
            }

            attempt += 1;
            let next_in = backoff(attempt);
            self.state
                .send_replace(ConnectionState::Reconnecting { attempt, next_in });
            if self.idle(next_in).await {
                break;
            }

            self.state.send_replace(ConnectionState::Connecting);
            wire = match dial(&self.socket).await {
                Ok(wire) => Some(wire),
                Err(error) => {
                    tracing::debug!(%error, attempt, "dial failed");
                    None
                }
            };
        }

        self.fail_pending("the client stopped");
        for (_, mailbox) in self.subscriptions.drain(..) {
            if let Some(mailbox) = mailbox.upgrade() {
                mailbox.close();
            }
        }
    }

    fn prune(&mut self) {
        self.subscriptions
            .retain(|(_, mailbox)| mailbox.strong_count() > 0);
    }

    async fn session(&mut self, mut wire: Wire) -> Stop {
        if let Err(error) = self.resubscribe(&mut wire).await {
            tracing::warn!(%error, "resubscribe failed");
            return Stop::Lost;
        }

        loop {
            // Every branch is built before any is polled, so the deadline has to be read before
            // `inbox.recv()` takes its mutable borrow. Resolving the select to a value then ends
            // those borrows, which is what lets the handling below reach `self` and `wire` again.
            let deadline = self.pending.values().map(|waiting| waiting.deadline).min();
            let step = tokio::select! {
                request = self.inbox.recv() => Step::Request(request),
                frame = wire.next() => Step::Frame(frame),
                // A disabled branch still evaluates its expression but never polls it, so the
                // fallback below is a value nothing waits on rather than a timer that fires.
                () = time::sleep_until(deadline.unwrap_or_else(time::Instant::now)),
                    if deadline.is_some() => Step::Expire,
            };

            match step {
                Step::Request(None) => return Stop::CallerGone,
                Step::Request(Some(request)) => {
                    if let Err(error) = self.dispatch(request, &mut wire).await {
                        tracing::warn!(%error, "write failed, closing the connection");
                        return Stop::Lost;
                    }
                }
                Step::Frame(None) => return Stop::Lost,
                Step::Frame(Some(Err(error))) => {
                    tracing::warn!(%error, "protocol error, closing the connection");
                    return Stop::Lost;
                }
                Step::Frame(Some(Ok(frame))) => self.receive(frame),
                Step::Expire => self.expire(),
            }
        }
    }

    /// Reconnecting is resubscribing is a fresh snapshot, which is why no caller writes recovery.
    async fn resubscribe(&mut self, wire: &mut Wire) -> Result<(), CodecError> {
        self.prune();
        let mut patterns: Vec<String> = self
            .subscriptions
            .iter()
            .map(|(pattern, _)| pattern.clone())
            .collect();
        // Two subscriptions may share a pattern; the daemon registers it once either way.
        patterns.sort_unstable();
        patterns.dedup();

        for pattern in patterns {
            // No id: nobody is waiting on the ack, and the matched count is already known.
            let frame = Frame {
                id: None,
                body: Body::Subscribe { pattern },
            };
            wire.send(frame).await?;
        }
        Ok(())
    }

    async fn dispatch(&mut self, request: Request, wire: &mut Wire) -> Result<(), CodecError> {
        let (body, pending, permit) = match request {
            // Only once no live subscription still wants it: two callers may hold the same pattern,
            // and releasing it for one of them would stop the other's events.
            Request::Unwatch { pattern } => {
                self.prune();
                if !self.subscriptions.iter().any(|(held, _)| *held == pattern) {
                    let body = Body::Unsubscribe { pattern };
                    wire.send(Frame { id: None, body }).await?;
                }
                return Ok(());
            }
            Request::Ask {
                body,
                pending,
                permit,
            } => (body, pending, permit),
            Request::Watch {
                pattern,
                mailbox,
                reply,
                permit,
            } => {
                self.subscriptions.push((pattern.clone(), mailbox));
                (
                    Body::Subscribe { pattern },
                    Pending::Subscribe(reply),
                    permit,
                )
            }
        };

        self.next_id += 1;
        let id = self.next_id;
        self.pending.insert(
            id,
            Waiting {
                pending,
                deadline: time::Instant::now() + REQUEST_TIMEOUT,
                _permit: permit,
            },
        );
        wire.send(Frame { id: Some(id), body }).await
    }

    fn receive(&mut self, frame: Frame) {
        match frame.body {
            Body::Event(event) => self.fan_out(event),
            body => match frame.id.and_then(|id| self.pending.remove(&id)) {
                Some(waiting) => waiting.pending.settle(body),
                None => tracing::debug!(id = ?frame.id, ?body, "a reply nobody is waiting for"),
            },
        }
    }

    /// Never awaits a subscriber: a mailbox coalesces, so a slow reader costs it intermediate
    /// values rather than costing every other subscription its delivery.
    fn fan_out(&mut self, event: Event) {
        self.prune();

        for (pattern, mailbox) in &self.subscriptions {
            if !pattern::matches(pattern, &event.topic) {
                continue;
            }
            if let Some(mailbox) = mailbox.upgrade() {
                mailbox.offer(event.clone());
            }
        }
    }

    /// Expiring here rather than in the caller is what returns the in-flight slot: a caller that
    /// gave up on its own future would leave the entry until the connection died.
    fn expire(&mut self) {
        let now = time::Instant::now();
        let expired: Vec<u64> = self
            .pending
            .iter()
            .filter(|(_, waiting)| waiting.deadline <= now)
            .map(|(id, _)| *id)
            .collect();

        for id in expired {
            if let Some(waiting) = self.pending.remove(&id) {
                waiting.pending.fail(CallError::new(
                    ErrorCode::Timeout,
                    format!("no answer within {REQUEST_TIMEOUT:?}"),
                ));
            }
        }
    }

    fn fail_pending(&mut self, reason: &str) {
        for (_, waiting) in self.pending.drain() {
            waiting
                .pending
                .fail(CallError::new(ErrorCode::Unavailable, reason));
        }
    }

    /// Returns true when the caller dropped the client. Requests that arrive while disconnected
    /// fail immediately rather than queueing behind a reconnect that may never happen.
    async fn idle(&mut self, delay: time::Duration) -> bool {
        let sleep = time::sleep(delay);
        tokio::pin!(sleep);

        loop {
            tokio::select! {
                () = &mut sleep => return false,
                request = self.inbox.recv() => match request {
                    None => return true,
                    Some(request) => request.fail(CallError::new(
                        ErrorCode::Unavailable,
                        "the daemon is not reachable",
                    )),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_then_stops() {
        assert_eq!(backoff(1), time::Duration::from_millis(500));
        assert_eq!(backoff(2), time::Duration::from_secs(1));
        assert_eq!(backoff(20), BACKOFF_MAX);
    }

    #[tokio::test]
    async fn open_survives_a_socket_nothing_is_listening_on() {
        let client = Client::open(Path::new("/nonexistent/glimpsed.sock")).await;

        assert_eq!(*client.watch_state().borrow(), ConnectionState::Connecting);
        let refused = client.get("solar.status").await.expect_err("no daemon");
        assert_eq!(refused.code, ErrorCode::Unavailable);
    }

    #[tokio::test]
    async fn open_is_connected_when_the_daemon_answers_the_first_dial() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("glimpsed.sock");
        let listener = tokio::net::UnixListener::bind(&path).expect("bind");
        tokio::spawn(answer_one_handshake(listener));

        let client = Client::open(&path).await;

        assert_eq!(*client.watch_state().borrow(), ConnectionState::Connected);
    }

    #[tokio::test(start_paused = true)]
    async fn a_socket_that_accepts_but_never_answers_fails_instead_of_hanging() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("glimpsed.sock");
        let listener = tokio::net::UnixListener::bind(&path).expect("bind");
        tokio::spawn(async move {
            let held = listener.accept().await;
            std::future::pending::<()>().await;
            drop(held);
        });

        let Err(error) = Client::connect(&path).await else {
            panic!("connect succeeded against a socket that never answered");
        };

        assert!(matches!(error, ConnectError::Handshake), "{error}");
    }

    async fn answer_one_handshake(listener: tokio::net::UnixListener) {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

        let (stream, _) = listener.accept().await.expect("accept");
        let mut wire = tokio::io::BufReader::new(stream);
        let mut hello = String::new();
        wire.read_line(&mut hello).await.expect("hello");
        wire.get_mut()
            .write_all(b"{\"type\":\"hello_ack\",\"data\":{\"daemon_version\":\"test\"}}\n")
            .await
            .expect("ack");
        std::future::pending::<()>().await;
    }
}
