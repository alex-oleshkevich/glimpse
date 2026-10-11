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

mod enumeration;
mod keyboard;
mod session;
mod sources;
mod writes;
