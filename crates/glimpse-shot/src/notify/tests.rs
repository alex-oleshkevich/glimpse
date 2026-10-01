use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use glimpse_dbus::testing::PrivateBus;
use glimpse_shot::ports::Notice;
use tokio::sync::{Notify, watch};
use zbus::connection::Connection;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedValue;

use super::*;

const PATH: &str = "/org/freedesktop/Notifications";
const NAME: &str = "org.freedesktop.Notifications";
const VIEWER: &str = "org.example.Image-Viewer";

#[derive(Clone)]
struct Posted {
    id: u32,
    app_name: String,
    app_icon: String,
    summary: String,
    body: String,
    actions: Vec<String>,
    hints: HashMap<String, OwnedValue>,
    expire_timeout: i32,
}

struct Server {
    tx: watch::Sender<Option<Posted>>,
    next: u32,
    delay: Duration,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Server {
    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &mut self,
        app_name: String,
        _replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        tokio::time::sleep(self.delay).await;
        self.next = self.next.saturating_add(1).max(1);
        let id = self.next;
        let _ = self.tx.send(Some(Posted {
            id,
            app_name,
            app_icon,
            summary,
            body,
            actions,
            hints,
            expire_timeout,
        }));
        id
    }

    async fn close_notification(&mut self, _id: u32) {}

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn activation_token(
        emitter: &SignalEmitter<'_>,
        id: u32,
        token: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;
}

#[derive(Clone, PartialEq, Eq, Debug)]
struct Opened {
    uris: Vec<String>,
    activation_token: String,
    startup_id: String,
}

struct Viewer {
    entered: watch::Sender<bool>,
    release: Arc<Notify>,
    calls: watch::Sender<Option<Opened>>,
}

#[zbus::interface(name = "org.freedesktop.Application")]
impl Viewer {
    async fn open(&mut self, uris: Vec<String>, platform_data: HashMap<String, OwnedValue>) {
        let _ = self.entered.send(true);
        self.release.notified().await;
        let _ = self.calls.send(Some(Opened {
            uris,
            activation_token: value_str(&platform_data, "activation-token"),
            startup_id: value_str(&platform_data, "desktop-startup-id"),
        }));
    }
}

fn value_str(data: &HashMap<String, OwnedValue>, key: &str) -> String {
    data.get(key)
        .and_then(|value| <&str>::try_from(value).ok())
        .unwrap_or("")
        .to_owned()
}

struct Fixture {
    server: Connection,
    client: Connection,
    incoming: watch::Receiver<Option<Posted>>,
    _bus: PrivateBus,
}

impl Fixture {
    async fn start() -> Self {
        Self::answering_after(Duration::ZERO).await
    }

    async fn answering_after(delay: Duration) -> Self {
        let bus = PrivateBus::isolated();
        let (tx, incoming) = watch::channel(None);
        let server = bus.connection().await;
        server
            .object_server()
            .at(PATH, Server { tx, next: 0, delay })
            .await
            .expect("the fixture is exported");
        glimpse_dbus::own_name(&server, NAME)
            .await
            .expect("the fixture owns the name");
        let client = bus.connection().await;
        Self {
            server,
            client,
            incoming,
            _bus: bus,
        }
    }

    fn notifier(&self, opener: Arc<dyn Opener>) -> BusNotifier {
        BusNotifier::on(self.client.clone(), opener)
    }

    async fn posted(&mut self) -> Posted {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Some(posted) = self.incoming.borrow().clone() {
                    return posted;
                }
                self.incoming.changed().await.expect("the watch stays open");
            }
        })
        .await
        .expect("the fixture received the post")
    }

    async fn emit_token(&self, id: u32, token: &str) {
        let iface = self.interface().await;
        Server::activation_token(iface.signal_emitter(), id, token)
            .await
            .expect("activation token");
    }

    async fn emit_action(&self, id: u32, key: &str) {
        let iface = self.interface().await;
        Server::action_invoked(iface.signal_emitter(), id, key)
            .await
            .expect("action");
    }

    async fn emit_closed(&self, id: u32) {
        let iface = self.interface().await;
        Server::notification_closed(iface.signal_emitter(), id, 1)
            .await
            .expect("closed");
    }

    async fn interface(&self) -> zbus::object_server::InterfaceRef<Server> {
        self.server
            .object_server()
            .interface(PATH)
            .await
            .expect("the fixture interface")
    }
}

