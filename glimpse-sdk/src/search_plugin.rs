use std::{path::PathBuf, process};

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, WriteHalf},
    net::UnixStream,
};

use crate::{GlimpseError, JSONRPCRequest, JSONRPCResponse, Request, Response};

pub trait SearchPlugin {
    async fn search(&self, query: String, output: &mut ReplyWriter<'_>);

    async fn run(&self, socket_path: PathBuf) -> Result<(), GlimpseError> {
        setup_logging();
        let stream = tokio::net::UnixStream::connect(&socket_path).await;
        if stream.is_err() {
            return Err(GlimpseError::SocketError(
                "failed to connect to socket".to_string(),
            ));
        }
        let stream = stream.unwrap();

        let (reader, writer) = tokio::io::split(stream);
        let mut writer = writer;
        let mut reader = tokio::io::BufReader::new(reader);

        let mut line = String::new();
        while let Ok(_) = reader.read_line(&mut line).await {
            if line.is_empty() {
                continue;
            }

            tracing::debug!("received line: {}", line.trim());
            let rpc_request = JSONRPCRequest::from_string(&line);
            if let Err(e) = rpc_request {
                tracing::error!("invalid JSON-RPC payload: {}", e);
                continue;
            }

            let rpc_request = rpc_request.unwrap();
            let mut output = ReplyWriter {
                writer: &mut writer,
                rpc_request: rpc_request.clone(),
            };
            match rpc_request.request {
                Request::Search { query } => self.search(query.clone(), &mut output).await,
                Request::Quit => process::exit(0),
                _ => {}
            }

            line.clear();
        }

        Ok(())
    }
}

pub struct ReplyWriter<'a> {
    rpc_request: JSONRPCRequest,
    writer: &'a mut WriteHalf<UnixStream>,
}

impl<'a> ReplyWriter<'a> {
    pub async fn reply(&mut self, resp: Response) {
        if self.rpc_request.id.is_none() {
            tracing::warn!("cannot reply to notification request");
            return;
        }

        let rpc_message = JSONRPCResponse::success(self.rpc_request.id.unwrap(), resp);
        let serialized = rpc_message.to_string();
        if let Err(e) = serialized {
            eprintln!("Error serializing response: {}", e);
            return;
        }
        let rpc_message = serialized.unwrap();

        if let Ok(_) = self.writer.write_all(rpc_message.as_bytes()).await {
            if let Err(e) = self.writer.write_all(b"\n").await {
                eprintln!("Error sending reply: {}", e);
            }
        }
    }
}

fn setup_logging() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
}
