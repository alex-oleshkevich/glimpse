use serde::{Deserialize, Deserializer};

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
    #[serde(default)]
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

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Window {
    pub id: WindowId,
    #[serde(default, deserialize_with = "capped_title")]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "capped_app_id")]
    pub app_id: Option<String>,
    #[serde(default)]
    pub pid: Option<i32>,
    #[serde(default)]
    pub workspace_id: Option<WorkspaceId>,
    #[serde(default)]
    pub is_focused: bool,
    #[serde(default)]
    pub is_floating: bool,
    #[serde(default)]
    pub is_urgent: bool,
    /// Where the compositor places this window among its peers. Niri's scrolling column index;
    /// Hyprland has no equivalent and leaves it `None`.
    #[serde(rename = "layout", default, deserialize_with = "scrolling_column")]
    pub layout_order: Option<u16>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub connector: String,
    pub make: Option<String>,
    pub model: Option<String>,
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub outputs: Vec<Output>,
    pub workspaces: Vec<Workspace>,
    pub windows: Vec<Window>,
    pub keyboard: KeyboardLayouts,
    pub focused_window: Option<WindowId>,
    pub focused_output: Option<String>,
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

/// Hyprland sends an absent title or class as `""` where niri sends `null`, so both have to
/// arrive as `None` or a caller sees a difference that is not about the window.
pub(crate) fn capped_title_str(text: &str) -> Option<String> {
    non_empty(sanitize(text, TITLE_CAP))
}

pub(crate) fn capped_app_id_str(text: &str) -> Option<String> {
    non_empty(sanitize(text, APP_ID_CAP))
}

fn non_empty(text: String) -> Option<String> {
    (!text.is_empty()).then_some(text)
}

pub(crate) fn is_built_in(connector: &str) -> bool {
    let connector = connector.to_ascii_lowercase();
    connector.starts_with("edp-") || connector.starts_with("lvds-")
}

/// Niri nests the scrolling position under `layout.pos_in_scrolling_layout` as `[column, row]`, and
/// leaves it null for a window that is not in the scrolling layout at all.
fn scrolling_column<'de, D: Deserializer<'de>>(input: D) -> Result<Option<u16>, D::Error> {
    #[derive(Deserialize)]
    struct Layout {
        #[serde(default)]
        pos_in_scrolling_layout: Option<(u16, u16)>,
    }

    Ok(Option::<Layout>::deserialize(input)?
        .and_then(|layout| layout.pos_in_scrolling_layout)
        .map(|(column, _row)| column))
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
    fn internal_panels_are_recognized_by_connector() {
        assert!(is_built_in("eDP-1"));
        assert!(is_built_in("LVDS-1"));
        assert!(!is_built_in("DP-3"));
        assert!(!is_built_in("HDMI-A-1"));
    }
}
