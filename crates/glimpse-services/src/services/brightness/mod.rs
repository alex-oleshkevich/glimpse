mod composite;
mod ddc;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use futures_util::future::BoxFuture;
use futures_util::{Stream, StreamExt, stream};
use glimpse_dbus::login1::{self, Login1SessionProxy};
use glimpse_dbus::upower;
use tokio::sync::{oneshot, watch};
use zbus::zvariant::OwnedObjectPath;

pub use composite::CompositeBacklight;
pub use ddc::DdcBacklight;

use super::say;
use crate::{
    ServiceState,
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, Service, ServiceEndpoint, ServiceError},
    subscription::Sub,
};

const BACKLIGHT_ROOT: &str = "/sys/class/backlight";
const LEDS_ROOT: &str = "/sys/class/leds";
const KEYBOARD_ID: &str = "keyboard";
const LABEL_CAP: usize = 64;

fn cap(value: &str) -> String {
    value.chars().take(LABEL_CAP).collect()
}

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

pub enum ReadOutcome {
    Found(Entry),
    Gone,
    Unavailable,
}

pub trait Backlight: Send + Sync + 'static {
    fn enumerate(&self) -> BoxFuture<'_, Vec<Entry>>;
    fn read(&self, id: String) -> BoxFuture<'_, ReadOutcome>;
    fn write(&self, id: String, value: u32) -> BoxFuture<'_, Result<(), String>>;
}

/// A null backend for when no backlight source can be reached at all — no system bus to build
/// `SysfsBacklight` on. It enumerates nothing, so the applet renders the ordinary "no backlight"
/// state rather than a fault, and it refuses a write plainly rather than pretending one landed.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnavailableBacklight;

impl Backlight for UnavailableBacklight {
    fn enumerate(&self) -> BoxFuture<'_, Vec<Entry>> {
        Box::pin(async { Vec::new() })
    }

    fn read(&self, _id: String) -> BoxFuture<'_, ReadOutcome> {
        Box::pin(async { ReadOutcome::Gone })
    }

    fn write(&self, _id: String, _value: u32) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async { Err("no system bus".to_owned()) })
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
}

#[cfg(test)]
impl FakeBacklight {
    pub fn new(entries: Vec<Entry>) -> Self {
        Self {
            record: Arc::new(Mutex::new(FakeRecord {
                entries,
                writes: Vec::new(),
                failure: None,
            })),
        }
    }

    pub fn writes(&self) -> Vec<(String, u32)> {
        self.record().writes.clone()
    }

