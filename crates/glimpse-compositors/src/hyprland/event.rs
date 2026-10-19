use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::event::{Event, Resync};
use crate::model::{Cast, CastKind, CastTarget, WindowId};

#[derive(Debug, Default)]
pub struct Shared {
    casts: AtomicUsize,
    urgent: Mutex<HashSet<WindowId>>,
}

impl Shared {
    pub fn reset(&self) {
        self.casts.store(0, Ordering::Relaxed);
        self.urgent().clear();
    }

    pub fn casts(&self) -> Vec<Cast> {
        match self.casts.load(Ordering::Relaxed) > 0 {
            true => vec![Cast {
                stream_id: 0,
                session_id: None,
                kind: CastKind::Unknown,
                target: CastTarget::Unknown,
                pw_node_id: None,
                active: true,
            }],
            false => Vec::new(),
        }
    }

    pub fn is_urgent(&self, id: WindowId) -> bool {
        self.urgent().contains(&id)
    }

    fn urgent(&self) -> MutexGuard<'_, HashSet<WindowId>> {
        self.urgent.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub struct EventState {
    shared: Arc<Shared>,
}

impl EventState {
    pub fn new(shared: Arc<Shared>) -> Self {
        Self { shared }
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
            "activewindowv2" => {
                let id = address(payload);
                if let Some(id) = id {
                    self.shared.urgent().remove(&id);
                }
                vec![Event::WindowFocusChanged(id)]
            }
            "closewindow" => address(payload)
                .map(|id| {
                    self.shared.urgent().remove(&id);
                    Event::WindowClosed(id)
                })
                .into_iter()
                .collect(),
            "urgent" => address(payload)
                .map(|id| {
                    self.shared.urgent().insert(id);
                    Event::WindowUrgencyChanged { id, urgent: true }
                })
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
        let update = match payload.split(',').next() {
            Some("1") => |count: usize| Some(count.saturating_add(1)),
            Some("0") => |count: usize| Some(count.saturating_sub(1)),
            _ => return Vec::new(),
        };
        let _ = self
            .shared
            .casts
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, update);
        vec![Event::CastsChanged(self.shared.casts())]
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
