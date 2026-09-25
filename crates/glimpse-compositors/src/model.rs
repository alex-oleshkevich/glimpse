use serde::{Deserialize, Deserializer, Serialize};

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

const TITLE_CAP: usize = 512;
const APP_ID_CAP: usize = 128;
const NAME_CAP: usize = 128;
const EDID_CAP: usize = 128;

/// Titles and app ids belong to other applications: unbounded, and in practice already carrying
/// bidi overrides that would reorder whatever a panel draws next to them.
fn sanitize(text: &str, cap: usize) -> String {
    text.chars()
        .filter(|character| !is_hostile(*character))
        .take(cap)
        .collect()
}

fn is_hostile(character: char) -> bool {
    character.is_control() || matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

fn capped_title<'de, D: Deserializer<'de>>(input: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(input)?.map(|text| sanitize(&text, TITLE_CAP)))
}

fn capped_app_id<'de, D: Deserializer<'de>>(input: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(input)?.map(|text| sanitize(&text, APP_ID_CAP)))
}

fn capped_name<'de, D: Deserializer<'de>>(input: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(input)?.and_then(|text| non_empty(sanitize(&text, NAME_CAP))))
}

/// Hyprland sends an absent title or class as `""` where niri sends `null`, so both have to
/// arrive as `None` or a caller sees a difference that is not about the window.
pub(crate) fn capped_title_str(text: &str) -> Option<String> {
    non_empty(sanitize(text, TITLE_CAP))
}

pub(crate) fn capped_app_id_str(text: &str) -> Option<String> {
    non_empty(sanitize(text, APP_ID_CAP))
}

pub(crate) fn capped_name_str(text: &str) -> Option<String> {
    non_empty(sanitize(text, NAME_CAP))
}

pub(crate) fn capped_edid_str(text: &str) -> Option<String> {
    non_empty(sanitize(text, EDID_CAP).trim().to_owned())
}

fn non_empty(text: String) -> Option<String> {
    (!text.is_empty()).then_some(text)
}

pub(crate) fn is_built_in(connector: &str) -> bool {
    let connector = connector.to_ascii_lowercase();
    connector.starts_with("edp-") || connector.starts_with("lvds-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_keeps_its_text_and_loses_its_bidi_overrides() {
        let hostile = "\u{200e}\u{2068}Jenny\u{2069} @ \u{2068}Alex\u{2069}";
        assert_eq!(sanitize(hostile, TITLE_CAP), "\u{200e}Jenny @ Alex");
    }

    #[test]
    fn a_title_loses_control_characters_that_would_break_a_log_line() {
        assert_eq!(sanitize("a\nb\tc\u{0}d", TITLE_CAP), "abcd");
    }

    /// `take` counts characters, so a cap landing mid-codepoint cannot slice one in half.
    #[test]
    fn an_overlong_title_truncates_on_a_char_boundary() {
        let long = "\u{1f600}".repeat(TITLE_CAP + 10);
        let capped = sanitize(&long, TITLE_CAP);

        assert_eq!(capped.chars().count(), TITLE_CAP);
        assert_eq!(capped.len(), TITLE_CAP * 4);
    }

    #[test]
    fn an_app_id_is_capped_shorter_than_a_title() {
        let long = "x".repeat(1000);

        assert_eq!(sanitize(&long, APP_ID_CAP).len(), APP_ID_CAP);
        assert_eq!(sanitize(&long, TITLE_CAP).len(), TITLE_CAP);
    }

    #[test]
    fn a_workspace_name_is_capped_and_sanitized_like_a_window_title() {
        assert_eq!(capped_name_str("a\nb"), Some("ab".to_owned()));
        assert_eq!(capped_name_str(""), None);
        assert_eq!(
            capped_name_str(&"x".repeat(NAME_CAP + 10)),
            Some("x".repeat(NAME_CAP)),
            "a workspace name is set by whatever ran renameworkspace, not only by hand"
        );
    }

    #[test]
    fn an_edid_string_is_sanitized_before_it_is_trimmed() {
        assert_eq!(
            capped_edid_str("AW2725Q \u{1}"),
            Some("AW2725Q".to_owned()),
            "trimming first would leave the control character in place to defeat it"
        );
        assert_eq!(capped_edid_str("\u{202a} \u{202a}"), None);
    }

    #[test]
    fn internal_panels_are_recognized_by_connector() {
        assert!(is_built_in("eDP-1"));
        assert!(is_built_in("LVDS-1"));
        assert!(!is_built_in("DP-3"));
        assert!(!is_built_in("HDMI-A-1"));
    }

    #[test]
    fn a_tiled_window_has_no_rect_and_a_floating_window_does() {
        let tiled: Window = serde_json::from_value(serde_json::json!({
            "id": 374, "is_floating": false,
            "layout": { "pos_in_scrolling_layout": [4, 1], "window_size": [1018, 1644],
                        "tile_pos_in_workspace_view": null, "window_offset_in_tile": [0.0, 0.0] }
        }))
        .expect("tiled window");

        assert_eq!(tiled.size, Some((1018, 1644)));
        assert_eq!(tiled.rect, None);

        let floating: Window = serde_json::from_value(serde_json::json!({
            "id": 435, "is_floating": true,
            "layout": { "pos_in_scrolling_layout": null, "window_size": [1807, 924],
                        "tile_pos_in_workspace_view": [259.2, 189.6],
                        "window_offset_in_tile": [3.0, -2.0] }
        }))
        .expect("floating window");

        assert_eq!(floating.size, Some((1807, 924)));
        assert_eq!(
            floating.rect,
            Some(Rect {
                x: 262.2,
                y: 187.6,
                width: 1807.0,
                height: 924.0
            })
        );
    }

    #[test]
    fn a_window_with_no_layout_at_all_decodes_with_neither_size_nor_rect() {
        let window: Window =
            serde_json::from_value(serde_json::json!({ "id": 1, "is_focused": false }))
                .expect("window with no layout key");

        assert_eq!(window.size, None);
        assert_eq!(window.rect, None);
        assert_eq!(window.layout_order, None);
    }

    #[test]
    fn a_negative_window_size_drops_the_geometry_but_not_the_window() {
        let window: Window = serde_json::from_value(serde_json::json!({
            "id": 1,
            "layout": { "window_size": [-1, 924], "tile_pos_in_workspace_view": [10.0, 20.0] }
        }))
        .expect("a negative size still decodes the window");

        assert_eq!(window.size, None);
        assert_eq!(window.rect, None);
    }

    #[test]
    fn a_window_s_output_comes_from_its_workspace() {
        let mut windows = vec![Window {
            id: WindowId(9),
            title: None,
            app_id: None,
            pid: None,
            workspace_id: Some(WorkspaceId(5)),
            is_focused: false,
            is_floating: false,
            is_urgent: false,
            layout_order: None,
            size: None,
            rect: None,
            output: None,
        }];
        let workspaces = vec![Workspace {
            id: WorkspaceId(5),
            idx: None,
            name: None,
            output: Some("eDP-1".to_owned()),
            is_active: false,
            is_focused: false,
            is_urgent: false,
            active_window_id: None,
        }];

        fill_window_outputs(&mut windows, &workspaces);

        assert_eq!(windows[0].output.as_deref(), Some("eDP-1"));
    }

    #[test]
    fn a_window_deserialized_directly_still_caps_its_title() {
        let long = "x".repeat(5000);
        let window: Window =
            serde_json::from_value(serde_json::json!({ "id": 1, "title": long })).expect("window");

        assert_eq!(
            window.title.map(|title| title.chars().count()),
            Some(TITLE_CAP)
        );
    }
}
