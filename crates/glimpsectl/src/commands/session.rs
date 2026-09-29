use std::collections::HashMap;

use anyhow::{Context, Result};
use glimpse_dbus::systemd1::Systemd1ManagerProxy;
use zbus::Connection;
use zbus::fdo::DBusProxy;

use super::{proxy, within};

const DECLARED: &str = "glimpse:niri";

pub async fn session_declare(connection: &Connection) -> Result<()> {
    let current = std::env::var("XDG_CURRENT_DESKTOP").ok();

    if !should_declare(current.as_deref()) {
        tracing::info!(
            current = current.as_deref().unwrap_or("<unset>"),
            "not a plain niri session; nothing to declare"
        );
        return Ok(());
    }

    let manager = proxy::<Systemd1ManagerProxy>(connection).await?;
    let assignment = format!("XDG_CURRENT_DESKTOP={DECLARED}");
    within(manager.set_environment(&[assignment.as_str()]))
        .await
        .context("cannot set XDG_CURRENT_DESKTOP in the user manager's environment")?;

    let dbus = DBusProxy::new(connection).await?;
    within(dbus.update_activation_environment(HashMap::from([("XDG_CURRENT_DESKTOP", DECLARED)])))
        .await
        .context("cannot update the activation environment")?;

    within(manager.try_restart_unit("xdg-desktop-portal.service", "replace"))
        .await
        .context("cannot restart xdg-desktop-portal.service")?;

    Ok(())
}

pub fn should_declare(current: Option<&str>) -> bool {
    current == Some("niri")
}
