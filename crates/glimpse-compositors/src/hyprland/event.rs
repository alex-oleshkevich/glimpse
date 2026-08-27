use crate::event::{Event, Resync};
use crate::keyboard::layout_code;
use crate::model::WindowId;

/// Hyprland says an address changed and little else, so the configured layout codes are cached here
/// to turn `activelayout`'s display name back into an index. They are read once when the stream
/// opens, because Hyprland has no event that carries them.
pub struct EventState {
    layout_codes: Vec<String>,
}

impl EventState {
    pub fn new(layout_codes: Vec<String>) -> Self {
        Self { layout_codes }
    }

    pub fn decode(&mut self, line: &str) -> Vec<Event> {
        let Some((name, payload)) = line.split_once(">>") else {
            tracing::debug!(line, "ignored a hyprland line with no `>>`");
            return Vec::new();
        };

        match name {
            "activewindowv2" => vec![Event::WindowFocusChanged(address(payload))],
            "closewindow" => address(payload)
                .map(Event::WindowClosed)
                .into_iter()
                .collect(),
            "urgent" => address(payload)
                .map(|id| Event::WindowUrgencyChanged { id, urgent: true })
                .into_iter()
                .collect(),
            "activelayout" => self.layout_switched(payload),

            // Everything below carries an address or a name but never the whole record, and
            // rebuilding a `Window` or `Workspace` from a partial line would publish a worse
            // version of what one re-fetch supplies correctly.
            "openwindow" | "movewindowv2" | "windowtitlev2" | "windowtitle"
            | "changefloatingmode" | "fullscreen" | "workspacev2" | "createworkspacev2"
            | "destroyworkspacev2" | "moveworkspacev2" | "renameworkspace" | "focusedmonv2"
            | "activespecialv2" => vec![Event::Resync(Resync::Structure)],

            "monitoraddedv2" | "monitorremovedv2" => vec![Event::Resync(Resync::Outputs)],
            "configreloaded" => vec![Event::Resync(Resync::Keyboard)],

            _ => {
                tracing::debug!(event = name, "ignored an unhandled hyprland event");
                Vec::new()
            }
        }
    }

    fn layout_switched(&self, payload: &str) -> Vec<Event> {
        // `activelayout>>keyboard-name,Layout Display Name` — and the keyboard name may itself
        // contain a comma, so the layout is everything after the last one.
        let Some((_keyboard, keymap)) = payload.rsplit_once(',') else {
            return Vec::new();
        };
        let Some(idx) = layout_index(&self.layout_codes, keymap) else {
            tracing::debug!(keymap, "hyprland reported a layout that is not configured");
            return Vec::new();
        };

        vec![Event::KeyboardLayoutSwitched {
            idx,
            name: Some(keymap.to_owned()),
        }]
    }
}

/// Hyprland writes addresses bare in events (`5591e8b2f5a0`) and prefixed in JSON (`0x5591e8b2f5a0`).
pub(crate) fn address(text: &str) -> Option<WindowId> {
    let text = text.trim();
    let digits = text.strip_prefix("0x").unwrap_or(text);
    // Hyprland writes `0x0` where it means "none", and no real window lives at address zero.
    u64::from_str_radix(digits, 16)
        .ok()
        .filter(|address| *address != 0)
        .map(WindowId)
}

/// Hyprland's configured layouts are xkb codes (`pl`) while the active one is a description
/// (`Polish`), so the two have to be reconciled from both directions.
pub(crate) fn layout_index(codes: &[String], active_keymap: &str) -> Option<usize> {
    codes.iter().position(|code| matches(code, active_keymap))
}

fn matches(code: &str, active_keymap: &str) -> bool {
    let code = code.trim().to_lowercase();
    let keymap = active_keymap.trim();

    code == keymap.to_lowercase()
        || parenthesized(keymap).is_some_and(|inner| code == inner.to_lowercase())
        || code == layout_code(keymap).to_lowercase()
}

fn parenthesized(value: &str) -> Option<&str> {
    let (_, rest) = value.split_once('(')?;
    let (inner, _) = rest.split_once(')')?;
    let inner = inner.trim();

    (!inner.is_empty()).then_some(inner)
}
