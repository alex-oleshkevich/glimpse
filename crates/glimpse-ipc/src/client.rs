use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::{
    net::UnixStream,
    sync::{mpsc, oneshot, watch},
    time,
};
use tokio_util::codec::Framed;

use std::io;

use crate::{
    PROTOCOL_VERSION,
    codec::{CodecError, FrameCodec},
    frame::{Body, CallError, ErrorCode, Event, Frame},
    pattern,
};

const EVENT_QUEUE: usize = 64;
/// Both the in-flight cap and the request channel's bound: queueing more than can ever be in
/// flight at once only defers the `LimitExceeded` the caller is going to get anyway.
const MAX_INFLIGHT: usize = 32;
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
    #[error("the daemon speaks protocol version {daemon}, we speak {ours}")]
    ProtocolMismatch { daemon: u32, ours: u32 },
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
    state: watch::Receiver<ConnectionState>,
}

impl Client {
    /// The first connection is made here rather than in the task, so a missing daemon and a refused
    /// protocol version are answers the caller gets from `connect` instead of a hang.
    ///
    /// `request_timeout` is enforced by the connection, not by the caller: a caller that wraps its
    /// own future gives up without releasing the in-flight slot the request still holds.
    pub async fn connect(
        socket: &Path,
        request_timeout: time::Duration,
    ) -> Result<Self, ConnectError> {
        let wire = dial(socket).await?;
        let (requests, inbox) = mpsc::channel(MAX_INFLIGHT);
        let (state, watcher) = watch::channel(ConnectionState::Connected);

        tokio::spawn(
            Connection {
                socket: socket.to_owned(),
                request_timeout,
                inbox,
                state,
                subscriptions: Vec::new(),
                pending: HashMap::new(),
                next_id: 0,
            }
            .run(wire),
        );

        Ok(Self {
            requests,
            state: watcher,
        })
    }

    pub fn watch_state(&self) -> watch::Receiver<ConnectionState> {
        self.state.clone()
    }

    pub async fn get(&self, topic: &str) -> Result<Option<Event>, CallError> {
        let (reply, answer) = oneshot::channel();
        let request = Request::Ask {
            body: Body::Get {
                topic: topic.to_owned(),
            },
            pending: Pending::Get(reply),
        };
        self.ask(request, answer).await
    }

    pub async fn call(&self, command: &str, args: Value) -> Result<Value, CallError> {
        let (reply, answer) = oneshot::channel();
        let request = Request::Ask {
            body: Body::Call {
                command: command.to_owned(),
                args,
            },
            pending: Pending::Call(reply),
        };
        self.ask(request, answer).await
    }

    pub async fn subscribe(&self, pattern: &str) -> Result<Subscription, CallError> {
        let (events, stream) = mpsc::channel(EVENT_QUEUE);
        let (reply, answer) = oneshot::channel();
        let request = Request::Watch {
            pattern: pattern.to_owned(),
            events,
            reply,
        };

        let matched = self.ask(request, answer).await?;
        tracing::debug!(pattern, matched, "subscribed");
        Ok(Subscription { events: stream })
    }

    async fn ask<T>(
        &self,
        request: Request,
        answer: oneshot::Receiver<Result<T, CallError>>,
    ) -> Result<T, CallError> {
        self.requests.send(request).await.map_err(|_| gone())?;
        answer.await.map_err(|_| gone())?
    }
}

fn gone() -> CallError {
    CallError::new(
        ErrorCode::Unavailable,
        "the connection to the daemon has stopped",
    )
}

/// Survives a reconnect: a daemon that goes away does not end the subscription, and the next value
/// after one is a fresh snapshot. `None` means the caller dropped the `Client`, or that the daemon
/// came back speaking a protocol version this build cannot talk to.
pub struct Subscription {
    events: mpsc::Receiver<Event>,
}

impl Subscription {
    pub async fn next(&mut self) -> Option<Event> {
        self.events.recv().await
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
        body: Body::Hello {
            protocol: PROTOCOL_VERSION,
        },
    };
    if let Err(error) = wire.send(hello).await {
        tracing::warn!(%error, "the handshake could not be written");
        return Err(ConnectError::Handshake);
    }

