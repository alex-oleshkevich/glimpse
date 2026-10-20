use crate::ServiceState;
use glimpse_dbus::Buses;
use tokio::sync::watch;

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

#[derive(Debug, Clone, Default)]
pub struct FakeBacklight {
    record: Arc<Mutex<FakeRecord>>,
}

#[derive(Debug, Default)]
struct FakeRecord {
    entries: Vec<Entry>,
    writes: Vec<(String, u32)>,
    failure: Option<String>,
    late: Option<(String, usize, Entry)>,
}

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

    fn record(&self) -> std::sync::MutexGuard<'_, FakeRecord> {
        self.record.lock().unwrap_or_else(|held| held.into_inner())
    }
}

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
    .await;

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

fn method_error(name: &str) -> zbus::Error {
    zbus::Error::MethodError(
        zbus::names::OwnedErrorName::try_from(name.to_owned()).expect("a well-formed error name"),
        None,
        zbus::message::Message::method_call("/org/freedesktop/login1", "Whatever")
            .expect("a call")
            .build(&())
            .expect("a message"),
    )
}

#[tokio::test]
#[ignore = "reads the live /sys/class/backlight and writes it back; run under just test-crate-compositor"]
async fn enumerate_sysfs_reads_a_real_backlight() {
    let entries = enumerate_sysfs(Path::new(BACKLIGHT_ROOT)).await;

    assert!(!entries.is_empty(), "this host has no backlight to read");
    assert!(
        entries
            .iter()
            .any(|entry| connector(entry.device_link.as_deref()).is_some()),
        "at least one entry should resolve a card-derived connector"
    );

    let entry = entries.first().expect("at least one backlight entry");
    let backend = SysfsBacklight::new(glimpse_dbus::Buses::connect().await);
    backend
        .write(entry.id.clone(), entry.brightness)
        .await
        .expect("writing the current level back succeeds");
}

#[tokio::test]
#[ignore = "manual: needs `sudo udevadm trigger --subsystem-match=drm --action=change` in another \
                terminal within 60s; run with just test-one glimpse-services \
                services::brightness::tests::a_real_drm_change_event_reaches_the_hotplug_stream; \
                just test-compositor leaves it out"]
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

#[tokio::test]
async fn sysfs_without_a_system_bus_lists_nothing_and_refuses_writes() {
    let root = tempfile::tempdir().expect("temporary directory");
    let device = root.path().join("intel_backlight");
    std::fs::create_dir(&device).expect("creates");
    for (file, value) in [
        ("brightness", "40"),
        ("max_brightness", "100"),
        ("type", "raw"),
    ] {
        std::fs::write(device.join(file), value).expect("writes");
    }
    let backend = SysfsBacklight::at(
        glimpse_dbus::Buses::unavailable("no bus in tests"),
        root.path().to_owned(),
    );

    let enumeration = backend.enumerate().await;
    let read = backend.read("intel_backlight".to_owned()).await;
    let refused = backend.write("panel".to_owned(), 1).await;

    assert!(enumeration.entries.is_empty());
    assert!(enumeration.unanswered.is_empty());
    assert!(matches!(read, ReadOutcome::Gone));
    assert!(refused.is_err());
}

mod enumeration;
mod keyboard;
mod session;
mod sources;
mod writes;
