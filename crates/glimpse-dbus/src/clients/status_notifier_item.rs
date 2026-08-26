use zbus::zvariant::OwnedObjectPath;

pub type IconPixmap = Vec<(i32, i32, Vec<u8>)>;

pub type ToolTip = (String, IconPixmap, String, String);

#[zbus::proxy(interface = "org.kde.StatusNotifierItem", assume_defaults = false)]
pub trait StatusNotifierItem {
    fn activate(&self, x: i32, y: i32) -> zbus::Result<()>;
    fn context_menu(&self, x: i32, y: i32) -> zbus::Result<()>;
    fn secondary_activate(&self, x: i32, y: i32) -> zbus::Result<()>;
    fn scroll(&self, delta: i32, orientation: &str) -> zbus::Result<()>;
    fn provide_xdg_activation_token(&self, token: &str) -> zbus::Result<()>;

    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn title(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn category(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn icon_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn icon_pixmap(&self) -> zbus::Result<IconPixmap>;
    #[zbus(property)]
    fn overlay_icon_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn overlay_icon_pixmap(&self) -> zbus::Result<IconPixmap>;
    #[zbus(property)]
    fn attention_icon_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> zbus::Result<IconPixmap>;
    #[zbus(property)]
    fn icon_theme_path(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn item_is_menu(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn menu(&self) -> zbus::Result<OwnedObjectPath>;
    #[zbus(property)]
    fn tool_tip(&self) -> zbus::Result<ToolTip>;

    #[zbus(signal)]
    fn new_title(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn new_icon(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn new_overlay_icon(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn new_attention_icon(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn new_tool_tip(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn new_menu(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn new_status(&self, status: String) -> zbus::Result<()>;
}