    match wire.next().await {
        Some(Ok(Frame {
            body:
                Body::HelloAck {
                    protocol,
                    daemon_version,
                },
            ..
        })) => {
            if protocol != PROTOCOL_VERSION {
                return Err(ConnectError::ProtocolMismatch {
                    daemon: protocol,
                    ours: PROTOCOL_VERSION,
                });
            }
            tracing::debug!(daemon_version, "connected");
            Ok(wire)
        }
        answer => {
            tracing::warn!(?answer, "expected hello_ack");
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
    },
    Watch {
        pattern: String,
        events: mpsc::Sender<Event>,
        reply: oneshot::Sender<Result<usize, CallError>>,
    },
}

impl Request {
    fn fail(self, error: CallError) {
        match self {
            Self::Ask { pending, .. } => pending.fail(error),
            Self::Watch { reply, .. } => {
                let _ = reply.send(Err(error));
            }
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
    request_timeout: time::Duration,
    inbox: mpsc::Receiver<Request>,
    state: watch::Sender<ConnectionState>,
    subscriptions: Vec<(String, mpsc::Sender<Event>)>,
    pending: HashMap<u64, Waiting>,
    next_id: u64,
}

impl Connection {
    async fn run(mut self, first: Wire) {
        let mut wire = Ok(first);
        let mut attempt = 0;

        loop {
            match wire {
                Ok(connected) => {
                    attempt = 0;
                    self.state.send_replace(ConnectionState::Connected);
                    if let Stop::CallerGone = self.session(connected).await {
                        break;
                    }
                    self.fail_pending("the connection to the daemon was lost");
                }
                // Reconnecting cannot fix a version mismatch, and retrying one forever hides it.
                Err(ConnectError::ProtocolMismatch { daemon, ours }) => {
                    tracing::error!(daemon, ours, "protocol mismatch, not reconnecting");
                    break;
                }
                Err(error) => {
                    tracing::debug!(%error, attempt, "reconnect failed");
                    self.fail_pending("the daemon is not reachable");
                }
            }

            attempt += 1;
            let next_in = backoff(attempt);
            self.state
                .send_replace(ConnectionState::Reconnecting { attempt, next_in });
            if self.idle(next_in).await {
                break;
            }

            self.state.send_replace(ConnectionState::Connecting);
            wire = dial(&self.socket).await;
        }

        self.fail_pending("the client stopped");
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
        self.subscriptions.retain(|(_, events)| !events.is_closed());
        let patterns: Vec<String> = self
            .subscriptions
            .iter()
            .map(|(pattern, _)| pattern.clone())
            .collect();

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
        if self.pending.len() >= MAX_INFLIGHT {
            request.fail(CallError::new(
                ErrorCode::LimitExceeded,
                "too many requests are already in flight",
            ));
            return Ok(());
        }

        let (body, pending) = match request {
            Request::Ask { body, pending } => (body, pending),
            Request::Watch {
                pattern,
                events,
                reply,
            } => {
                self.subscriptions.push((pattern.clone(), events));
                (Body::Subscribe { pattern }, Pending::Subscribe(reply))
            }
        };

        self.next_id += 1;
        let id = self.next_id;
        self.pending.insert(
            id,
            Waiting {
                pending,
                deadline: time::Instant::now() + self.request_timeout,
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

    fn fan_out(&mut self, event: Event) {
        self.subscriptions.retain(|(_, events)| !events.is_closed());

        for (pattern, events) in &self.subscriptions {
            if !pattern::matches(pattern, &event.topic) {
                continue;
            }
            // try_send, never send: a subscriber that reads slowly loses intermediate values, which
            // is lossless because every event carries the full one. Blocking here would stall the
            // connection for every other subscription.
            if let Err(error) = events.try_send(event.clone()) {
                tracing::warn!(topic = %event.topic, %error, "subscriber is behind");
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
                    format!("no answer within {:?}", self.request_timeout),
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
}
