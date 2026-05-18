//! Minimal client for the Glimpse IPC socket.
//!
//! [`ipc`] resolves a [`Subscriber`] for a service (no I/O — the connection
//! is opened lazily). [`Subscriber::listen`] subscribes to an event channel
//! and streams decoded [`Event`]s; [`Subscriber::dispatch`] sends an action
//! and awaits the server ack on a one-shot connection. The wire protocol is
//! the same line format the `glimpse-shell watch` / `dispatch` CLIs speak.

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

use crate::AppletResult;

/// One decoded event line: `name key=value … ts=<epoch>` with values
/// unescaped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub name: String,
    pub ts: u64,
    pub fields: HashMap<String, String>,
}

/// A resolved IPC endpoint. Cheap to create; holds only the socket path.
#[derive(Debug, Clone)]
pub struct Subscriber {
    socket: PathBuf,
}

/// Resolve the [`Subscriber`] for `service` (use `"shell"` for the panel).
///
/// The socket is `<dir>/<service>.sock` — `shell` maps to `ipc.sock` — where
/// `<dir>` is `$GLIMPSE_IPC_DIR`, else `$XDG_RUNTIME_DIR/glimpse`. No
/// connection is made here; it happens in [`Subscriber::listen`] /
/// [`Subscriber::dispatch`].
pub fn ipc(service: &str) -> AppletResult<Subscriber> {
    Ok(Subscriber {
        socket: socket_path(service)?,
    })
}

fn socket_path(service: &str) -> AppletResult<PathBuf> {
    let service = if service.is_empty() { "shell" } else { service };
    let dir = if let Some(d) = env::var_os("GLIMPSE_IPC_DIR") {
        PathBuf::from(d)
    } else if let Some(x) = env::var_os("XDG_RUNTIME_DIR") {
        PathBuf::from(x).join("glimpse")
    } else {
        return Err("neither GLIMPSE_IPC_DIR nor XDG_RUNTIME_DIR is set; \
                    cannot locate the Glimpse IPC socket"
            .into());
    };
    let file = if service == "shell" {
        "ipc.sock".to_owned()
    } else {
        format!("{service}.sock")
    };
    Ok(dir.join(file))
}

impl Subscriber {
    /// Subscribe to `channel` (an exact name, a `prefix.*` pattern, or `*`)
    /// and stream events until the server closes the connection.
    pub async fn listen(&self, channel: &str) -> AppletResult<EventStream> {
        let mut conn = self.connect().await?;
        conn.writer
            .write_all(format!("subscribe {channel}\n").as_bytes())
            .await?;
        Ok(EventStream { lines: conn.lines })
    }

    /// Dispatch `action` with `params` on a fresh connection and await the
    /// ack. Returns the extra ack fields on success; errors if the server
    /// replies `ok=false`.
    pub async fn dispatch<I, K, V>(
        &self,
        action: &str,
        params: I,
    ) -> AppletResult<HashMap<String, String>>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        validate_token("action", action, false)?;
        let mut pairs: Vec<(String, String)> = Vec::new();
        for (k, v) in params {
            validate_token("param key", k.as_ref(), true)?;
            pairs.push((k.as_ref().to_owned(), v.as_ref().to_owned()));
        }
        let mut conn = self.connect().await?;
        let mut line = String::from(action);
        for (k, v) in &pairs {
            line.push(' ');
            line.push_str(k);
            line.push('=');
            line.push_str(&escape(v));
        }
        line.push('\n');
        conn.writer.write_all(line.as_bytes()).await?;
        let ack = conn
            .lines
            .next_line()
            .await?
            .ok_or("IPC server closed connection without ack")?;
        parse_ack(&ack)
    }

    async fn connect(&self) -> AppletResult<Conn> {
        let stream = UnixStream::connect(&self.socket).await.map_err(|e| {
            format!(
                "cannot connect to IPC socket at {}: {e}",
                self.socket.display()
            )
        })?;
        let (reader, writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();
        match lines.next_line().await? {
            Some(l) if l.starts_with("hello") => {}
            Some(l) => return Err(format!("unexpected IPC greeting: {l}").into()),
            None => return Err("IPC server closed connection before hello".into()),
        }
        Ok(Conn { writer, lines })
    }
}

struct Conn {
    writer: OwnedWriteHalf,
    lines: Lines<BufReader<OwnedReadHalf>>,
}

/// Stream of [`Event`]s from an active subscription.
pub struct EventStream {
    lines: Lines<BufReader<OwnedReadHalf>>,
}

impl EventStream {
    /// The next event, or `None` once the server closes the connection.
    pub async fn next(&mut self) -> Option<AppletResult<Event>> {
        loop {
            match self.lines.next_line().await {
                Ok(Some(line)) if line.trim().is_empty() => continue,
                Ok(Some(line)) => return Some(Ok(parse_event(&line))),
                Ok(None) => return None,
                Err(e) => return Some(Err(e.into())),
            }
        }
    }
}

