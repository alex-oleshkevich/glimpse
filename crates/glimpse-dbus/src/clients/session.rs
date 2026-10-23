pub const AGENTS_BUS_NAME: &str = "me.aresa.Glimpse.Agents";
pub const SESSION_PATH: &str = "/me/aresa/Glimpse/Session";
pub const SESSION_INTERFACE: &str = "me.aresa.Glimpse.Session";

#[zbus::proxy(
    interface = "me.aresa.Glimpse.Session",
    default_service = "me.aresa.Glimpse.Agents",
    default_path = "/me/aresa/Glimpse/Session"
)]
pub trait Session {
    fn request_action(&self, action: &str) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests;
