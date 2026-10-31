use std::collections::HashMap;

use relm4::Sender;
use zbus::fdo;
use zbus::zvariant::OwnedValue;

use crate::ask::Input;

const ICON_MAX_BYTES: usize = 128;
const LABEL_MAX_CHARS: usize = 120;
const VALUE_MAX: f64 = 2.0;

pub struct Message {
    pub icon: String,
    pub label: String,
    pub value: Option<f64>,
}

pub struct Server {
    input: Sender<Input>,
}

impl Server {
    pub fn new(input: Sender<Input>) -> Self {
        Self { input }
    }
}

#[zbus::interface(name = "me.aresa.Glimpse.Osd1")]
impl Server {
    async fn show(&self, values: HashMap<String, OwnedValue>) -> fdo::Result<()> {
        let message = parse(values)?;
        self.input
            .send(Input::Osd(message))
            .map_err(|_| unavailable())
    }

    async fn hide(&self) -> fdo::Result<()> {
        self.input.send(Input::OsdHide).map_err(|_| unavailable())
    }
}

fn unavailable() -> fdo::Error {
    fdo::Error::Failed("the display service is unavailable".into())
}

fn string(values: &HashMap<String, OwnedValue>, key: &str) -> fdo::Result<Option<String>> {
    values
        .get(key)
        .map(|value| {
            <&str>::try_from(value)
                .map(str::to_owned)
                .map_err(|_| fdo::Error::InvalidArgs(format!("{key} has the wrong type")))
        })
        .transpose()
}

fn parse(values: HashMap<String, OwnedValue>) -> fdo::Result<Message> {
    let icon = string(&values, "icon")?
        .map(|icon| {
            if icon.len() > ICON_MAX_BYTES || icon.contains('/') {
                String::new()
            } else {
                icon
            }
        })
        .unwrap_or_default();
    let label = string(&values, "label")?
        .map(|label| glimpse_utils::clean(&label, LABEL_MAX_CHARS))
        .unwrap_or_default();
    let value = values
        .get("value")
        .map(|value| {
            let value = f64::try_from(value)
                .map_err(|_| fdo::Error::InvalidArgs("value has the wrong type".into()))?;
            if !value.is_finite() || !(0.0..=VALUE_MAX).contains(&value) {
                return Err(fdo::Error::InvalidArgs(
                    "value must be between 0 and 2".into(),
                ));
            }
            Ok(value)
        })
        .transpose()?;
    if icon.is_empty() && label.is_empty() && value.is_none() {
        return Err(fdo::Error::InvalidArgs("nothing to show".into()));
    }
    Ok(Message { icon, label, value })
}

#[cfg(test)]
mod tests;
