use std::{collections::HashSet, pin::Pin, sync::Arc};

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    sync::broadcast,
};

use super::protocol::{ClientMsg, IpcEvent, ack_line, escape, hello_line, matches_pattern, parse_client_line};

const MAX_IPC_LINE: usize = 64 * 1024;

pub trait CommandHandler: Send + 'static {
    fn execute<'a>(
        &'a self,
        name: &'a str,
        fields: &'a [(String, String)],
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<(String, String)>, String>> + Send + 'a>>;
}

#[derive(Clone)]
pub struct NoopCommandHandler;

impl CommandHandler for NoopCommandHandler {
    fn execute<'a>(
        &'a self,
        name: &'a str,
        _fields: &'a [(String, String)],
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<(String, String)>, String>> + Send + 'a>> {
        let msg = format!("unknown command: {name}");
        Box::pin(async move { Err(msg) })
    }
}

pub struct IpcClientHandler<H: CommandHandler> {
    stream: UnixStream,
    events: broadcast::Receiver<Arc<IpcEvent>>,
    command_handler: H,
}

impl<H: CommandHandler> IpcClientHandler<H> {
    pub fn new(
        stream: UnixStream,
        events: broadcast::Receiver<Arc<IpcEvent>>,
        command_handler: H,
    ) -> Self {
        Self { stream, events, command_handler }
    }

    pub async fn run(mut self) {
        let (reader, mut writer) = self.stream.into_split();
        let mut lines = BufReader::new(reader).lines();

        let hello = format!("{}\n", hello_line());
        if writer.write_all(hello.as_bytes()).await.is_err() {
            return;
        }

        let mut subscriptions: HashSet<String> = HashSet::new();

        loop {
            tokio::select! {
                line = lines.next_line() => {
                    match line {
                        Ok(Some(line)) if !line.trim().is_empty() => {
                            if line.len() > MAX_IPC_LINE {
                                tracing::debug!(len = line.len(), "IPC client sent oversize line; disconnecting");
                                break;
                            }
                            match parse_client_line(line.trim()) {
                                Ok(ClientMsg::Subscribe(patterns)) => {
                                    subscriptions.extend(patterns);
                                }
                                Ok(ClientMsg::Unsubscribe(patterns)) => {
                                    for p in patterns {
                                        subscriptions.remove(&p);
                                    }
                                }
                                Ok(ClientMsg::Command { name, fields }) => {
                                    let result = self.command_handler.execute(&name, &fields).await;
                                    let response = match result {
                                        Ok(extra) => {
                                            let mut line = "ack ok=true".to_owned();
                                            for (k, v) in &extra {
                                                line.push(' ');
                                                line.push_str(k);
                                                line.push('=');
                                                line.push_str(&escape(v));
                                            }
                                            line
                                        }
                                        Err(e) => ack_line(false, Some(&e)),
                                    };
                                    let _ = writer.write_all(format!("{response}\n").as_bytes()).await;
                                }
                                Err(e) => {
                                    tracing::debug!(error = %e, "IPC client sent unparseable line");
                                    let _ = writer.write_all(
                                        format!("{}\n", ack_line(false, Some(&e))).as_bytes()
                                    ).await;
                                }
                            }
                        }
                        Ok(Some(_)) => {}
                        Ok(None) | Err(_) => break,
                    }
                }
                result = self.events.recv() => {
                    match result {
                        Ok(event) => {
                            if subscriptions.iter().any(|p| matches_pattern(p, &event.name)) {
                                let line = format!("{}\n", event.encode());
                                if writer.write_all(line.as_bytes()).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::debug!(dropped = n, "IPC client lagged; events dropped");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    }
}
