use anyhow::{Context, Result};
use glimpse_dbus::night_light::{NightLight1Proxy, NightLightSnapshot};
use zbus::Connection;

use super::{emit, proxy, reason_or_absent, safe, within, yes_no};
use crate::cli::Mode;
use crate::render::{Section, Table, styled};

pub async fn sunset_status(connection: &Connection, json: bool) -> Result<()> {
    let snapshot = within(proxy::<NightLight1Proxy>(connection).await?.snapshot())
        .await
        .context("cannot read the night light")?;

    if json {
        return emit(&snapshot);
    }

    crate::render::print(&status_text(&snapshot))?;
    Ok(())
}

fn status_text(snapshot: &NightLightSnapshot) -> String {
    Section::new("Night light")
        .with(
            Table::default()
                .with_row(["mode".to_owned(), mode(snapshot)])
                .with_row(["temperature".to_owned(), temperature(snapshot)])
                .with_row(["night target".to_owned(), kelvin(snapshot.target)])
                .with_row(["applying".to_owned(), yes_no(snapshot.active).to_owned()])
                .with_row(["paused until".to_owned(), paused_until(snapshot)])
                .with_row(["serving".to_owned(), serving(snapshot)])
                .render(),
        )
        .render()
}

pub async fn sunset_watch(connection: &Connection, json: bool) -> Result<()> {
    super::watch::watch(
        connection,
        glimpse_dbus::night_light::GLIMPSE_NIGHT_LIGHT_BUS_NAME,
        glimpse_dbus::night_light::GLIMPSE_NIGHT_LIGHT_OBJECT_PATH,
        "me.aresa.Glimpse.NightLight1",
        |owner| async move {
            let proxy = within(
                NightLight1Proxy::builder(connection)
                    .destination(owner)?
                    .cache_properties(zbus::proxy::CacheProperties::No)
                    .build(),
            )
            .await?;
            let snapshot = within(proxy.snapshot()).await?;
            if json {
                Ok(serde_json::to_string(&snapshot)?)
            } else {
                Ok(status_text(&snapshot))
            }
        },
    )
    .await
}

pub async fn sunset_pause(connection: &Connection) -> Result<()> {
    within(proxy::<NightLight1Proxy>(connection).await?.pause())
        .await
        .context("cannot pause the night light")?;
    Ok(())
}

pub async fn sunset_resume(connection: &Connection) -> Result<()> {
    within(proxy::<NightLight1Proxy>(connection).await?.resume())
        .await
        .context("cannot resume the night light")?;
    Ok(())
}

pub async fn sunset_mode(connection: &Connection, mode: Mode) -> Result<()> {
    within(
        proxy::<NightLight1Proxy>(connection)
            .await?
            .set_schedule(mode.as_str()),
    )
    .await
    .with_context(|| format!("cannot set the night light to `{}`", mode.as_str()))?;
    Ok(())
}

pub async fn sunset_temperature(connection: &Connection, kelvin: u32) -> Result<()> {
    within(
        proxy::<NightLight1Proxy>(connection)
            .await?
            .set_temperature(kelvin),
    )
    .await
    .context(match kelvin {
        0 => "cannot clear the night light temperature".to_owned(),
        _ => format!("cannot set the night light to {kelvin} K"),
    })?;
    Ok(())
}

pub fn mode(snapshot: &NightLightSnapshot) -> String {
    match snapshot.overridden {
        true => format!(
            "{}  {}",
            safe(&snapshot.schedule),
            styled::warn("(override)")
        ),
        false => safe(&snapshot.schedule),
    }
}

pub fn temperature(snapshot: &NightLightSnapshot) -> String {
    match snapshot.manual {
        true => format!(
            "{}  {}",
            kelvin(snapshot.temperature),
            styled::warn("(manual)")
        ),
        false => kelvin(snapshot.temperature),
    }
}

pub fn serving(snapshot: &NightLightSnapshot) -> String {
    match snapshot.serving {
        true => styled::good("yes"),
        false => format!(
            "{}  {}",
            styled::bad("no"),
            styled::key(&reason_or_absent(&safe(&snapshot.reason)))
        ),
    }
}

pub fn paused_until(snapshot: &NightLightSnapshot) -> String {
    chrono::DateTime::from_timestamp(snapshot.paused_until, 0)
        .filter(|_| snapshot.paused_until != 0)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "no".to_owned())
}

fn kelvin(temperature: u32) -> String {
    format!("{temperature} K")
}
