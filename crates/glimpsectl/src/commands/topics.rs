use anyhow::{Result, bail};
use glimpse_contracts::SystemTopics;
use glimpse_ipc::pattern;
use serde_json::Value;

use super::{ABSENT, Session, absent, write_line};
use crate::render::Table;

const TOPICS: &str = "system.topics";

pub async fn topics(session: &Session, pattern: Option<String>) -> Result<()> {
    let Some(event) = session.client.get(TOPICS).await? else {
        return absent(TOPICS, false);
    };

    let data = match pattern {
        None => event.data,
        Some(pattern) => narrow(event.data, &pattern)?,
    };

    let report: SystemTopics = serde_json::from_value(data)?;

    write_line(
        &Table::new()
            .with_headers(["TOPIC", "OWNER", "VALUE"])
            .with_empty("no topic matches that pattern")
            .with_rows(report.topics.iter().map(|(topic, report)| {
                [
                    topic.clone(),
                    report.service.clone().unwrap_or_else(|| ABSENT.to_owned()),
                    match report.has_value {
                        true => "yes".to_owned(),
                        false => ABSENT.to_owned(),
                    },
                ]
            }))
            .render(),
    )?;
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

#[cfg(test)]
mod tests {
    use super::narrow;

    #[test]
    fn a_pattern_filters_the_topic_keys() {
        let data = serde_json::json!({ "topics": { "audio.volume": 1, "solar.status": 2 } });
        let narrowed = narrow(data, "audio.*").expect("narrow");
        assert_eq!(
            narrowed,
            serde_json::json!({ "topics": { "audio.volume": 1 } })
        );
    }
}
