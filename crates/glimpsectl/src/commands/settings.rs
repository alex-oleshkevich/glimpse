use anyhow::{Context, Result};
use glimpse_dbus::settings::{Mode, Settings1Proxy, decode_geolocation, decode_mode, decode_solar};
use serde_json::json;
use zbus::Connection;

use super::{emit, proxy, reason_or_absent, safe, within};
use crate::{
    cli::DesktopMode,
    render::{Section, Table},
};

pub async fn settings_status(connection: &Connection, json_output: bool) -> Result<()> {
    let proxy = proxy::<Settings1Proxy>(connection).await?;
    let (solar, location, mode) =
        within(async { tokio::try_join!(proxy.solar(), proxy.geolocation(), proxy.mode()) })
            .await
            .context("cannot read the settings provider")?;
    let phase = decode_solar(solar.clone()).map_err(anyhow::Error::msg)?;
    let coordinates = decode_geolocation(location.clone()).map_err(anyhow::Error::msg)?;
    let status = decode_mode(mode.clone()).map_err(anyhow::Error::msg)?;
    if json_output {
        return emit(&json!({
            "solar": { "available": solar.0, "status": phase, "reason": safe(&solar.3) },
            "geolocation": { "available": location.0, "coordinates": coordinates, "reason": safe(&location.3) },
            "mode": { "available": mode.0, "policy": if status.automatic { "auto" } else { "system" },
                "requested": status.requested, "observed": status.observed, "temporary": status.temporary, "reason": safe(&mode.5) }
        }));
    }
    Section::new("Settings")
        .with(
            Table::default()
                .with_row([
                    "location".into(),
                    coordinates.map_or_else(
                        || "unknown".into(),
                        |fix| format!("{}, {}", fix.latitude, fix.longitude),
                    ),
                ])
                .with_row([
                    "location reason".into(),
                    reason_or_absent(&safe(&location.3)),
                ])
                .with_row([
                    "solar".into(),
                    phase.as_ref().map_or_else(
                        || "unknown".into(),
                        |solar| format!("{:?}", solar.phase).to_lowercase(),
                    ),
                ])
                .with_row([
                    "next change".into(),
                    phase
                        .and_then(|solar| solar.next_change)
                        .map_or_else(|| "-".into(), |time| time.to_rfc3339()),
                ])
                .with_row(["solar reason".into(), reason_or_absent(&safe(&solar.3))])
                .with_row([
                    "policy".into(),
                    if status.automatic {
                        "auto".into()
                    } else {
                        "system".into()
                    },
                ])
                .with_row(["requested".into(), name(status.requested).into()])
                .with_row(["observed".into(), name(status.observed).into()])
                .with_row(["temporary".into(), name(status.temporary).into()])
                .with_row(["mode reason".into(), reason_or_absent(&safe(&mode.5))])
                .render(),
        )
        .print()
}

pub async fn settings_mode(connection: &Connection, mode: DesktopMode) -> Result<()> {
    within(
        proxy::<Settings1Proxy>(connection)
            .await?
            .set_temporary_mode(Mode::from(mode).code()),
    )
    .await
    .context("cannot set temporary desktop mode")?;
    Ok(())
}

pub async fn settings_resume(connection: &Connection) -> Result<()> {
    within(proxy::<Settings1Proxy>(connection).await?.resume())
        .await
        .context("cannot resume desktop mode")?;
    Ok(())
}

fn name(mode: Option<Mode>) -> &'static str {
    match mode {
        None => "-",
        Some(Mode::Light) => "light",
        Some(Mode::Dark) => "dark",
    }
}
