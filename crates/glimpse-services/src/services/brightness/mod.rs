mod composite;
mod ddc;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::future::BoxFuture;
use futures_util::{Stream, StreamExt, stream};
use glimpse_dbus::Buses;
use glimpse_dbus::login1::{self, Login1SessionProxy};
use glimpse_dbus::upower;
use tokio::sync::oneshot;
use zbus::zvariant::OwnedObjectPath;

pub use composite::CompositeBacklight;
pub use ddc::DdcBacklight;

use super::{cap, say};
use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, Service, ServiceEndpoint},
    subscription::Sub,
};

const BACKLIGHT_ROOT: &str = "/sys/class/backlight";
const LEDS_ROOT: &str = "/sys/class/leds";
const KEYBOARD_ID: &str = "keyboard";
const LABEL_CAP: usize = 64;
const REPROBE_DELAYS: [Duration; 5] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
    Duration::from_secs(16),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub device_link: Option<String>,
    pub type_name: String,
    pub brightness: u32,
    pub max_brightness: u32,
    pub kind: Kind,
}

impl Entry {
    pub fn keyboard(id: impl Into<String>, brightness: u32, max_brightness: u32) -> Self {
        Self {
            id: id.into(),
            device_link: None,
            type_name: String::new(),
            brightness,
            max_brightness,
            kind: Kind::Keyboard,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Display,
    Keyboard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub id: String,
    pub kind: Kind,
    pub connector: Option<String>,
    pub current: u32,
    pub max: u32,
    pub floor: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BrightnessState {
    pub sources: Vec<Source>,
}

#[derive(Debug, Default)]
pub struct Enumeration {
    pub entries: Vec<Entry>,
    pub unanswered: Vec<String>,
}

impl From<Vec<Entry>> for Enumeration {
    fn from(entries: Vec<Entry>) -> Self {
        Self {
            entries,
            unanswered: Vec::new(),
        }
    }
}

pub enum ReadOutcome {
    Found(Entry),
    Gone,
    Unavailable,
}

pub trait Backlight: Send + Sync + 'static {
    fn enumerate(&self) -> BoxFuture<'_, Enumeration>;
    fn read(&self, id: String) -> BoxFuture<'_, ReadOutcome>;
    fn write(&self, id: String, value: u32) -> BoxFuture<'_, Result<(), String>>;
    fn reprobe(&self, _connector: String) -> BoxFuture<'_, Option<Entry>> {
        Box::pin(async { None })
    }
}

fn rescan_event(id: String, outcome: ReadOutcome) -> Option<Event> {
    match outcome {
        ReadOutcome::Found(entry) => Some(Event::Rescanned {
            id,
            entry: Some(entry),
        }),
        ReadOutcome::Gone => Some(Event::Rescanned { id, entry: None }),
        ReadOutcome::Unavailable => None,
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub struct FakeBacklight {
    record: Arc<Mutex<FakeRecord>>,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct FakeRecord {
    entries: Vec<Entry>,
    writes: Vec<(String, u32)>,
    failure: Option<String>,
    late: Option<(String, usize, Entry)>,
}

#[cfg(test)]
impl FakeBacklight {
    pub fn new(entries: Vec<Entry>) -> Self {
        Self {
            record: Arc::new(Mutex::new(FakeRecord {
                entries,
                writes: Vec::new(),
                failure: None,
                late: None,
            })),
        }
    }

    pub fn writes(&self) -> Vec<(String, u32)> {
        self.record().writes.clone()
    }

    pub fn fail(&self, reason: Option<&str>) {
        self.record().failure = reason.map(str::to_owned);
    }

    pub fn answer_late(&self, connector: &str, misses: usize, entry: Entry) {
        self.record().late = Some((connector.to_owned(), misses, entry));
    }

    pub fn set_brightness(&self, id: &str, brightness: u32) {
        let mut record = self.record();
        if let Some(entry) = record.entries.iter_mut().find(|entry| entry.id == id) {
            entry.brightness = brightness;
        }
    }

    fn record(&self) -> std::sync::MutexGuard<'_, FakeRecord> {
        self.record.lock().unwrap_or_else(|held| held.into_inner())
    }
}

#[cfg(test)]
impl Backlight for FakeBacklight {
    fn enumerate(&self) -> BoxFuture<'_, Enumeration> {
        let record = self.record();
        let enumeration = Enumeration {
            entries: record.entries.clone(),
            unanswered: record.late.iter().map(|(name, ..)| name.clone()).collect(),
        };
        Box::pin(async move { enumeration })
    }

    fn read(&self, id: String) -> BoxFuture<'_, ReadOutcome> {
        let entry = self
            .record()
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .cloned();
        Box::pin(async move {
            match entry {
                Some(entry) => ReadOutcome::Found(entry),
                None => ReadOutcome::Gone,
            }
        })
    }

    fn write(&self, id: String, value: u32) -> BoxFuture<'_, Result<(), String>> {
        let mut record = self.record();
        let outcome = match &record.failure {
            Some(reason) => Err(reason.clone()),
            None => {
                record.writes.push((id, value));
                Ok(())
            }
        };
        Box::pin(async move { outcome })
    }

