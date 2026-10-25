use std::collections::HashSet;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use glimpse_compositors::event::{Event, Resync};
use glimpse_compositors::model::{Cast, WindowId};
use glimpse_compositors::sanitize::{TITLE_CAP, capped_name_str, sanitize};
use tokio::sync::Notify;
use tokio::time::Instant;

const MONITOR: &str = "monitor";
const REGION: &str = "region";
const WINDOW: &str = "window";
pub(crate) const GRACE: Duration = Duration::from_secs(2);

#[derive(Debug, Default)]
pub struct Shared {
    shares: Mutex<Shares>,
    urgent: Mutex<HashSet<WindowId>>,
    pub stopped: Notify,
}

#[derive(Debug, Default)]
struct Shares {
    next_cast: u64,
    list: Vec<Share>,
}

#[derive(Debug)]
struct Share {
    kind: String,
    name: String,
    cast: Cast,
    stopping: Option<Instant>,
}

impl Shared {
    pub fn casts(&self) -> Vec<Cast> {
        self.shares()
            .list
            .iter()
            .map(|share| share.cast.clone())
            .collect()
    }

    pub fn expire(&self) -> Option<Vec<Cast>> {
        let now = Instant::now();
        let mut shares = self.shares();
        let before = shares.list.len();
        shares
            .list
            .retain(|share| share.stopping.is_none_or(|at| at > now));
        (shares.list.len() < before)
            .then(|| shares.list.iter().map(|share| share.cast.clone()).collect())
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.shares()
            .list
            .iter()
            .filter_map(|share| share.stopping)
            .min()
    }

    pub fn is_urgent(&self, id: WindowId) -> bool {
        self.urgent().contains(&id)
    }

    fn shares(&self) -> MutexGuard<'_, Shares> {
        self.shares.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn urgent(&self) -> MutexGuard<'_, HashSet<WindowId>> {
        self.urgent.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub fn decode(shared: &Shared, line: &str) -> Vec<Event> {
    let Some((name, payload)) = line.split_once(">>") else {
        tracing::debug!(line, "ignored a hyprland line with no `>>`");
        return Vec::new();
    };

    match name {
        "activewindowv2" => {
            let id = address(payload);
            if let Some(id) = id {
                shared.urgent().remove(&id);
            }
            vec![Event::WindowFocusChanged(id)]
        }
        "closewindow" => address(payload)
            .map(|id| {
                shared.urgent().remove(&id);
                Event::WindowClosed(id)
            })
            .into_iter()
            .collect(),
        "urgent" => address(payload)
            .map(|id| {
                shared.urgent().insert(id);
                Event::WindowUrgencyChanged { id, urgent: true }
            })
            .into_iter()
            .collect(),
        "screencastv2" => screencast(shared, payload),

        "openwindow" | "movewindowv2" | "windowtitlev2" | "changefloatingmode" | "fullscreen"
        | "workspacev2" | "createworkspacev2" | "destroyworkspacev2" | "moveworkspacev2"
        | "renameworkspace" | "focusedmonv2" | "activespecialv2" => {
            vec![Event::Resync(Resync::Structure)]
        }

        "monitoraddedv2" | "monitorremovedv2" => vec![Event::Resync(Resync::Outputs)],
        "configreloaded" => vec![
            Event::Resync(Resync::Outputs),
            Event::Resync(Resync::Keyboard),
        ],

        _ => {
            tracing::debug!(event = name, "ignored an unhandled hyprland event");
            Vec::new()
        }
    }
}

fn screencast(shared: &Shared, payload: &str) -> Vec<Event> {
    let mut fields = payload.splitn(3, ',');
    let (Some(state), Some(kind), Some(name)) = (fields.next(), fields.next(), fields.next())
    else {
        return Vec::new();
    };
    let (kind, name) = (kind.to_owned(), sanitize(name, TITLE_CAP));
    if !matches!(kind.as_str(), MONITOR | REGION | WINDOW) {
        return Vec::new();
    }
    let mut shares = shared.shares();
    match state {
        "1" => {
            if let Some(share) = shares
                .list
                .iter_mut()
                .find(|share| share.kind == kind && share.name == name && share.stopping.is_some())
            {
                share.stopping = None;
                return Vec::new();
            }
            let output = (kind == MONITOR || kind == REGION)
                .then(|| capped_name_str(&name))
                .flatten();
            let id = shares.next_cast;
            shares.next_cast += 1;
            shares.list.push(Share {
                cast: Cast {
                    id,
                    active: true,
                    output,
                    session: None,
                    pw_node: None,
                },
                kind,
                name,
                stopping: None,
            });
        }
        "0" => {
            let at = shares
                .list
                .iter()
                .position(|share| {
                    share.kind == kind && share.name == name && share.stopping.is_none()
                })
                .or_else(|| {
                    shares
                        .list
                        .iter()
                        .position(|share| share.kind == kind && share.stopping.is_none())
                });
            if let Some(at) = at {
                shares.list[at].stopping = Some(Instant::now() + GRACE);
                shared.stopped.notify_one();
            }
            return Vec::new();
        }
        _ => return Vec::new(),
    }
    vec![Event::CastsChanged(
        shares.list.iter().map(|share| share.cast.clone()).collect(),
    )]
}

pub fn address(text: &str) -> Option<WindowId> {
    let text = text.trim();
    let digits = text.strip_prefix("0x").unwrap_or(text);
    u64::from_str_radix(digits, 16)
        .ok()
        .filter(|address| *address != 0)
        .map(WindowId)
}
