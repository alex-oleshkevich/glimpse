pub const BUS_NAME: &str = "me.aresa.Glimpse.Compositor";
pub const OBJECT_PATH: &str = "/me/aresa/Glimpse/Compositor";

pub type Item = std::collections::HashMap<String, zbus::zvariant::OwnedValue>;

pub mod key {
    pub const ACTIVE: &str = "active";
    pub const APP_ID: &str = "app_id";
    pub const CODES: &str = "codes";
    pub const CONNECTOR: &str = "connector";
    pub const CURRENT: &str = "current";
    pub const DESCRIPTION: &str = "description";
    pub const ENABLED: &str = "enabled";
    pub const FLOATING: &str = "floating";
    pub const HEIGHT: &str = "height";
    pub const ID: &str = "id";
    pub const INDEX: &str = "index";
    pub const LOGICAL: &str = "logical";
    pub const MAKE: &str = "make";
    pub const MODE: &str = "mode";
    pub const MODEL: &str = "model";
    pub const NAME: &str = "name";
    pub const NAMES: &str = "names";
    pub const ORDER: &str = "order";
    pub const OUTPUT: &str = "output";
    pub const PID: &str = "pid";
    pub const PW_NODE: &str = "pw_node";
    pub const SESSION: &str = "session";
    pub const TITLE: &str = "title";
    pub const URGENT: &str = "urgent";
    pub const WIDTH: &str = "width";
    pub const WINDOW: &str = "window";
    pub const WORKSPACE: &str = "workspace";
    pub const X: &str = "x";
    pub const Y: &str = "y";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    RenameWorkspace,
    OutputPower,
    BlankScreens,
    WindowScreenshot,
    WindowGeometry,
    OverviewBackdrop,
}

impl Capability {
    const ALL: [Self; 6] = [
        Self::RenameWorkspace,
        Self::OutputPower,
        Self::BlankScreens,
        Self::WindowScreenshot,
        Self::WindowGeometry,
        Self::OverviewBackdrop,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::RenameWorkspace => "rename-workspace",
            Self::OutputPower => "output-power",
            Self::BlankScreens => "blank-screens",
            Self::WindowScreenshot => "window-screenshot",
            Self::WindowGeometry => "window-geometry",
            Self::OverviewBackdrop => "overview-backdrop",
        }
    }

    pub fn parse_all<S: AsRef<str>>(strings: &[S]) -> Vec<Self> {
        strings
            .iter()
            .filter_map(|text| {
                Self::ALL
                    .into_iter()
                    .find(|capability| capability.as_str() == text.as_ref())
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, zbus::DBusError)]
#[zbus(prefix = "me.aresa.Glimpse.Compositor1.Error")]
pub enum Error {
    Unsupported(String),
    Refused(String),
    Unavailable(String),
    Failed(String),
}

#[cfg(test)]
mod tests;
