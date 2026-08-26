use anyhow::Result;
use serde_json::{Map, Value};

use super::{Session, write_line};
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
    write_line(&render::lines(&result))?;
    Ok(())
}
