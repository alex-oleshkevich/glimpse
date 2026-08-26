use anyhow::Result;
use glimpse_contracts::SystemTopics;

use super::{ABSENT, Session, absent, empty_reason, narrow};
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
        Some(pattern) => narrow(event.data, TOPICS, "topics", &pattern)?,
    };

    let mut report: SystemTopics = serde_json::from_value(data)?;
    // `narrow` filters the wire object by topic name; this filters the decoded report by owner, so
    // the two compose and neither has to know about the other.
    if let Some(owner) = &owner {
        report
            .topics
            .retain(|_, entry| entry.service.as_deref() == Some(owner.as_str()));
    }

    Table::new()
        .with_headers(["TOPIC", "OWNER", "VALUE"])
        .with_empty(&empty_reason("topic", pattern_given, owner.as_deref()))
        .with_rows(report.topics.iter().map(|(topic, entry)| {
            [
                topic.clone(),
                entry.service.clone().unwrap_or_else(|| ABSENT.to_owned()),
                match entry.has_value {
                    true => "yes".to_owned(),
                    false => ABSENT.to_owned(),
                },
            ]
        }))
        .print()
}
