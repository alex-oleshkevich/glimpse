use std::sync::{
    Arc,
    atomic::{AtomicI16, Ordering},
};

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{
        UnixListener,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::{Mutex, broadcast},
};

use crate::{
    jsonrpc::JSONRPCRequest,
    messages::{Message, MessageBus, Request},
};

static PLUGIN_ID: AtomicI16 = AtomicI16::new(0);

pub struct RPCHost {
    receiver: broadcast::Receiver<Message>,
    publisher: broadcast::Sender<Message>,
    clients: Arc<Mutex<Vec<RPCClient>>>,
}

struct RPCClient {
    id: i16,
    writer: OwnedWriteHalf,
}

impl RPCClient {
    fn new(id: i16, writer: OwnedWriteHalf) -> Self {
        RPCClient { id, writer }
    }

    async fn write(&mut self, msg: &str) -> Result<(), std::io::Error> {
        self.writer.write_all(msg.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        Ok(())
    }
}

impl RPCHost {
    pub fn new(message_bus: &MessageBus) -> Self {
        RPCHost {
            receiver: message_bus.subscribe(),
            publisher: message_bus.publisher(),
            clients: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let socket_path = dirs::runtime_dir()
            .expect("failed to get runtime directory")
            .join("glimpsed.sock");

        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
        }

        let listener = UnixListener::bind(&socket_path)?;
        tracing::info!("listening on {}", socket_path.display());

        // plugins -> clients
        let clients_for_dispatch = Arc::clone(&self.clients);
        let mut receiver = self.receiver;
        tokio::spawn(async move {
            while let Ok(msg) = receiver.recv().await {
                match msg {
                    Message::PluginResponse(response) => {
                        let mut connections = clients_for_dispatch.lock().await;
                        for client in connections.iter_mut() {
                            let json = response.to_json().unwrap_or_else(|e| {
                                tracing::error!("failed to serialize response: {}", e);
                                "{}".to_string()
                            });
                            if let Err(e) = client.write(&json).await {
                                tracing::error!("failed to send message to client: {}", e)
                            }
                        }
                    }
                    _ => {}
                }
            }
        });

        while let Ok((stream, _)) = listener.accept().await {
            tracing::info!("accepted connection from {:?}", stream.peer_addr());
            let (reader, writer) = stream.into_split();
            let clients = Arc::clone(&self.clients);
            let publisher = self.publisher.clone();
            tokio::spawn(async move {
                let next_id = PLUGIN_ID.fetch_add(1, Ordering::SeqCst);
                let handle = RPCClient::new(next_id, writer);
                clients.lock().await.push(handle);

                let results = parse_client_input(reader, publisher).await;
                if let Err(e) = results {
                    tracing::error!("client handler crashed: {}", e);
                } else {
                    tracing::info!("client disconnected");
                }

                let mut clients = clients.lock().await;
                clients.retain(|c| c.id != next_id);
            });
        }
        Ok(())
    }
}

async fn parse_client_input(
    reader: OwnedReadHalf,
    publisher: broadcast::Sender<Message>,
) -> Result<(), serde_json::Error> {
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        match reader.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {
                tracing::debug!("received message from client: {}", &line);
                match JSONRPCRequest::<Request>::from_json(&line) {
                    Ok(request) => {
                        tracing::debug!("received client request: {}", request.method);
                        let message = Message::ClientRequest(request);
                        if let Err(e) = publisher.send(message) {
                            tracing::error!("failed to forward client message to plugins: {}", e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("failed to parse JSON-RPC request: {}", e);
                    }
                }
                line.clear();
            }
            Err(e) => {
                eprintln!("Failed to read from socket: {}", e);
                break;
            }
        }
    }
    Ok(())
}
