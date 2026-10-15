use std::time::Duration;

use anyhow::{Context, Result};
use glimpse_dbus::idle::{
    BackendHealth, HealthKind, Idle1Proxy, IdleInhibitorRecord, InhibitionTargets,
};
use zbus::Connection;

use super::{emit, proxy, reason_or_absent, safe, within, yes_no};
use crate::render::{self, Section, Table, styled};

pub async fn idle_status(connection: &Connection, json: bool) -> Result<()> {
    let idle = proxy::<Idle1Proxy>(connection).await?;
    let inhibitors = within(idle.inhibitors())
        .await
        .context("cannot read the idle inhibitors")?;
    let health = within(idle.health())
        .await
        .context("cannot read the idle health")?;

    if json {
        return emit(&serde_json::json!({ "inhibitors": inhibitors, "health": health }));
    }

    Section::new("Sources")
        .with(
            Table::default()
                .with_headers(["SOURCE", "STATE", "REASON"])
                .with_rows([
                    backend("screensaver", &health.screen_saver),
                    backend("portal", &health.portal),
                    backend("login1", &health.login1),
                    backend("wayland", &health.wayland),
                ])
                .render(),
        )
        .print()?;

    render::print("")?;
    Section::new("Inhibitors")
        .with(
            Table::default()
                .with_headers(["ID", "WHO", "WHY", "BLOCKS", "RELEASABLE"])
                .with_empty("nothing holds off idle")
                .with_rows(inhibitors.iter().map(row))
                .render(),
        )
        .print()
}

pub async fn idle_inhibit(connection: &Connection, lapse: Option<u32>) -> Result<()> {
    let id = within(
        proxy::<Idle1Proxy>(connection)
            .await?
            .hold(lapse.unwrap_or(0)),
    )
    .await
    .context("cannot hold off idle")?;
    render::print(&id.to_string())?;

    let lapsed = async {
        match lapse {
            Some(seconds) => tokio::time::sleep(Duration::from_secs(u64::from(seconds))).await,
            None => std::future::pending().await,
        }
    };
    tokio::select! {
        () = lapsed => Ok(()),
        interrupted = tokio::signal::ctrl_c() => Ok(interrupted?),
    }
}

pub async fn idle_release(connection: &Connection, id: u64) -> Result<()> {
    within(proxy::<Idle1Proxy>(connection).await?.release(id))
        .await
        .with_context(|| format!("cannot release idle inhibitor {id}"))?;
    Ok(())
}

fn backend(name: &str, health: &BackendHealth) -> [String; 3] {
    let state = match health.kind {
        HealthKind::Ready => styled::good("ready"),
        HealthKind::Degraded => styled::warn("degraded"),
        HealthKind::Unsupported => styled::key("unsupported"),
    };
    [
        name.to_owned(),
        state,
        styled::key(&reason_or_absent(&safe(&health.message))),
    ]
}

fn row(record: &IdleInhibitorRecord) -> [String; 5] {
    [
        record.id.to_string(),
        safe(&record.who),
        safe(&record.why),
        blocks(&record.targets),
        yes_no(record.can_release).to_owned(),
    ]
}

fn blocks(targets: &InhibitionTargets) -> String {
    [
        (targets.idle, "idle"),
        (targets.suspend, "suspend"),
        (targets.shutdown, "shutdown"),
        (targets.lid_switch, "lid"),
        (targets.power_key, "power key"),
        (targets.suspend_key, "suspend key"),
        (targets.hibernate_key, "hibernate key"),
    ]
    .into_iter()
    .filter_map(|(blocked, name)| blocked.then_some(name))
    .collect::<Vec<_>>()
    .join(", ")
}

#[cfg(test)]
mod tests;
