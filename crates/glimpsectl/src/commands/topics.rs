use anyhow::{Result, bail};
use glimpse_contracts::SystemTopics;
use glimpse_ipc::pattern;
use serde_json::Value;

use super::{ABSENT, Session, absent};
use crate::render::Table;

const TOPICS: &str = "system.topics";

pub async fn topics(
    session: &Session,
    pattern: Option<String>,
    owner: Option<String>,
) -> Result<()> {
    let Some(event) = session.client.get(TOPICS).await? else {
        return absent(TOPICS, false);
    };

    let pattern_given = pattern.is_some();
    let data = match pattern {
        None => event.data,
        Some(pattern) => narrow(event.data, &pattern)?,
    };

    let mut report: SystemTopics = serde_json::from_value(data)?;
    // `narrow` filters the wire object by topic name; this filters the decoded report by owner, so
    // the two compose and neither has to know about the other.
    if let Some(owner) = &owner {
        report
            .topics
            .retain(|_, report| report.service.as_deref() == Some(owner.as_str()));
    }

    Table::new()
        .with_headers(["TOPIC", "OWNER", "VALUE"])
        .with_empty(&empty_reason(pattern_given, owner.as_deref()))
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
        .print()
}

/// Naming the filter that excluded everything, so an empty table is not mistaken for an empty
/// daemon.
fn empty_reason(pattern: bool, owner: Option<&str>) -> String {
    match (pattern, owner) {
        (true, Some(owner)) => format!("no topic matches that pattern and is owned by `{owner}`"),
        (true, None) => "no topic matches that pattern".to_owned(),
        (false, Some(owner)) => format!("no topic is owned by `{owner}`"),
        (false, None) => "the daemon declares no topics".to_owned(),
    }
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
    use super::{empty_reason, narrow};

    #[test]
    fn an_empty_result_names_whichever_filter_emptied_it() {
        assert_eq!(
            empty_reason(false, Some("audio")),
            "no topic is owned by `audio`"
        );
        assert_eq!(empty_reason(true, None), "no topic matches that pattern");
        assert_eq!(empty_reason(false, None), "the daemon declares no topics");
    }

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
