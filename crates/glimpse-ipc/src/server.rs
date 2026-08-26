use std::{
    collections::HashMap,
    future::Future,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard},
};

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::{
    fs,
    io::{self, AsyncWriteExt},
    net::{
        UnixListener, UnixStream,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::{Notify, Semaphore},
    task::JoinSet,
};
use tokio_util::{
    bytes::Bytes,
    codec::{FramedRead, FramedWrite},
    sync::CancellationToken,
};

use crate::{
    VERSION,
    codec::{CodecError, FrameCodec, MAX_LINE_BYTES},
    frame::{Body, CallError, ErrorCode, Event, Frame, Status},
    outbox::Outbox,
};

type Reader = FramedRead<OwnedReadHalf, FrameCodec>;
type Registry = Arc<RwLock<HashMap<ClientId, Arc<Client>>>>;

/// How many `get`s and `call`s one client may have outstanding. Each runs in a task of its own, so
/// the cap is what stops a client from spawning without bound; the client caps itself at the same
/// number, which makes reaching this one a sign of a client that is not.
const MAX_INFLIGHT: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientId(u64);

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("a daemon is already listening at {0}")]
    AlreadyRunning(PathBuf),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
enum HandshakeError {
    #[error("the client closed the connection")]
    Closed,
    #[error("the client sent a frame before hello")]
    OutOfOrder,
    #[error(transparent)]
    Codec(#[from] CodecError),
}

/// What a `subscribe` produced: how many declared topics the pattern matched, and the current value
/// of those that have one. A topic that is declared but has never published contributes to
/// `matched` and not to `snapshot`, because an `Event` has no way to say "no value".
pub struct Subscribed {
    pub matched: usize,
    pub snapshot: Vec<Event>,
}

/// Declared with `-> impl Future + Send` rather than `async fn` so the futures are known to be
/// `Send` at the trait, which is what lets `serve` spawn one task per client.
pub trait Handler: Send + Sync + 'static {
    fn subscribe(
        &self,
        client: ClientId,
        pattern: &str,
    ) -> impl Future<Output = Result<Subscribed, CallError>> + Send;

    fn unsubscribe(&self, client: ClientId, pattern: &str) -> impl Future<Output = ()> + Send;

    fn get(&self, topic: &str) -> impl Future<Output = Result<Option<Event>, CallError>> + Send;

    fn call(
        &self,
        command: &str,
        args: Value,
    ) -> impl Future<Output = Result<Value, CallError>> + Send;

    fn disconnected(&self, client: ClientId) -> impl Future<Output = ()> + Send;
}

/// One connected client's pending writes. The mutex is never held across an `.await`, and the lock
/// order is always registry then client — no task takes the registry lock while holding this one.
struct Client {
    outbox: Mutex<Outbox>,
    notify: Notify,
    cancel: CancellationToken,
}

impl Client {
    fn lock(&self) -> MutexGuard<'_, Outbox> {
        self.outbox
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn push_response(&self, frame: Bytes) {
        let closed = {
            let mut outbox = self.lock();
            outbox.push_response(frame);
            outbox.is_closed()
        };
        self.wake(closed);
    }

    fn offer(&self, topic: &str, seq: u64, frame: &Bytes) {
        let closed = {
            let mut outbox = self.lock();
            if outbox.is_closed() || !outbox.wants(topic) {
                return;
            }
            outbox.push_event(topic, seq, frame.clone());
            outbox.is_closed()
        };
        self.wake(closed);
    }

    fn wake(&self, closed: bool) {
        // Cancelling is what turns an over-cap outbox into a disconnect: the writer stops, and the
        // reader parked on the socket stops with it rather than waiting for the peer to notice.
        if closed {
            self.cancel.cancel();
        }
        self.notify.notify_one();
    }