#[derive(Clone, Default)]
struct Recording {
    launches: Arc<Mutex<Vec<(PathBuf, String)>>>,
    arrived: Arc<Notify>,
}

impl Recording {
    async fn launched(&self) -> (PathBuf, String) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(launch) = self.latest() {
                    return launch;
                }
                self.arrived.notified().await;
            }
        })
        .await
        .expect("the opener is called")
    }

    fn latest(&self) -> Option<(PathBuf, String)> {
        self.launches.lock().expect("lock").last().cloned()
    }
}

impl Opener for Recording {
    fn launch(&self, _connection: &Connection, path: &Path, token: &str) -> BoxFuture<'static, ()> {
        let launches = Arc::clone(&self.launches);
        let arrived = Arc::clone(&self.arrived);
        let path = path.to_owned();
        let token = token.to_owned();
        Box::pin(async move {
            launches.lock().expect("lock").push((path, token));
            arrived.notify_one();
        })
    }
}

fn saved(body: &str) -> Notice {
    Notice {
        summary: "Screenshot saved".to_owned(),
        body: body.to_owned(),
        image: Some(PathBuf::from(body)),
        actions: true,
    }
}

fn hint_str(hints: &HashMap<String, OwnedValue>, key: &str) -> String {
    hints
        .get(key)
        .and_then(|value| <&str>::try_from(value).ok())
        .unwrap_or("")
        .to_owned()
}

fn hint_bool(hints: &HashMap<String, OwnedValue>, key: &str) -> Option<bool> {
    hints.get(key).and_then(|value| bool::try_from(value).ok())
}

#[test]
fn an_activatable_app_id_becomes_its_object_path() {
    assert_eq!(
        application_path("org.example.Image-Viewer"),
        "/org/example/Image_Viewer"
    );
}

#[test]
fn the_bus_name_is_the_desktop_file_name_when_it_is_a_valid_one() {
    assert_eq!(
        bus_name_of(Path::new(
            "/usr/share/applications/kde/org.kde.Viewer.desktop"
        )),
        Some("org.kde.Viewer".to_owned())
    );
    assert_eq!(
        bus_name_of(Path::new("/usr/share/applications/viewer.desktop")),
        None
    );
    assert_eq!(
        bus_name_of(Path::new("/usr/share/applications/org.kde.Viewer")),
        None
    );
}

#[tokio::test]
async fn a_saved_shot_is_posted_transient_with_actions_and_an_image() {
    let mut fixture = Fixture::start().await;
    let notifier = fixture.notifier(Arc::new(Recording::default()));
    let path = "/tmp/shot.png";
    notifier.notify(saved(path)).expect("started");
    let posted = fixture.posted().await;

    assert_eq!(posted.app_name, gettext("Screenshot"));
    assert_eq!(posted.app_icon, "");
    assert_eq!(posted.summary, gettext("Screenshot saved"));
    assert_eq!(posted.expire_timeout, EXPIRE_MS);
    assert_eq!(hint_bool(&posted.hints, "transient"), Some(true));
    assert_eq!(hint_str(&posted.hints, "desktop-entry"), DESKTOP_ENTRY);
    assert_eq!(
        hint_str(&posted.hints, "image-path"),
        file_uri(Path::new(path))
    );
    assert_eq!(
        posted.actions,
        vec![
            "default".to_owned(),
            pgettext("notification action", "Open"),
            "folder".to_owned(),
            gettext("Show in Folder"),
        ]
    );
}

