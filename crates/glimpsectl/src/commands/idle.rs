use std::os::unix::process::ExitStatusExt;
use std::time::Duration;

use anyhow::{Context, Result};
use glimpse_dbus::idle::{
    BackendHealth, GLIMPSE_IDLE_BUS_NAME, HealthKind, Idle1Proxy, IdleInhibitorRecord,
    InhibitionTargets,
};
use zbus::Connection;

use super::{emit, proxy, reason_or_absent, safe, within, yes_no};
use crate::render::{self, Section, Table, styled};

pub async fn idle_status(connection: &Connection, json: bool) -> Result<()> {
    crate::render::print(
        &status_snapshot(&proxy::<Idle1Proxy>(connection).await?, json, false).await?,
    )?;
    Ok(())
}

async fn status_snapshot(idle: &Idle1Proxy<'_>, json: bool, compact: bool) -> Result<String> {
    let inhibitors = within(idle.inhibitors())
        .await
        .context("cannot read the idle inhibitors")?;
    let health = within(idle.health())
        .await
        .context("cannot read the idle health")?;

    if json {
        let value = serde_json::json!({ "inhibitors": inhibitors, "health": health });
        return Ok(if compact {
            serde_json::to_string(&value)?
        } else {
            serde_json::to_string_pretty(&value)?
        });
    }
    let expiries = within(idle.hold_expiries())
        .await
        .context("cannot read idle inhibitor expiries")?;
    let sources = Section::new("Sources")
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
        .render();
    let records = Section::new("Inhibitors")
        .with(
            Table::default()
                .with_headers(["ID", "WHO", "WHY", "BLOCKS", "RELEASABLE", "EXPIRES"])
                .with_empty("nothing holds off idle")
                .with_rows(
                    inhibitors
                        .iter()
                        .map(|record| row(record, expiries.get(&record.id).copied())),
                )
                .render(),
        )
        .render();
    Ok(format!("{sources}\n\n{records}"))
}

pub async fn idle_watch(connection: &Connection, json: bool) -> Result<()> {
    super::watch::watch(
        connection,
        glimpse_dbus::idle::GLIMPSE_IDLE_BUS_NAME,
        glimpse_dbus::idle::GLIMPSE_IDLE_OBJECT_PATH,
        "me.aresa.Glimpse.Idle1",
        |owner| async move {
            let proxy = within(
                Idle1Proxy::builder(connection)
                    .destination(owner)?
                    .cache_properties(zbus::proxy::CacheProperties::No)
                    .build(),
            )
            .await?;
            status_snapshot(&proxy, json, true).await
        },
    )
    .await
}

pub async fn idle_inhibit_child(
    connection: &Connection,
    lapse: Option<u32>,
    command: &[String],
    json: bool,
) -> Result<()> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let mut hangup = signal(SignalKind::hangup())?;
    let dbus = within(zbus::fdo::DBusProxy::new(connection)).await?;
    let owner = within(dbus.get_name_owner(GLIMPSE_IDLE_BUS_NAME.try_into()?)).await?;
    let idle = within(
        Idle1Proxy::builder(connection)
            .destination(owner)?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build(),
    )
    .await?;
    let id = within(idle.hold(lapse.unwrap_or(0)))
        .await
        .context("cannot hold off idle")?;
    let outcome = async {
        if let Some((program, args)) = command.split_first() {
            let mut child = tokio::process::Command::new(program).args(args)
                .process_group(0).kill_on_drop(true).spawn()
                .with_context(|| format!("cannot start `{program}`"))?;
            loop {
                let interrupted = tokio::select! {
                    status = child.wait() => {
                        let status = status.context("cannot wait for inhibited command")?;
                        let code = status.code().unwrap_or_else(|| 128 + status.signal().unwrap_or(0));
                        break if code == 0 { Ok(()) } else { Err(crate::errors::ChildExit(code as u8).into()) };
                    }
                    _ = interrupt.recv() => libc::SIGINT,
                    _ = terminate.recv() => libc::SIGTERM,
                    _ = hangup.recv() => libc::SIGHUP,
                };
                if let Some(pid) = child.id() {
                    unsafe { libc::kill(-(pid as i32), interrupted); }
                }
            }
        } else {
            if json { emit(&serde_json::json!({"id": id}))? } else { render::print(&id.to_string())?; }
            let lapsed = async {
                match lapse {
                    Some(seconds) => tokio::time::sleep(Duration::from_secs(u64::from(seconds))).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                () = lapsed => Ok(()),
                _ = interrupt.recv() => Ok(()),
                _ = terminate.recv() => Ok(()),
                _ = hangup.recv() => Ok(()),
            }
        }
    }.await;
    let release = within(idle.release(id))
        .await
        .with_context(|| format!("cannot release idle inhibitor {id}"));
    if let Err(error) = release {
        let expired = lapse.is_some()
            && matches!(error.downcast_ref::<zbus::Error>(),
            Some(zbus::Error::MethodError(name, _, _)) if name.as_str() == "org.freedesktop.DBus.Error.UnknownObject");
        if !expired {
            if outcome.is_ok() {
                return Err(error);
            }
            eprintln!("{error:#}");
        }
    }
    outcome
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

fn row(record: &IdleInhibitorRecord, expires: Option<u64>) -> [String; 6] {
    [
        record.id.to_string(),
        safe(&record.who),
        safe(&record.why),
        blocks(&record.targets),
        yes_no(record.can_release).to_owned(),
        expires
            .and_then(|time| i64::try_from(time).ok())
            .and_then(|time| chrono::DateTime::from_timestamp(time, 0))
            .map_or_else(
                || "-".to_owned(),
                |time| {
                    time.with_timezone(&chrono::Local)
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string()
                },
            ),
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
