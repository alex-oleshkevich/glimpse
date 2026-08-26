use anyhow::Result;
use serde_json::{Map, Value};

use super::Session;
use crate::render;

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
    // A command that returns nothing has already said so by exiting 0; printing `null` would give
    // a script a line to strip.
    if !result.is_null() {
        render::print(&render::lines(&result))?;
    }
    Ok(())
}
