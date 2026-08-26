//! One module per subcommand. Everything shared lives here: the session they run against, and the
//! one place stdout is written.

mod call;
mod config;
mod doctor;
mod get;
mod methods;
mod monitor;
mod services;
mod topics;
mod watch;

pub use call::call;
pub use config::{config_path, config_show, config_validate};
pub use doctor::doctor;
pub use get::get;
pub use methods::methods;
pub use monitor::monitor;
pub use services::services;
pub use topics::topics;
pub use watch::watch;

use anyhow::{Result, bail};
use glimpse_ipc::{Client, pattern};
use serde_json::Value;

use crate::render;

/// Stands in for a field with nothing in it, so a column is never blank about it.
const ABSENT: &str = "-";

pub struct Session {
    pub client: Client,
}

/// A declared topic with no value is a different answer from an unknown one, and exits 0. It says
/// so on stderr, leaving stdout empty, so a script reading the value sees nothing rather than a
/// sentence it would have to recognise.
fn absent(topic: &str, json: bool) -> Result<()> {
    match json {
        // A passthrough has to answer on stdout, and `null` is what the daemon means by no value.
        true => render::print("null").map(|_| ())?,
        false => anstream::eprintln!("glimpsectl: `{topic}` has no value yet"),
    }
    Ok(())
}

/// `system.topics` and `system.methods` are the same shape — one object under one field, keyed by
/// name — so narrowing either is filtering that object's keys.
fn narrow(mut data: Value, topic: &str, field: &str, pattern: &str) -> Result<Value> {
    let Some(Value::Object(entries)) = data.get_mut(field) else {
        bail!("`{topic}` does not carry a `{field}` object");
    };

    entries.retain(|name, _| pattern::matches(pattern, name));
    Ok(data)
}

/// Naming the filter that excluded everything, so an empty table is not mistaken for an empty
/// daemon. `noun` is the singular of what was being listed.
fn empty_reason(noun: &str, pattern: bool, owner: Option<&str>) -> String {
    match (pattern, owner) {
        (true, Some(owner)) => format!("no {noun} matches that pattern and is owned by `{owner}`"),
        (true, None) => format!("no {noun} matches that pattern"),
        (false, Some(owner)) => format!("no {noun} is owned by `{owner}`"),
        (false, None) => format!("the daemon declares no {noun}s"),
    }
}

#[cfg(test)]
mod tests {
    use super::{empty_reason, narrow};

    #[test]
    fn an_empty_result_names_whichever_filter_emptied_it() {
        assert_eq!(
            empty_reason("topic", false, Some("audio")),
            "no topic is owned by `audio`"
        );
        assert_eq!(
            empty_reason("method", true, None),
            "no method matches that pattern"
        );
        assert_eq!(
            empty_reason("topic", false, None),
            "the daemon declares no topics"
        );
    }

    #[test]
    fn a_pattern_filters_the_keys_of_the_named_object() {
        let data = serde_json::json!({ "topics": { "audio.volume": 1, "solar.status": 2 } });
        let narrowed = narrow(data, "system.topics", "topics", "audio.*").expect("narrow");
        assert_eq!(
            narrowed,
            serde_json::json!({ "topics": { "audio.volume": 1 } })
        );
    }

    #[test]
    fn a_payload_without_the_expected_object_is_an_error_not_an_empty_table() {
        let data = serde_json::json!({ "methods": 7 });
        assert!(narrow(data, "system.methods", "methods", "*").is_err());
    }
}
