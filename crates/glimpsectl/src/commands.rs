use std::{
    io::{self, Write},
    path::PathBuf,
};

use anyhow::{Context, Result, bail};
use glimpse_ipc::{Client, pattern};
use serde_json::{Map, Value};

const TOPICS: &str = "system.topics";
const SERVICES: &str = "system.services";

pub struct Session {
    pub client: Client,
    pub json: bool,
}

pub async fn get(session: &Session, topic: String, field: Option<String>) -> Result<()> {
    let answer = session.client.get(&topic).await?;

    let Some(event) = answer else {
        return absent(session, &topic);
    };

    let data = match &field {
        Some(path) => {
            select(&event.data, path).with_context(|| format!("`{topic}` has no field `{path}`"))?
        }
        None => &event.data,
    };

    write_line(&render(data, session.json)?)?;
    Ok(())
}

pub async fn watch(session: &Session, pattern: String, count: Option<u64>) -> Result<()> {
    let mut subscription = session.client.subscribe(&pattern).await?;
    let mut seen = 0;

    while let Some(event) = subscription.next().await {
        let line = match session.json {
            true => serde_json::to_string(&event)?,
            false => {
                let stale = if event.stale { " (stale)" } else { "" };
                format!(
                    "{}{stale}\t{}",
                    event.topic,
                    serde_json::to_string(&event.data)?
                )
            }
        };

        if let Flow::Stop = write_line(&line)? {
            break;
        }

        seen += 1;
        if count.is_some_and(|limit| seen >= limit) {
            break;
        }
    }

    Ok(())
}

pub async fn call(
    session: &Session,
    method: String,
    arguments: Vec<(String, String)>,
) -> Result<()> {
    let args = arguments
        .into_iter()
        .map(|(key, raw)| {
            // JSON when it parses, a string otherwise, so `mode=auto` is not a parse error.
            let value = match serde_json::from_str(&raw) {
                Ok(value) => value,
                Err(_) => Value::String(raw),
            };
            (key, value)
        })
        .collect::<Map<String, Value>>();

    let result = session.client.call(&method, Value::Object(args)).await?;
    write_line(&render(&result, session.json)?)?;
    Ok(())
}

pub async fn topics(session: &Session, pattern: Option<String>) -> Result<()> {
    let Some(event) = session.client.get(TOPICS).await? else {
        return absent(session, TOPICS);
    };

    let data = match pattern {
        None => event.data,
        Some(pattern) => narrow(event.data, &pattern)?,
    };

    write_line(&render(&data, session.json)?)?;
    Ok(())
}

pub async fn services(session: &Session) -> Result<()> {
    get(session, SERVICES.to_owned(), None).await
}

pub fn doctor() -> Result<()> {
    bail!("doctor is not implemented yet")
}

pub async fn monitor(_session: &Session) -> Result<()> {
    bail!("monitor is not implemented yet")
}

pub fn config_show(override_path: Option<PathBuf>, json: bool) -> Result<()> {
    let config = glimpse_config::load(override_path.as_deref())?;
    match json {
        true => write_line(&serde_json::to_string_pretty(&config)?)?,
        false => write_line(toml::to_string_pretty(&config)?.trim_end())?,
    };
    Ok(())
}

pub fn config_validate(override_path: Option<PathBuf>) -> Result<()> {
    glimpse_config::load(override_path.as_deref())?;
    Ok(())
}

pub fn config_path(config: Option<PathBuf>) -> Result<()> {
    for path in glimpse_config::resolved_files(config.as_deref())? {
        if let Flow::Stop = write_line(&path.display().to_string())? {
            break;
        }
    }
    Ok(())
}

/// A declared topic with no value is a different answer from an unknown one, and exits 0.
fn absent(session: &Session, topic: &str) -> Result<()> {
    match session.json {
        true => {
            write_line("null")?;
        }
        false => anstream::eprintln!("glimpsectl: `{topic}` has no value yet"),
    }
    Ok(())
}

/// `system.topics` carries an object keyed by topic name, so filtering it is filtering keys.
fn narrow(mut data: Value, pattern: &str) -> Result<Value> {
    let Some(Value::Object(topics)) = data.get_mut("topics") else {
        bail!("`{TOPICS}` does not carry a `topics` object");
    };

    topics.retain(|topic, _| pattern::matches(pattern, topic));
    Ok(data)
}

/// `--field a.b` is a JSON Pointer with different punctuation, so each segment is escaped the way
/// RFC 6901 requires before it becomes one: a key containing `/` would otherwise silently address
/// something else rather than fail.
fn select<'a>(data: &'a Value, path: &str) -> Option<&'a Value> {
    let pointer: String = path
        .split('.')
        .map(|part| format!("/{}", part.replace('~', "~0").replace('/', "~1")))
        .collect();
    data.pointer(&pointer)
}

fn render(value: &Value, json: bool) -> Result<String> {
    let text = match json {
        true => serde_json::to_string(value)?,
        false => serde_json::to_string_pretty(value)?,
    };
    Ok(text)
}

enum Flow {
    Continue,
    Stop,
}

fn write_line(text: &str) -> Result<Flow> {
    match writeln!(io::stdout(), "{text}") {
        Ok(()) => Ok(Flow::Continue),
        // Rust ignores SIGPIPE, so `glimpsectl watch … | head -1` arrives here rather than
        // panicking out of `println!` with exit 101, which no exit table contains.
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(Flow::Stop),
        Err(error) => Err(error.into()),
    }
}
