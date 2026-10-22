use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::stream;
use glimpse_dbus::testing::PrivateBus;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use zbus::fdo::{PropertiesChangedStream, PropertiesProxy};
use zbus::proxy::CacheProperties;

use super::*;
use crate::client::{Compositor1Proxy, CompositorProvider, Handle};
use crate::contract::Capability;
use crate::model::{Focus, KeyboardLayouts, Logical, Mode, Rect};

#[derive(Default)]
struct Shared {
    snapshot: Mutex<Snapshot>,
    session_lock_restore: Mutex<Option<bool>>,
    calls: Mutex<Vec<String>>,
    snapshot_fails: AtomicBool,
    lock_fails: AtomicBool,
}

impl Shared {
    fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("calls").clone()
    }
}

struct Fake {
    shared: Arc<Shared>,
    events: Mutex<Option<mpsc::UnboundedReceiver<Event>>>,
}

impl Fake {
    fn record(&self, call: String) -> Result<(), Error> {
        self.shared.calls.lock().expect("calls").push(call);
        Ok(())
    }
}

fn failing(flag: &AtomicBool) -> Result<(), Error> {
    match flag.load(Ordering::SeqCst) {
        true => Err(Error::Unavailable("the fake is failing".to_owned())),
        false => Ok(()),
    }
}

impl Backend for Fake {
    const NAME: &'static str = "fake";
    const CAPABILITIES: &'static [Capability] = &[Capability::RenameWorkspace];

    async fn snapshot(&self) -> Result<Snapshot, Error> {
        failing(&self.shared.snapshot_fails)?;
        Ok(self.shared.snapshot.lock().expect("snapshot").clone())
    }

    async fn keyboard_layouts(&self) -> Result<KeyboardLayouts, Error> {
        Ok(self
            .shared
            .snapshot
            .lock()
            .expect("snapshot")
            .keyboard
            .clone())
    }

    async fn session_lock_restore(&self) -> Result<Option<bool>, Error> {
        failing(&self.shared.lock_fails)?;
        Ok(*self.shared.session_lock_restore.lock().expect("lock"))
    }

    async fn events(&self) -> Result<BoxStream<'static, Event>, Error> {
        let events = self.events.lock().expect("events").take();
        let events = events.ok_or_else(|| Error::Failed("events taken".to_owned()))?;
        Ok(stream::unfold(events, |mut events| async move {
            Some((events.recv().await?, events))
        })
        .boxed())
    }

    async fn focus_workspace(&self, id: WorkspaceId) -> Result<(), Error> {
        self.record(format!("focus_workspace {}", id.0))
    }

    async fn step_workspace(&self, forward: bool) -> Result<(), Error> {
        self.record(format!("step_workspace {forward}"))
    }

    async fn rename_workspace(&self, id: WorkspaceId, name: String) -> Result<(), Error> {
        self.record(format!("rename_workspace {} {name}", id.0))
    }

    async fn focus_window(&self, id: WindowId) -> Result<(), Error> {
        self.record(format!("focus_window {}", id.0))
    }

    async fn step_window(&self, forward: bool) -> Result<(), Error> {
        self.record(format!("step_window {forward}"))
    }

    async fn switch_keyboard_layout(&self, index: u32) -> Result<(), Error> {
        self.record(format!("switch_keyboard_layout {index}"))
    }

    async fn step_keyboard_layout(&self, forward: bool) -> Result<(), Error> {
        self.record(format!("step_keyboard_layout {forward}"))
    }

    async fn set_output_enabled(&self, connector: String, enabled: bool) -> Result<(), Error> {
        self.record(format!("set_output_enabled {connector} {enabled}"))
    }

    async fn power_off_monitors(&self) -> Result<(), Error> {
        self.record("power_off_monitors".to_owned())
    }

    async fn stop_screencast(&self, session: u64) -> Result<(), Error> {
        self.record(format!("stop_screencast {session}"))
    }

    async fn screenshot_window(&self, id: WindowId, path: PathBuf) -> Result<(), Error> {
        self.record(format!("screenshot_window {} {}", id.0, path.display()))
    }

    async fn window_geometries(&self) -> Result<HashMap<WindowId, Rect>, Error> {
        Ok(HashMap::new())
    }

    async fn probe(&self) -> bool {
        true
    }
}

fn window(id: u64, title: &str) -> Window {
    Window {
        id: WindowId(id),
        title: Some(title.to_owned()),
        app_id: Some("app".to_owned()),
        pid: Some(40),
        workspace: Some(WorkspaceId(1)),
        floating: id == 2,
        urgent: id == 1,
        order: Some(3),
    }
}