#[tokio::test]
async fn a_copied_shot_is_posted_without_actions_or_an_image() {
    let mut fixture = Fixture::start().await;
    let notifier = fixture.notifier(Arc::new(Recording::default()));
    notifier
        .notify(Notice {
            summary: "Screenshot copied".to_owned(),
            body: String::new(),
            image: None,
            actions: false,
        })
        .expect("started");
    let posted = fixture.posted().await;

    assert_eq!(posted.summary, gettext("Screenshot copied"));
    assert!(posted.actions.is_empty());
    assert!(!posted.hints.contains_key("image-path"));
    assert_eq!(posted.expire_timeout, EXPIRE_MS);
    assert_eq!(hint_bool(&posted.hints, "transient"), Some(true));
}

#[tokio::test]
async fn a_file_name_with_markup_is_escaped_in_the_body() {
    let mut fixture = Fixture::start().await;
    let notifier = fixture.notifier(Arc::new(Recording::default()));
    notifier.notify(saved("/tmp/a<b>&c>.png")).expect("started");
    let posted = fixture.posted().await;

    assert_eq!(posted.body, "/tmp/a&lt;b&gt;&amp;c&gt;.png");
}

#[tokio::test]
async fn an_open_action_launches_with_the_activation_token() {
    let mut fixture = Fixture::start().await;
    let recording = Recording::default();
    let notifier = fixture.notifier(Arc::new(recording.clone()));
    let path = PathBuf::from("/tmp/shot.png");
    notifier
        .notify(saved(path.to_str().expect("utf-8")))
        .expect("started");
    let posted = fixture.posted().await;

    fixture.emit_token(posted.id, "token-7").await;
    fixture.emit_action(posted.id, "default").await;

    let (opened, token) = recording.launched().await;
    assert_eq!(opened, path);
    assert_eq!(token, "token-7");
    tokio::time::timeout(Duration::from_secs(2), notifier.closed())
        .await
        .expect("handling the action ends the duty");
}

#[tokio::test]
async fn an_activatable_handler_is_opened_with_the_activation_token() {
    let mut fixture = Fixture::start().await;
    let (entered_tx, mut entered) = watch::channel(false);
    let (calls_tx, mut calls) = watch::channel(None);
    let release = Arc::new(Notify::new());
    fixture
        .server
        .object_server()
        .at(
            application_path(VIEWER),
            Viewer {
                entered: entered_tx,
                release: Arc::clone(&release),
                calls: calls_tx,
            },
        )
        .await
        .expect("the viewer is exported");
    glimpse_dbus::own_name(&fixture.server, VIEWER)
        .await
        .expect("the viewer owns its name");
    let opener = Arc::new(Desktop {
        resolve: Arc::new(|_| Some(Handler::Activatable(VIEWER.to_owned()))),
    });
    let notifier = fixture.notifier(opener);
    let path = "/tmp/shot.png";
    notifier.notify(saved(path)).expect("started");
    let posted = fixture.posted().await;

    fixture.emit_token(posted.id, "token-7").await;
    fixture.emit_action(posted.id, "default").await;
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if *entered.borrow() {
                return;
            }
            entered.changed().await.expect("the watch stays open");
        }
    })
    .await
    .expect("Open was called");

    let duty = notifier.closed();
    tokio::pin!(duty);
    tokio::select! {
        _ = &mut duty => panic!("hold returned before Open answered"),
        () = tokio::time::sleep(Duration::from_millis(200)) => {}
    }
    release.notify_one();

    let opened = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(opened) = calls.borrow().clone() {
                return opened;
            }
            calls.changed().await.expect("the watch stays open");
        }
    })
    .await
    .expect("Open answered");
    assert_eq!(
        opened,
        Opened {
            uris: vec![file_uri(Path::new(path))],
            activation_token: "token-7".to_owned(),
            startup_id: "token-7".to_owned(),
        }
    );
    tokio::time::timeout(Duration::from_secs(2), duty)
        .await
        .expect("the duty ends after Open answers");
}

