use std::sync::atomic::{AtomicBool, Ordering};

use glimpse_dbus::systemd1::Systemd1ManagerProxy;
use zbus::zvariant::Value;

use crate::Ctx;

use super::Exec;

pub fn unit_name(id: &str, pid: u32) -> String {
    format!("app-glimpse-{}-{pid}.scope", id.replace('-', "\\x2d"))
}

pub fn unit_pattern(id: &str) -> String {
    format!("app-glimpse-{}-*.scope", id.replace('-', "\\x2d"))
}

pub async fn adopt(ctx: &Ctx<Exec>, pid: u32, id: &str, warned: &AtomicBool) {
    if let Err(error) = start_scope(ctx, pid, id).await
        && !warned.swap(true, Ordering::Relaxed)
    {
        tracing::warn!(%error, applet = %id, "cannot adopt applet scope");
    }
}

async fn start_scope(ctx: &Ctx<Exec>, pid: u32, id: &str) -> Result<(), String> {
    let bus = ctx.session_bus().map_err(str::to_owned)?;
    let proxy = Systemd1ManagerProxy::new(bus)
        .await
        .map_err(|error| error.to_string())?;
    let properties = [
        ("PIDs", Value::from(vec![pid])),
        ("Slice", Value::from("app.slice")),
        ("CollectMode", Value::from("inactive-or-failed")),
    ];
    proxy
        .start_transient_unit(&unit_name(id, pid), "fail", &properties, &[])
        .await
        .map(drop)
        .map_err(|error| error.to_string())
}