    pub fn fail(&self, reason: Option<&str>) {
        self.record().failure = reason.map(str::to_owned);
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
    fn enumerate(&self) -> BoxFuture<'_, Vec<Entry>> {
        let entries = self.record().entries.clone();
        Box::pin(async move { entries })
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
}

#[derive(Clone)]
struct KeyboardProxy {
    proxy: upower::UPowerKbdBacklightProxy<'static>,
    max_brightness: u32,
}

pub struct SysfsBacklight {
    bus: zbus::Connection,
    session: Mutex<Option<OwnedObjectPath>>,
    keyboard: Mutex<Option<KeyboardProxy>>,
}

impl SysfsBacklight {
    pub fn new(bus: zbus::Connection) -> Self {
        Self {
            bus,
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
        let (_, proxy) = upower::kbd_backlight_proxy(&self.bus).await?;
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

    async fn session(&self) -> Result<OwnedObjectPath, String> {
        match self.cached_session() {
            Some(path) => Ok(path),
            None => {
                let path = login1::session_path(&self.bus).await?;
                self.cache_session(path.clone());
                Ok(path)
            }
        }
    }

    async fn write_backlight(&self, id: &str, value: u32) -> Result<(), String> {
        let path = self.session().await?;
        set_brightness_at(&self.bus, &path, "backlight", id, value)
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
        set_brightness_at(&self.bus, &path, "leds", name, value)
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
    fn enumerate(&self) -> BoxFuture<'_, Vec<Entry>> {
        Box::pin(async move {
            let mut entries = enumerate_sysfs().await;
            entries.extend(self.keyboard_entry().await);
            entries
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

async fn keyboard_watch(ctx: Ctx<Brightness>) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
    match keyboard_change_events(&ctx).await {
        Ok(events) => Box::pin(events),
        Err(error) => {
            tracing::debug!(%error, "no keyboard backlight signal to follow");
            Box::pin(stream::empty())
        }
    }
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
            connector: connector.map(|value| cap(&value)),
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
    Enumerated(Vec<Entry>),
    WriteSettled {
        id: String,
        value: u32,
        failure: Option<String>,
    },
    Rescanned {
        id: String,
        entry: Option<Entry>,
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
}

pub struct Dependencies {
    pub backend: Arc<dyn Backlight>,
}

#[derive(Clone)]
pub struct BrightnessHandle(ServiceEndpoint<Brightness>);

impl BrightnessHandle {
    pub fn snapshot(&self) -> BrightnessState {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> watch::Receiver<BrightnessState> {
        self.0.subscribe()
    }

    pub fn health(&self) -> watch::Receiver<ServiceState> {
        self.0.health()
    }

    pub async fn set_level(&self, id: String, value: u32) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::SetLevel { id, value, reply })?;
        result.await.map_err(|_| {
            CommandError::Unavailable("brightness stopped before accepting the level".to_owned())
        })?
    }

    pub async fn adjust_level(&self, id: String, delta: i32) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::AdjustLevel { id, delta, reply })?;
        result.await.map_err(|_| {
            CommandError::Unavailable(
                "brightness stopped before accepting the adjustment".to_owned(),
            )
        })?
    }

    pub async fn refresh(&self) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::Refresh { reply })?;
        result.await.map_err(|_| {
            CommandError::Unavailable("brightness stopped before refreshing".to_owned())
        })?
    }
}

pub struct Brightness {
    state: Publisher<BrightnessState>,
    backend: Arc<dyn Backlight>,
    minimum: u8,
    sources: Vec<SourceRecord>,
}

impl Service for Brightness {
    const NAME: &'static str = "brightness";

    type Config = Config;
    type State = BrightnessState;
    type Handle = BrightnessHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = Watch;

    fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
        BrightnessHandle(endpoint)
    }

    fn initial_state(_config: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let enumerate_backend = Arc::clone(&self.backend);
        let udev_backend = Arc::clone(&self.backend);
        let drm_backend = Arc::clone(&self.backend);
        vec![
            Sub::stream(Watch::Enumerate, move |_ctx| async move {
                stream::once(async move { Event::Enumerated(enumerate_backend.enumerate().await) })
            }),
            Sub::stream(Watch::Udev, move |_ctx| async move {
                backlight_uevents(udev_backend)
            }),
            Sub::stream(Watch::DrmHotplug, move |_ctx| async move {
                drm_hotplug_events(drm_backend)
            }),
            Sub::stream(Watch::Keyboard, keyboard_watch),
        ]
    }

    async fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        dependencies: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        Ok(Self {
            state: ctx.publisher(),
            backend: dependencies.backend,
            minimum: config.minimum,
            sources: Vec::new(),
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Enumerated(entries)) => {
                self.sources = sources_from(entries);
                self.publish();
            }
            Input::Event(Event::WriteSettled { id, value, failure }) => {
                self.settle(ctx, &id, value, failure);
            }
            Input::Event(Event::Rescanned { id, entry }) => {
                self.rescanned(&id, entry);
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
                self.publish();
                self.raise_floors(ctx);
            }
        }
    }
}

impl Brightness {
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
                id: cap(&record.id),
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
mod tests {
    use glimpse_dbus::Buses;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::subscription::Live;

    fn entry(
        id: &str,
        type_name: &str,
        brightness: u32,
        max_brightness: u32,
        device_link: Option<&str>,
    ) -> Entry {
        Entry {
            id: id.to_owned(),
            device_link: device_link.map(str::to_owned),
            type_name: type_name.to_owned(),
            brightness,
            max_brightness,
            kind: Kind::Display,
        }
    }

    fn led(name: &str, brightness: u32, max_brightness: u32) -> LedEntry {
        LedEntry {
            name: name.to_owned(),
            brightness,
            max_brightness,
        }
    }

