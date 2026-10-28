use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use futures_util::stream;
use glimpse_dbus::testing::PrivateBus;
use tokio::sync::{Notify, broadcast, mpsc};
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
    keyboard_fails: AtomicBool,
    keyboard_paused: AtomicBool,
    keyboard_resume: Notify,
    step_fails: AtomicBool,
    lock_fails: AtomicBool,
    snapshots: AtomicUsize,
    subscription_fails: AtomicBool,
    subscriptions: Mutex<Vec<mpsc::UnboundedSender<Event>>>,
    supersede_switch: Mutex<Option<u32>>,
    queue_switch_echo: AtomicBool,
}

impl Shared {
    fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("calls").clone()
    }
}

struct Fake {
    shared: Arc<Shared>,
    events: broadcast::WeakSender<Event>,
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
        self.shared.snapshots.fetch_add(1, Ordering::SeqCst);
        failing(&self.shared.snapshot_fails)?;
        Ok(self.shared.snapshot.lock().expect("snapshot").clone())
    }

    async fn keyboard_layouts(&self) -> Result<KeyboardLayouts, Error> {
        failing(&self.shared.keyboard_fails)?;
        if self.shared.keyboard_paused.load(Ordering::SeqCst) {
            self.shared.keyboard_resume.notified().await;
        }
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
        failing(&self.shared.subscription_fails)?;
        let events = self
            .events
            .upgrade()
            .ok_or_else(|| Error::Unavailable("events closed".to_owned()))?
            .subscribe();
        let (sender, late) = mpsc::unbounded_channel();
        self.shared
            .subscriptions
            .lock()
            .expect("subscriptions")
            .push(sender);
        Ok(stream::unfold((events, late), |(mut events, mut late)| async move {
            let event = tokio::select! {
                event = events.recv() => match event {
                    Ok(event) => event,
                    Err(broadcast::error::RecvError::Lagged(_)) => Event::Resync(Resync::Structure),
                    Err(broadcast::error::RecvError::Closed) => return None,
                },
                event = late.recv() => event?,
            };
            Some((event, (events, late)))
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
        self.record(format!("switch_keyboard_layout {index}"))?;
        if self.shared.queue_switch_echo.load(Ordering::SeqCst) {
            self.shared
                .subscriptions
                .lock()
                .expect("subscriptions")
                .last()
                .expect("subscription")
                .send(Event::KeyboardLayoutSwitched(index))
                .expect("queued own echo");
        }
        self.shared
            .snapshot
            .lock()
            .expect("snapshot")
            .keyboard
            .current = Some(
            self.shared
                .supersede_switch
                .lock()
                .expect("superseded layout")
                .unwrap_or(index),
        );
        Ok(())
    }

    async fn step_keyboard_layout(&self, forward: bool) -> Result<(), Error> {
        failing(&self.shared.step_fails)?;
        self.record(format!("step_keyboard_layout {forward}"))?;
        let mut snapshot = self.shared.snapshot.lock().expect("snapshot");
        let keyboard = &mut snapshot.keyboard;
        let count = keyboard.names.len().max(keyboard.codes.len()) as u32;
        keyboard.current = keyboard.current.filter(|_| count > 0).map(|index| {
            if forward {
                (index + 1) % count
            } else {
                (index + count - 1) % count
            }
        });
        Ok(())
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

fn fake(session_lock_restore: Option<bool>) -> (Fake, broadcast::Sender<Event>, Arc<Shared>) {
    let (events, _) = broadcast::channel(1024);
    let shared = Arc::new(Shared {
        snapshot: Mutex::new(snapshot()),
        session_lock_restore: Mutex::new(session_lock_restore),
        ..Shared::default()
    });
    let fake = Fake {
        shared: shared.clone(),
        events: events.downgrade(),
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

#[tokio::test]
async fn a_native_layout_step_publishes_the_selected_index_before_replying() {
    let bus = PrivateBus::start();
    let (fake, _events, shared) = fake(None);
    let (proxy, _task) = served(&bus, fake).await;
    let provider = CompositorProvider::start(bus.connection().await);
    let handle = Handle(provider.handle());
    let mut state = handle.subscribe();
    tokio::time::timeout(
        Duration::from_secs(2),
        state.wait_for(|state| state.name == "fake"),
    )
    .await
    .expect("the provider arrives")
    .expect("the client stays open");

    assert_eq!(handle.step_keyboard_layout(true).await, Ok(0));
    assert_eq!(
        u32::try_from(&proxy.keyboard().await.expect("keyboard")[key::CURRENT]).expect("current"),
        0
    );
    assert_eq!(
        proxy
            .step_keyboard_layout(false)
            .await
            .expect("a native step"),
        1
    );
    assert_eq!(
        shared.calls(),
        ["step_keyboard_layout true", "step_keyboard_layout false"]
    );
}

#[tokio::test]
async fn a_native_layout_step_preserves_failures_and_refuses_an_unknown_result() {
    let bus = PrivateBus::start();
    let (fake, _events, shared) = fake(None);
    let (proxy, _task) = served(&bus, fake).await;

    shared.step_fails.store(true, Ordering::SeqCst);
    assert!(matches!(proxy.step_keyboard_layout(true).await,
        Err(zbus::Error::MethodError(name, _, _))
        if name.as_str() == "me.aresa.Glimpse.Compositor1.Error.Unavailable"));
    assert!(shared.calls().is_empty());

    shared.step_fails.store(false, Ordering::SeqCst);
    shared.keyboard_fails.store(true, Ordering::SeqCst);
    assert!(matches!(proxy.step_keyboard_layout(true).await,
        Err(zbus::Error::MethodError(name, _, _))
        if name.as_str() == "me.aresa.Glimpse.Compositor1.Error.Unavailable"));

    shared.keyboard_fails.store(false, Ordering::SeqCst);
    shared.snapshot.lock().expect("snapshot").keyboard.current = None;
    assert!(matches!(proxy.step_keyboard_layout(true).await,
        Err(zbus::Error::MethodError(name, _, _))
        if name.as_str() == "me.aresa.Glimpse.Compositor1.Error.Failed"));
    assert_eq!(
        shared.calls(),
        ["step_keyboard_layout true", "step_keyboard_layout true"]
    );
}

#[tokio::test]
async fn a_layout_restore_waits_until_a_native_step_reads_its_result() {
    let bus = PrivateBus::start();
    let (fake, _events, shared) = fake(None);
    let (proxy, _task) = served(&bus, fake).await;
    shared.keyboard_paused.store(true, Ordering::SeqCst);
    let step = tokio::spawn({
        let proxy = proxy.clone();
        async move { proxy.step_keyboard_layout(true).await }
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        while shared.calls().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the step started");
    assert_eq!(shared.calls(), ["step_keyboard_layout true"]);

    let restore = tokio::spawn({
        let proxy = proxy.clone();
        async move { proxy.switch_keyboard_layout(1).await }
    });
    tokio::task::yield_now().await;
    assert_eq!(shared.calls(), ["step_keyboard_layout true"]);

    shared.keyboard_resume.notify_one();
    assert_eq!(step.await.expect("step task").expect("step"), 0);
    restore.await.expect("restore task").expect("restore");
    assert_eq!(
        shared.calls(),
        ["step_keyboard_layout true", "switch_keyboard_layout 1"]
    );
    assert_eq!(
        shared.snapshot.lock().expect("snapshot").keyboard.current,
        Some(1)
    );
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

async fn keyboard_change(signals: &mut PropertiesChangedStream) -> u32 {
    let signal = tokio::time::timeout(Duration::from_secs(2), signals.next())
        .await
        .expect("keyboard change")
        .expect("signal");
    let args = signal.args().expect("arguments");
    assert_eq!(
        args.changed_properties()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        ["Keyboard"]
    );
    let keyboard: Item = args.changed_properties()["Keyboard"]
        .try_clone()
        .expect("keyboard")
        .try_into()
        .expect("map");
    u32::try_from(&keyboard[key::CURRENT]).expect("current")
}

#[tokio::test]
async fn a_noop_keyboard_command_publishes_a_terminal_observation() {
    let bus = PrivateBus::start();
    let (fake, _events, shared) = fake(Some(true));
    let (proxy, _task) = served(&bus, fake).await;
    let mut signals = signals(&proxy).await;
    shared.lock_fails.store(true, Ordering::SeqCst);
    proxy
        .switch_keyboard_layout(1)
        .await
        .expect("no-op restore");
    assert_eq!(keyboard_change(&mut signals).await, 1);
    assert!(changed(&mut signals).await.is_empty());
    assert_eq!(shared.subscriptions.lock().expect("subscriptions").len(), 2);
}

#[tokio::test]
async fn superseded_keyboard_commands_discard_old_subscription_echoes() {
    let bus = PrivateBus::start();
    let (fake, _events, shared) = fake(None);
    shared.snapshot.lock().expect("snapshot").keyboard.current = Some(0);
    let (proxy, _task) = served(&bus, fake).await;
    let mut signals = signals(&proxy).await;
    shared.queue_switch_echo.store(true, Ordering::SeqCst);
    *shared.supersede_switch.lock().expect("superseded layout") = Some(0);
    proxy
        .switch_keyboard_layout(1)
        .await
        .expect("superseded restore");
    assert_eq!(keyboard_change(&mut signals).await, 0);
    assert!(
        shared.subscriptions.lock().expect("subscriptions")[0]
            .send(Event::KeyboardLayoutSwitched(1))
            .is_err()
    );
    assert!(changed(&mut signals).await.is_empty());

    *shared.supersede_switch.lock().expect("superseded layout") = None;
    proxy.switch_keyboard_layout(1).await.expect("next restore");
    assert_eq!(keyboard_change(&mut signals).await, 1);
    assert!(
        shared.subscriptions.lock().expect("subscriptions")[1]
            .send(Event::KeyboardLayoutSwitched(0))
            .is_err()
    );
    assert!(changed(&mut signals).await.is_empty());
}

#[tokio::test]
async fn a_failed_keyboard_cutover_keeps_a_usable_native_source() {
    for subscription_failure in [true, false] {
        let bus = PrivateBus::start();
        let (fake, events, shared) = fake(None);
        let (proxy, _task) = served(&bus, fake).await;
        let mut signals = signals(&proxy).await;
        if subscription_failure {
            shared.subscription_fails.store(true, Ordering::SeqCst);
        } else {
            shared.snapshot_fails.store(true, Ordering::SeqCst);
        }
        assert!(proxy.switch_keyboard_layout(0).await.is_err());
        shared.subscription_fails.store(false, Ordering::SeqCst);
        shared.snapshot_fails.store(false, Ordering::SeqCst);
        events
            .send(Event::Resync(Resync::Structure))
            .expect("native update");
        assert_eq!(keyboard_change(&mut signals).await, 0);
        proxy
            .switch_keyboard_layout(1)
            .await
            .expect("recovered command");
        assert_eq!(keyboard_change(&mut signals).await, 1);
    }
}

#[tokio::test]
async fn ready_native_events_do_not_starve_keyboard_commands() {
    let bus = PrivateBus::start();
    let (fake, events, _) = fake(None);
    let (proxy, _task) = served(&bus, fake).await;
    let flood = tokio::spawn(async move {
        loop {
            for _ in 0..2048 {
                let _ = events.send(Event::WindowFocusChanged(Some(WindowId(1))));
            }
            tokio::task::yield_now().await;
        }
    });
    tokio::time::timeout(Duration::from_secs(2), proxy.switch_keyboard_layout(0))
        .await
        .expect("command not starved")
        .expect("command");
    flood.abort();
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

async fn next_change(signals: &mut PropertiesChangedStream) -> (Vec<String>, Option<String>) {
    let signal = tokio::time::timeout(Duration::from_secs(2), signals.next())
        .await
        .expect("a change")
        .expect("a signal");
    let args = signal.args().expect("signal arguments");
    let mut names: Vec<String> = args
        .changed_properties()
        .keys()
        .map(|name| name.to_string())
        .collect();
    names.sort();
    let title = args.changed_properties().get("Windows").map(|windows| {
        let items = Vec::<Item>::try_from(windows.try_clone().expect("a value")).expect("items");
        let first = items
            .iter()
            .find(|item| item.get(key::ID).and_then(|id| u64::try_from(id).ok()) == Some(1))
            .expect("window 1");
        String::try_from(
            first
                .get(key::TITLE)
                .expect("a title")
                .try_clone()
                .expect("a value"),
        )
        .expect("a string")
    });
    (names, title)
}

#[tokio::test]
async fn a_spinning_title_is_signalled_at_most_once_a_second() {
    let bus = PrivateBus::start();
    let (fake, events, _) = fake(None);
    let (proxy, _task) = served(&bus, fake).await;
    let mut signals = signals(&proxy).await;
    let title = |frame: u32| Event::WindowOpenedOrChanged {
        window: window(1, &format!("spin {frame}")),
        focused: false,
    };

    let started = Instant::now();
    for frame in 0..20 {
        events.send(title(frame)).expect("an event");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let first = next_change(&mut signals).await;
    assert_eq!(
        first,
        (vec!["Windows".to_owned()], Some("spin 0".to_owned()))
    );
    let held = next_change(&mut signals).await;
    assert_eq!(
        held,
        (vec!["Windows".to_owned()], Some("spin 19".to_owned()))
    );
    assert!(started.elapsed() >= Duration::from_millis(900));

    events.send(title(20)).expect("an event");
    events
        .send(Event::WindowFocusChanged(Some(WindowId(2))))
        .expect("an event");
    let focus = next_change(&mut signals).await;
    assert_eq!(
        focus,
        (
            vec!["Focus".to_owned(), "Windows".to_owned()],
            Some("spin 20".to_owned())
        )
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(1300), signals.next())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_native_snapshot_publishes_focus_and_keyboard_together() {
    let bus = PrivateBus::start();
    let (fake, events, shared) = fake(None);
    shared.snapshot.lock().expect("snapshot").keyboard.current = Some(0);
    let (proxy, _task) = served(&bus, fake).await;
    let mut signals = signals(&proxy).await;

    {
        let mut snapshot = shared.snapshot.lock().expect("snapshot");
        snapshot.focus.window = Some(WindowId(2));
        snapshot.keyboard.current = Some(1);
    }
    events
        .send(Event::Resync(Resync::Structure))
        .expect("an event");

    let signal = tokio::time::timeout(Duration::from_secs(5), signals.next())
        .await
        .expect("a change")
        .expect("a signal");
    let args = signal.args().expect("signal arguments");
    assert_eq!(
        args.interface_name().as_str(),
        "me.aresa.Glimpse.Compositor1"
    );
    assert!(args.invalidated_properties().is_empty());
    let mut names: Vec<_> = args.changed_properties().keys().copied().collect();
    names.sort();
    assert_eq!(names, ["Focus", "Keyboard"]);
    let focus: Item = args.changed_properties()["Focus"]
        .try_clone()
        .expect("focus value")
        .try_into()
        .expect("focus map");
    let keyboard: Item = args.changed_properties()["Keyboard"]
        .try_clone()
        .expect("keyboard value")
        .try_into()
        .expect("keyboard map");
    assert_eq!(u64::try_from(&focus[key::WINDOW]).expect("window"), 2);
    assert_eq!(u32::try_from(&keyboard[key::CURRENT]).expect("layout"), 1);
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

    assert!(matches!(
        properties
            .get(interface.clone(), "SessionLockRestore")
            .await,
        Err(zbus::fdo::Error::UnknownProperty(_))
    ));
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
async fn session_lock_restore_reads_live_without_an_event_and_falls_back_on_error() {
    let bus = PrivateBus::start();
    let (fake, _events, shared) = fake(Some(false));
    let (proxy, _task) = served(&bus, fake).await;
    assert_eq!(proxy.session_lock_restore().await.ok(), Some(false));

    *shared.session_lock_restore.lock().expect("lock") = Some(true);
    assert_eq!(proxy.session_lock_restore().await.ok(), Some(true));

    shared.lock_fails.store(true, Ordering::SeqCst);
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
async fn a_keyboard_resync_rereads_the_layouts_without_refetching_the_structure() {
    let bus = PrivateBus::start();
    let (fake, events, shared) = fake(None);
    let (proxy, _task) = served(&bus, fake).await;
    let mut signals = signals(&proxy).await;
    let before = shared.snapshots.load(Ordering::SeqCst);

    shared.snapshot.lock().expect("snapshot").keyboard.current = Some(0);
    events
        .send(Event::Resync(Resync::Keyboard))
        .expect("an event");

    assert_eq!(changed(&mut signals).await, [["Keyboard"]]);
    assert_eq!(shared.snapshots.load(Ordering::SeqCst), before);
}

#[tokio::test]
async fn a_failed_first_snapshot_ends_serve_with_a_backend_error() {
    let bus = PrivateBus::start();
    let (fake, _events, shared) = fake(None);
    shared.snapshot_fails.store(true, Ordering::SeqCst);

    let error = tokio::time::timeout(Duration::from_secs(10), serve(bus.connection().await, fake))
        .await
        .expect("serve to return");

    assert!(matches!(error, ServeError::Backend(_)), "{error}");
    let dbus = zbus::fdo::DBusProxy::new(&bus.connection().await)
        .await
        .expect("dbus");
    let name = zbus::names::BusName::try_from(BUS_NAME).expect("a bus name");
    assert!(!dbus.name_has_owner(name).await.expect("an owner query"));
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

#[tokio::test]
async fn serve_returns_a_clean_exit_when_the_session_bus_dies() {
    let mut bus = PrivateBus::start();
    let (fake, _events, _) = fake(None);
    let (_proxy, task) = served(&bus, fake).await;

    bus.kill();
    let error = tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .expect("serve to return once its bus is gone")
        .expect("serve to finish");

    assert!(matches!(error, ServeError::BusClosed), "{error}");
    assert_eq!(error.exit_code(), 0);
}
