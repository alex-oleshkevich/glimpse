use crate::event::{Event, Resync};
use crate::model::{Cast, CastKind, CastTarget, WindowId};

pub struct EventState {
    active_casts: usize,
}

impl EventState {
    pub fn new() -> Self {
        Self { active_casts: 0 }
    }

    pub fn is_layout_event(line: &str) -> bool {
        line.starts_with("activelayout>>")
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
            "activelayout" => Vec::new(),
            "screencast" => self.screencast(payload),

            "openwindow" | "movewindowv2" | "windowtitlev2" | "changefloatingmode"
            | "fullscreen" | "workspacev2" | "createworkspacev2" | "destroyworkspacev2"
            | "moveworkspacev2" | "renameworkspace" | "focusedmonv2" | "activespecialv2" => {
                vec![Event::Resync(Resync::Structure)]
            }

            "monitoraddedv2" | "monitorremovedv2" => vec![Event::Resync(Resync::Outputs)],
            "configreloaded" => vec![Event::Resync(Resync::Keyboard)],

            _ => {
                tracing::debug!(event = name, "ignored an unhandled hyprland event");
                Vec::new()
            }
        }
    }

    fn screencast(&mut self, payload: &str) -> Vec<Event> {
        match payload.split(',').next() {
            Some("1") => self.active_casts = self.active_casts.saturating_add(1),
            Some("0") => self.active_casts = self.active_casts.saturating_sub(1),
            _ => return Vec::new(),
        }
        let casts = match self.active_casts > 0 {
            true => vec![Cast {
                stream_id: 0,
                session_id: None,
                kind: CastKind::Unknown,
                target: CastTarget::Unknown,
                pw_node_id: None,
                active: true,
            }],
            false => Vec::new(),
        };
        vec![Event::CastsChanged(casts)]
    }
}

pub(crate) fn address(text: &str) -> Option<WindowId> {
    let text = text.trim();
    let digits = text.strip_prefix("0x").unwrap_or(text);
    u64::from_str_radix(digits, 16)
        .ok()
        .filter(|address| *address != 0)
        .map(WindowId)
}