    #[test]
    fn a_card_symlink_target_yields_the_connector_after_the_first_dash() {
        assert_eq!(
            connector(Some("../../card1-eDP-1")),
            Some("eDP-1".to_owned())
        );
        assert_eq!(connector(Some("../../card0-DP-2")), Some("DP-2".to_owned()));
    }

    #[test]
    fn a_missing_device_link_has_no_connector() {
        assert_eq!(connector(None), None);
    }

    #[test]
    fn a_backlight_with_no_device_link_still_produces_a_source() {
        let records = sources_from(vec![entry("ddcci1", "raw", 50, 100, None)]);

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].connector, None);
    }

    #[test]
    fn several_backlights_on_the_same_connector_collapse_to_the_preferred_one() {
        let records = sources_from(vec![
            entry("amdgpu_bl1", "raw", 100, 100, Some("../../card1-eDP-1")),
            entry(
                "intel_backlight",
                "firmware",
                50,
                100,
                Some("../../card1-eDP-1"),
            ),
        ]);

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "intel_backlight");
    }

    #[test]
    fn the_floor_never_reaches_zero_through_rounding_on_a_small_max() {
        assert_eq!(floor(400_000, 3), 12_000);
        assert_eq!(floor(3, 3), 1);
        assert_eq!(floor(400_000, 0), 0);
    }

    #[test]
    fn a_zero_max_brightness_floors_to_zero_rather_than_panicking() {
        assert_eq!(floor(0, 3), 0);
    }

    #[test]
    fn preference_orders_firmware_before_platform_before_raw() {
        let mut kinds = vec!["raw", "platform", "firmware"];
        kinds.sort_by_key(|kind| preference(kind));

        assert_eq!(kinds, vec!["firmware", "platform", "raw"]);
    }

    #[test]
    fn several_backlights_are_ordered_by_type_preference() {
        let records = sources_from(vec![
            entry("raw1", "raw", 1, 1, None),
            entry("firmware1", "firmware", 1, 1, None),
            entry("platform1", "platform", 1, 1, None),
        ]);

        let ids: Vec<&str> = records.iter().map(|record| record.id.as_str()).collect();
        assert_eq!(ids, vec!["firmware1", "platform1", "raw1"]);
    }

    struct Harness {
        service: Brightness,
        backend: FakeBacklight,
        ctx: Ctx<Brightness>,
        live: Live<Brightness>,
        health: watch::Receiver<ServiceState>,
        state: watch::Receiver<BrightnessState>,
        inbox: mpsc::Receiver<Input<Brightness>>,
        _cancel: CancellationToken,
    }

    impl Harness {
        async fn feed(&mut self, input: Input<Brightness>) {
            self.service.handle(&self.ctx, input).await;
        }

        async fn settle_next(&mut self) {
            let input = self
                .inbox
                .recv()
                .await
                .expect("a dispatched write reports its outcome");
            self.feed(input).await;
        }

        async fn enumerate(&mut self) {
            self.live.reconcile(&self.ctx, self.service.subscriptions());
            self.settle_next().await;
        }
    }

    async fn started(entries: Vec<Entry>, minimum: u8) -> Harness {
        let (events, inbox) = mpsc::channel(32);
        let cancel = CancellationToken::new();
        let buses = Buses::unavailable("no bus in tests");
        let (health, health_rx) = watch::channel(ServiceState::Starting);
        let config = Config { minimum };
        let (published, state) = watch::channel(Brightness::initial_state(&config));
        let ctx = Ctx::<Brightness>::new(events, &cancel, published, health, buses);
        let backend = FakeBacklight::new(entries);
        let service = Brightness::start(
            &ctx,
            config,
            Dependencies {
                backend: Arc::new(backend.clone()),
            },
        )
        .await
        .expect("the service starts");

        Harness {
            service,
            backend,
            ctx,
            live: Live::new(),
            health: health_rx,
            state,
            inbox,
            _cancel: cancel,
        }
    }

    async fn harness(entries: Vec<Entry>, minimum: u8) -> Harness {
        let mut harness = started(entries, minimum).await;
        harness.enumerate().await;
        harness
    }

    #[tokio::test]
    async fn set_level_clamps_to_the_floor_rather_than_reaching_zero() {
        let mut harness = harness(vec![entry("panel", "raw", 400_000, 400_000, None)], 3).await;
        let (reply, result) = oneshot::channel();

        harness
            .feed(Input::Command(Command::SetLevel {
                id: "panel".to_owned(),
                value: 0,
                reply,
            }))
            .await;
        harness.settle_next().await;

        assert_eq!(harness.backend.writes(), vec![("panel".to_owned(), 12_000)]);
        assert!(result.await.expect("answered").is_ok());
    }

    #[tokio::test]
    async fn adjust_level_clamps_at_the_floor_without_underflow() {
        let mut harness = harness(vec![entry("panel", "raw", 100, 100, None)], 3).await;
        let (reply, result) = oneshot::channel();

        harness
            .feed(Input::Command(Command::AdjustLevel {
                id: "panel".to_owned(),
                delta: -250,
                reply,
            }))
            .await;
        harness.settle_next().await;

        assert_eq!(harness.backend.writes(), vec![("panel".to_owned(), 3)]);
        assert!(result.await.expect("answered").is_ok());
    }

    #[tokio::test]
    async fn a_middle_value_is_dropped_while_a_write_is_in_flight() {
        let mut harness = harness(vec![entry("panel", "raw", 50, 100, None)], 0).await;
        let (reply1, result1) = oneshot::channel();
        let (reply2, result2) = oneshot::channel();
        let (reply3, result3) = oneshot::channel();

        harness
            .feed(Input::Command(Command::SetLevel {
                id: "panel".to_owned(),
                value: 10,
                reply: reply1,
            }))
            .await;
        harness
            .feed(Input::Command(Command::SetLevel {
                id: "panel".to_owned(),
                value: 20,
                reply: reply2,
            }))
            .await;
        harness
            .feed(Input::Command(Command::SetLevel {
                id: "panel".to_owned(),
                value: 30,
                reply: reply3,
            }))
            .await;

        assert!(matches!(
            result2.await.expect("answered"),
            Err(CommandError::Unavailable(_))
        ));

        harness.settle_next().await;
        harness.settle_next().await;

        assert_eq!(
            harness.backend.writes(),
            vec![("panel".to_owned(), 10), ("panel".to_owned(), 30)]
        );
        assert!(result1.await.expect("answered").is_ok());
        assert!(result3.await.expect("answered").is_ok());
    }

    #[tokio::test]
    async fn an_unknown_id_is_rejected_without_reaching_the_backend() {
        let mut harness = harness(vec![entry("panel", "raw", 100, 100, None)], 3).await;
        let (reply, result) = oneshot::channel();

        harness
            .feed(Input::Command(Command::SetLevel {
                id: "missing".to_owned(),
                value: 50,
                reply,
            }))
            .await;

        assert!(matches!(
            result.await.expect("answered"),
            Err(CommandError::InvalidArgument(_))
        ));
        assert!(harness.backend.writes().is_empty());
    }

    #[tokio::test]
    async fn a_failed_write_answers_an_error_and_current_is_reconciled_back() {
        let mut harness = harness(vec![entry("panel", "raw", 50, 100, None)], 3).await;
        harness.backend.fail(Some("no backlight control"));
        let (reply, result) = oneshot::channel();

        harness
            .feed(Input::Command(Command::SetLevel {
                id: "panel".to_owned(),
                value: 80,
                reply,
            }))
            .await;
        assert_eq!(harness.state.borrow().sources[0].current, 80);

        harness.settle_next().await;

        assert!(matches!(
            result.await.expect("answered"),
            Err(CommandError::Internal(_))
        ));
        assert_eq!(harness.state.borrow().sources[0].current, 50);
    }

    #[tokio::test]
    async fn a_success_after_a_failed_in_flight_write_lands_the_queued_value_in_current() {
        let mut harness = harness(vec![entry("panel", "raw", 50, 100, None)], 3).await;
        harness.backend.fail(Some("transient logind hiccup"));
        let (reply1, result1) = oneshot::channel();
        let (reply2, result2) = oneshot::channel();

        harness
            .feed(Input::Command(Command::SetLevel {
                id: "panel".to_owned(),
                value: 80,
                reply: reply1,
            }))
            .await;
        harness
            .feed(Input::Command(Command::SetLevel {
                id: "panel".to_owned(),
                value: 90,
                reply: reply2,
            }))
            .await;

        harness.settle_next().await;
        harness.backend.fail(None);
        harness.settle_next().await;

        assert_eq!(harness.state.borrow().sources[0].current, 90);
        assert!(matches!(
            result1.await.expect("answered"),
            Err(CommandError::Internal(_))
        ));
        assert!(result2.await.expect("answered").is_ok());
    }

    #[tokio::test]
    async fn a_success_while_a_newer_value_is_queued_does_not_snap_current_backwards() {
        let mut harness = harness(vec![entry("panel", "raw", 50, 100, None)], 3).await;
        let (reply1, result1) = oneshot::channel();
        let (reply2, result2) = oneshot::channel();

        harness
            .feed(Input::Command(Command::SetLevel {
                id: "panel".to_owned(),
                value: 80,
                reply: reply1,
            }))
            .await;
        harness
            .feed(Input::Command(Command::SetLevel {
                id: "panel".to_owned(),
                value: 90,
                reply: reply2,
            }))
            .await;

        harness.settle_next().await;

        assert_eq!(harness.state.borrow().sources[0].current, 90);

        harness.settle_next().await;

        assert!(result1.await.expect("answered").is_ok());
        assert!(result2.await.expect("answered").is_ok());
    }

    #[tokio::test]
    async fn raising_the_minimum_writes_up_a_source_left_below_the_new_floor() {
        let mut harness = harness(vec![entry("panel", "raw", 10, 100, None)], 3).await;

        harness.feed(Input::Config(Config { minimum: 50 })).await;
        harness.settle_next().await;

        assert_eq!(harness.backend.writes(), vec![("panel".to_owned(), 50)]);
        assert_eq!(harness.state.borrow().sources[0].current, 50);
    }

    #[tokio::test]
    async fn a_fresh_service_before_enumeration_is_empty_and_not_degraded() {
        let harness = started(Vec::new(), 3).await;

        assert!(harness.state.borrow().sources.is_empty());
        assert!(!matches!(
            &*harness.health.borrow(),
            ServiceState::Degraded { .. }
        ));
    }

    #[tokio::test]
    async fn enumeration_runs_through_the_declared_subscription() {
        let harness = harness(vec![entry("panel", "raw", 50, 100, None)], 3).await;

        assert_eq!(harness.state.borrow().sources.len(), 1);
        assert_eq!(harness.state.borrow().sources[0].id, "panel");
    }

    #[tokio::test]
    async fn the_declared_subscriptions_include_a_live_udev_source() {
        let harness = harness(vec![entry("panel", "raw", 50, 100, None)], 3).await;
        let subs = harness.service.subscriptions();

        assert!(subs.iter().any(|sub| *sub.key() == Watch::Enumerate));
        assert!(subs.iter().any(|sub| *sub.key() == Watch::Udev));
        assert!(subs.iter().any(|sub| *sub.key() == Watch::DrmHotplug));
        assert!(subs.iter().any(|sub| *sub.key() == Watch::Keyboard));
    }

    #[tokio::test]
    async fn a_rescan_touches_only_the_device_it_names() {
        let mut harness = harness(
            vec![
                entry("panel", "raw", 50, 100, None),
                entry("kbd", "raw", 20, 100, None),
            ],
            0,
        )
        .await;

        harness
            .feed(Input::Event(Event::Rescanned {
                id: "panel".to_owned(),
                entry: Some(entry("panel", "raw", 65, 100, None)),
            }))
            .await;

        let sources = harness.state.borrow().sources.clone();
        let panel = sources
            .iter()
            .find(|source| source.id == "panel")
            .expect("the panel source is still published");
        let keyboard = sources
            .iter()
            .find(|source| source.id == "kbd")
            .expect("the keyboard source is untouched");
        assert_eq!(panel.current, 65);
        assert_eq!(keyboard.current, 20);
    }

    #[tokio::test]
    async fn a_rescan_reporting_no_entry_drops_the_vanished_source() {
        let mut harness = harness(
            vec![
                entry("panel", "raw", 50, 100, None),
                entry("kbd", "raw", 20, 100, None),
            ],
            0,
        )
        .await;

        harness
            .feed(Input::Event(Event::Rescanned {
                id: "panel".to_owned(),
                entry: None,
            }))
            .await;

        let sources = harness.state.borrow().sources.clone();
        assert!(sources.iter().all(|source| source.id != "panel"));
        assert!(sources.iter().any(|source| source.id == "kbd"));
    }

    #[tokio::test]
    async fn a_rescan_after_our_own_write_settles_does_not_trigger_a_second_write() {
        let mut harness = harness(vec![entry("panel", "raw", 50, 100, None)], 0).await;
        let (reply, result) = oneshot::channel();

        harness
            .feed(Input::Command(Command::SetLevel {
                id: "panel".to_owned(),
                value: 80,
                reply,
            }))
            .await;
        harness.settle_next().await;
        assert!(result.await.expect("answered").is_ok());
        assert_eq!(harness.backend.writes(), vec![("panel".to_owned(), 80)]);

        harness
            .feed(Input::Event(Event::Rescanned {
                id: "panel".to_owned(),
                entry: Some(entry("panel", "raw", 80, 100, None)),
            }))
            .await;

        assert_eq!(harness.backend.writes(), vec![("panel".to_owned(), 80)]);
    }

    #[tokio::test]
    async fn a_refresh_command_rereads_every_known_source() {
        let mut harness = harness(
            vec![
                entry("panel", "raw", 50, 100, None),
                entry("kbd", "raw", 20, 100, None),
            ],
            0,
        )
        .await;
        harness.backend.set_brightness("panel", 66);
        harness.backend.set_brightness("kbd", 33);
        let (reply, result) = oneshot::channel();

        harness
            .feed(Input::Command(Command::Refresh { reply }))
            .await;
        assert!(result.await.expect("answered").is_ok());

        harness.settle_next().await;
        harness.settle_next().await;

        let sources = harness.state.borrow().sources.clone();
        let panel = sources
            .iter()
            .find(|source| source.id == "panel")
            .expect("the panel source is still published");
        let keyboard = sources
            .iter()
            .find(|source| source.id == "kbd")
            .expect("the keyboard source is still published");
        assert_eq!(panel.current, 66);
        assert_eq!(keyboard.current, 33);
    }

    #[tokio::test]
    async fn a_rescan_reconciles_a_value_that_moved_out_of_band() {
        let mut harness = harness(vec![entry("panel", "raw", 50, 100, None)], 0).await;
        assert_eq!(harness.state.borrow().sources[0].current, 50);

        harness
            .feed(Input::Event(Event::Rescanned {
                id: "panel".to_owned(),
                entry: Some(entry("panel", "raw", 77, 100, None)),
            }))
            .await;

        assert_eq!(harness.state.borrow().sources[0].current, 77);
    }

    #[tokio::test]
    async fn a_rescan_naming_the_in_flight_write_does_not_clobber_a_newer_queued_value() {
        let mut harness = harness(vec![entry("panel", "raw", 50, 100, None)], 0).await;
        let (reply1, result1) = oneshot::channel();
        let (reply2, result2) = oneshot::channel();

        harness
            .feed(Input::Command(Command::SetLevel {
                id: "panel".to_owned(),
                value: 80,
                reply: reply1,
            }))
            .await;
        harness
            .feed(Input::Command(Command::SetLevel {
                id: "panel".to_owned(),
                value: 90,
                reply: reply2,
            }))
            .await;
        assert_eq!(harness.state.borrow().sources[0].current, 90);

        harness
            .feed(Input::Event(Event::Rescanned {
                id: "panel".to_owned(),
                entry: Some(entry("panel", "raw", 80, 100, None)),
            }))
            .await;
        assert_eq!(harness.state.borrow().sources[0].current, 90);

        harness.settle_next().await;
        harness.settle_next().await;

        assert_eq!(
            harness.backend.writes(),
            vec![("panel".to_owned(), 80), ("panel".to_owned(), 90)]
        );
        assert!(result1.await.expect("answered").is_ok());
        assert!(result2.await.expect("answered").is_ok());
        assert_eq!(harness.state.borrow().sources[0].current, 90);
    }

    #[tokio::test]
    #[ignore = "reads the live /sys/class/backlight and writes it back; run under just test-crate-compositor"]
    async fn enumerate_sysfs_reads_a_real_backlight() {
        let entries = enumerate_sysfs().await;

        assert!(!entries.is_empty(), "this host has no backlight to read");
        assert!(
            entries
                .iter()
                .any(|entry| connector(entry.device_link.as_deref()).is_some()),
            "at least one entry should resolve a card-derived connector"
        );

        let entry = entries.first().expect("at least one backlight entry");
        let bus = zbus::Connection::system()
            .await
            .expect("a system bus connection");
        let backend = SysfsBacklight::new(bus);
        backend
            .write(entry.id.clone(), entry.brightness)
            .await
            .expect("writing the current level back succeeds");
    }

    #[tokio::test]
    #[ignore = "listens on the real drm udev subsystem and needs a `sudo udevadm trigger \
                --subsystem-match=drm --action=change` run in another terminal while this test \
                is running, since writing a uevent needs root; run under \
                just test-crate-compositor"]
    async fn a_real_drm_change_event_reaches_the_hotplug_stream() {
        let socket = open_drm_monitor().expect("a drm udev monitor for this session");
        let backend: Arc<dyn Backlight> = Arc::new(FakeBacklight::new(vec![entry(
            "panel", "raw", 50, 100, None,
        )]));
        let mut events = Box::pin(reenumerate_on_hotplug(socket, backend));

        let event = tokio::time::timeout(std::time::Duration::from_secs(60), events.next())
            .await
            .expect(
                "no drm change event arrived in 60s — run `sudo udevadm trigger \
                 --subsystem-match=drm --action=change` in another terminal while this test runs",
            )
            .expect("the udev monitor stream stayed open");

        assert!(matches!(event, Event::Enumerated(_)));
    }

    fn method_error(name: &str) -> zbus::Error {
        zbus::Error::MethodError(
            zbus::names::OwnedErrorName::try_from(name.to_owned())
                .expect("a well-formed error name"),
            None,
            zbus::message::Message::method_call("/org/freedesktop/login1", "Whatever")
                .expect("a call")
                .build(&())
                .expect("a message"),
        )
    }

    #[test]
    fn is_stale_session_matches_the_four_stale_names() {
        assert!(is_stale_session(&method_error(
            "org.freedesktop.DBus.Error.UnknownObject"
        )));
        assert!(is_stale_session(&method_error(
            "org.freedesktop.login1.NoSuchSession"
        )));
        assert!(is_stale_session(&method_error(
            "org.freedesktop.DBus.Error.ServiceUnknown"
        )));
        assert!(is_stale_session(&method_error(
            "org.freedesktop.DBus.Error.NameHasNoOwner"
        )));
    }

    #[test]
    fn is_stale_session_ignores_an_unrelated_method_error() {
        assert!(!is_stale_session(&method_error(
            "org.freedesktop.DBus.Error.AccessDenied"
        )));
    }

    #[test]
    fn is_stale_session_ignores_a_non_method_error_variant() {
        assert!(!is_stale_session(&zbus::Error::Unsupported));
    }

    #[test]
    fn only_the_kbd_backlight_led_passes_the_filter() {
        let names = [
            "input17::capslock",
            "input17::compose",
            "input17::kana",
            "input17::numlock",
            "input17::scrolllock",
            "input35::capslock",
            "input35::compose",
            "input35::kana",
            "input35::numlock",
            "input35::scrolllock",
            "platform::micmute",
        ];
        for name in names {
            assert!(
                !is_keyboard_backlight_led(name),
                "{name} should be excluded"
            );
        }
        assert!(is_keyboard_backlight_led("acme::kbd_backlight"));
        assert!(is_keyboard_backlight_led("acme:white:kbd_backlight"));
    }

    #[test]
    fn keyboard_led_selection_is_sorted_by_name() {
        let leds = vec![
            led("zzz::kbd_backlight", 1, 3),
            led("aaa::kbd_backlight", 2, 3),
        ];

        let selected = select_keyboard_led(leds).expect("a led is selected");

        assert_eq!(selected.name, "aaa::kbd_backlight");
    }

    #[test]
    fn a_keyboard_signal_clamps_a_negative_value_to_zero() {
        let event = keyboard_rescan_event(-5, 3);

        match event {
            Event::Rescanned { id, entry } => {
                assert_eq!(id, KEYBOARD_ID);
                assert_eq!(entry, Some(Entry::keyboard(KEYBOARD_ID, 0, 3)));
            }
            _ => panic!("expected a Rescanned event"),
        }
    }

    #[test]
    fn a_keyboard_signal_carries_the_max_brightness_from_discovery() {
        let event = keyboard_rescan_event(2, 3);

        match event {
            Event::Rescanned { id, entry } => {
                assert_eq!(id, KEYBOARD_ID);
                assert_eq!(entry, Some(Entry::keyboard(KEYBOARD_ID, 2, 3)));
            }
            _ => panic!("expected a Rescanned event"),
        }
    }

    #[test]
    fn the_keyboard_floor_is_always_zero() {
        assert_eq!(floor_for(Kind::Keyboard, 3, 50), 0);
        assert_eq!(floor_for(Kind::Display, 3, 50), floor(3, 50));
    }

    #[tokio::test]
    async fn a_keyboard_source_can_be_set_all_the_way_to_zero() {
        let mut harness = harness(vec![Entry::keyboard(KEYBOARD_ID, 2, 3)], 50).await;
        let (reply, result) = oneshot::channel();

        harness
            .feed(Input::Command(Command::SetLevel {
                id: KEYBOARD_ID.to_owned(),
                value: 0,
                reply,
            }))
            .await;
        harness.settle_next().await;

        assert_eq!(harness.backend.writes(), vec![(KEYBOARD_ID.to_owned(), 0)]);
        assert!(result.await.expect("answered").is_ok());
        assert_eq!(harness.state.borrow().sources[0].floor, 0);
    }

    #[tokio::test]
    async fn a_keyboard_signal_reconciles_state_and_never_causes_a_second_write() {
        let mut harness = harness(vec![Entry::keyboard(KEYBOARD_ID, 1, 3)], 0).await;
        let (reply, result) = oneshot::channel();

        harness
            .feed(Input::Command(Command::SetLevel {
                id: KEYBOARD_ID.to_owned(),
                value: 3,
                reply,
            }))
            .await;
        harness.settle_next().await;
        assert!(result.await.expect("answered").is_ok());
        assert_eq!(harness.backend.writes(), vec![(KEYBOARD_ID.to_owned(), 3)]);

        harness
            .feed(Input::Event(Event::Rescanned {
                id: KEYBOARD_ID.to_owned(),
                entry: Some(Entry::keyboard(KEYBOARD_ID, 2, 3)),
            }))
            .await;

        assert_eq!(harness.state.borrow().sources[0].current, 2);
        assert_eq!(harness.backend.writes(), vec![(KEYBOARD_ID.to_owned(), 3)]);
    }

    #[tokio::test]
    async fn no_upower_and_no_led_yields_no_keyboard_source_without_degrading_health() {
        let harness = harness(Vec::new(), 3).await;

        assert!(harness.state.borrow().sources.is_empty());
        assert!(!matches!(
            &*harness.health.borrow(),
            ServiceState::Degraded { .. }
        ));
    }

    #[tokio::test]
    async fn adjust_level_with_a_negative_delta_lowers_a_keyboard_source_without_underflow() {
        let mut harness = harness(vec![Entry::keyboard(KEYBOARD_ID, 2, 3)], 50).await;
        let (reply, result) = oneshot::channel();

        harness
            .feed(Input::Command(Command::AdjustLevel {
                id: KEYBOARD_ID.to_owned(),
                delta: -10,
                reply,
            }))
            .await;
        harness.settle_next().await;

        assert_eq!(harness.backend.writes(), vec![(KEYBOARD_ID.to_owned(), 0)]);
        assert!(result.await.expect("answered").is_ok());
    }

    #[tokio::test]
    async fn raising_the_minimum_does_not_lift_a_keyboard_off_zero() {
        let mut harness = harness(vec![Entry::keyboard(KEYBOARD_ID, 0, 3)], 3).await;

        harness.feed(Input::Config(Config { minimum: 50 })).await;

        assert!(harness.backend.writes().is_empty());
        assert_eq!(harness.state.borrow().sources[0].current, 0);
    }
}