    fn reprobe(&self, connector: String) -> BoxFuture<'_, Option<Entry>> {
        let answer = match &mut self.record().late {
            Some((name, 0, entry)) if *name == connector => Some(entry.clone()),
            Some((name, misses, _)) if *name == connector => {
                *misses -= 1;
                None
            }
            _ => None,
        };
        Box::pin(async move { answer })
    }
}

#[derive(Clone)]
struct KeyboardProxy {
    proxy: upower::UPowerKbdBacklightProxy<'static>,
    max_brightness: u32,
}

pub struct SysfsBacklight {
    buses: Buses,
    session: Mutex<Option<OwnedObjectPath>>,
    keyboard: Mutex<Option<KeyboardProxy>>,
}

impl SysfsBacklight {
    pub fn new(buses: Buses) -> Self {
        Self {
            buses,
            session: Mutex::new(None),
            keyboard: Mutex::new(None),
        }
    }

    fn cached_keyboard(&self) -> Option<KeyboardProxy> {
        self.keyboard
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .clone()
    }

    fn cache_keyboard(&self, keyboard: KeyboardProxy) {
        *self
            .keyboard
            .lock()
            .unwrap_or_else(|held| held.into_inner()) = Some(keyboard);
    }

    fn forget_keyboard(&self) {
        *self
            .keyboard
            .lock()
            .unwrap_or_else(|held| held.into_inner()) = None;
    }

    async fn keyboard(&self) -> zbus::Result<Option<KeyboardProxy>> {
        if let Some(keyboard) = self.cached_keyboard() {
            return Ok(Some(keyboard));
        }
        let Ok(bus) = self.buses.system_bus() else {
            return Ok(None);
        };
        let (_, proxy) = upower::kbd_backlight_proxy(bus).await?;
        let max_brightness = proxy.get_max_brightness().await?;
        if max_brightness <= 0 {
            return Ok(None);
        }
        let keyboard = KeyboardProxy {
            proxy,
            max_brightness: max_brightness as u32,
        };
        self.cache_keyboard(keyboard.clone());
        Ok(Some(keyboard))
    }

    async fn keyboard_outcome(&self) -> ReadOutcome {
        match self.keyboard().await {
            Ok(Some(keyboard)) => match keyboard.proxy.get_brightness().await {
                Ok(value) => ReadOutcome::Found(Entry::keyboard(
                    KEYBOARD_ID,
                    value.max(0) as u32,
                    keyboard.max_brightness,
                )),
                Err(_) => {
                    self.forget_keyboard();
                    ReadOutcome::Unavailable
                }
            },
            Ok(None) => match keyboard_led().await {
                Some(led) => ReadOutcome::Found(Entry::keyboard(
                    KEYBOARD_ID,
                    led.brightness,
                    led.max_brightness,
                )),
                None => ReadOutcome::Gone,
            },
            Err(_) => ReadOutcome::Unavailable,
        }
    }

    async fn keyboard_entry(&self) -> Option<Entry> {
        match self.keyboard_outcome().await {
            ReadOutcome::Found(entry) => Some(entry),
            ReadOutcome::Gone | ReadOutcome::Unavailable => None,
        }
    }

    fn cached_session(&self) -> Option<OwnedObjectPath> {
        self.session
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .clone()
    }

    fn cache_session(&self, path: OwnedObjectPath) {
        *self.session.lock().unwrap_or_else(|held| held.into_inner()) = Some(path);
    }

    fn forget_session(&self) {
        *self.session.lock().unwrap_or_else(|held| held.into_inner()) = None;
    }

    fn system_bus(&self) -> Result<&zbus::Connection, String> {
        self.buses
            .system_bus()
            .map_err(|reason| format!("no system bus: {reason}"))
    }

    async fn session(&self) -> Result<OwnedObjectPath, String> {
        match self.cached_session() {
            Some(path) => Ok(path),
            None => {
                let path = login1::session_path(self.system_bus()?).await?;
                self.cache_session(path.clone());
                Ok(path)
            }
        }
    }