    fn drain(&self) -> (Vec<Bytes>, bool) {
        let mut outbox = self.lock();
        let mut frames = Vec::new();
        while let Some(frame) = outbox.pop() {
            frames.push(frame);
        }
        (frames, outbox.is_closed())
    }
}

/// Hands published events to every client subscribed to their topic. Never blocks and never fails,
/// so the broker cannot learn about a slow client by being slowed down.
#[derive(Clone)]
pub struct Publisher {
    clients: Registry,
}

impl Publisher {
    pub fn publish(&self, event: Event) {
        let topic = event.topic.clone();
        let seq = event.seq;

        let frame = match encode(&Frame {
            id: None,
            body: Body::Event(event),
        }) {
            Ok(frame) => frame,
            Err(error) => {
                tracing::error!(%topic, %error, "event failed to serialize");
                return;
            }
        };

        // A frame over the line cap would be refused by every client's decoder and take each
        // connection down with it, so it is dropped here instead.
        if frame.len() > MAX_LINE_BYTES {
            tracing::error!(
                %topic,
                bytes = frame.len(),
                "event exceeds the line cap and was dropped; publish a path, not a payload"
            );
            return;
        }

        for client in read(&self.clients).values() {
            client.offer(&topic, seq, &frame);
        }
    }
}

pub struct Server<H> {
    listener: UnixListener,
    socket: PathBuf,
    handler: Arc<H>,
    clients: Registry,
}

