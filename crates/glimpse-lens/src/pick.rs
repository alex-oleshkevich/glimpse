use anyhow::Result;
use glimpse_config::ColorFormat;

use crate::errors::Cancelled;
use crate::lens::{Outcome, Session, Settings};

pub fn pick(settings: Settings) -> Result<[u8; 3]> {
    match crate::overlay::run(
        |frames, done| Session::open(frames, settings, done),
        Session::close,
    )? {
        Some(Outcome::Picked(color)) => Ok(color),
        Some(Outcome::Cancelled) | None => Err(Cancelled.into()),
    }
}

pub fn json(format: ColorFormat, [red, green, blue]: [u8; 3]) -> String {
    serde_json::json!({
        "format": format.name(),
        "value": format.render([red, green, blue]),
        "red": red,
        "green": green,
        "blue": blue,
    })
    .to_string()
}