    async fn write_backlight(&self, id: &str, value: u32) -> Result<(), String> {
        let path = self.session().await?;
        set_brightness_at(self.system_bus()?, &path, "backlight", id, value)
            .await
            .map_err(|error| {
                if is_stale_session(&error) {
                    self.forget_session();
                }
                say(error)
            })
    }

    async fn write_led(&self, name: &str, value: u32) -> Result<(), String> {
        let path = self.session().await?;
        set_brightness_at(self.system_bus()?, &path, "leds", name, value)
            .await
            .map_err(|error| {
                if is_stale_session(&error) {
                    self.forget_session();
                }
                say(error)
            })
    }

    async fn write_keyboard(&self, value: u32) -> Result<(), String> {
        match self.keyboard().await {
            Ok(Some(keyboard)) => match keyboard.proxy.set_brightness(value as i32).await {
                Ok(()) => Ok(()),
                Err(error) => {
                    self.forget_keyboard();
                    Err(say(error))
                }
            },
            Ok(None) | Err(_) => match keyboard_led().await {
                Some(led) => self.write_led(&led.name, value).await,
                None => Err("no keyboard backlight source is available".to_owned()),
            },
        }
    }
}

impl Backlight for SysfsBacklight {
    fn enumerate(&self) -> BoxFuture<'_, Enumeration> {
        Box::pin(async move {
            let mut entries = enumerate_sysfs().await;
            entries.extend(self.keyboard_entry().await);
            entries.into()
        })
    }

    fn read(&self, id: String) -> BoxFuture<'_, ReadOutcome> {
        if id == KEYBOARD_ID {
            return Box::pin(async move { self.keyboard_outcome().await });
        }
        Box::pin(async move {
            match read_entry(Path::new(BACKLIGHT_ROOT), &id).await {
                Some(entry) => ReadOutcome::Found(entry),
                None => ReadOutcome::Gone,
            }
        })
    }

    fn write(&self, id: String, value: u32) -> BoxFuture<'_, Result<(), String>> {
        if id == KEYBOARD_ID {
            return Box::pin(async move { self.write_keyboard(value).await });
        }
        Box::pin(async move { self.write_backlight(&id, value).await })
    }
}

async fn set_brightness_at(
    bus: &zbus::Connection,
    path: &OwnedObjectPath,
    subsystem: &str,
    id: &str,
    value: u32,
) -> zbus::Result<()> {
    let session = Login1SessionProxy::builder(bus)
        .path(path.clone())?
        .build()
        .await?;
    session.set_brightness(subsystem, id, value).await
}

struct LedEntry {
    name: String,
    brightness: u32,
    max_brightness: u32,
}

fn is_keyboard_backlight_led(name: &str) -> bool {
    name.rsplit(':').next() == Some("kbd_backlight")
}

async fn keyboard_led() -> Option<LedEntry> {
    select_keyboard_led(enumerate_leds().await)
}

fn select_keyboard_led(mut leds: Vec<LedEntry>) -> Option<LedEntry> {
    leds.sort_by(|a, b| a.name.cmp(&b.name));
    leds.into_iter().next()
}

async fn enumerate_leds() -> Vec<LedEntry> {
    let mut dir = match tokio::fs::read_dir(LEDS_ROOT).await {
        Ok(dir) => dir,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!(%error, root = LEDS_ROOT, "no LEDs to enumerate");
            return Vec::new();
        }
        Err(error) => {
            tracing::warn!(%error, root = LEDS_ROOT, "could not read the LED directory");
            return Vec::new();
        }
    };
    let mut entries = Vec::new();
    loop {
        let next = match dir.next_entry().await {
            Ok(Some(next)) => next,
            Ok(None) => break,
            Err(error) => {
                tracing::warn!(%error, "could not continue reading /sys/class/leds");
                break;
            }
        };
        let name = next.file_name().to_string_lossy().into_owned();
        if !is_keyboard_backlight_led(&name) {
            continue;
        }
        if let Some(entry) = read_led_at(next.path(), name).await {
            entries.push(entry);
        }
    }
    entries
}

async fn read_led_at(path: PathBuf, name: String) -> Option<LedEntry> {
    let brightness = read_number(&path.join("brightness")).await?;
    let max_brightness = read_number(&path.join("max_brightness")).await?;
    if max_brightness == 0 {
        return None;
    }
    Some(LedEntry {
        name,
        brightness,
        max_brightness,
    })
}

