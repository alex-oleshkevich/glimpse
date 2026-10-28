pub const APPLET_HOST_PATH: &str = "/me/aresa/Glimpse/AppletHost";

pub fn applet_host_name(app_id: &str) -> String {
    format!("{app_id}.AppletHost")
}

pub fn panel_app_id() -> String {
    std::env::var("GLIMPSE_PANEL_APP_ID").unwrap_or_else(|_| "me.aresa.GlimpsePanel".to_owned())
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

    fn tree(&self, handle: &str) -> zbus::Result<String>;

    fn detach(&self, handle: &str) -> zbus::Result<()>;

    fn set_options(&self, handle: &str, options_json: &str) -> zbus::Result<()>;
}
