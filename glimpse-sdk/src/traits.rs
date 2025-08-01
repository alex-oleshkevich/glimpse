use std::{fmt::Display, path::PathBuf, process};

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, WriteHalf},
    net::UnixStream,
};

use crate::{JSONRPCRequest, JSONRPCResponse, Request, Response};

#[derive(Debug, Clone)]
pub enum Error {
    SocketError(String),
    Custom(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::SocketError(msg) => write!(f, "Socket error: {}", msg),
            Error::Custom(msg) => write!(f, "Custom error: {}", msg),
        }
    }
}

pub trait Plugin {
    async fn search(&self, query: String, output: &mut ReplyWriter<'_>);

    async fn run(&self, socket_path: PathBuf) -> Result<(), Error> {
        setup_logging();
        let stream = tokio::net::UnixStream::connect(&socket_path).await;
        if stream.is_err() {
            return Err(Error::SocketError(
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
            let rpc_request = JSONRPCRequest::<Request>::from_json(&line);
            if let Err(e) = rpc_request {
                tracing::error!("invalid JSON-RPC payload: {}", e);
                continue;
            }

            let rpc_request = rpc_request.unwrap();
            let request = rpc_request.unwrap();
            let mut output = ReplyWriter {
                writer: &mut writer,
                rpc_request: rpc_request.clone(),
            };
            match request {
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
    rpc_request: JSONRPCRequest<Request>,
    writer: &'a mut WriteHalf<UnixStream>,
}

impl<'a> ReplyWriter<'a> {
    pub async fn reply(&mut self, resp: Response) {
        let rpc_message = JSONRPCResponse::success_for(&self.rpc_request, resp);
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
