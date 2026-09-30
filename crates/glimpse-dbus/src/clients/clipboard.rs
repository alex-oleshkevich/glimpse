use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

pub const GLIMPSE_CLIPBOARD_BUS_NAME: &str = "me.aresa.Glimpse.Clipboard";
pub const GLIMPSE_CLIPBOARD_OBJECT_PATH: &str = "/me/aresa/Glimpse/Clipboard";

#[derive(Debug, Clone, PartialEq, Eq, Type, Serialize, Deserialize)]
pub struct ClipboardItem {
    pub id: u64,
    pub kind: String,
    pub mime: String,
    pub pinned: bool,
    pub copied_at_secs: i64,
    pub size: u64,
    pub preview: String,
}

#[zbus::proxy(
    interface = "me.aresa.Glimpse.Clipboard1",
    default_service = "me.aresa.Glimpse.Clipboard",
    default_path = "/me/aresa/Glimpse/Clipboard"
)]
pub trait Clipboard1 {
    fn list(&self) -> zbus::Result<Vec<ClipboardItem>>;

    fn get(&self, id: u64) -> zbus::Result<(String, Vec<u8>)>;

    fn copy(&self, id: u64) -> zbus::Result<()>;

    fn pin(&self, id: u64, pinned: bool) -> zbus::Result<()>;

    fn forget(&self, id: u64) -> zbus::Result<()>;

    fn clear(&self) -> zbus::Result<()>;
}
