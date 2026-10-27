use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use futures_util::{Stream, StreamExt, stream};
use glimpse_dbus::geoclue::GeoClueManagerProxy;
use glimpse_utils::clean;
use inotify::{EventMask, Inotify, WatchDescriptor, WatchMask};
use tokio::time::Instant;

use crate::context::Ctx;
use crate::services::audio::{App, AudioState, Direction};
use crate::services::compositor::{CastInfo, CompositorPrivacy};

use super::{Event, Fact, NAME_CAP, Privacy, Resource};

const BLOCKED_APP_IDS: [&str; 2] = ["org.gnome.VolumeControl", "org.PulseAudio.pavucontrol"];

type Events = Pin<Box<dyn Stream<Item = Event> + Send>>;

fn location_unavailable(reason: &'static str) -> Events {
    Box::pin(stream::once(async move {
        Event::Unavailable(Resource::Location, reason)
    }))
}

pub fn camera(dev_root: PathBuf, proc_root: PathBuf) -> Events {
    Box::pin(
        stream::once(async move {
            match CameraSource::new(dev_root, proc_root) {
                Ok(source) => Box::pin(stream::unfold(source, |mut source| async move {
                    let event = source.next().await;
                    event.map(|event| (event, source))
                })) as Events,
                Err(error) => {
                    tracing::warn!(%error, "camera watches unavailable");
                    Box::pin(stream::once(async {
                        Event::Unavailable(Resource::Camera, "camera: inotify is unavailable")
                    })) as Events
                }
            }
        })
        .flatten(),
    )
}

#[derive(Default)]
struct CameraSchedule {
    pending: bool,
    close_at: Option<Instant>,
    generation: u64,
}

impl CameraSchedule {
    fn event(&mut self, mask: EventMask) {
        if mask.intersects(EventMask::OPEN | EventMask::Q_OVERFLOW) {
            self.immediate();
        } else if mask.intersects(EventMask::CLOSE_WRITE | EventMask::CLOSE_NOWRITE) {
            self.close_at = Some(Instant::now() + Duration::from_millis(500));
        }
    }

    fn immediate(&mut self) {
        self.pending = true;
        self.close_at = None;
        self.generation = self.generation.wrapping_add(1);
    }

    fn publish(&self, generation: u64, facts: &[Fact]) -> bool {
        !facts.is_empty() || (generation == self.generation && self.close_at.is_none())
    }
}

struct CameraSource {
    events: inotify::EventStream<[u8; 4096]>,
    hotplug: Pin<Box<dyn Stream<Item = ()> + Send>>,
    dev_root: PathBuf,
    proc_root: PathBuf,
    watches: HashMap<PathBuf, WatchDescriptor>,
    departed: HashSet<WatchDescriptor>,
    logged: HashSet<PathBuf>,
    unavailable: bool,
    schedule: CameraSchedule,
    scan: Option<tokio::task::JoinHandle<Vec<Fact>>>,
    scan_generation: u64,
}

impl CameraSource {
    fn new(dev_root: PathBuf, proc_root: PathBuf) -> std::io::Result<Self> {
        let hotplug: Pin<Box<dyn Stream<Item = ()> + Send>> =
            match tokio_udev::MonitorBuilder::new()
                .and_then(|builder| builder.match_subsystem("video4linux"))
                .and_then(|builder| builder.listen())
                .and_then(tokio_udev::AsyncMonitorSocket::new)
            {
                Ok(socket) => Box::pin(
                    socket
                        .filter_map(|event| async move {
                            match event {
                                Ok(_) => Some(()),
                                Err(error) => {
                                    tracing::warn!(%error, "camera hotplug event failed");
                                    None
                                }
                            }
                        })
                        .chain(stream::pending()),
                ),
                Err(error) => {
                    tracing::warn!(%error, "no udev camera monitor; hotplug disabled");
                    Box::pin(stream::pending())
                }
            };
        let mut source = Self {
            events: Inotify::init()?.into_event_stream([0; 4096])?,
            hotplug,
            dev_root,
            proc_root,
            watches: HashMap::new(),
            departed: HashSet::new(),
            logged: HashSet::new(),
            unavailable: false,
            schedule: CameraSchedule::default(),
            scan: None,
            scan_generation: 0,
        };
        source.rearm();
        Ok(source)
    }