#[tokio::test]
async fn show_in_folder_falls_back_to_opening_the_containing_folder() {
    let mut fixture = Fixture::start().await;
    let recording = Recording::default();
    let notifier = fixture.notifier(Arc::new(recording.clone()));
    notifier.notify(saved("/tmp/shots/a.png")).expect("started");
    let posted = fixture.posted().await;

    fixture.emit_token(posted.id, "token-folder").await;
    fixture.emit_action(posted.id, "folder").await;

    let (opened, token) = recording.launched().await;
    assert_eq!(opened, PathBuf::from("/tmp/shots"));
    assert_eq!(token, "token-folder");
}

#[tokio::test]
async fn a_close_for_this_notification_ends_a_hold_with_no_clipboard_duty() {
    let mut fixture = Fixture::start().await;
    let notifier = fixture.notifier(Arc::new(Recording::default()));
    notifier.notify(saved("/tmp/shot.png")).expect("started");
    let posted = fixture.posted().await;
    let duty = crate::hold::until_done(None::<std::future::Ready<()>>, Some(notifier.closed()));
    tokio::pin!(duty);

    fixture.emit_closed(posted.id).await;

    tokio::time::timeout(Duration::from_secs(2), duty)
        .await
        .expect("the notify duty ends and so does hold");
}

#[tokio::test]
async fn a_close_for_another_notification_does_not_end_the_duty() {
    let mut fixture = Fixture::start().await;
    let notifier = fixture.notifier(Arc::new(Recording::default()));
    notifier.notify(saved("/tmp/shot.png")).expect("started");
    let posted = fixture.posted().await;
    let duty = notifier.closed();
    tokio::pin!(duty);

    fixture.emit_closed(posted.id.wrapping_add(1)).await;
    tokio::select! {
        _ = &mut duty => panic!("a different id ended the duty"),
        () = tokio::time::sleep(Duration::from_millis(400)) => {}
    }

    fixture.emit_closed(posted.id).await;
    tokio::time::timeout(Duration::from_secs(2), duty)
        .await
        .expect("our id ends the duty");
}

#[tokio::test]
async fn a_missing_notification_server_ends_the_duty() {
    let bus = PrivateBus::isolated();
    let client = bus.connection().await;
    let notifier = BusNotifier::on(client, Arc::new(Recording::default()));
    notifier.notify(saved("/tmp/shot.png")).expect("started");
    tokio::time::timeout(Duration::from_secs(2), notifier.closed())
        .await
        .expect("no server still ends the duty");
}

#[tokio::test(start_paused = true)]
async fn the_notify_duty_ends_at_the_grace_when_nothing_arrives() {
    tokio::time::resume();
    let fixture = Fixture::answering_after(Duration::from_millis(50)).await;
    let notifier = fixture.notifier(Arc::new(Recording::default()));
    notifier.hold_grace();
    let mut returned = notifier.notify_returned();
    let duty = crate::hold::notified(&notifier, saved("/tmp/shot.png"));
    tokio::pin!(duty);
    while !*returned.borrow() {
        tokio::select! {
            biased;
            _ = &mut duty => panic!("the duty ended before Notify returned"),
            changed = returned.changed() => {
                changed.expect("the watch stays open");
            }
        }
    }
    tokio::time::pause();
    notifier.release_grace();
    tokio::task::yield_now().await;

    let started = tokio::time::Instant::now();
    tokio::time::advance(crate::hold::NOTIFY_GRACE - Duration::from_millis(1)).await;
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
    tokio::select! {
        biased;
        _ = &mut duty => panic!("the duty ended before the grace"),
        _ = std::future::ready(()) => {}
    }
    tokio::time::advance(Duration::from_millis(1)).await;
    (&mut duty).await;
    let elapsed = started.elapsed();
    assert!(
        elapsed >= crate::hold::NOTIFY_GRACE
            && elapsed - crate::hold::NOTIFY_GRACE < Duration::from_millis(2),
        "the duty ended {elapsed:?} after Notify returned"
    );
}
