use serde::{Deserialize, Deserializer, Serialize};

#[cfg(test)]
use crate::sanitize::NAME_CAP;
use crate::sanitize::{APP_ID_CAP, TITLE_CAP, sanitize};
pub(crate) use crate::sanitize::{
    capped_app_id_str, capped_edid_str, capped_name_str, capped_title_str,
};

/// Opaque and scoped to one compositor run: niri assigns a counter, Hyprland hands out the window's
/// address. Never render it, never persist it across sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub struct WindowId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub struct WorkspaceId(pub u64);

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    #[serde(default)]
    pub idx: Option<u8>,
    #[serde(default, deserialize_with = "capped_name")]
    pub name: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub is_focused: bool,
    #[serde(default)]
    pub is_urgent: bool,
    #[serde(default)]
    pub active_window_id: Option<WindowId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(from = "RawWindow")]
pub struct Window {
    pub id: WindowId,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub pid: Option<i32>,
    pub workspace_id: Option<WorkspaceId>,
    pub is_focused: bool,
    pub is_floating: bool,
    pub is_urgent: bool,
    /// Where the compositor places this window among its peers. Niri's scrolling column index;
    /// Hyprland has no equivalent and leaves it `None`.
    pub layout_order: Option<u16>,
    pub size: Option<(u32, u32)>,
    pub rect: Option<Rect>,
    pub output: Option<String>,
}

#[derive(Deserialize)]
struct RawWindow {
    id: WindowId,
    #[serde(default, deserialize_with = "capped_title")]
    title: Option<String>,
    #[serde(default, deserialize_with = "capped_app_id")]
    app_id: Option<String>,
    #[serde(default)]
    pid: Option<i32>,
    #[serde(default)]
    workspace_id: Option<WorkspaceId>,
    #[serde(default)]
    is_focused: bool,
    #[serde(default)]
    is_floating: bool,
    #[serde(default)]
    is_urgent: bool,
    #[serde(default)]
    layout: Option<RawLayout>,
}

#[derive(Deserialize)]
struct RawLayout {
    #[serde(default)]
    pos_in_scrolling_layout: Option<(u16, u16)>,
    #[serde(default)]
    window_size: Option<(i32, i32)>,
    #[serde(default)]
    tile_pos_in_workspace_view: Option<(f64, f64)>,
    #[serde(default)]
    window_offset_in_tile: Option<(f64, f64)>,
}

impl From<RawWindow> for Window {
    fn from(raw: RawWindow) -> Self {
        let layout_order = raw
            .layout
            .as_ref()
            .and_then(|layout| layout.pos_in_scrolling_layout)
            .map(|(column, _row)| column);
        let size = raw.layout.as_ref().and_then(|layout| {
            let (width, height) = layout.window_size?;
            Some((u32::try_from(width).ok()?, u32::try_from(height).ok()?))
        });
        let rect = raw.layout.as_ref().and_then(|layout| {
            let (x, y) = layout.tile_pos_in_workspace_view?;
            let (offset_x, offset_y) = layout.window_offset_in_tile.unwrap_or((0.0, 0.0));
            let (width, height) = size?;
            Some(Rect {
                x: x + offset_x,
                y: y + offset_y,
                width: f64::from(width),
                height: f64::from(height),
            })
        });

        Self {
            id: raw.id,
            title: raw.title,
            app_id: raw.app_id,
            pid: raw.pid,
            workspace_id: raw.workspace_id,
            is_focused: raw.is_focused,
            is_floating: raw.is_floating,
            is_urgent: raw.is_urgent,
            layout_order,
            size,
            rect,
            output: None,
        }
    }
}

pub(crate) fn fill_window_outputs(windows: &mut [Window], workspaces: &[Workspace]) {
    for window in windows.iter_mut() {
        window.output = window
            .workspace_id
            .and_then(|id| workspaces.iter().find(|workspace| workspace.id == id))
            .and_then(|workspace| workspace.output.clone());
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub connector: String,
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub description: Option<String>,
    pub logical: Option<Logical>,
    pub current_mode: Option<Mode>,
    pub enabled: bool,
    pub built_in: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    pub refresh_mhz: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Logical {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

/// `names` is what a panel renders, `codes` is what a configuration file matches against. The two
/// compositors supply opposite halves — niri gives descriptions, Hyprland gives xkb codes — so both
/// are filled on both, and they are parallel to `current`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyboardLayouts {
    pub names: Vec<String>,
    pub codes: Vec<String>,
    pub current: Option<usize>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CastKind {
    PipeWire,
    Screencopy,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CastTarget {
    Output(String),
    Window(WindowId),
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cast {
    pub stream_id: u64,
    pub session_id: Option<u64>,
    pub kind: CastKind,
    pub target: CastTarget,
    pub pw_node_id: Option<u32>,
    pub active: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub outputs: Vec<Output>,
    pub workspaces: Vec<Workspace>,
    pub windows: Vec<Window>,
    pub keyboard: KeyboardLayouts,
    pub focused_window: Option<WindowId>,
    pub focused_output: Option<String>,
    pub casts: Vec<Cast>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutTarget {
    Index(u8),
    Next,
    Prev,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceTarget {
    Id(WorkspaceId),
    Index(u8),
    Name(String),
    Next,
    Prev,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowTarget {
    Id(WindowId),
    Next,
    Prev,
}

fn capped_title<'de, D: Deserializer<'de>>(input: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(input)?.map(|text| sanitize(&text, TITLE_CAP)))
}

fn capped_app_id<'de, D: Deserializer<'de>>(input: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(input)?.map(|text| sanitize(&text, APP_ID_CAP)))
}

fn capped_name<'de, D: Deserializer<'de>>(input: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(input)?.and_then(|text| capped_name_str(&text)))
}

pub(crate) fn is_built_in(connector: &str) -> bool {
    let connector = connector.to_ascii_lowercase();
    connector.starts_with("edp-") || connector.starts_with("lvds-")
}

#[cfg(test)]
mod tests;