fn workspace(id: u64) -> Workspace {
    Workspace {
        id: WorkspaceId(id),
        index: Some(1),
        name: Some("web".to_owned()),
        output: Some("eDP-1".to_owned()),
        active: id == 1,
    }
}

fn snapshot() -> Snapshot {
    Snapshot {
        outputs: vec![Output {
            connector: "eDP-1".to_owned(),
            enabled: true,
            description: Some("Built-in".to_owned()),
            make: Some("BOE".to_owned()),
            model: Some("0x0BCA".to_owned()),
            mode: Some(Mode {
                width: 2880,
                height: 1800,
                refresh_mhz: 120_000,
            }),
            logical: Some(Logical {
                x: -10,
                y: 20,
                width: 1440,
                height: 900,
                scale: 2.0,
            }),
        }],
        workspaces: vec![workspace(1)],
        windows: vec![window(1, "one"), window(2, "two")],
        focus: Focus {
            output: Some("eDP-1".to_owned()),
            window: Some(WindowId(1)),
        },
        keyboard: KeyboardLayouts {
            names: vec!["English (US)".to_owned(), "Polish".to_owned()],
            codes: vec!["us".to_owned(), "pl".to_owned()],
            current: Some(1),
        },
        casts: vec![Cast {
            id: 5,
            active: true,
            output: Some("eDP-1".to_owned()),
            session: Some(6),
            pw_node: Some(70),
        }],
    }
}

fn fake(session_lock_restore: Option<bool>) -> (Fake, mpsc::UnboundedSender<Event>, Arc<Shared>) {
    let (events, receiver) = mpsc::unbounded_channel();
    let shared = Arc::new(Shared {
        snapshot: Mutex::new(snapshot()),
        session_lock_restore: Mutex::new(session_lock_restore),
        ..Shared::default()
    });
    let fake = Fake {
        shared: shared.clone(),
        events: Mutex::new(Some(receiver)),
    };
    (fake, events, shared)
}