impl<H: Handler> Server<H> {
    pub async fn bind(socket: &Path, handler: H) -> Result<Self, ServerError> {
        // The directory is narrowed before the socket exists, which is what closes the window
        // between `bind` and the `set_permissions` below, where the socket carries whatever the
        // umask gave it.
        if let Some(parent) = socket.parent() {
            fs::create_dir_all(parent).await?;
            fs::set_permissions(parent, PermissionsExt::from_mode(0o700)).await?;
        }

        match UnixStream::connect(socket).await {
            Ok(_) => return Err(ServerError::AlreadyRunning(socket.to_owned())),
            Err(err) if err.kind() == io::ErrorKind::ConnectionRefused => {
                fs::remove_file(socket).await?;
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }

        let listener = UnixListener::bind(socket)?;
        fs::set_permissions(socket, PermissionsExt::from_mode(0o600)).await?;
        Ok(Self {
            listener,
            socket: socket.to_owned(),
            handler: Arc::new(handler),
            clients: Registry::default(),
        })
    }

    pub fn publisher(&self) -> Publisher {
        Publisher {
            clients: Arc::clone(&self.clients),
        }
    }

    pub async fn serve(self, cancel: CancellationToken) {
        let mut clients = JoinSet::new();
        let mut issued = 0;

        loop {
            tokio::select! {
                () = cancel.cancelled() => break,
                accepted = self.listener.accept() => match accepted {
                    Ok((stream, _)) => {
                        issued += 1;
                        clients.spawn(serve_client(
                            ClientId(issued),
                            stream,
                            Arc::clone(&self.handler),
                            Arc::clone(&self.clients),
                            cancel.child_token(),
                        ));
                    },
                    Err(err) => tracing::warn!(%err, "failed to accept a client"),
                },
                Some(Err(err)) = clients.join_next(), if !clients.is_empty() => {
                    tracing::warn!(%err, "client task failed");
                }
            }
        }
        while clients.join_next().await.is_some() {}

        // Unlinked while the listener is still held, so nothing can have taken the path in between.
        // A socket left behind is not a correctness problem — `bind` connects first and clears a
        // dead one — but leaving it means every start goes through that path instead of none.
        if let Err(error) = fs::remove_file(&self.socket).await {
            tracing::debug!(path = ?self.socket, %error, "could not remove the socket");
        }
    }
}

async fn serve_client<H: Handler>(
    id: ClientId,
    stream: UnixStream,
    handler: Arc<H>,
    registry: Registry,
    cancel: CancellationToken,
) {
    let (read_half, write_half) = stream.into_split();
    let mut reader = FramedRead::new(read_half, FrameCodec::default());
    let mut writer = FramedWrite::new(write_half, FrameCodec::default());

    if let Err(error) = handshake(&mut reader, &mut writer).await {
        tracing::warn!(?id, %error, "handshake refused");
        return;
    }

    let client = Arc::new(Client {
        outbox: Mutex::new(Outbox::new()),
        notify: Notify::new(),
        cancel: cancel.clone(),
    });
    write(&registry).insert(id, Arc::clone(&client));

    // `send` flushes, so the handshake left nothing buffered and the raw half can be taken over.
    let writing = tokio::spawn(write_client(
        writer.into_inner(),
        Arc::clone(&client),
        cancel.clone(),
    ));

    read_client(id, &mut reader, &handler, &client, &cancel).await;

    client.lock().close();
    client.notify.notify_one();
    cancel.cancel();
    if let Err(error) = writing.await {
        tracing::warn!(?id, %error, "writer task failed");
    }

    write(&registry).remove(&id);
    handler.disconnected(id).await;
    tracing::debug!(?id, "client disconnected");
}

async fn read_client<H: Handler>(
    id: ClientId,
    reader: &mut Reader,
    handler: &Arc<H>,
    client: &Arc<Client>,
    cancel: &CancellationToken,
) {
    let inflight = Arc::new(Semaphore::new(MAX_INFLIGHT));

    loop {
        let frame = tokio::select! {
            () = cancel.cancelled() => return,
            frame = reader.next() => frame,
        };

        let frame = match frame {
            Some(Ok(frame)) => frame,
            Some(Err(error)) => {
                tracing::warn!(?id, %error, "protocol error, closing the connection");
                return;
            }
            None => return,
        };

        let correlation = frame.id;
        let ask = match frame.body {
            Body::Get { topic } => Ask::Get(topic),
            Body::Call { command, args } => Ask::Call(command, args),
            Body::Subscribe { pattern } => {
                subscribe(&**handler, id, pattern, client, correlation).await;
                continue;
            }
            Body::Unsubscribe { pattern } => {
                client.lock().remove_pattern(&pattern);
                handler.unsubscribe(id, &pattern).await;
                continue;
            }
            other => {
                tracing::warn!(
                    ?id,
                    ?other,
                    "a client sent a frame only the daemon may send"
                );
                continue;
            }
        };

        // Answered in a task of its own. Awaiting the handler here would stop this loop from
        // taking the client's next frame off the socket, so one command waiting on a wedged
        // backend would hold up every unrelated request on the same connection.
        let Ok(permit) = Arc::clone(&inflight).try_acquire_owned() else {
            let error = CallError::new(
                ErrorCode::LimitExceeded,
                "this connection already has the maximum number of requests in flight",
            );
            reply(client, correlation, ask.refused(error));
            continue;
        };

        let handler = Arc::clone(handler);
        let client = Arc::clone(client);
        let cancel = cancel.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let body = tokio::select! {
                () = cancel.cancelled() => return,
                body = ask.answer(&*handler) => body,
            };
            reply(&client, correlation, body);
        });
    }
}

/// The two requests that reach a handler, kept as a value so the spawn below is written once and
/// the reply shape cannot drift from the request it answers.
enum Ask {
    Get(String),
    Call(String, Value),
}

impl Ask {
    async fn answer<H: Handler>(self, handler: &H) -> Body {
        match self {
            Self::Get(topic) => Body::GetResult(handler.get(&topic).await.into()),
            Self::Call(command, args) => {
                Body::CallResult(handler.call(&command, args).await.into())
            }
        }
    }

    fn refused(&self, error: CallError) -> Body {
        match self {
            Self::Get(_) => Body::GetResult(Status::Error { error }),
            Self::Call(..) => Body::CallResult(Status::Error { error }),
        }
    }
}

