use anyhow::Result;

use super::Session;
use crate::render::{self, Flow, styled};

pub async fn watch(
    session: &Session,
    pattern: String,
    count: Option<u64>,
    json: bool,
) -> Result<()> {
    let mut subscription = session.client.subscribe(&pattern).await?;
    let mut seen = 0;

    while let Some(event) = subscription.next().await {
        let line = match json {
            true => serde_json::to_string(&event)?,
            false => {
                let topic = &event.topic;
                let stale = styled::warn(if event.stale { " (stale)" } else { "" });
                format!("{topic}{stale}\t{}", render::inline(&event.data))
            }
        };

        if let Flow::Stop = render::print(&line)? {
            break;
        }

        seen += 1;
        if count.is_some_and(|limit| seen >= limit) {
            break;
        }
    }

    Ok(())
}
