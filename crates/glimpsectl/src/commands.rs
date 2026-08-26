use std::{
    io::{self, Write},
    path::PathBuf,
};

use anyhow::{Context, Result, bail};
use glimpse_contracts::{ServiceState, SystemServices, SystemTopics};
use glimpse_ipc::{Client, pattern};
use serde_json::{Map, Value};

use crate::render::{self, BAD, GOOD, HEADER, KEY, PLAIN, WARN};

const TOPICS: &str = "system.topics";
const SERVICES: &str = "system.services";

/// Stands in for a field with nothing in it, so a column is never blank about it.
const ABSENT: &str = "-";

pub struct Session {
    pub client: Client,
}

pub async fn get(session: &Session, topic: String, field: Option<String>) -> Result<()> {
    let answer = session.client.get(&topic).await?;

    let Some(event) = answer else {
        return absent(&topic);
    };

    let data = match &field {
        Some(path) => {
            select(&event.data, path).with_context(|| format!("`{topic}` has no field `{path}`"))?
        }
        None => &event.data,
    };

    write_line(&values(data))?;
    Ok(())
}

pub async fn watch(session: &Session, pattern: String, count: Option<u64>) -> Result<()> {
    let mut subscription = session.client.subscribe(&pattern).await?;
    let mut seen = 0;

    while let Some(event) = subscription.next().await {
        let pairs = render::leaves(&event.data)
            .iter()
            .map(|(path, leaf)| match path.is_empty() {
                true => leaf.clone(),
                false => format!("{path}={leaf}"),
            })
            .collect::<Vec<_>>()
            .join(" ");
        let topic = render::styled(&event.topic, HEADER);
        let stale = render::styled(if event.stale { " (stale)" } else { "" }, WARN);

        if let Flow::Stop = write_line(&format!("{topic}{stale}\t{pairs}"))? {
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
    write_line(&values(&result))?;
    Ok(())
}

pub async fn topics(session: &Session, pattern: Option<String>) -> Result<()> {
    let Some(event) = session.client.get(TOPICS).await? else {
        return absent(TOPICS);
    };

    let data = match pattern {
        None => event.data,
        Some(pattern) => narrow(event.data, &pattern)?,
    };

    let report: SystemTopics = serde_json::from_value(data)?;
    let rows = report.topics.iter().map(|(topic, report)| {
        render::cells(
            [
                topic.clone(),
                report.service.clone().unwrap_or_else(|| ABSENT.into()),
                match report.has_value {
                    true => "yes".into(),
                    false => ABSENT.into(),
                },
            ],
            [PLAIN, KEY, PLAIN],
        )
    });

    write_line(&render::table(["TOPIC", "OWNER", "VALUE"], rows))?;
    Ok(())
}

pub async fn services(session: &Session) -> Result<()> {
    let Some(event) = session.client.get(SERVICES).await? else {
        return absent(SERVICES);
    };

    let report: SystemServices = serde_json::from_value(event.data)?;
    let rows = report.services.iter().map(|(name, state)| {
        let (word, style, detail) = described(state);
        render::cells([name.clone(), word.to_owned(), detail], [PLAIN, style, KEY])
    });

    write_line(&render::table(["SERVICE", "STATE", "DETAIL"], rows))?;
    Ok(())
}

fn described(state: &ServiceState) -> (&'static str, anstyle::Style, String) {
    match state {
        ServiceState::Starting => ("starting", WARN, String::new()),
        ServiceState::Running => ("running", GOOD, String::new()),
        ServiceState::Degraded { reason } => ("degraded", WARN, reason.clone()),
        ServiceState::Stopped { reason } => (
            "stopped",
            BAD,
            reason.clone().unwrap_or_else(|| ABSENT.into()),
        ),
    }
}

pub fn doctor() -> Result<()> {
    bail!("doctor is not implemented yet")
}

pub async fn monitor(_session: &Session) -> Result<()> {
    bail!("monitor is not implemented yet")
}

pub fn config_show(override_path: Option<PathBuf>) -> Result<()> {
    let config = glimpse_config::load(override_path.as_deref())?;
    write_line(toml::to_string_pretty(&config)?.trim_end())?;
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

/// One leaf per line. A payload that is a bare scalar prints as itself, which is what makes
/// `get --field` usable from a script.
fn values(data: &Value) -> String {
    let leaves = render::leaves(data);
    if let [(path, only)] = leaves.as_slice()
        && path.is_empty()
    {
        return only.clone();
    }

    render::rows(
        leaves
            .iter()
            .map(|(path, leaf)| render::cells([path.clone(), leaf.clone()], [KEY, PLAIN])),
    )
}

/// A declared topic with no value is a different answer from an unknown one, and exits 0. It says
/// so on stderr, leaving stdout empty, so a script reading the value sees nothing rather than a
/// sentence it would have to recognise.
fn absent(topic: &str) -> Result<()> {
    anstream::eprintln!("glimpsectl: `{topic}` has no value yet");
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

enum Flow {
    Continue,
    Stop,
}

fn write_line(text: &str) -> Result<Flow> {
    // `anstream` strips the styling when stdout is not a terminal, so a pipe gets plain text
    // without any command having to ask whether it is being piped.
    match writeln!(anstream::stdout(), "{text}") {
        Ok(()) => Ok(Flow::Continue),
        // Rust ignores SIGPIPE, so `glimpsectl watch … | head -1` arrives here rather than
        // panicking out of `println!` with exit 101, which no exit table contains.
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(Flow::Stop),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::tests::plain;

    #[test]
    fn a_bare_scalar_prints_as_itself() {
        assert_eq!(values(&serde_json::json!(42)), "42");
        assert_eq!(values(&serde_json::json!("auto")), "auto");
    }

    #[test]
    fn a_payload_prints_one_leaf_per_line() {
        let data = serde_json::json!({ "at": { "lat": 52.2 }, "names": ["a", "b"] });
        assert_eq!(
            plain(&values(&data)),
            "at.lat    52.2\nnames[0]  a\nnames[1]  b"
        );
    }

    #[test]
    fn a_degraded_service_shows_its_reason() {
        let (word, _, detail) = described(&ServiceState::Degraded {
            reason: "no system bus".into(),
        });
        assert_eq!((word, detail.as_str()), ("degraded", "no system bus"));
    }
}
