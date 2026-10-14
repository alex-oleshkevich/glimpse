pub const APPLET_HOST_INTERFACE: &str = "me.aresa.Glimpse.AppletHost1";
pub const APPLET_HOST_PATH: &str = "/me/aresa/Glimpse/AppletHost";

pub fn applet_host_name(app_id: &str) -> String {
    format!("{app_id}.AppletHost")
}

pub fn panel_app_id() -> String {
    panel_app_id_from(std::env::var("GLIMPSE_PANEL_APP_ID").ok())
}

fn panel_app_id_from(value: Option<String>) -> String {
    value.unwrap_or_else(|| "me.aresa.GlimpsePanel".to_owned())
}

#[zbus::proxy(
    interface = "me.aresa.Glimpse.AppletHost1",
    default_path = "/me/aresa/Glimpse/AppletHost"
)]
pub trait AppletHost {
    #[allow(clippy::too_many_arguments)]
    fn attach(
        &self,
        desktop_path: &str,
        panel: u32,
        output: &str,
        zone: &str,
        options_json: &str,
        env: std::collections::HashMap<&str, &str>,
        stderr: zbus::zvariant::Fd<'_>,
    ) -> zbus::Result<String>;

    fn restart(&self, handle: &str) -> zbus::Result<()>;

    fn detach(&self, handle: &str) -> zbus::Result<()>;

    fn set_options(&self, handle: &str, options_json: &str) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests;
