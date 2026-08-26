use anyhow::{Context, Result};
use serde_json::Value;

use super::{Session, absent};
use crate::render;

pub async fn get(
    session: &Session,
    topic: String,
    field: Option<String>,
    json: bool,
) -> Result<()> {
    let Some(event) = session.client.get(&topic).await? else {
        return absent(&topic, json);
    };

    let data = match &field {
        Some(path) => {
            select(&event.data, path).with_context(|| format!("`{topic}` has no field `{path}`"))?
        }
        None => &event.data,
    };

    // `--json` is a passthrough, not a second rendering: whatever the daemon sent is what a script
    // reading this sees.
    match json {
        true => render::print(&serde_json::to_string(data)?)?,
        false => render::print(&render::lines(data))?,
    };
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::select;

    #[test]
    fn a_field_path_escapes_before_it_becomes_a_pointer() {
        let data = serde_json::json!({ "a/b": 1, "sink": { "a": 2 } });
        assert_eq!(select(&data, "a/b"), Some(&serde_json::json!(1)));
        assert_eq!(select(&data, "sink.a"), Some(&serde_json::json!(2)));
        assert_eq!(select(&data, "missing"), None);
    }
}
