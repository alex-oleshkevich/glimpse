use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use glimpse_compositors::contract::{BUS_NAME, Item, OBJECT_PATH, key};
use glimpse_dbus::testing::PrivateBus;
use zbus::zvariant::{OwnedValue, Value};

use glimpse_compositors::client::{CompositorProvider, Handle};
use glimpse_shot::ports::Screens;
use glimpse_wayland::capture::CaptureError;

use super::{Compositor, WaylandScreens, read_layout, read_on, runtime};

fn owned(value: impl Into<Value<'static>>) -> OwnedValue {
    OwnedValue::try_from(value.into()).expect("an owned value")
}

fn entry(key: &str, value: impl Into<Value<'static>>) -> (String, OwnedValue) {
    (key.to_owned(), owned(value))
}

struct Squatter {
    geometry: bool,
    fail: bool,
    calls: Arc<AtomicUsize>,
    focused_output: String,
}

#[zbus::interface(name = "me.aresa.Glimpse.Compositor1")]
impl Squatter {
    #[zbus(property)]
    fn name(&self) -> String {
        "squatter".to_owned()
    }

    #[zbus(property)]
    fn capabilities(&self) -> Vec<String> {
        self.geometry
            .then(|| "window-geometry".to_owned())
            .into_iter()
            .collect()
    }

    #[zbus(property)]
    fn outputs(&self) -> Vec<Item> {
        vec![
            Item::from([
                entry(key::CONNECTOR, "eDP-1"),
                entry(key::LOGICAL, (10_i32, 20_i32, 100_u32, 50_u32, 1.0_f64)),
            ]),
            Item::from([entry(key::CONNECTOR, "HDMI-A-1")]),
        ]
    }

    #[zbus(property)]
    fn workspaces(&self) -> Vec<Item> {
        vec![
            Item::from([
                entry(key::ID, 8_u64),
                entry(key::OUTPUT, "eDP-1"),
                entry(key::ACTIVE, true),
            ]),
            Item::from([
                entry(key::ID, 2_u64),
                entry(key::OUTPUT, "HDMI-A-1"),
                entry(key::ACTIVE, false),
            ]),
        ]
    }

    #[zbus(property)]
    fn windows(&self) -> Vec<Item> {
        vec![
            Item::from([
                entry(key::ID, 7_u64),
                entry(key::TITLE, "Hi"),
                entry(key::APP_ID, "app"),
                entry(key::WORKSPACE, 8_u64),
                entry(key::FLOATING, true),
            ]),
            Item::from([
                entry(key::ID, 9_u64),
                entry(key::WORKSPACE, 2_u64),
                entry(key::FLOATING, false),
            ]),
        ]
    }

    #[zbus(property)]
    fn focus(&self) -> Item {
        Item::from([
            entry(key::OUTPUT, self.focused_output.clone()),
            entry(key::WINDOW, 7_u64),
        ])
    }

    fn window_geometries(&self) -> zbus::fdo::Result<HashMap<u64, Item>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(zbus::fdo::Error::Failed("geometry failed".to_owned()));
        }
        if !self.geometry {
            return Ok(HashMap::new());
        }
        Ok(HashMap::from([(
            7,
            Item::from([
                entry(key::X, 1.0_f64),
                entry(key::Y, 2.0_f64),
                entry(key::WIDTH, 3.0_f64),
                entry(key::HEIGHT, 4.0_f64),
            ]),
        )]))
    }
}

async fn squat(
    bus: &PrivateBus,
    geometry: bool,
    fail: bool,
    calls: Arc<AtomicUsize>,
) -> zbus::Connection {
    let owner = bus.connection().await;
    owner
        .object_server()
        .at(
            OBJECT_PATH,
            Squatter {
                geometry,
                fail,
                calls,
                focused_output: "eDP-1".to_owned(),
            },
        )
        .await
        .expect("an exported object");
    glimpse_dbus::own_name(&owner, BUS_NAME)
        .await
        .expect("the bus name");
    owner
}

#[test]
fn current_output_follows_focus_until_capture_and_refuses_a_lost_provider() {
    let bus = PrivateBus::start();
    let runtime = runtime().expect("a runtime");
    let (owner, provider) = runtime.block_on(async {
        let owner = squat(&bus, false, false, Arc::new(AtomicUsize::new(0))).await;
        let provider = CompositorProvider::start(bus.connection().await);
        read_layout(&provider)
            .await
            .expect("a read")
            .expect("a layout");
        (owner, provider)
    });
    let compositor = Compositor { provider, runtime };
    let screens = WaylandScreens {
        compositor: Some(&compositor),
        counted: Default::default(),
    };
    assert_eq!(screens.current().expect("initial output"), "eDP-1");

    compositor.runtime.block_on(async {
        let interface = owner
            .object_server()
            .interface::<_, Squatter>(OBJECT_PATH)
            .await
            .expect("an interface");
        interface.get_mut().await.focused_output = "HDMI-A-1".to_owned();
        interface
            .get()
            .await
            .focus_changed(interface.signal_emitter())
            .await
            .expect("focus changed");
        let mut state = compositor.provider.handle().subscribe();
        tokio::time::timeout(
            Duration::from_secs(2),
            state.wait_for(|state| state.focus.output.as_deref() == Some("HDMI-A-1")),
        )
        .await
        .expect("updated focus in time")
        .expect("a live follower");
    });
    assert_eq!(screens.current().expect("capture output"), "HDMI-A-1");

    compositor.runtime.block_on(async {
        owner.close().await.expect("the owner leaves");
        let mut state = compositor.provider.handle().subscribe();
        tokio::time::timeout(
            Duration::from_secs(2),
            state.wait_for(|state| state.unavailable.is_some()),
        )
        .await
        .expect("the loss is observed")
        .expect("a live follower");
    });
    assert!(matches!(
        screens.current(),
        Err(CaptureError::NoSuchOutput(_))
    ));
}