fn is_stale_session(error: &zbus::Error) -> bool {
    let zbus::Error::MethodError(name, _, _) = error else {
        return false;
    };
    matches!(
        name.as_str().rsplit('.').next().unwrap_or_default(),
        "UnknownObject" | "NoSuchSession" | "ServiceUnknown" | "NameHasNoOwner"
    )
}

async fn enumerate_sysfs() -> Vec<Entry> {
    let mut dir = match tokio::fs::read_dir(BACKLIGHT_ROOT).await {
        Ok(dir) => dir,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!(%error, root = BACKLIGHT_ROOT, "no backlight to enumerate");
            return Vec::new();
        }
        Err(error) => {
            tracing::warn!(%error, root = BACKLIGHT_ROOT, "could not read the backlight directory");
            return Vec::new();
        }
    };
    let mut entries = Vec::new();
    loop {
        let next = match dir.next_entry().await {
            Ok(Some(next)) => next,
            Ok(None) => break,
            Err(error) => {
                tracing::warn!(%error, "could not continue reading /sys/class/backlight");
                break;
            }
        };
        let id = next.file_name().to_string_lossy().into_owned();
        if id == KEYBOARD_ID {
            continue;
        }
        if let Some(entry) = read_entry_at(next.path(), id).await {
            entries.push(entry);
        }
    }
    entries
}

async fn read_entry(root: &Path, id: &str) -> Option<Entry> {
    read_entry_at(root.join(id), id.to_owned()).await
}

async fn read_entry_at(path: PathBuf, id: String) -> Option<Entry> {
    let brightness = read_number(&path.join("brightness")).await?;
    let max_brightness = read_number(&path.join("max_brightness")).await?;
    if max_brightness == 0 {
        return None;
    }
    let type_name = read_trimmed(&path.join("type")).await.unwrap_or_default();
    let device_link = tokio::fs::read_link(path.join("device"))
        .await
        .ok()
        .map(|target| target.to_string_lossy().into_owned());
    Some(Entry {
        id,
        device_link,
        type_name,
        brightness,
        max_brightness,
        kind: Kind::Display,
    })
}

async fn read_trimmed(path: &Path) -> Option<String> {
    tokio::fs::read_to_string(path)
        .await
        .ok()
        .map(|text| text.trim().to_owned())
}

async fn read_number(path: &Path) -> Option<u32> {
    read_trimmed(path).await?.parse().ok()
}

fn open_backlight_monitor() -> std::io::Result<tokio_udev::AsyncMonitorSocket> {
    tokio_udev::MonitorBuilder::new()?
        .match_subsystem("backlight")?
        .listen()
        .and_then(tokio_udev::AsyncMonitorSocket::new)
}

fn backlight_uevents(backend: Arc<dyn Backlight>) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
    match open_backlight_monitor() {
        Ok(socket) => Box::pin(rescans_from(socket, backend)),
        Err(error) => {
            tracing::info!(%error, "no udev backlight monitor; live updates disabled");
            Box::pin(stream::empty())
        }
    }
}

fn rescans_from(
    socket: tokio_udev::AsyncMonitorSocket,
    backend: Arc<dyn Backlight>,
) -> impl Stream<Item = Event> {
    socket
        .filter_map(|event| async move {
            match event {
                Ok(event) => Some(event),
                Err(error) => {
                    tracing::warn!(%error, "udev backlight monitor read failed");
                    None
                }
            }
        })
        .filter(|event| {
            futures_util::future::ready(event.event_type() == tokio_udev::EventType::Change)
        })
        .filter_map(|event| async move { event.sysname().to_str().map(str::to_owned) })
        .filter_map(move |id| {
            let backend = Arc::clone(&backend);
            async move {
                let outcome = backend.read(id.clone()).await;
                rescan_event(id, outcome)
            }
        })
}

/// `backlight` uevents only cover the class this crate already reads through sysfs; a monitor
/// changing ports, or being plugged or unplugged, is a `drm` subsystem event on the connector's
/// own device and reaches neither `SysfsBacklight` nor `DdcBacklight` any other way. Unlike a
/// `backlight` uevent, which names one device to re-read, `drm`'s `change` carries no reliable
/// per-connector routing here, so every one re-runs a full `enumerate` — which is what a source
/// appearing on a bus nothing has seen before actually needs, and which `Publisher::set`'s
/// equality gate keeps cheap when nothing on the resulting list has changed.
fn open_drm_monitor() -> std::io::Result<tokio_udev::AsyncMonitorSocket> {
    tokio_udev::MonitorBuilder::new()?
        .match_subsystem("drm")?
        .listen()
        .and_then(tokio_udev::AsyncMonitorSocket::new)
}

