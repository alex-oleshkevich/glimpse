use anyhow::{Context, Result, bail};
use clap::Args;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace(' ', "\\s")
}

fn socket_path() -> String {
    std::env::var("GLIMPSE_IPC_SOCKET").unwrap_or_else(|_| {
        std::env::var("XDG_RUNTIME_DIR")
            .map(|d| format!("{d}/glimpse/ipc.sock"))
            .unwrap_or_else(|_| "/tmp/glimpse/ipc.sock".into())
    })
}

async fn connect() -> Result<UnixStream> {
    let path = socket_path();
    UnixStream::connect(&path)
        .await
        .with_context(|| format!("cannot connect to IPC socket at {path}"))
}

/// Consume the `hello` line sent immediately after connect.
async fn read_hello(lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>) -> Result<()> {
    match lines.next_line().await? {
        Some(line) if line.starts_with("hello ") => Ok(()),
        Some(line) => bail!("unexpected server greeting: {line}"),
        None => bail!("server closed connection before hello"),
    }
}

// ---------------------------------------------------------------------------
// watch
// ---------------------------------------------------------------------------

#[derive(Args, Debug)]
pub struct WatchArgs {
    /// Event patterns to subscribe to (e.g. `bluetooth.*` or `*`).
    /// Defaults to `*` when omitted.
    #[arg(default_value = "*")]
    pub patterns: Vec<String>,
}

pub async fn watch(args: WatchArgs) -> Result<()> {
    let stream = connect().await?;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    read_hello(&mut lines).await?;

    let subscribe_line = format!("subscribe {}\n", args.patterns.join(" "));
    writer.write_all(subscribe_line.as_bytes()).await?;

    while let Some(line) = lines.next_line().await? {
        println!("{line}");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// dispatch
// ---------------------------------------------------------------------------

#[derive(Args, Debug)]
pub struct DispatchArgs {
    /// Command name (e.g. `lock_screen` or `open_uri`).
    pub command: String,
    /// Key=value fields forwarded with the command (e.g. `uri=https://example.com`).
    pub fields: Vec<String>,
}

pub async fn dispatch(args: DispatchArgs) -> Result<()> {
    let stream = connect().await?;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    read_hello(&mut lines).await?;

    let parts: Vec<String> = std::iter::once(args.command.clone())
        .chain(args.fields.iter().map(|f| match f.split_once('=') {
            Some((k, v)) => format!("{}={}", k, escape(v)),
            None => f.clone(),
        }))
        .collect();
    let command_line = format!("{}\n", parts.join(" "));
    writer.write_all(command_line.as_bytes()).await?;

    match lines.next_line().await? {
        Some(line) => {
            println!("{line}");
            let failed = line
                .split_ascii_whitespace()
                .find(|t| t.starts_with("ok="))
                .map(|t| t == "ok=false")
                .unwrap_or(false);
            if failed {
                bail!("command failed");
            }
            Ok(())
        }
        None => bail!("server closed connection without ack"),
    }
}
