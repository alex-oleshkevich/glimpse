use std::collections::HashMap;

use zbus::zvariant::Value;

pub const OSD_BUS_NAME: &str = "me.aresa.Glimpse.Osd";
pub const OSD_PATH: &str = "/me/aresa/Glimpse/Osd";

#[zbus::proxy(
    interface = "me.aresa.Glimpse.Osd1",
    default_service = "me.aresa.Glimpse.Osd",
    default_path = "/me/aresa/Glimpse/Osd"
)]
pub trait Osd {
    fn show(&self, values: HashMap<&str, Value<'_>>) -> zbus::Result<()>;
    fn hide(&self) -> zbus::Result<()>;
}