fn drm_hotplug_events(backend: Arc<dyn Backlight>) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
    match open_drm_monitor() {
        Ok(socket) => Box::pin(reenumerate_on_hotplug(socket, backend)),
        Err(error) => {
            tracing::info!(%error, "no udev drm monitor; DDC/CI hotplug updates disabled");
            Box::pin(stream::empty())
        }
    }
}

fn reenumerate_on_hotplug(
    socket: tokio_udev::AsyncMonitorSocket,
    backend: Arc<dyn Backlight>,
) -> impl Stream<Item = Event> {
    socket
        .filter_map(|event| async move {
            match event {
                Ok(event) => Some(event),
                Err(error) => {
                    tracing::warn!(%error, "udev drm monitor read failed");
                    None
                }
            }
        })
        .filter(|event| {
            futures_util::future::ready(event.event_type() == tokio_udev::EventType::Change)
        })
        .then(move |_event| {
            let backend = Arc::clone(&backend);
            async move { Event::Enumerated(backend.enumerate().await) }
        })
}

async fn keyboard_watch(
    ctx: Ctx<Brightness>,
    backend: Arc<dyn Backlight>,
) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
    let mut appearances = upower_owner_changes(&ctx).await;
    let mut waited = false;
    loop {
        match keyboard_change_events(&ctx).await {
            Ok(events) if waited => {
                let enumerated = Event::Enumerated(backend.enumerate().await);
                return Box::pin(stream::once(async move { enumerated }).chain(events));
            }
            Ok(events) => return Box::pin(events),
            Err(error) => {
                tracing::debug!(%error, "no keyboard backlight signal to follow");
                let Some(owners) = appearances.as_mut() else {
                    return Box::pin(stream::empty());
                };
                if owners.next().await.is_none() {
                    return Box::pin(stream::empty());
                }
                waited = true;
            }
        }
    }
}

async fn upower_owner_changes(
    ctx: &Ctx<Brightness>,
) -> Option<Pin<Box<dyn Stream<Item = ()> + Send>>> {
    let dbus = zbus::fdo::DBusProxy::new(ctx.system_bus().ok()?)
        .await
        .ok()?;
    let owners = dbus.receive_name_owner_changed().await.ok()?;
    Some(Box::pin(owners.filter_map(|signal| async move {
        let args = signal.args().ok()?;
        (args.name.as_str() == upower::SERVICE && args.new_owner.is_some()).then_some(())
    })))
}

async fn keyboard_change_events(
    ctx: &Ctx<Brightness>,
) -> Result<impl Stream<Item = Event> + Send + 'static, String> {
    let bus = ctx.system_bus().map_err(str::to_owned)?.clone();
    let source = upower::discover_kbd_backlight(&bus)
        .await
        .map_err(say)?
        .ok_or_else(|| "no UPower keyboard backlight to follow".to_owned())?;
    let max_brightness = source.max_brightness;
    let proxy = upower::UPowerKbdBacklightProxy::builder(&bus)
        .path(source.path)
        .map_err(say)?
        .build()
        .await
        .map_err(say)?;
    let changes = proxy
        .receive_brightness_changed_with_source()
        .await
        .map_err(say)?;
    Ok(changes.filter_map(move |signal| async move {
        let args = signal.args().ok()?;
        Some(keyboard_rescan_event(args.value, max_brightness))
    }))
}

fn reprobes(
    backend: Arc<dyn Backlight>,
    connector: String,
    enumeration: u64,
) -> impl Stream<Item = Event> {
    let probed = connector.clone();
    stream::iter(REPROBE_DELAYS)
        .then(move |delay| {
            let backend = Arc::clone(&backend);
            let connector = probed.clone();
            async move {
                tokio::time::sleep(delay).await;
                backend.reprobe(connector).await
            }
        })
        .filter_map(futures_util::future::ready)
        .take(1)
        .map(move |entry| Event::Reprobed {
            connector: connector.clone(),
            enumeration,
            entry,
        })
}

fn keyboard_rescan_event(value: i32, max_brightness: u32) -> Event {
    Event::Rescanned {
        id: KEYBOARD_ID.to_owned(),
        entry: Some(Entry::keyboard(
            KEYBOARD_ID,
            value.max(0) as u32,
            max_brightness,
        )),
    }
}