/// The wire protocol tokenizes client lines on whitespace and never
/// unescapes the command name or a field key, so an `action`/key carrying
/// whitespace would forge extra tokens or whole client lines. Values are
/// safe (they are escaped). Reject the unsafe shapes loudly.
fn validate_token(label: &str, s: &str, forbid_eq: bool) -> AppletResult<()> {
    if s.is_empty() {
        return Err(format!("IPC {label} must not be empty").into());
    }
    if s.chars().any(|c| c.is_ascii_whitespace()) {
        return Err(format!("IPC {label} {s:?} must not contain whitespace").into());
    }
    if forbid_eq && s.contains('=') {
        return Err(format!("IPC param key {s:?} must not contain '='").into());
    }
    Ok(())
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace(' ', "\\s")
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('s') => out.push(' '),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some(x) => {
                    out.push('\\');
                    out.push(x);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_event(line: &str) -> Event {
    let mut tokens = line.split_ascii_whitespace();
    let name = tokens.next().unwrap_or_default().to_owned();
    let mut ts = 0u64;
    let mut fields = HashMap::new();
    for token in tokens {
        let Some((k, v)) = token.split_once('=') else {
            continue;
        };
        let v = unescape(v);
        if k == "ts" {
            if let Ok(n) = v.parse::<u64>() {
                ts = n;
                continue;
            }
        }
        fields.insert(k.to_owned(), v);
    }
    Event { name, ts, fields }
}

fn parse_ack(line: &str) -> AppletResult<HashMap<String, String>> {
    let mut tokens = line.split_ascii_whitespace();
    if tokens.next() != Some("ack") {
        return Err(format!("expected an ack, got: {line}").into());
    }
    let mut ok = false;
    let mut fields = HashMap::new();
    for token in tokens {
        let Some((k, v)) = token.split_once('=') else {
            continue;
        };
        let v = unescape(v);
        match k {
            "ok" => ok = v == "true",
            _ => {
                fields.insert(k.to_owned(), v);
            }
        }
    }
    if ok {
        Ok(fields)
    } else {
        let msg = fields
            .remove("error")
            .unwrap_or_else(|| "command failed".to_owned());
        Err(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::net::UnixListener;

    fn tmp_socket() -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("glimpse-ipc-test-{n}.sock"))
    }

    #[test]
    fn escape_roundtrip() {
        let s = "a b\tc\nd\\e";
        assert_eq!(unescape(&escape(s)), s);
    }

    #[test]
    fn parse_event_unescapes_and_extracts_ts() {
        let ev = parse_event("notification.received body=line1\\nline2\\sword ts=42");
        assert_eq!(ev.name, "notification.received");
        assert_eq!(ev.ts, 42);
        assert_eq!(
            ev.fields.get("body").map(String::as_str),
            Some("line1\nline2 word")
        );
    }

    #[test]
    fn validate_token_rejects_injection() {
        assert!(validate_token("action", "open_uri", false).is_ok());
        assert!(validate_token("action", "a\nsubscribe *", false).is_err());
        assert!(validate_token("action", "a b", false).is_err());
        assert!(validate_token("action", "", false).is_err());
        assert!(validate_token("param key", "k=v", true).is_err());
        assert!(validate_token("param key", "ok", true).is_ok());
    }

    #[tokio::test]
    async fn dispatch_rejects_unsafe_action_before_connecting() {
        let sub = Subscriber {
            socket: PathBuf::from("/nonexistent/glimpse-x.sock"),
        };
        let err = sub
            .dispatch("evil\naction", [("k", "v")])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("whitespace"));
        // A bad key must also be rejected before any connect attempt.
        let err = sub.dispatch("ok", [("bad key", "v")]).await.unwrap_err();
        assert!(err.to_string().contains("whitespace"));
    }

    #[test]
    fn parse_ack_failure_is_err() {
        assert!(parse_ack("ack ok=false error=nope").is_err());
        let ok = parse_ack("ack ok=true echo=hi").unwrap();
        assert_eq!(ok.get("echo").map(String::as_str), Some("hi"));
    }

    #[tokio::test]
    async fn dispatch_and_listen_against_fake_server() {
        let socket = tmp_socket();
        let listener = UnixListener::bind(&socket).unwrap();
        let server = tokio::spawn(async move {
            // First connection: a dispatch.
            let (stream, _) = listener.accept().await.unwrap();
            let (r, mut w) = stream.into_split();
            w.write_all(b"hello version=test\n").await.unwrap();
            let mut lines = BufReader::new(r).lines();
            let cmd = lines.next_line().await.unwrap().unwrap();
            assert_eq!(cmd, "open_uri uri=https://example.com");
            w.write_all(b"ack ok=true echo=done\n").await.unwrap();

            // Second connection: a subscription.
            let (stream, _) = listener.accept().await.unwrap();
            let (r, mut w) = stream.into_split();
            w.write_all(b"hello version=test\n").await.unwrap();
            let mut lines = BufReader::new(r).lines();
            let sub = lines.next_line().await.unwrap().unwrap();
            assert_eq!(sub, "subscribe audio.*");
            w.write_all(b"audio.volume_changed volume=42 ts=7\n")
                .await
                .unwrap();
        });

        let sub = Subscriber { socket };
        let ack = sub
            .dispatch("open_uri", [("uri", "https://example.com")])
            .await
            .unwrap();
        assert_eq!(ack.get("echo").map(String::as_str), Some("done"));

        let mut events = sub.listen("audio.*").await.unwrap();
        let ev = events.next().await.unwrap().unwrap();
        assert_eq!(ev.name, "audio.volume_changed");
        assert_eq!(ev.ts, 7);
        assert_eq!(ev.fields.get("volume").map(String::as_str), Some("42"));

        server.await.unwrap();
    }
}
