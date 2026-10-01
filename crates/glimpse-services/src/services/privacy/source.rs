use std::collections::HashMap;
use std::path::Path;
use std::pin::Pin;

use futures_util::{Stream, StreamExt, stream};
use glimpse_dbus::geoclue::GeoClueManagerProxy;
use glimpse_utils::clean;

use crate::context::Ctx;
use crate::services::audio::{App, AudioState, Direction};
use crate::services::compositor::{CastInfo, CastKindInfo, CastTargetInfo, CompositorPrivacy};

use super::{Event, Fact, NAME_CAP, Privacy, Resource};

const PROC_ROOT: &str = "/proc";
const BLOCKED_APP_IDS: [&str; 2] = ["org.gnome.VolumeControl", "org.PulseAudio.pavucontrol"];

type Events = Pin<Box<dyn Stream<Item = Event> + Send>>;

fn location_unavailable(reason: &'static str) -> Events {
    Box::pin(stream::once(async move {
        Event::Unavailable(Resource::Location, reason)
    }))
}

pub async fn scan_camera() -> Event {
    match tokio::task::spawn_blocking(|| scan(Path::new(PROC_ROOT))).await {
        Ok(Ok(facts)) => Event::Camera(facts),
        Ok(Err(reason)) => Event::Unavailable(Resource::Camera, reason),
        Err(_) => Event::Unavailable(Resource::Camera, "camera: the scan task did not complete"),
    }
}

fn scan(root: &Path) -> Result<Vec<Fact>, &'static str> {
    let modules = std::fs::read_to_string(root.join("modules"))
        .map_err(|_| "camera: /proc/modules is unreadable")?;
    let refcount = uvcvideo_refcount(&modules).ok_or(
        "camera: uvcvideo is not loaded, so camera activity cannot be observed on this hardware",
    )?;
    if refcount == 0 {
        return Ok(Vec::new());
    }
    let holders = holders(root);
    if holders.is_empty() {
        unreadable_holder();
    }
    Ok(holders)
}

fn unreadable_holder() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        tracing::warn!(
            "uvcvideo reports the camera in use but no readable process holds /dev/video*; the \
             holder belongs to another user, or this process has a private /proc that hides fd \
             links — systemd's ProtectKernelTunables, ProtectKernelModules and \
             ProtectControlGroups each imply MountAPIVFS and cause exactly that"
        );
    });
}

fn uvcvideo_refcount(modules: &str) -> Option<u32> {
    modules
        .lines()
        .find(|line| line.split_whitespace().next() == Some("uvcvideo"))
        .and_then(|line| line.split_whitespace().nth(2))
        .and_then(|field| field.parse().ok())
}

fn holders(root: &Path) -> Vec<Fact> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut apps: Vec<Option<String>> = Vec::new();
    for entry in entries.flatten() {
        let is_pid_dir = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.parse::<u32>().is_ok());
        if !is_pid_dir {
            continue;
        }
        if !holds_camera(&entry.path()) {
            continue;
        }
        let Ok(comm) = std::fs::read_to_string(entry.path().join("comm")) else {
            continue;
        };
        let app = app_from_comm(&comm);
        if !apps.contains(&app) {
            apps.push(app);
        }
    }

    apps.into_iter()
        .map(|app| Fact {
            app,
            icon: None,
            detail: None,
            stream_id: None,
            session: None,
        })
        .collect()
}

fn holds_camera(pid_dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(pid_dir.join("fd")) else {
        return false;
    };
    entries.flatten().any(|entry| {
        std::fs::read_link(entry.path())
            .map(|target| target.to_string_lossy().starts_with("/dev/video"))
            .unwrap_or(false)
    })
}

fn app_from_comm(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "pipewire" || trimmed == "wireplumber" {
        None
    } else {
        Some(clean(trimmed, NAME_CAP))
    }
}

pub fn microphone(state: &AudioState) -> Vec<Fact> {
    state
        .apps
        .iter()
        .filter_map(|app| {
            let role = app.capture.as_ref()?;
            if role.corked {
                return None;
            }
            let captures_a_real_source = role
                .streams
                .iter()
                .any(|stream| state.device(Direction::Input, &stream.device).is_some());
            if !captures_a_real_source {
                return None;
            }
            if app
                .app_id
                .as_deref()
                .is_some_and(|id| BLOCKED_APP_IDS.contains(&id))
            {
                return None;
            }

            Some(Fact {
                app: display_name(app),
                icon: app.icon_name.as_deref().map(|icon| clean(icon, NAME_CAP)),
                detail: None,
                stream_id: None,
                session: None,
            })
        })
        .collect()
}

fn display_name(app: &App) -> Option<String> {
    let stripped = app.name.strip_suffix(" input").unwrap_or(&app.name);
    let raw = if stripped.is_empty() {
        app.binary.as_deref().unwrap_or_default()
    } else {
        stripped
    };
    Some(clean(raw, NAME_CAP)).filter(|name| !name.is_empty())
}

pub fn screen(privacy: &CompositorPrivacy, attribution: &HashMap<u32, String>) -> Vec<Fact> {
    privacy
        .casts
        .iter()
        .filter(|cast| cast.active)
        .map(|cast| Fact {
            app: screen_app(cast, attribution),
            icon: None,
            detail: match &cast.target {
                CastTargetInfo::Output(name) => Some(clean(name, NAME_CAP)),
                CastTargetInfo::Window(_) | CastTargetInfo::Unknown => None,
            },
            stream_id: Some(cast.stream_id),
            session: match cast.kind {
                CastKindInfo::PipeWire => cast.session_id,
                CastKindInfo::Screencopy | CastKindInfo::Unknown => None,
            },
        })
        .collect()
}

fn screen_app(cast: &CastInfo, attribution: &HashMap<u32, String>) -> Option<String> {
    if cast.kind != CastKindInfo::PipeWire {
        return None;
    }
    let name = attribution.get(&cast.pw_node_id?)?;
    Some(clean(name, NAME_CAP))
}

pub async fn location(ctx: Ctx<Privacy>) -> Events {
    let Ok(bus) = ctx.system_bus().cloned() else {
        return location_unavailable("location: no system bus");
    };
    let manager = match GeoClueManagerProxy::new(&bus).await {
        Ok(manager) => manager,
        Err(_) => return location_unavailable("location: GeoClue is unavailable"),
    };

    let changes = manager.receive_in_use_changed().await;
    let known = manager.in_use().await.unwrap_or(false);

    let first = stream::once(async move { Event::Location(known) });
    let following =
        changes.then(|change| async move { Event::Location(change.get().await.unwrap_or(false)) });

    Box::pin(first.chain(following))
}

#[cfg(test)]
mod tests;
