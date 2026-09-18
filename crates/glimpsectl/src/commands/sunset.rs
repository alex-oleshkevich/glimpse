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

    Section::new("Night light")
        .with(
            Table::new()
                .with_row(["mode".to_owned(), mode(&snapshot)])
                .with_row(["temperature".to_owned(), temperature(&snapshot)])
                .with_row(["night target".to_owned(), kelvin(snapshot.target)])
                .with_row(["applying".to_owned(), yes_no(snapshot.active).to_owned()])
                .with_row(["serving".to_owned(), serving(&snapshot)])
                .render(),
        )
        .print()
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

fn mode(snapshot: &NightLightSnapshot) -> String {
    match snapshot.overridden {
        true => format!(
            "{}  {}",
            safe(&snapshot.schedule),
            styled::warn("(override)")
        ),
        false => safe(&snapshot.schedule),
    }
}

fn temperature(snapshot: &NightLightSnapshot) -> String {
    match snapshot.manual {
        true => format!(
            "{}  {}",
            kelvin(snapshot.temperature),
            styled::warn("(manual)")
        ),
        false => kelvin(snapshot.temperature),
    }
}

fn serving(snapshot: &NightLightSnapshot) -> String {
    match snapshot.serving {
        true => styled::good("yes"),
        false => format!(
            "{}  {}",
            styled::bad("no"),
            styled::key(&reason_or_absent(&safe(&snapshot.reason)))
        ),
    }
}

fn kelvin(temperature: u32) -> String {
    format!("{temperature} K")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> NightLightSnapshot {
        NightLightSnapshot {
            schedule: "automatic".to_owned(),
            overridden: false,
            temperature: 6500,
            target: 4200,
            active: false,
            serving: true,
            reason: String::new(),
            configured: "automatic".to_owned(),
            manual: false,
        }
    }

    #[test]
    fn an_overridden_mode_says_that_the_document_is_not_the_one_talking() {
        let mut overridden = snapshot();
        overridden.schedule = "off".to_owned();
        overridden.overridden = true;

        assert!(mode(&overridden).contains("off"));
        assert!(mode(&overridden).contains("override"));
        assert!(!mode(&snapshot()).contains("override"));
    }

    #[test]
    fn a_manual_temperature_says_so_and_a_scheduled_one_does_not() {
        let mut manual = snapshot();
        manual.manual = true;

        assert!(temperature(&manual).contains("manual"));
        assert!(!temperature(&snapshot()).contains("manual"));
    }

    #[test]
    fn a_provider_that_is_not_serving_shows_why() {
        let mut degraded = snapshot();
        degraded.serving = false;
        degraded.reason = "another gamma client holds the outputs".to_owned();

        assert!(serving(&degraded).contains("another gamma client"));
        assert!(!serving(&snapshot()).contains('-'));
    }
}
