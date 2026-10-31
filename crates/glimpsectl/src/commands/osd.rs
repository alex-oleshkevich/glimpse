use std::collections::HashMap;

use anyhow::{Context, Result};
use glimpse_dbus::osd::OsdProxy;
use zbus::Connection;
use zbus::zvariant::Value;

use super::{proxy, within};

pub async fn osd_show(
    connection: &Connection,
    icon: Option<&str>,
    label: Option<&str>,
    value: Option<u32>,
    percent: bool,
) -> Result<()> {
    let osd = proxy::<OsdProxy>(connection).await?;
    let mut values = HashMap::new();
    if let Some(icon) = icon {
        values.insert("icon", Value::from(icon));
    }
    if let Some(label) = label {
        values.insert("label", Value::from(label));
    }
    if let Some(value) = value {
        values.insert("value", Value::from(f64::from(value) / 100.0));
    }
    if percent {
        values.insert("percent", Value::from(true));
    }
    within(osd.show(values))
        .await
        .context("the on-screen display did not answer")
}

pub async fn osd_hide(connection: &Connection) -> Result<()> {
    let osd = proxy::<OsdProxy>(connection).await?;
    within(osd.hide())
        .await
        .context("the on-screen display did not answer")
}
