use std::{
    future::Future,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Arc,
};

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::{
    fs, io,
    net::{UnixListener, UnixStream},
    task::JoinSet,
};
use tokio_util::{codec::Framed, sync::CancellationToken};

use crate::{
    PROTOCOL_VERSION, VERSION,
    codec::{CodecError, FrameCodec},
    frame::{Body, CallError, Event, Frame},
};

type Wire = Framed<UnixStream, FrameCodec>;

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
    #[error("the client speaks protocol version {0}, we speak {PROTOCOL_VERSION}")]
    ProtocolVersion(u32),
    #[error(transparent)]
    Codec(#[from] CodecError),
}

/// Declared with `-> impl Future + Send` rather than `async fn` so the futures are known to be
/// `Send` at the trait, which is what lets `serve` spawn one task per client.
pub trait Handler: Send + Sync + 'static {
    fn subscribe(
        &self,
        client: ClientId,
        pattern: &str,
    ) -> impl Future<Output = Result<usize, CallError>> + Send;

    fn unsubscribe(&self, client: ClientId, pattern: &str) -> impl Future<Output = ()> + Send;

    fn get(&self, topic: &str) -> impl Future<Output = Result<Option<Event>, CallError>> + Send;

    fn call(
        &self,
        command: &str,
        args: Value,
    ) -> impl Future<Output = Result<Value, CallError>> + Send;

    fn disconnected(&self, client: ClientId) -> impl Future<Output = ()> + Send;
}

pub struct Server<H> {
    listener: UnixListener,
    handler: Arc<H>,
}

impl<H: Handler> Server<H> {
    pub async fn bind(socket: &Path, handler: H) -> Result<Self, ServerError> {
        // The directory is narrowed before the socket exists, which is what closes the window
        // between `bind` and the `set_permissions` below, where the socket carries whatever the
        // umask gave it.
        if let Some(parent) = socket.canonicalize()?.parent() {
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
            handler: Arc::new(handler),
        })
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
    }
}

async fn serve_client<H: Handler>(
    id: ClientId,
    stream: UnixStream,
    handler: Arc<H>,
    cancel: CancellationToken,
) {
    let mut wire = Framed::new(stream, FrameCodec::default());

    if let Err(error) = handshake(&mut wire).await {
        tracing::warn!(?id, %error, "handshake refused");
        return;
    }

    loop {
        let frame = tokio::select! {
            () = cancel.cancelled() => break,
            frame = wire.next() => frame,
        };

        let frame = match frame {
            Some(Ok(frame)) => frame,
            Some(Err(error)) => {
                tracing::warn!(?id, %error, "protocol error, closing the connection");
                break;
            }
            None => break,
        };

        let correlation = frame.id;
        let Some(body) = answer(&*handler, id, frame.body).await else {
            continue;
        };

        if let Err(error) = wire
            .send(Frame {
                id: correlation,
                body,
            })
            .await
        {
            tracing::warn!(?id, %error, "write failed, closing the connection");
            break;
        }
    }

    handler.disconnected(id).await;
    tracing::debug!(?id, "client disconnected");
}

async fn answer<H: Handler>(handler: &H, id: ClientId, body: Body) -> Option<Body> {
    Some(match body {
        Body::Get { topic } => Body::GetResult(handler.get(&topic).await.into()),
        Body::Call { command, args } => Body::CallResult(handler.call(&command, args).await.into()),
        Body::Subscribe { pattern } => match handler.subscribe(id, &pattern).await {
            Ok(matched) => Body::SubscribeAck { matched },
            // A refused subscribe has no frame to travel in, so the connection is what closes.
            Err(error) => {
                tracing::warn!(?id, pattern, %error, "subscribe refused");
                return None;
            }
        },
        Body::Unsubscribe { pattern } => {
            handler.unsubscribe(id, &pattern).await;
            return None;
        }
        other => {
            tracing::warn!(
                ?id,
                ?other,
                "a client sent a frame only the daemon may send"
            );
            return None;
        }
    })
}

/// The ack carries our version even when it does not match, so the client can name both numbers.
/// Refusing silently would leave every mismatch looking like a dead daemon.
async fn handshake(wire: &mut Wire) -> Result<(), HandshakeError> {
    let frame = match wire.next().await {
        Some(Ok(frame)) => frame,
        Some(Err(error)) => return Err(error.into()),
        None => return Err(HandshakeError::Closed),
    };

    let Body::Hello { protocol } = frame.body else {
        return Err(HandshakeError::OutOfOrder);
    };

    let ack = Body::HelloAck {
        protocol: PROTOCOL_VERSION,
        daemon_version: VERSION.to_owned(),
    };
    wire.send(Frame {
        id: frame.id,
        body: ack,
    })
    .await?;

    match protocol == PROTOCOL_VERSION {
        true => Ok(()),
        false => Err(HandshakeError::ProtocolVersion(protocol)),
    }
}
