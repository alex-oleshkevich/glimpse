#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkspaceId(pub u64);

#[derive(Debug, Clone, PartialEq)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub index: Option<u8>,
    pub name: Option<String>,
    pub output: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub id: WindowId,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub pid: Option<i32>,
    pub workspace: Option<WorkspaceId>,
    pub floating: bool,
    pub urgent: bool,
    pub order: Option<u16>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub connector: String,
    pub enabled: bool,
    pub description: Option<String>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub mode: Option<Mode>,
    pub logical: Option<Logical>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    pub refresh_mhz: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Logical {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Focus {
    pub output: Option<String>,
    pub window: Option<WindowId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyboardLayouts {
    pub names: Vec<String>,
    pub codes: Vec<String>,
    pub current: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cast {
    pub id: u64,
    pub active: bool,
    pub output: Option<String>,
    pub session: Option<u64>,
    pub pw_node: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub outputs: Vec<Output>,
    pub workspaces: Vec<Workspace>,
    pub windows: Vec<Window>,
    pub focus: Focus,
    pub keyboard: KeyboardLayouts,
    pub casts: Vec<Cast>,
}