async fn served(
    bus: &PrivateBus,
    fake: Fake,
) -> (Compositor1Proxy<'static>, JoinHandle<ServeError>) {
    let task = tokio::spawn(serve(bus.connection().await, fake));
    let connection = bus.connection().await;
    let dbus = zbus::fdo::DBusProxy::new(&connection).await.expect("dbus");
    let name = zbus::names::BusName::try_from(BUS_NAME).expect("a bus name");
    for _ in 0..500 {
        if dbus
            .name_has_owner(name.clone())
            .await
            .expect("an owner query")
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let proxy = Compositor1Proxy::builder(&connection)
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .expect("a proxy");
    (proxy, task)
}

fn refused<T: std::fmt::Debug>(answer: zbus::Result<T>) -> bool {
    matches!(answer, Err(zbus::Error::MethodError(name, _, _))
        if name.as_str() == "me.aresa.Glimpse.Compositor1.Error.Refused")
}

#[tokio::test]
async fn invalid_input_is_refused_before_the_backend_sees_it() {
    let bus = PrivateBus::start();
    let (fake, _events, shared) = fake(None);
    let (proxy, _task) = served(&bus, fake).await;

    assert!(refused(proxy.rename_workspace(1, "a\u{7}b").await));
    assert!(refused(proxy.rename_workspace(1, &"x".repeat(129)).await));
    assert!(refused(proxy.rename_workspace(9, "name").await));
    assert!(refused(proxy.set_output_enabled("HDMI-A-9", false).await));
    assert!(refused(proxy.screenshot_window(1, "shot.png").await));
    assert!(refused(proxy.focus_window(9).await));
    assert!(refused(proxy.focus_workspace(9).await));
    assert!(refused(proxy.stop_screencast(5).await));
    assert!(shared.calls().is_empty());

    let name = "x".repeat(128);
    proxy
        .rename_workspace(1, &name)
        .await
        .expect("a valid name");
    proxy.stop_screencast(6).await.expect("a known session");
    assert_eq!(
        shared.calls(),
        [
            format!("rename_workspace 1 {name}"),
            "stop_screencast 6".to_owned()
        ]
    );
}

#[tokio::test]
async fn a_second_provider_finds_the_name_taken() {
    let bus = PrivateBus::start();
    let (first, _first_events, _) = fake(None);
    let (_proxy, _task) = served(&bus, first).await;
    let (second, _second_events, _) = fake(None);

    let error = serve(bus.connection().await, second).await;

    assert!(matches!(error, ServeError::NameTaken), "{error}");
    assert_eq!(error.exit_code(), 4);
}

async fn properties(proxy: &Compositor1Proxy<'static>) -> PropertiesProxy<'static> {
    PropertiesProxy::builder(proxy.inner().connection())
        .destination(BUS_NAME)
        .expect("a destination")
        .path(OBJECT_PATH)
        .expect("a path")
        .build()
        .await
        .expect("a properties proxy")
}

async fn signals(proxy: &Compositor1Proxy<'static>) -> PropertiesChangedStream {
    properties(proxy)
        .await
        .receive_properties_changed()
        .await
        .expect("a signal stream")
}

async fn changed(signals: &mut PropertiesChangedStream) -> Vec<Vec<String>> {
    let mut seen = Vec::new();
    while let Ok(Some(signal)) =
        tokio::time::timeout(Duration::from_millis(300), signals.next()).await
    {
        let args = signal.args().expect("signal arguments");
        let mut names: Vec<String> = args
            .changed_properties()
            .keys()
            .map(|name| name.to_string())
            .collect();
        names.sort();
        seen.push(names);
    }
    seen
}

#[tokio::test]
async fn only_the_section_an_event_changed_is_signalled() {
    let bus = PrivateBus::start();
    let (fake, events, _) = fake(None);
    let (proxy, _task) = served(&bus, fake).await;
    let mut signals = signals(&proxy).await;

    events
        .send(Event::WindowFocusChanged(Some(WindowId(2))))
        .expect("an event");
    assert_eq!(changed(&mut signals).await, [["Focus"]]);

    events
        .send(Event::WindowOpenedOrChanged {
            window: window(1, "renamed"),
            focused: false,
        })
        .expect("an event");
    assert_eq!(changed(&mut signals).await, [["Windows"]]);

    events
        .send(Event::WindowFocusChanged(Some(WindowId(2))))
        .expect("an event");
    assert!(changed(&mut signals).await.is_empty());
}

#[tokio::test]
async fn session_lock_restore_is_served_only_when_the_backend_has_it() {
    let bus = PrivateBus::start();
    let (without, _events, _) = fake(None);
    let (proxy, task) = served(&bus, without).await;
    let properties = properties(&proxy).await;
    let interface =
        zbus::names::InterfaceName::try_from("me.aresa.Glimpse.Compositor1").expect("an interface");

    assert!(proxy.session_lock_restore().await.is_err());
    let all = properties.get_all(interface.clone()).await.expect("GetAll");
    assert!(all.contains_key("Name"));
    assert!(!all.contains_key("SessionLockRestore"));
    task.abort();
    drop(task.await);

    let bus = PrivateBus::start();
    let (with, _events, _) = fake(Some(false));
    let (proxy, _task) = served(&bus, with).await;
    assert_eq!(proxy.session_lock_restore().await.ok(), Some(false));
}

#[tokio::test]
async fn a_failed_structure_refetch_keeps_the_last_state_and_retries_on_the_next_event() {
    let bus = PrivateBus::start();
    let (fake, events, shared) = fake(None);
    let (proxy, _task) = served(&bus, fake).await;
    let mut signals = signals(&proxy).await;
    shared.snapshot_fails.store(true, Ordering::SeqCst);
    shared
        .snapshot
        .lock()
        .expect("snapshot")
        .workspaces
        .push(workspace(2));

    events
        .send(Event::Resync(Resync::Structure))
        .expect("an event");
    assert!(changed(&mut signals).await.is_empty());
    assert_eq!(proxy.workspaces().await.expect("workspaces").len(), 1);

    shared.snapshot_fails.store(false, Ordering::SeqCst);
    events
        .send(Event::WindowClosed(WindowId(99)))
        .expect("an event");
    assert_eq!(changed(&mut signals).await, [["Workspaces"]]);
    assert_eq!(proxy.workspaces().await.expect("workspaces").len(), 2);
}

#[tokio::test]
async fn a_keyboard_resync_rereads_session_lock_restore_until_it_succeeds() {
    let bus = PrivateBus::start();
    let (fake, events, shared) = fake(Some(false));
    let (proxy, _task) = served(&bus, fake).await;
    let mut signals = signals(&proxy).await;

    *shared.session_lock_restore.lock().expect("lock") = Some(true);
    events
        .send(Event::Resync(Resync::Keyboard))
        .expect("an event");
    assert_eq!(changed(&mut signals).await, [["SessionLockRestore"]]);
    assert_eq!(proxy.session_lock_restore().await.ok(), Some(true));

    shared.lock_fails.store(true, Ordering::SeqCst);
    *shared.session_lock_restore.lock().expect("lock") = Some(false);
    events
        .send(Event::Resync(Resync::Keyboard))
        .expect("an event");
    assert!(changed(&mut signals).await.is_empty());

    shared.lock_fails.store(false, Ordering::SeqCst);
    events
        .send(Event::WindowClosed(WindowId(99)))
        .expect("an event");
    assert_eq!(changed(&mut signals).await, [["SessionLockRestore"]]);
    assert_eq!(proxy.session_lock_restore().await.ok(), Some(false));
}

#[tokio::test]
async fn the_client_decodes_every_section_the_provider_serves() {
    let bus = PrivateBus::start();
    let (fake, events, shared) = fake(Some(true));
    let (_proxy, _task) = served(&bus, fake).await;
    let provider = CompositorProvider::start(bus.connection().await);
    let mut state = Handle(provider.handle()).subscribe();

    let first = tokio::time::timeout(
        Duration::from_secs(10),
        state.wait_for(|state| state.unavailable.is_none()),
    )
    .await
    .expect("the provider in time")
    .expect("a live follower")
    .clone();

    let mut expected = snapshot();
    assert_eq!(first.name, "fake");
    assert_eq!(first.capabilities, [Capability::RenameWorkspace]);
    assert_eq!(first.outputs, expected.outputs);
    assert_eq!(first.workspaces, expected.workspaces);
    assert_eq!(first.windows, expected.windows);
    assert_eq!(first.focus, expected.focus);
    assert_eq!(first.keyboard, expected.keyboard);
    assert_eq!(first.casts, expected.casts);
    assert_eq!(first.session_lock_restore, Some(true));
    assert!(first.windows.iter().any(|window| window.urgent));

    type Change = fn(&mut Snapshot, &mut Option<bool>);
    let changes: [(Resync, Change); 7] = [
        (Resync::Structure, |snapshot, _| {
            snapshot.outputs[0].enabled = false;
            snapshot.outputs[0].description = Some("Dock".to_owned());
        }),
        (Resync::Structure, |snapshot, _| {
            snapshot.workspaces = vec![workspace(1), workspace(3)];
        }),
        (Resync::Structure, |snapshot, _| {
            snapshot.windows = vec![window(2, "renamed"), window(4, "four")];
        }),
        (Resync::Structure, |snapshot, _| {
            snapshot.focus = Focus {
                output: Some("HDMI-A-1".to_owned()),
                window: None,
            };
        }),
        (Resync::Keyboard, |snapshot, _| {
            snapshot.keyboard = KeyboardLayouts {
                names: vec!["Deutsch".to_owned()],
                codes: vec!["de".to_owned()],
                current: Some(0),
            };
        }),
        (Resync::Structure, |snapshot, _| {
            snapshot.casts = vec![Cast {
                id: 8,
                active: false,
                output: None,
                session: None,
                pw_node: None,
            }];
        }),
        (Resync::Keyboard, |_, lock| *lock = Some(false)),
    ];
    let mut lock = Some(true);
    for (resync, change) in changes {
        change(&mut expected, &mut lock);
        *shared.snapshot.lock().expect("snapshot") = expected.clone();
        *shared.session_lock_restore.lock().expect("lock") = lock;
        events.send(Event::Resync(resync)).expect("an event");
        let reached = tokio::time::timeout(
            Duration::from_secs(10),
            state.wait_for(|state| {
                state.outputs == expected.outputs
                    && state.workspaces == expected.workspaces
                    && state.windows == expected.windows
                    && state.focus == expected.focus
                    && state.keyboard == expected.keyboard
                    && state.casts == expected.casts
                    && state.session_lock_restore == lock
            }),
        )
        .await;
        assert!(
            reached.is_ok(),
            "the client never reached {expected:?} {lock:?}"
        );
    }
}

#[tokio::test]
async fn serve_returns_ended_and_releases_the_name_when_the_event_stream_ends() {
    let bus = PrivateBus::start();
    let (fake, events, _) = fake(None);
    let (proxy, task) = served(&bus, fake).await;

    drop(events);
    let error = tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .expect("serve to return")
        .expect("serve to finish");

    assert!(matches!(error, ServeError::Ended), "{error}");
    assert_eq!(error.exit_code(), 1);
    let dbus = zbus::fdo::DBusProxy::new(proxy.inner().connection())
        .await
        .expect("dbus");
    let name = zbus::names::BusName::try_from(BUS_NAME).expect("a bus name");
    assert!(!dbus.name_has_owner(name).await.expect("an owner query"));
}
