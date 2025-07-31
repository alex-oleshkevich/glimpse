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
    sync::{Mutex, mpsc},
};

static PLUGIN_ID: AtomicI16 = AtomicI16::new(0);

pub struct RPCHost {
    input: mpsc::Receiver<String>,
    output: mpsc::Sender<String>,
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
        self.writer.write_all(msg.as_bytes()).await
    }
}

impl RPCHost {
    pub fn new(input: mpsc::Receiver<String>, output: mpsc::Sender<String>) -> Self {
        RPCHost {
            input,
            output,
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

        // deliver plugin messages to the RPC output
        let input = self.input;
        let clients_for_dispatch = Arc::clone(&self.clients);
        tokio::spawn(async move {
            let mut rx = input;
            while let Some(msg) = rx.recv().await {
                let mut connections = clients_for_dispatch.lock().await;
                for client in connections.iter_mut() {
                    if let Err(e) = client.write(&msg).await {
                        tracing::error!("failed to send message to client: {}", e)
                    }
                }
            }
        });

        while let Ok((stream, _)) = listener.accept().await {
            tracing::info!("accepted connection from {:?}", stream.peer_addr());
            let (reader, writer) = stream.into_split();
            let tx = self.output.clone();
            let clients = Arc::clone(&self.clients);
            tokio::spawn(async move {
                let next_id = PLUGIN_ID.fetch_add(1, Ordering::SeqCst);
                let handle = RPCClient::new(next_id, writer);
                clients.lock().await.push(handle);

                if let Err(e) = handle_ui_client(reader, tx).await {
                    tracing::error!("error handling client: {}", e);
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

async fn handle_ui_client(
    reader: OwnedReadHalf,
    tx: mpsc::Sender<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        match reader.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {
                if let Err(e) = tx.send(line.clone()).await {
                    tracing::error!("failed to send message: {}", e);
                    break;
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