    fn rearm(&mut self) {
        let entries = match std::fs::read_dir(&self.dev_root) {
            Ok(entries) => entries,
            Err(error) => {
                tracing::warn!(%error, path = %self.dev_root.display(), "cannot enumerate cameras");
                self.unavailable = true;
                self.schedule.immediate();
                return;
            }
        };
        let nodes: HashSet<_> = entries
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("video"))
            .map(|entry| entry.path())
            .collect();
        let gone: Vec<_> = self
            .watches
            .keys()
            .filter(|path| !nodes.contains(*path))
            .cloned()
            .collect();
        for path in gone {
            if let Some(watch) = self.watches.remove(&path) {
                self.departed.insert(watch);
            }
        }
        for path in &nodes {
            if self.watches.contains_key(path) {
                continue;
            }
            match self.events.watches().add(
                path,
                WatchMask::OPEN | WatchMask::CLOSE_WRITE | WatchMask::CLOSE_NOWRITE,
            ) {
                Ok(watch) => {
                    self.watches.insert(path.clone(), watch);
                }
                Err(error) => {
                    if self.logged.insert(path.clone()) {
                        tracing::warn!(%error, path = %path.display(), "cannot watch camera");
                    }
                }
            }
        }
        self.unavailable = !nodes.is_empty() && self.watches.is_empty();
        self.schedule.immediate();
    }

    async fn next(&mut self) -> Option<Event> {
        loop {
            if self.scan.is_none() && self.schedule.pending {
                self.schedule.pending = false;
                if self.unavailable {
                    return Some(Event::Unavailable(
                        Resource::Camera,
                        "camera: no camera nodes can be watched",
                    ));
                }
                let root = self.proc_root.clone();
                let has_nodes = !self.watches.is_empty() || !self.departed.is_empty();
                self.scan_generation = self.schedule.generation;
                self.scan = Some(tokio::task::spawn_blocking(move || {
                    if has_nodes {
                        holders(&root)
                    } else {
                        Vec::new()
                    }
                }));
            }
            let close_at = self.schedule.close_at;
            tokio::select! {
                biased;
                event = self.events.next() => {
                    match event {
                        Some(Ok(event)) => {
                            if event.mask.intersects(EventMask::IGNORED) {
                                self.departed.remove(&event.wd);
                                let wd = event.wd;
                                self.watches.retain(|_, watch| watch != &wd);
                                self.schedule.immediate();
                            } else {
                                self.schedule.event(event.mask);
                            }
                        }
                        Some(Err(error)) => {
                            tracing::warn!(%error, "camera event stream failed");
                            return Some(Event::Unavailable(Resource::Camera, "camera: inotify event stream failed"));
                        }
                        None => return None,
                    }
                }
                _ = self.hotplug.next() => self.rearm(),
                _ = async { tokio::time::sleep_until(close_at.unwrap_or_else(Instant::now)).await }, if close_at.is_some() => {
                    self.schedule.close_at = None;
                    self.schedule.pending = true;
                }
                result = async { match self.scan.as_mut() {
                    Some(scan) => scan.await,
                    None => std::future::pending().await,
                }} => {
                    self.scan = None;
                    match result {
                        Ok(facts) if self.schedule.publish(self.scan_generation, &facts) => return Some(Event::Camera(facts)),
                        Ok(_) => {}
                        Err(_) => return Some(Event::Unavailable(Resource::Camera, "camera: the scan task did not complete")),
                    }
                }
            }
        }
    }
}

impl Drop for CameraSource {
    fn drop(&mut self) {
        if let Some(scan) = &self.scan {
            scan.abort();
        }
    }
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
            detail: cast.output.as_deref().map(|name| clean(name, NAME_CAP)),
            stream_id: Some(cast.stream_id),
            session: cast.session_id,
        })
        .collect()
}

fn screen_app(cast: &CastInfo, attribution: &HashMap<u32, String>) -> Option<String> {
    cast.session_id?;
    let name = attribution.get(&cast.pw_node_id?)?;
    Some(clean(name, NAME_CAP))
}

pub async fn location(ctx: Ctx<Privacy>) -> Events {
    let Ok(bus) = ctx.system_bus().cloned() else {
        return location_unavailable("location: no system bus");
    };
    let manager = match GeoClueManagerProxy::builder(&bus)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
    {
        Ok(manager) => manager,
        Err(_) => return location_unavailable("location: GeoClue is unavailable"),
    };
    let properties = match zbus::fdo::PropertiesProxy::builder(&bus)
        .destination(manager.inner().destination().to_owned())
        .and_then(|builder| builder.path(manager.inner().path().to_owned()))
    {
        Ok(builder) => builder.build().await,
        Err(error) => Err(error),
    };
    let Ok(properties) = properties else {
        return location_unavailable("location: GeoClue is unavailable");
    };
    let (Ok(owners), Ok(changes)) = (
        manager
            .inner()
            .receive_owner_changed()
            .await
            .map(glimpse_dbus::wake),
        properties
            .receive_properties_changed()
            .await
            .map(glimpse_dbus::wake),
    ) else {
        return location_unavailable("location: GeoClue is unavailable");
    };

    let wakes = stream::select(owners, changes);
    let reads = stream::once(async {}).chain(wakes);
    Box::pin(reads.then(move |()| {
        let manager = manager.clone();
        async move { Event::Location(manager.in_use().await.unwrap_or(false)) }
    }))
}

#[cfg(test)]
mod tests;