#[test]
fn a_replacement_owner_is_visible_on_the_next_read() {
    let bus = PrivateBus::start();
    let link = runtime().expect("a runtime");
    let provider = link.block_on(async {
        let owner = squat(&bus, true, false, Arc::new(AtomicUsize::new(0))).await;
        let provider = CompositorProvider::start(bus.connection().await);
        let layout = read_layout(&provider)
            .await
            .expect("a read")
            .expect("a layout");
        assert!(layout.windows.iter().any(|window| window.id == 7));
        owner.close().await.expect("the owner leaves");
        let mut watch = Handle(provider.handle()).subscribe();
        tokio::time::timeout(
            Duration::from_secs(2),
            watch.wait_for(|state| state.unavailable.is_some()),
        )
        .await
        .expect("the loss is observed")
        .expect("the watch stays open");
        provider
    });
    let compositor = Compositor {
        provider,
        runtime: link,
    };
    let harness = runtime().expect("a harness runtime");
    let _owner = harness.block_on(squat(&bus, true, false, Arc::new(AtomicUsize::new(0))));

    let deadline = Instant::now() + Duration::from_secs(2);
    while compositor
        .layout()
        .expect("a read")
        .is_none_or(|layout| layout.windows.iter().all(|window| window.id != 7))
    {
        assert!(
            Instant::now() < deadline,
            "a replacement owner stays invisible"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[tokio::test]
async fn a_private_bus_with_no_owner_reads_nothing_within_a_second() {
    let bus = PrivateBus::start();
    let connection = bus.connection().await;
    let started = Instant::now();
    let layout = read_on(&connection).await.expect("a read");
    assert!(layout.is_none());
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[tokio::test]
async fn a_provider_timeout_reports_only_the_error_description() {
    let bus = PrivateBus::start();
    let owner = squat(&bus, false, false, Arc::new(AtomicUsize::new(0))).await;
    let interface = owner
        .object_server()
        .interface::<_, Squatter>(OBJECT_PATH)
        .await
        .expect("an interface");
    let _guard = interface.get_mut().await;

    let error = read_on(&bus.connection().await)
        .await
        .expect_err("a timeout");
    assert_eq!(
        error.to_string(),
        "the compositor provider did not answer in time"
    );
}

#[tokio::test]
async fn a_provider_projects_output_focus_and_global_rects() {
    let bus = PrivateBus::start();
    let calls = Arc::new(AtomicUsize::new(0));
    let _owner = squat(&bus, true, false, Arc::clone(&calls)).await;

    let layout = read_on(&bus.connection().await)
        .await
        .expect("a read")
        .expect("a layout");
    let focused = layout
        .windows
        .iter()
        .find(|window| window.id == 7)
        .expect("window 7");
    assert_eq!(focused.output.as_deref(), Some("eDP-1"));
    assert!(focused.focused);
    assert_eq!(focused.workspace, Some(8));
    let rect = focused.rect.expect("a rect");
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (11.0, 22.0, 3.0, 4.0)
    );
    let other = layout
        .windows
        .iter()
        .find(|window| window.id == 9)
        .expect("window 9");
    assert_eq!(other.output.as_deref(), Some("HDMI-A-1"));
    assert!(!other.focused);
    assert!(other.rect.is_none());
    assert_eq!(layout.focused_output.as_deref(), Some("eDP-1"));
    assert!(calls.load(Ordering::SeqCst) > 0);
}

#[tokio::test]
async fn without_window_geometry_rects_stay_empty_and_geometries_is_not_called() {
    let bus = PrivateBus::start();
    let calls = Arc::new(AtomicUsize::new(0));
    let _owner = squat(&bus, false, false, Arc::clone(&calls)).await;

    let layout = read_on(&bus.connection().await)
        .await
        .expect("a read")
        .expect("a layout");
    assert!(layout.windows.iter().all(|window| window.rect.is_none()));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_geometry_error_keeps_outputs_and_windows_with_empty_rects() {
    let bus = PrivateBus::start();
    let calls = Arc::new(AtomicUsize::new(0));
    let _owner = squat(&bus, true, true, Arc::clone(&calls)).await;

    let layout = read_on(&bus.connection().await)
        .await
        .expect("a read")
        .expect("a layout");
    assert!(!layout.outputs.is_empty());
    assert!(!layout.windows.is_empty());
    assert!(layout.windows.iter().all(|window| window.rect.is_none()));
    assert!(calls.load(Ordering::SeqCst) > 0);
}