fn connector(device_link_target: Option<&str>) -> Option<String> {
    let target = device_link_target?;
    let basename = target.rsplit('/').next()?;
    if !basename.starts_with("card") {
        return None;
    }
    let (_, rest) = basename.split_once('-')?;
    Some(rest.to_owned())
}

fn floor(max: u32, minimum: u8) -> u32 {
    if minimum == 0 {
        return 0;
    }
    let scaled = (u64::from(max) * u64::from(minimum)) as f64 / 100.0;
    (scaled.round() as u32).max(1).min(max)
}

fn floor_for(kind: Kind, max: u32, minimum: u8) -> u32 {
    match kind {
        Kind::Keyboard => 0,
        Kind::Display => floor(max, minimum),
    }
}

fn preference(type_name: &str) -> u8 {
    match type_name {
        "firmware" => 0,
        "platform" => 1,
        "raw" => 2,
        _ => 3,
    }
}

struct SourceRecord {
    id: String,
    kind: Kind,
    connector: Option<String>,
    max: u32,
    confirmed: u32,
    current: u32,
    inflight: bool,
    queued: Option<(u32, oneshot::Sender<Result<(), CommandError>>)>,
}

fn sources_from(mut entries: Vec<Entry>) -> Vec<SourceRecord> {
    entries.sort_by(|a, b| {
        preference(&a.type_name)
            .cmp(&preference(&b.type_name))
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut seen_connectors = HashSet::new();
    let mut records = Vec::new();
    for entry in entries {
        let connector = connector(entry.device_link.as_deref());
        if let Some(value) = &connector
            && !seen_connectors.insert(value.clone())
        {
            continue;
        }
        let level = entry.brightness.min(entry.max_brightness);
        records.push(SourceRecord {
            id: entry.id,
            kind: entry.kind,
            connector: connector.map(|value| cap(&value, LABEL_CAP)),
            max: entry.max_brightness,
            confirmed: level,
            current: level,
            inflight: false,
            queued: None,
        });
    }
    records
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    minimum: u8,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self {
            minimum: document.brightness.minimum,
        }
    }
}

pub enum Event {
    Enumerated(Enumeration),
    WriteSettled {
        id: String,
        value: u32,
        failure: Option<String>,
    },
    Rescanned {
        id: String,
        entry: Option<Entry>,
    },
    Reprobed {
        connector: String,
        enumeration: u64,
        entry: Entry,
    },
}

pub enum Command {
    SetLevel {
        id: String,
        value: u32,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    AdjustLevel {
        id: String,
        delta: i32,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    Refresh {
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
}

#[derive(PartialEq, Eq, Hash)]
pub enum Watch {
    Enumerate,
    Udev,
    DrmHotplug,
    Keyboard,
    Reprobe { connector: String, enumeration: u64 },
}

pub struct Dependencies {
    pub backend: Arc<dyn Backlight>,
}

pub type BrightnessHandle = ServiceEndpoint<Brightness>;

impl ServiceEndpoint<Brightness> {
    pub async fn set_level(&self, id: String, value: u32) -> Result<(), CommandError> {
        self.call("brightness stopped before accepting the level", |reply| {
            Command::SetLevel { id, value, reply }
        })
        .await?
    }

    pub async fn adjust_level(&self, id: String, delta: i32) -> Result<(), CommandError> {
        self.call(
            "brightness stopped before accepting the adjustment",
            |reply| Command::AdjustLevel { id, delta, reply },
        )
        .await?
    }

    pub async fn refresh(&self) -> Result<(), CommandError> {
        self.call("brightness stopped before refreshing", |reply| {
            Command::Refresh { reply }
        })
        .await?
    }
}

pub struct Brightness {
    state: Publisher<BrightnessState>,
    backend: Arc<dyn Backlight>,
    minimum: u8,
    sources: Vec<SourceRecord>,
    inert: HashSet<String>,
    unanswered: Vec<String>,
    enumeration: u64,
}

impl Service for Brightness {
    const NAME: &'static str = "brightness";

    type Config = Config;
    type State = BrightnessState;
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = Watch;

    fn initial_state(_config: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let enumerate_backend = Arc::clone(&self.backend);
        let udev_backend = Arc::clone(&self.backend);
        let drm_backend = Arc::clone(&self.backend);
        let keyboard_backend = Arc::clone(&self.backend);
        let mut subs = vec![
            Sub::stream(Watch::Enumerate, move |_ctx| async move {
                stream::once(async move { Event::Enumerated(enumerate_backend.enumerate().await) })
            }),
            Sub::stream(Watch::Udev, move |_ctx| async move {
                backlight_uevents(udev_backend)
            }),
            Sub::stream(Watch::DrmHotplug, move |_ctx| async move {
                drm_hotplug_events(drm_backend)
            }),
            Sub::stream(Watch::Keyboard, move |ctx| {
                keyboard_watch(ctx, Arc::clone(&keyboard_backend))
            }),
        ];
        subs.extend(self.unanswered.iter().map(|connector| {
            let backend = Arc::clone(&self.backend);
            let probed = connector.clone();
            let enumeration = self.enumeration;
            Sub::stream(
                Watch::Reprobe {
                    connector: connector.clone(),
                    enumeration,
                },
                move |_ctx| async move { reprobes(backend, probed, enumeration) },
            )
        }));
        subs
    }

    async fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        dependencies: Self::Dependencies,
    ) -> Self {
        Self {
            state: ctx.publisher(),
            backend: dependencies.backend,
            minimum: config.minimum,
            sources: Vec::new(),
            inert: HashSet::new(),
            unanswered: Vec::new(),
            enumeration: 0,
        }
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Enumerated(Enumeration {
                entries,
                unanswered,
            })) => {
                self.enumeration = self.enumeration.wrapping_add(1);
                self.unanswered = unanswered;
                let mut sources = sources_from(entries);
                for source in &mut sources {
                    if let Some(previous) = self
                        .sources
                        .iter_mut()
                        .find(|previous| previous.id == source.id && previous.inflight)
                    {
                        source.confirmed = previous.confirmed;
                        source.current = previous.current;
                        source.inflight = true;
                        source.queued = previous.queued.take();
                    }
                }
                for vanished in &mut self.sources {
                    if let Some((_, reply)) = vanished.queued.take() {
                        let _ = reply.send(Err(CommandError::Unavailable(
                            "the backlight source went away".to_owned(),
                        )));
                    }
                }
                self.sources = sources;
                self.warn_inert();
                self.publish();
            }
            Input::Event(Event::WriteSettled { id, value, failure }) => {
                self.settle(ctx, &id, value, failure);
            }
            Input::Event(Event::Rescanned { id, entry }) => {
                self.rescanned(&id, entry);
            }
            Input::Event(Event::Reprobed {
                connector,
                enumeration,
                entry,
            }) => {
                self.reprobed(&connector, enumeration, entry);
            }
            Input::Command(Command::Refresh { reply }) => {
                self.refresh_all(ctx);
                let _ = reply.send(Ok(()));
            }
            Input::Command(Command::SetLevel { id, value, reply }) => {
                let minimum = self.minimum;
                self.request(ctx, &id, reply, move |record| {
                    value.clamp(floor_for(record.kind, record.max, minimum), record.max)
                });
            }
            Input::Command(Command::AdjustLevel { id, delta, reply }) => {
                let minimum = self.minimum;
                self.request(ctx, &id, reply, move |record| {
                    let low = i64::from(floor_for(record.kind, record.max, minimum));
                    let high = i64::from(record.max);
                    (i64::from(record.current) + i64::from(delta)).clamp(low, high) as u32
                });
            }
            Input::Config(config) => {
                self.minimum = config.minimum;
                self.warn_inert();
                self.publish();
                self.raise_floors(ctx);
            }
        }
    }
}

impl Brightness {
    fn newly_inert(&mut self) -> Vec<String> {
        let inert: HashSet<String> = self
            .sources
            .iter()
            .filter(|record| {
                record.kind == Kind::Display
                    && self.minimum > 0
                    && floor(record.max, self.minimum) >= record.max
            })
            .map(|record| record.id.clone())
            .collect();
        let mut fresh: Vec<String> = inert.difference(&self.inert).cloned().collect();
        fresh.sort_unstable();
        self.inert = inert;
        fresh
    }

    fn warn_inert(&mut self) {
        for id in self.newly_inert() {
            tracing::warn!(
                id,
                minimum = self.minimum,
                "the configured brightness minimum leaves this source no range to move in"
            );
        }
    }

    fn request(
        &mut self,
        ctx: &Ctx<Self>,
        id: &str,
        reply: oneshot::Sender<Result<(), CommandError>>,
        target: impl FnOnce(&SourceRecord) -> u32,
    ) {
        let Some(index) = self.sources.iter().position(|record| record.id == id) else {
            let _ = reply.send(Err(CommandError::InvalidArgument(format!(
                "no such backlight source: {id}"
            ))));
            return;
        };

        let value = target(&self.sources[index]);
        self.apply(ctx, index, value, reply);
    }

    fn apply(
        &mut self,
        ctx: &Ctx<Self>,
        index: usize,
        value: u32,
        reply: oneshot::Sender<Result<(), CommandError>>,
    ) {
        self.sources[index].current = value;
        self.publish();

        if self.sources[index].inflight {
            if let Some((_, superseded)) = self.sources[index].queued.take() {
                let _ = superseded.send(Err(CommandError::Unavailable(
                    "superseded by a newer level".to_owned(),
                )));
            }
            self.sources[index].queued = Some((value, reply));
            return;
        }

        self.dispatch(ctx, index, value, reply);
    }

    fn raise_floors(&mut self, ctx: &Ctx<Self>) {
        for index in 0..self.sources.len() {
            let floor_value = floor_for(
                self.sources[index].kind,
                self.sources[index].max,
                self.minimum,
            );
            if self.sources[index].current < floor_value {
                let (reply, _dropped) = oneshot::channel();
                self.apply(ctx, index, floor_value, reply);
            }
        }
    }

    fn dispatch(
        &mut self,
        ctx: &Ctx<Self>,
        index: usize,
        value: u32,
        reply: oneshot::Sender<Result<(), CommandError>>,
    ) {
        self.sources[index].inflight = true;
        let id = self.sources[index].id.clone();
        let backend = Arc::clone(&self.backend);
        let events = ctx.events();
        ctx.spawn_detached(move |_ctx| async move {
            let outcome = backend.write(id.clone(), value).await;
            let failure = outcome.as_ref().err().cloned();
            let _ = events
                .send(Input::Event(Event::WriteSettled { id, value, failure }))
                .await;
            let _ = reply.send(outcome.map_err(CommandError::Internal));
        });
    }

    fn settle(&mut self, ctx: &Ctx<Self>, id: &str, value: u32, failure: Option<String>) {
        let Some(index) = self.sources.iter().position(|record| record.id == id) else {
            return;
        };
        self.sources[index].inflight = false;
        match failure {
            None => {
                self.sources[index].confirmed = value;
                if self.sources[index].queued.is_none() {
                    self.sources[index].current = value;
                }
            }
            Some(_) if self.sources[index].queued.is_none() => {
                self.sources[index].current = self.sources[index].confirmed;
            }
            Some(_) => {}
        }
        let queued = self.sources[index].queued.take();
        self.publish();
        if let Some((queued_value, queued_reply)) = queued {
            self.dispatch(ctx, index, queued_value, queued_reply);
        }
    }

    fn rescanned(&mut self, id: &str, entry: Option<Entry>) {
        let Some(index) = self.sources.iter().position(|record| record.id == id) else {
            return;
        };
        if self.sources[index].inflight {
            return;
        }
        let Some(entry) = entry else {
            self.sources.remove(index);
            self.publish();
            return;
        };
        let level = entry.brightness.min(entry.max_brightness);
        self.sources[index].confirmed = level;
        if level != self.sources[index].current {
            self.sources[index].current = level;
            self.publish();
        }
    }

    fn reprobed(&mut self, connector: &str, enumeration: u64, entry: Entry) {
        if enumeration != self.enumeration {
            return;
        }
        let Some(index) = self.unanswered.iter().position(|name| name == connector) else {
            return;
        };
        self.unanswered.remove(index);
        let Some(record) = sources_from(vec![entry]).pop() else {
            return;
        };
        if self.sources.iter().any(|source| source.id == record.id) {
            return;
        }
        let at = self
            .sources
            .iter()
            .position(|source| source.kind == Kind::Keyboard)
            .unwrap_or(self.sources.len());
        self.sources.insert(at, record);
        self.warn_inert();
        self.publish();
    }

    fn refresh_all(&self, ctx: &Ctx<Self>) {
        for record in &self.sources {
            let id = record.id.clone();
            let backend = Arc::clone(&self.backend);
            let events = ctx.events();
            ctx.spawn_detached(move |_ctx| async move {
                let outcome = backend.read(id.clone()).await;
                if let Some(event) = rescan_event(id, outcome) {
                    let _ = events.send(Input::Event(event)).await;
                }
            });
        }
    }

    fn publish(&self) {
        let sources = self
            .sources
            .iter()
            .map(|record| Source {
                id: cap(&record.id, LABEL_CAP),
                kind: record.kind,
                connector: record.connector.clone(),
                current: record.current,
                max: record.max,
                floor: floor_for(record.kind, record.max, self.minimum),
            })
            .collect();
        self.state.set(BrightnessState { sources });
    }
}

#[cfg(test)]
mod tests;
