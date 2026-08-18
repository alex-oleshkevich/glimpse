use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

use super::protocol::{escape, unescape};

pub struct WatchArgs {
    pub patterns: Vec<String>,
    pub json: bool,
}

pub struct DispatchArgs {
    pub command: String,
    pub fields: Vec<String>,
    pub json: bool,
}

pub async fn watch(args: WatchArgs, socket_path: PathBuf) -> Result<()> {
    let stream = connect(&socket_path).await?;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    read_hello(&mut lines).await?;

    let patterns = if args.patterns.is_empty() {
        vec!["*".to_owned()]
    } else {
        args.patterns
    };
    writer
        .write_all(format!("subscribe {}\n", patterns.join(" ")).as_bytes())
        .await?;

    while let Some(line) = lines.next_line().await? {
        if args.json {
            println!("{}", event_to_json(&line));
        } else {
            println!("{line}");
        }
    }

    Ok(())
}

pub async fn dispatch(args: DispatchArgs, socket_path: PathBuf) -> Result<()> {
    let stream = connect(&socket_path).await?;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    read_hello(&mut lines).await?;

    let parts: Vec<String> = std::iter::once(args.command.clone())
        .chain(args.fields.iter().map(|f| match f.split_once('=') {
            Some((k, v)) => format!("{}={}", k, escape(v)),
            None => f.clone(),
        }))
        .collect();
    writer
        .write_all(format!("{}\n", parts.join(" ")).as_bytes())
        .await?;

    match lines.next_line().await? {
        Some(line) => {
            if args.json {
                println!("{}", ack_to_json(&line));
            } else {
                println!("{line}");
            }
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

/// Parse an event wire line into a JSON object string.
///
/// Wire: `<name> key=val key=val ts=<epoch>`
/// JSON: `{"type":"event","name":"...","key":"val",...,"ts":1234}`
fn event_to_json(line: &str) -> String {
    let mut tokens = line.split_ascii_whitespace();
    let Some(name) = tokens.next() else {
        return "{}".into();
    };
    let mut map = Map::new();
    for token in tokens {
        let Some((k, v)) = token.split_once('=') else {
            continue;
        };
        let v = unescape(v);
        if k == "ts" {
            if let Ok(n) = v.parse::<u64>() {
                map.insert(k.to_owned(), Value::Number(n.into()));
                continue;
            }
        }
        map.insert(k.to_owned(), Value::String(v));
    }
    // Envelope keys are inserted last so an event field of the same name
    // (e.g. bluetooth.device_added's own "name" field) can never shadow them.
    map.insert("type".into(), Value::String("event".into()));
    map.insert("name".into(), Value::String(name.to_owned()));
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".into())
}

/// Parse an ack wire line into a JSON object string.
///
/// Wire: `ack ok=true key=val ...` or `ack ok=false error=<msg>`
/// JSON: `{"ok":true,"key":"val"}` or `{"ok":false,"error":"..."}`
fn ack_to_json(line: &str) -> String {
    let mut tokens = line.split_ascii_whitespace();
    let _ = tokens.next(); // skip "ack"
    let mut map = Map::new();
    for token in tokens {
        let Some((k, v)) = token.split_once('=') else {
            continue;
        };
        let v = unescape(v);
        if k == "ok" {
            map.insert("ok".into(), Value::Bool(v == "true"));
        } else {
            map.insert(k.to_owned(), Value::String(v));
        }
    }
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".into())
}

async fn connect(path: &PathBuf) -> Result<UnixStream> {
    UnixStream::connect(path)
        .await
        .with_context(|| format!("cannot connect to IPC socket at {}", path.display()))
}

async fn read_hello(
    lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
) -> Result<()> {
    match lines.next_line().await? {
        Some(line) if line.starts_with("hello") => Ok(()),
        Some(line) => bail!("unexpected server greeting: {line}"),
        None => bail!("server closed connection before hello"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_to_json_keeps_event_name_when_a_field_is_also_named_name() {
        // e.g. bluetooth.device_added carries its own "name" field (device alias).
        let line = "bluetooth.device_added address=aa:bb name=AirPods ts=123";
        let parsed: Value = serde_json::from_str(&event_to_json(line)).unwrap();
        assert_eq!(parsed["type"], "event");
        assert_eq!(parsed["name"], "bluetooth.device_added");
        assert_eq!(parsed["ts"], 123);
    }

    #[test]
    fn event_to_json_keeps_envelope_type_when_a_field_is_also_named_type() {
        let line = "removable.device_added type=usb ts=123";
        let parsed: Value = serde_json::from_str(&event_to_json(line)).unwrap();
        assert_eq!(parsed["type"], "event");
        assert_eq!(parsed["name"], "removable.device_added");
    }
}
