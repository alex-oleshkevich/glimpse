use glimpse_dbus::login1::Login1ManagerProxy;
use glimpse_dbus::systemd1::{Systemd1ManagerProxy, Systemd1UnitProxy};

const GRAPHICAL_SESSION_TARGET: &str = "graphical-session.target";

pub(super) async fn log_out(
    system: &zbus::Connection,
    user: &zbus::Connection,
    manager: &Login1ManagerProxy<'_>,
) -> Result<(), String> {
    if graphical_session_has_compositor(user).await {
        return stop_graphical_session(user).await;
    }
    let session = crate::lock::session(system)
        .await
        .map_err(|error| error.to_string())?;
    let id = session.id().await.map_err(|error| error.to_string())?;
    manager
        .terminate_session(&id)
        .await
        .map_err(|error| error.to_string())
}

struct BoundUnit {
    name: String,
    active_state: String,
    before: Vec<String>,
}

fn stops_graphical_session(units: &[BoundUnit]) -> bool {
    units.iter().any(|unit| {
        unit.name.ends_with(".service")
            && unit.active_state == "active"
            && unit
                .before
                .iter()
                .any(|before| before == GRAPHICAL_SESSION_TARGET)
    })
}

async fn unit_proxy(
    bus: &zbus::Connection,
    manager: &Systemd1ManagerProxy<'_>,
    name: &str,
) -> zbus::Result<Systemd1UnitProxy<'static>> {
    let path = manager.get_unit(name).await?;
    Systemd1UnitProxy::builder(bus).path(path)?.build().await
}

async fn bound_unit(
    bus: &zbus::Connection,
    manager: &Systemd1ManagerProxy<'_>,
    name: String,
) -> Option<BoundUnit> {
    let unit = unit_proxy(bus, manager, &name).await.ok()?;
    let (active_state, before) = tokio::try_join!(unit.active_state(), unit.before()).ok()?;
    Some(BoundUnit {
        name,
        active_state,
        before,
    })
}

async fn graphical_session_has_compositor(bus: &zbus::Connection) -> bool {
    let Ok(manager) = Systemd1ManagerProxy::new(bus).await else {
        return false;
    };
    let Ok(target) = unit_proxy(bus, &manager, GRAPHICAL_SESSION_TARGET).await else {
        return false;
    };
    let Ok(bound_by) = target.bound_by().await else {
        return false;
    };
    let units = futures_util::future::join_all(
        bound_by
            .into_iter()
            .filter(|name| name.ends_with(".service"))
            .map(|name| bound_unit(bus, &manager, name)),
    )
    .await
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    stops_graphical_session(&units)
}

async fn stop_graphical_session(bus: &zbus::Connection) -> Result<(), String> {
    let manager = Systemd1ManagerProxy::new(bus)
        .await
        .map_err(|error| error.to_string())?;
    manager
        .stop_unit(GRAPHICAL_SESSION_TARGET, "replace-irreversibly")
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