fn reply(client: &Client, correlation: Option<u64>, body: Body) {
    match encode(&Frame {
        id: correlation,
        body,
    }) {
        Ok(frame) => client.push_response(frame),
        Err(error) => tracing::error!(%error, "reply failed to serialize"),
    }
}

async fn write_client(mut io: OwnedWriteHalf, client: Arc<Client>, cancel: CancellationToken) {
    loop {
        let (frames, closed) = client.drain();

        for frame in frames {
            if let Err(error) = io.write_all(&frame).await {
                tracing::debug!(%error, "write failed, closing the connection");
                cancel.cancel();
                return;
            }
        }

        if closed {
            break;
        }

        tokio::select! {
            // `notify_one` stores a permit, so a publish between the drain above and this await
            // still wakes us rather than being lost.
            () = client.notify.notified() => {}
            () = cancel.cancelled() => break,
        }
    }

    let _ = io.shutdown().await;
}

/// The pattern is registered before the snapshot is asked for, so a value published while the
/// handler is working still reaches this client. The outbox's `seq` guard is what makes that safe:
/// whichever of the two arrives second, the newer value is the one that survives.
async fn subscribe<H: Handler>(
    handler: &H,
    id: ClientId,
    pattern: String,
    client: &Client,
    correlation: Option<u64>,
) {
    if !client.lock().add_pattern(&pattern) {
        reply_subscribe(
            client,
            correlation,
            Status::Error {
                error: CallError::new(
                    ErrorCode::LimitExceeded,
                    "this connection already holds the maximum number of subscriptions",
                ),
            },
        );
        return;
    }

    let subscribed = match handler.subscribe(id, &pattern).await {
        Ok(subscribed) => subscribed,
        Err(error) => {
            client.lock().remove_pattern(&pattern);
            reply_subscribe(client, correlation, Status::Error { error });
            return;
        }
    };

    reply_subscribe(
        client,
        correlation,
        Status::Ok {
            value: subscribed.matched,
        },
    );

    for event in subscribed.snapshot {
        let topic = event.topic.clone();
        let seq = event.seq;
        match encode(&Frame {
            id: None,
            body: Body::Event(event),
        }) {
            Ok(frame) => client.offer(&topic, seq, &frame),
            Err(error) => tracing::error!(%topic, %error, "snapshot event failed to serialize"),
        }
    }
}

/// The correlation id is echoed, so a `subscribe` sent without one — the client's resubscribe after
/// a reconnect — stays fire-and-forget while a caller waiting on an ack is answered.
fn reply_subscribe(client: &Client, correlation: Option<u64>, status: Status<usize>) {
    reply(client, correlation, Body::SubscribeAck(status));
}

/// `hello` must be the first frame, and the ack naming this daemon is what lets a client tell a
/// glimpse socket from any other program's — the one thing `--socket` cannot check for itself.
async fn handshake(
    reader: &mut Reader,
    writer: &mut FramedWrite<OwnedWriteHalf, FrameCodec>,
) -> Result<(), HandshakeError> {
    let frame = match reader.next().await {
        Some(Ok(frame)) => frame,
        Some(Err(error)) => return Err(error.into()),
        None => return Err(HandshakeError::Closed),
    };

    if !matches!(frame.body, Body::Hello {}) {
        return Err(HandshakeError::OutOfOrder);
    }

    writer
        .send(Frame {
            id: frame.id,
            body: Body::HelloAck {
                daemon_version: VERSION.to_owned(),
            },
        })
        .await?;
    Ok(())
}

fn encode(frame: &Frame) -> Result<Bytes, serde_json::Error> {
    let mut line = serde_json::to_vec(frame)?;
    line.push(b'\n');
    Ok(Bytes::from(line))
}

fn read(registry: &Registry) -> RwLockReadGuard<'_, HashMap<ClientId, Arc<Client>>> {
    registry
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write(registry: &Registry) -> std::sync::RwLockWriteGuard<'_, HashMap<ClientId, Arc<Client>>> {
    registry
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
