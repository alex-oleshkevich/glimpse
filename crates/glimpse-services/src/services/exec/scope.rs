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

pub async fn adopt(ctx: &Ctx<Exec>, pid: u32, id: &str) {
    let Ok(bus) = ctx.session_bus() else {
        return;
    };
    let Ok(proxy) = Systemd1ManagerProxy::new(bus).await else {
        return;
    };
    let name = unit_name(id, pid);
    let properties = [
        ("PIDs", Value::from(vec![pid])),
        ("Slice", Value::from("app.slice")),
        ("CollectMode", Value::from("inactive-or-failed")),
    ];
    if let Err(error) = proxy
        .start_transient_unit(&name, "fail", &properties, &[])
        .await
    {
        tracing::warn!(%error, %name, "cannot adopt applet scope");
    }
}
