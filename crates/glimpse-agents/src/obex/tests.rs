use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

use super::file::{self, sanitize};
use super::{Agent, AgentError, Config, PATH};

const OBEX_MANAGER: &str = "/org/bluez/obex";
const TRANSFER: &str = "/org/bluez/obex/transfer0";
const OBEX_SESSION: &str = "/org/bluez/obex/session0";
const DEVICE: &str = "/org/bluez/hci0/dev_F8_4E_17_BC_EE_D5";
const DESTINATION: &str = "F8:4E:17:BC:EE:D5";
const LOGIN_SESSION: &str = "/org/freedesktop/login1/session/_1";
const NOTIFICATIONS: &str = "/org/freedesktop/Notifications";
const LOGIN: &str = "org.freedesktop.login1";
const BLUEZ: &str = "org.bluez";
const NOTIFICATIONS_NAME: &str = "org.freedesktop.Notifications";
const OBEX_NAME: &str = "org.bluez.obex";
const WAIT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Call {
    app: String,
    icon: String,
    replaces: u32,
    summary: String,
    body: String,
    actions: Vec<String>,
    value: Option<i32>,
}

struct Notifications {
    calls: Arc<Mutex<Vec<Call>>>,
    closes: Arc<Mutex<Vec<u32>>>,
    next: Arc<AtomicU32>,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Notifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        self.calls.lock().expect("calls").push(Call {
            app: app_name,
            icon: app_icon,
            replaces: replaces_id,
            summary,
            body,
            actions,
            value: hints
                .get("value")
                .and_then(|value| i32::try_from(value).ok()),
        });
        let _ = expire_timeout;
        match replaces_id {
            0 => self.next.fetch_add(1, Ordering::SeqCst) + 1,
            _ => replaces_id,
        }
    }

    fn close_notification(&self, id: u32) {
        self.closes.lock().expect("closes").push(id);
    }

    #[zbus(signal)]
    async fn action_invoked(emitter: &SignalEmitter<'_>, id: u32, action: &str)
    -> zbus::Result<()>;

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;
}

struct Transfer {
    name: String,
    filename: String,
    size: u64,
    transferred: Arc<AtomicU64>,
    status: Arc<Mutex<String>>,
}

#[zbus::interface(name = "org.bluez.obex.Transfer1")]
impl Transfer {
    #[zbus(property)]
    fn status(&self) -> String {
        self.status.lock().expect("status").clone()
    }

    #[zbus(property)]
    fn name(&self) -> String {
        self.name.clone()
    }

    #[zbus(property)]
    fn filename(&self) -> String {
        self.filename.clone()
    }

    #[zbus(property)]
    fn size(&self) -> u64 {
        self.size
    }

    #[zbus(property)]
    fn transferred(&self) -> u64 {
        self.transferred.load(Ordering::SeqCst)
    }

    #[zbus(property)]
    fn session(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(OBEX_SESSION).expect("session path")
    }
}

struct ObexSession {
    destination: String,
}

#[zbus::interface(name = "org.bluez.obex.Session1")]
impl ObexSession {
    #[zbus(property)]
    fn destination(&self) -> String {
        self.destination.clone()
    }
}

struct Bluez {
    present: bool,
    paired: bool,
    address: String,
    alias: String,
}

#[zbus::interface(name = "org.freedesktop.DBus.ObjectManager")]
impl Bluez {
    fn get_managed_objects(
        &self,
    ) -> HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>> {
        let mut objects = HashMap::new();
        if self.present {
            let mut device = HashMap::new();
            let address = Value::from(self.address.clone());
            let alias = Value::from(self.alias.clone());
            let paired = Value::from(self.paired);
            device.insert(
                "Address".to_owned(),
                OwnedValue::try_from(address).expect("address value"),
            );
            device.insert(
                "Alias".to_owned(),
                OwnedValue::try_from(alias).expect("alias value"),
            );
            device.insert(
                "Paired".to_owned(),
                OwnedValue::try_from(paired).expect("paired value"),
            );
            let mut interfaces = HashMap::new();
            interfaces.insert("org.bluez.Device1".to_owned(), device);
            objects.insert(
                OwnedObjectPath::try_from(DEVICE).expect("device path"),
                interfaces,
            );
        }
        objects
    }
}

struct BluezDevice {
    paired: bool,
    address: String,
    alias: String,
}

#[zbus::interface(name = "org.bluez.Device1")]
impl BluezDevice {
    #[zbus(property)]
    fn address(&self) -> String {
        self.address.clone()
    }

    #[zbus(property)]
    fn alias(&self) -> String {
        self.alias.clone()
    }

    #[zbus(property)]
    fn paired(&self) -> bool {
        self.paired
    }
}

struct LogindSession {
    hint: Arc<AtomicBool>,
    fail: bool,
}

#[zbus::interface(name = "org.freedesktop.login1.Session")]
impl LogindSession {
    #[zbus(property)]
    fn active(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn class(&self) -> String {
        "user".to_owned()
    }

    #[zbus(property, name = "Type")]
    fn kind(&self) -> String {
        "wayland".to_owned()
    }

    #[zbus(property, name = "LockedHint")]
    fn locked_hint(&self) -> zbus::fdo::Result<bool> {
        match self.fail {
            true => Err(zbus::fdo::Error::Failed("unavailable".to_owned())),
            false => Ok(self.hint.load(Ordering::SeqCst)),
        }
    }
}

struct LogindManager {
    uid: u32,
}

#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl LogindManager {
    fn list_sessions(&self) -> Vec<glimpse_dbus::login1::Login1SessionEntry> {
        vec![(
            "1".to_owned(),
            self.uid,
            "user".to_owned(),
            "seat0".to_owned(),
            OwnedObjectPath::try_from(LOGIN_SESSION).expect("session path"),
        )]
    }
}

#[derive(Clone, Copy)]
enum Outcome {
    Ok,
    AlreadyExists,
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.obex.Error")]
enum RegisterError {
    AlreadyExists(String),
}

struct ObexManager {
    calls: Arc<Mutex<Vec<String>>>,
    outcome: Outcome,
}

#[zbus::interface(name = "org.bluez.obex.AgentManager1")]
impl ObexManager {
    fn register_agent(&self, agent: ObjectPath<'_>) -> Result<(), RegisterError> {
        self.calls
            .lock()
            .expect("calls")
            .push(format!("register:{}", agent.as_str()));
        match self.outcome {
            Outcome::Ok => Ok(()),
            Outcome::AlreadyExists => Err(RegisterError::AlreadyExists(
                "already registered".to_owned(),
            )),
        }
    }
}

#[derive(Default)]
struct Options {
    alias: String,
    locked: bool,
    locked_fails: bool,
    paired: bool,
    present: bool,
    transfer_name: String,
    size: u64,
    downloads: Option<PathBuf>,
    root: PathBuf,
    config_root: Option<PathBuf>,
    home: PathBuf,
    no_notifications: bool,
    answer_timeout: Duration,
    tick: Duration,
    idle: Duration,
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "glimpse-obex-test-{}-{tag}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        Self(dir)
    }

    fn path(&self) -> PathBuf {
        self.0.clone()
    }

    fn root(&self) -> PathBuf {
        self.0.join("root")
    }

    fn downloads(&self) -> PathBuf {
        self.0.join("downloads")
    }

    fn home(&self) -> PathBuf {
        self.0.join("home")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Harness {
    _bus: glimpse_dbus::testing::PrivateBus,
    server: zbus::Connection,
    agent: Arc<Agent>,
    calls: Arc<Mutex<Vec<Call>>>,
    closes: Arc<Mutex<Vec<u32>>>,
    status: Arc<Mutex<String>>,
    transferred: Arc<AtomicU64>,
}

impl Harness {
    async fn new(options: Options) -> Self {
        let bus = glimpse_dbus::testing::PrivateBus::start();
        let server = bus.connection().await;
        let calls = Arc::new(Mutex::new(Vec::new()));
        let closes = Arc::new(Mutex::new(Vec::new()));
        let status = Arc::new(Mutex::new("active".to_owned()));
        let transferred = Arc::new(AtomicU64::new(0));
        server
            .object_server()
            .at(
                OBEX_MANAGER,
                ObexManager {
                    calls: Arc::new(Mutex::new(Vec::new())),
                    outcome: Outcome::Ok,
                },
            )
            .await
            .expect("obex manager");
        server
            .object_server()
            .at(
                NOTIFICATIONS,
                Notifications {
                    calls: Arc::clone(&calls),
                    closes: Arc::clone(&closes),
                    next: Arc::new(AtomicU32::new(0)),
                },
            )
            .await
            .expect("notifications");
        server
            .object_server()
            .at(
                TRANSFER,
                Transfer {
                    name: options.transfer_name.clone(),
                    filename: options
                        .root
                        .join(&options.transfer_name)
                        .to_string_lossy()
                        .into_owned(),
                    size: options.size,
                    transferred: Arc::clone(&transferred),
                    status: Arc::clone(&status),
                },
            )
            .await
            .expect("transfer");
        server
            .object_server()
            .at(
                OBEX_SESSION,
                ObexSession {
                    destination: DESTINATION.to_owned(),
                },
            )
            .await
            .expect("obex session");
        server
            .object_server()
            .at(
                "/",
                Bluez {
                    present: options.present,
                    paired: options.paired,
                    address: DESTINATION.to_owned(),
                    alias: options.alias.clone(),
                },
            )
            .await
            .expect("bluez");
        server
            .object_server()
            .at(
                DEVICE,
                BluezDevice {
                    paired: options.paired,
                    address: DESTINATION.to_owned(),
                    alias: options.alias.clone(),
                },
            )
            .await
            .expect("bluez device");
        server
            .object_server()
            .at(
                "/org/freedesktop/login1",
                LogindManager {
                    uid: glimpse_dbus::login1::current_uid(),
                },
            )
            .await
            .expect("logind manager");
        server
            .object_server()
            .at(
                LOGIN_SESSION,
                LogindSession {
                    hint: Arc::new(AtomicBool::new(options.locked)),
                    fail: options.locked_fails,
                },
            )
            .await
            .expect("logind session");
        for name in [OBEX_NAME, NOTIFICATIONS_NAME, BLUEZ, LOGIN] {
            if options.no_notifications && name == NOTIFICATIONS_NAME {
                continue;
            }
            glimpse_dbus::own_name(&server, name)
                .await
                .expect("own the bus name");
        }
        let config = Config {
            answer_timeout: options.answer_timeout,
            tick: options.tick,
            idle: options.idle,
            downloads: options.downloads,
            special: || None,
            launch: |_| Ok(()),
            home: options.home,
            root: options.config_root.unwrap_or_else(|| options.root.clone()),
            reactivate: Duration::from_secs(2),
        };
        let agent = Arc::new(Agent::new(
            bus.connection().await,
            bus.connection().await,
            config,
        ));
        Self {
            _bus: bus,
            server,
            agent,
            calls,
            closes,
            status,
            transferred,
        }
    }

    async fn emit_action(&self, id: u32, action: &str) {
        let interface = self
            .server
            .object_server()
            .interface::<_, Notifications>(NOTIFICATIONS)
            .await
            .expect("notifications");
        Notifications::action_invoked(interface.signal_emitter(), id, action)
            .await
            .expect("action invoked");
    }

    async fn emit_closed(&self, id: u32, reason: u32) {
        let interface = self
            .server
            .object_server()
            .interface::<_, Notifications>(NOTIFICATIONS)
            .await
            .expect("notifications");
        Notifications::notification_closed(interface.signal_emitter(), id, reason)
            .await
            .expect("notification closed");
    }

    async fn complete(&self) {
        *self.status.lock().expect("status") = "complete".to_owned();
        self.emit_status().await;
    }

    async fn fail(&self) {
        *self.status.lock().expect("status") = "error".to_owned();
        self.emit_status().await;
    }

    async fn emit_status(&self) {
        let interface = self
            .server
            .object_server()
            .interface::<_, Transfer>(TRANSFER)
            .await
            .expect("transfer");
        let transfer = interface.get().await;
        Transfer::status_changed(&transfer, interface.signal_emitter())
            .await
            .expect("status changed");
    }

    async fn progress(&self, value: u64) {
        self.transferred.store(value, Ordering::SeqCst);
        let interface = self
            .server
            .object_server()
            .interface::<_, Transfer>(TRANSFER)
            .await
            .expect("transfer");
        let transfer = interface.get().await;
        Transfer::transferred_changed(&transfer, interface.signal_emitter())
            .await
            .expect("transferred changed");
    }

    async fn remove_transfer(&self) {
        self.server
            .object_server()
            .remove::<Transfer, _>(TRANSFER)
            .await
            .expect("remove the transfer");
        self.server
            .emit_signal(
                None::<()>,
                "/",
                "org.freedesktop.DBus.ObjectManager",
                "InterfacesRemoved",
                &(path(TRANSFER), vec!["org.bluez.obex.Transfer1"]),
            )
            .await
            .expect("announce the removal");
    }
}

fn path(object: &str) -> ObjectPath<'static> {
    ObjectPath::try_from(object).expect("path").into_owned()
}

fn spawn_push(agent: &Arc<Agent>) -> tokio::task::JoinHandle<Result<String, AgentError>> {
    let agent = Arc::clone(agent);
    tokio::spawn(async move { agent.authorize_push(path(TRANSFER)).await })
}

async fn wait_calls(calls: &Mutex<Vec<Call>>, count: usize) -> Vec<Call> {
    let deadline = Instant::now() + WAIT;
    loop {
        let seen = calls.lock().expect("calls").clone();
        if seen.len() >= count || Instant::now() >= deadline {
            return seen;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn wait_for_call(calls: &Mutex<Vec<Call>>, predicate: impl Fn(&Call) -> bool) -> Vec<Call> {
    let deadline = Instant::now() + WAIT;
    loop {
        let seen = calls.lock().expect("calls").clone();
        if seen.iter().any(&predicate) || Instant::now() >= deadline {
            return seen;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn wait_closes(closes: &Mutex<Vec<u32>>, count: usize) -> Vec<u32> {
    let deadline = Instant::now() + WAIT;
    loop {
        let seen = closes.lock().expect("closes").clone();
        if seen.len() >= count || Instant::now() >= deadline {
            return seen;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn wait_registrations(calls: &Mutex<Vec<String>>, count: usize) -> Vec<String> {
    let deadline = Instant::now() + WAIT;
    loop {
        let seen = calls.lock().expect("calls").clone();
        if seen.len() >= count || Instant::now() >= deadline {
            return seen;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn sandbox(scratch: &Scratch) -> Options {
    Options {
        root: scratch.root(),
        downloads: Some(scratch.downloads()),
        ..defaults()
    }
}

async fn put(dir: &std::path::Path, name: &str, content: &[u8]) {
    tokio::fs::create_dir_all(dir)
        .await
        .expect("create the directory");
    tokio::fs::write(dir.join(name), content)
        .await
        .expect("write the file");
}

async fn missing(path: &std::path::Path) -> bool {
    !tokio::fs::try_exists(path).await.expect("check the path")
}

fn defaults() -> Options {
    Options {
        alias: "Pixel 9".to_owned(),
        paired: true,
        present: true,
        transfer_name: "photo.jpg".to_owned(),
        size: 100,
        answer_timeout: Duration::from_secs(5),
        tick: Duration::from_millis(10),
        idle: Duration::from_secs(600),
        ..Default::default()
    }
}

#[test]
fn sanitize_keeps_the_last_component_and_drops_hostile_characters() {
    assert_eq!(sanitize("../../etc/passwd").as_deref(), Some("passwd"));
    assert_eq!(sanitize("a/b/c.png").as_deref(), Some("c.png"));
    assert_eq!(sanitize("a\\b\\c.png").as_deref(), Some("c.png"));
    assert_eq!(sanitize(".hidden").as_deref(), Some("hidden"));
    assert_eq!(sanitize("..").as_deref(), None);
    assert_eq!(sanitize("...").as_deref(), None);
    assert_eq!(sanitize("").as_deref(), None);
    assert_eq!(sanitize("gpj.\u{202e}exe").as_deref(), Some("gpj. exe"));
    assert_eq!(sanitize("tab\tname.txt").as_deref(), Some("tab name.txt"));
    assert_eq!(sanitize(".\u{202e}gpj.exe").as_deref(), Some("gpj.exe"));
}

#[test]
fn sanitize_caps_the_name_keeping_the_extension() {
    let long = "a".repeat(200);
    assert_eq!(sanitize(&long).expect("a name").chars().count(), 128);
    let named = format!("{long}.txt");
    let capped = sanitize(&named).expect("a name");
    assert_eq!(capped.chars().count(), 128);
    assert!(capped.ends_with(".txt"));
    assert_eq!(
        capped.chars().take(124).collect::<String>(),
        "a".repeat(124)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]

async fn accepting_a_push_replies_a_bare_name_and_moves_the_finished_file() {
    let scratch = Scratch::new("accept");
    let harness = Harness::new(sandbox(&scratch)).await;
    let pushing = spawn_push(&harness.agent);
    let calls = wait_calls(&harness.calls, 1).await;
    assert_eq!(calls.len(), 1);
    let prompt = &calls[0];
    assert_eq!(prompt.app, "Bluetooth");
    assert_eq!(prompt.icon, "bluetooth-symbolic");
    assert_eq!(prompt.replaces, 0);
    assert_eq!(prompt.summary, "Incoming file");
    assert_eq!(prompt.body, "“Pixel 9” wants to send you “photo.jpg”");
    assert_eq!(prompt.actions, ["accept", "Accept", "decline", "Decline"]);
    assert!(prompt.value.is_none());

    harness.emit_action(1, "accept").await;
    let accepted = pushing
        .await
        .expect("the push finishes")
        .expect("the push is accepted");
    assert_eq!(accepted, "photo.jpg");
    assert!(!accepted.contains('/'));

    tokio::time::sleep(Duration::from_millis(100)).await;
    harness.progress(40).await;
    let calls = wait_for_call(&harness.calls, |call| call.value == Some(40)).await;
    let receiving = calls
        .iter()
        .find(|call| call.value == Some(40))
        .expect("a progress call");
    assert_eq!(receiving.replaces, 1);
    assert_eq!(receiving.summary, "Receiving “photo.jpg”");

    let content: Vec<u8> = (0..100).collect();
    put(&scratch.root(), "photo.jpg", &content).await;
    harness.complete().await;
    let calls = wait_for_call(&harness.calls, |call| call.summary.starts_with("Received")).await;
    let received = calls
        .iter()
        .find(|call| call.summary.starts_with("Received"))
        .expect("the completion call");
    assert_eq!(received.replaces, 1);
    assert_eq!(received.summary, "Received “photo.jpg”");
    assert_eq!(
        received.actions,
        ["default", "Open", "folder", "Open Folder"]
    );
    let moved = tokio::fs::read(scratch.downloads().join("photo.jpg"))
        .await
        .expect("the file is in the downloads");
    assert_eq!(moved, content);
    assert!(missing(&scratch.root().join("photo.jpg")).await);
    assert_eq!(wait_closes(&harness.closes, 0).await, Vec::<u32>::new());

    harness.emit_closed(1, 2).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(wait_closes(&harness.closes, 0).await, Vec::<u32>::new());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]

async fn a_failed_transfer_removes_the_root_file_and_notifies_the_failure() {
    let scratch = Scratch::new("error");
    let harness = Harness::new(sandbox(&scratch)).await;
    let pushing = spawn_push(&harness.agent);
    wait_calls(&harness.calls, 1).await;
    harness.emit_action(1, "accept").await;
    pushing
        .await
        .expect("the push finishes")
        .expect("the push is accepted");
    put(&scratch.root(), "photo.jpg", &[7; 30]).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    harness.fail().await;
    let calls = wait_for_call(&harness.calls, |call| {
        call.summary.starts_with("Could not receive")
    })
    .await;
    let failed = calls
        .iter()
        .find(|call| call.summary.starts_with("Could not receive"))
        .expect("the failure call");
    assert_eq!(failed.replaces, 1);
    assert_eq!(failed.summary, "Could not receive “photo.jpg”");
    assert!(failed.actions.is_empty());
    assert!(missing(&scratch.root().join("photo.jpg")).await);
    assert!(missing(&scratch.downloads().join("photo.jpg")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_decline_a_close_and_a_timeout_reject_the_push() {
    let scratch = Scratch::new("reject");
    let harness = Harness::new(Options {
        answer_timeout: Duration::from_millis(100),
        ..sandbox(&scratch)
    })
    .await;

    let pushing = spawn_push(&harness.agent);
    wait_calls(&harness.calls, 1).await;
    harness.emit_action(1, "decline").await;
    assert!(matches!(
        pushing.await.expect("the push finishes"),
        Err(AgentError::Rejected(_))
    ));
    assert_eq!(wait_closes(&harness.closes, 1).await, [1]);

    let pushing = spawn_push(&harness.agent);
    wait_calls(&harness.calls, 2).await;
    harness.emit_closed(2, 2).await;
    assert!(matches!(
        pushing.await.expect("the push finishes"),
        Err(AgentError::Rejected(_))
    ));
    assert_eq!(wait_closes(&harness.closes, 2).await, [1, 2]);

    let pushing = spawn_push(&harness.agent);
    wait_calls(&harness.calls, 3).await;
    assert!(matches!(
        pushing.await.expect("the push finishes"),
        Err(AgentError::Rejected(_))
    ));
    assert_eq!(wait_closes(&harness.closes, 3).await, [1, 2, 3]);
    assert_eq!(calls_count(&harness.calls).await, 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_locked_or_unreadable_session_rejects_the_push() {
    for locked in [true, false] {
        let harness = Harness::new(Options {
            locked,
            locked_fails: !locked,
            ..defaults()
        })
        .await;
        let pushing = spawn_push(&harness.agent);
        assert!(matches!(
            pushing.await.expect("the push finishes"),
            Err(AgentError::Rejected(_))
        ));
        assert_eq!(calls_count(&harness.calls).await, 0);
        assert_eq!(wait_closes(&harness.closes, 0).await, Vec::<u32>::new());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unpaired_or_unknown_sender_rejects_the_push() {
    for (paired, present) in [(false, true), (false, false)] {
        let harness = Harness::new(Options {
            paired,
            present,
            ..defaults()
        })
        .await;
        let pushing = spawn_push(&harness.agent);
        assert!(matches!(
            pushing.await.expect("the push finishes"),
            Err(AgentError::Rejected(_))
        ));
        assert_eq!(calls_count(&harness.calls).await, 0);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn obexd_canceling_closes_the_notification() {
    let scratch = Scratch::new("cancel");
    let harness = Harness::new(sandbox(&scratch)).await;
    let pushing = spawn_push(&harness.agent);
    wait_calls(&harness.calls, 1).await;
    harness.agent.cancel().await;
    assert!(matches!(
        pushing.await.expect("the push finishes"),
        Err(AgentError::Rejected(_))
    ));
    assert_eq!(wait_closes(&harness.closes, 1).await, [1]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]

async fn a_hostile_name_is_sanitised_and_duplicates_are_renamed() {
    let scratch = Scratch::new("hostile");
    put(&scratch.downloads(), "pic.png", b"old").await;
    let harness = Harness::new(Options {
        transfer_name: "folder/../../\u{202e}pic.png".to_owned(),
        ..sandbox(&scratch)
    })
    .await;
    let pushing = spawn_push(&harness.agent);
    let calls = wait_calls(&harness.calls, 1).await;
    assert_eq!(calls[0].body, "“Pixel 9” wants to send you “pic.png”");
    harness.emit_action(1, "accept").await;
    let accepted = pushing
        .await
        .expect("the push finishes")
        .expect("the push is accepted");
    assert_eq!(accepted, "pic.png");
    put(&scratch.root(), "pic.png", b"new").await;
    harness.complete().await;
    wait_for_call(&harness.calls, |call| call.summary.starts_with("Received")).await;
    let moved = tokio::fs::read(scratch.downloads().join("pic (1).png"))
        .await
        .expect("the file lands under a free name");
    assert_eq!(moved, b"new");
    let kept = tokio::fs::read(scratch.downloads().join("pic.png"))
        .await
        .expect("the old file stays");
    assert_eq!(kept, b"old");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unusable_name_rejects_the_push() {
    let harness = Harness::new(Options {
        transfer_name: "../".to_owned(),
        ..defaults()
    })
    .await;
    let pushing = spawn_push(&harness.agent);
    assert!(matches!(
        pushing.await.expect("the push finishes"),
        Err(AgentError::Rejected(_))
    ));
    assert_eq!(calls_count(&harness.calls).await, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unusable_download_directory_rejects_after_accept() {
    let scratch = Scratch::new("nodir");
    tokio::fs::create_dir_all(scratch.path())
        .await
        .expect("scratch");
    tokio::fs::write(scratch.path().join("occupied"), [])
        .await
        .expect("write");
    let blocked = scratch.path().join("occupied/nested");
    let harness = Harness::new(Options {
        downloads: Some(blocked.clone()),
        root: scratch.root(),
        ..defaults()
    })
    .await;
    let pushing = spawn_push(&harness.agent);
    wait_calls(&harness.calls, 1).await;
    harness.emit_action(1, "accept").await;
    assert!(matches!(
        pushing.await.expect("the push finishes"),
        Err(AgentError::Rejected(_))
    ));
    assert_eq!(wait_closes(&harness.closes, 1).await, [1]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]

async fn complete_then_object_removed_still_received() {
    let scratch = Scratch::new("complete-removed");
    let harness = Harness::new(sandbox(&scratch)).await;
    let pushing = spawn_push(&harness.agent);
    wait_calls(&harness.calls, 1).await;
    harness.emit_action(1, "accept").await;
    pushing
        .await
        .expect("the push finishes")
        .expect("the push is accepted");
    put(&scratch.root(), "photo.jpg", &[1; 100]).await;
    harness.complete().await;
    harness.remove_transfer().await;
    let calls = wait_for_call(&harness.calls, |call| call.summary.starts_with("Received")).await;
    let received = calls
        .iter()
        .find(|call| call.summary.starts_with("Received"))
        .expect("the completion call");
    assert_eq!(received.replaces, 1);
    assert_eq!(received.summary, "Received “photo.jpg”");
    assert!(!missing(&scratch.downloads().join("photo.jpg")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]

async fn error_then_object_removed_shows_the_failure() {
    let scratch = Scratch::new("error-removed");
    let harness = Harness::new(sandbox(&scratch)).await;
    let pushing = spawn_push(&harness.agent);
    wait_calls(&harness.calls, 1).await;
    harness.emit_action(1, "accept").await;
    pushing
        .await
        .expect("the push finishes")
        .expect("the push is accepted");
    put(&scratch.root(), "photo.jpg", &[1; 100]).await;
    harness.fail().await;
    harness.remove_transfer().await;
    let calls = wait_for_call(&harness.calls, |call| {
        call.summary.starts_with("Could not receive")
    })
    .await;
    let failed = calls
        .iter()
        .find(|call| call.summary.starts_with("Could not receive"))
        .expect("the failure call");
    assert_eq!(failed.replaces, 1);
    assert!(missing(&scratch.root().join("photo.jpg")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]

async fn an_object_removed_with_a_complete_file_moves_it_and_shows_received() {
    let scratch = Scratch::new("removed-complete");
    let harness = Harness::new(sandbox(&scratch)).await;
    let pushing = spawn_push(&harness.agent);
    wait_calls(&harness.calls, 1).await;
    harness.emit_action(1, "accept").await;
    pushing
        .await
        .expect("the push finishes")
        .expect("the push is accepted");
    put(&scratch.root(), "photo.jpg", &[3; 100]).await;
    harness.remove_transfer().await;
    let calls = wait_for_call(&harness.calls, |call| call.summary.starts_with("Received")).await;
    let received = calls
        .iter()
        .find(|call| call.summary.starts_with("Received"))
        .expect("the completion call");
    assert_eq!(received.replaces, 1);
    assert_eq!(received.summary, "Received “photo.jpg”");
    let moved = tokio::fs::read(scratch.downloads().join("photo.jpg"))
        .await
        .expect("the file is in the downloads");
    assert_eq!(moved, [3; 100]);
    assert!(missing(&scratch.root().join("photo.jpg")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]

async fn an_object_removed_with_a_short_file_removes_it_and_shows_the_failure() {
    let scratch = Scratch::new("removed-short");
    let harness = Harness::new(sandbox(&scratch)).await;
    let pushing = spawn_push(&harness.agent);
    wait_calls(&harness.calls, 1).await;
    harness.emit_action(1, "accept").await;
    pushing
        .await
        .expect("the push finishes")
        .expect("the push is accepted");
    put(&scratch.root(), "photo.jpg", &[3; 10]).await;
    harness.remove_transfer().await;
    let calls = wait_for_call(&harness.calls, |call| {
        call.summary.starts_with("Could not receive")
    })
    .await;
    let failed = calls
        .iter()
        .find(|call| call.summary.starts_with("Could not receive"))
        .expect("the failure call");
    assert_eq!(failed.replaces, 1);
    assert!(missing(&scratch.root().join("photo.jpg")).await);
    assert!(missing(&scratch.downloads().join("photo.jpg")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_notifications_replace_the_same_id_and_are_throttled() {
    let scratch = Scratch::new("progress");
    let harness = Harness::new(Options {
        tick: Duration::from_millis(300),
        ..sandbox(&scratch)
    })
    .await;
    let pushing = spawn_push(&harness.agent);
    wait_calls(&harness.calls, 1).await;
    harness.emit_action(1, "accept").await;
    pushing
        .await
        .expect("the push finishes")
        .expect("the push is accepted");
    tokio::time::sleep(Duration::from_millis(100)).await;
    harness.progress(40).await;
    let calls = wait_for_call(&harness.calls, |call| call.value == Some(40)).await;
    let receiving = calls
        .iter()
        .find(|call| call.value == Some(40))
        .expect("a progress call");
    assert_eq!(receiving.replaces, 1);
    assert_eq!(receiving.summary, "Receiving “photo.jpg”");
    harness.progress(41).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    let calls = harness.calls.lock().expect("calls").clone();
    assert!(!calls.iter().any(|call| call.value == Some(41)));
    tokio::time::sleep(Duration::from_millis(200)).await;
    harness.progress(50).await;
    let calls = wait_for_call(&harness.calls, |call| call.value == Some(50)).await;
    let receiving = calls
        .iter()
        .find(|call| call.value == Some(50))
        .expect("a later progress call");
    assert_eq!(receiving.replaces, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unrelated_close_does_not_cancel_a_pending_push() {
    let scratch = Scratch::new("unrelated-close");
    let harness = Harness::new(sandbox(&scratch)).await;
    let pushing = spawn_push(&harness.agent);
    wait_calls(&harness.calls, 1).await;
    harness.emit_closed(2, 2).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(!pushing.is_finished());
    assert_eq!(wait_closes(&harness.closes, 0).await, Vec::<u32>::new());
    harness.emit_action(1, "accept").await;
    pushing
        .await
        .expect("the push finishes")
        .expect("the push is accepted");
    assert_eq!(wait_closes(&harness.closes, 0).await, Vec::<u32>::new());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unrelated_close_does_not_end_the_received_open_handling() {
    let scratch = Scratch::new("unrelated-open");
    let harness = Harness::new(sandbox(&scratch)).await;
    let pushing = spawn_push(&harness.agent);
    wait_calls(&harness.calls, 1).await;
    harness.emit_action(1, "accept").await;
    pushing
        .await
        .expect("the push finishes")
        .expect("the push is accepted");
    put(&scratch.root(), "photo.jpg", &[1; 100]).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    harness.complete().await;
    wait_for_call(&harness.calls, |call| call.summary.starts_with("Received")).await;
    harness.emit_closed(9, 2).await;
    harness.emit_action(1, "default").await;
    assert_eq!(wait_closes(&harness.closes, 1).await, [1]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_quiet_transfer_fails_after_the_idle_bound() {
    let scratch = Scratch::new("idle");
    let harness = Harness::new(Options {
        idle: Duration::from_millis(200),
        ..sandbox(&scratch)
    })
    .await;
    let pushing = spawn_push(&harness.agent);
    wait_calls(&harness.calls, 1).await;
    harness.emit_action(1, "accept").await;
    pushing
        .await
        .expect("the push finishes")
        .expect("the push is accepted");
    let calls = wait_for_call(&harness.calls, |call| {
        call.summary.starts_with("Could not receive")
    })
    .await;
    let failed = calls
        .iter()
        .find(|call| call.summary.starts_with("Could not receive"))
        .expect("the failure call");
    assert_eq!(failed.replaces, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]

async fn the_downloads_fall_back_to_the_home_downloads_directory() {
    let scratch = Scratch::new("home");
    let harness = Harness::new(Options {
        downloads: None,
        home: scratch.home(),
        ..sandbox(&scratch)
    })
    .await;
    let pushing = spawn_push(&harness.agent);
    wait_calls(&harness.calls, 1).await;
    harness.emit_action(1, "accept").await;
    pushing
        .await
        .expect("the push finishes")
        .expect("the push is accepted");
    put(&scratch.root(), "photo.jpg", &[5; 100]).await;
    harness.complete().await;
    wait_for_call(&harness.calls, |call| call.summary.starts_with("Received")).await;
    let moved = tokio::fs::read(scratch.home().join("Downloads").join("photo.jpg"))
        .await
        .expect("the file is in the home downloads");
    assert_eq!(moved, [5; 100]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]

async fn the_filename_parent_is_the_source_directory() {
    let scratch = Scratch::new("filename");
    let harness = Harness::new(Options {
        config_root: Some(scratch.path().join("elsewhere")),
        ..sandbox(&scratch)
    })
    .await;
    let pushing = spawn_push(&harness.agent);
    wait_calls(&harness.calls, 1).await;
    harness.emit_action(1, "accept").await;
    pushing
        .await
        .expect("the push finishes")
        .expect("the push is accepted");
    put(&scratch.root(), "photo.jpg", &[5; 100]).await;
    harness.complete().await;
    wait_for_call(&harness.calls, |call| call.summary.starts_with("Received")).await;
    assert!(!missing(&scratch.downloads().join("photo.jpg")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]

async fn a_lost_notification_service_rejects_the_push() {
    let harness = Harness::new(Options {
        no_notifications: true,
        ..defaults()
    })
    .await;
    let pushing = spawn_push(&harness.agent);
    assert!(matches!(
        pushing.await.expect("the push finishes"),
        Err(AgentError::Rejected(_))
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]

async fn an_accept_emitted_with_a_close_still_accepts() {
    let scratch = Scratch::new("accept-close");
    let harness = Harness::new(sandbox(&scratch)).await;
    let pushing = spawn_push(&harness.agent);
    wait_calls(&harness.calls, 1).await;
    harness.emit_action(1, "accept").await;
    harness.emit_closed(1, 3).await;
    let accepted = pushing
        .await
        .expect("the push finishes")
        .expect("the accept wins over the close");
    assert_eq!(accepted, "photo.jpg");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_never_overwrites_an_existing_file() {
    let scratch = Scratch::new("no-overwrite");
    put(&scratch.downloads(), "name.ext", b"one").await;
    put(&scratch.downloads(), "name (1).ext", b"two").await;
    put(&scratch.root(), "name.ext", b"three").await;
    let target = file::relocate(
        &scratch.root().join("name.ext"),
        &scratch.downloads(),
        "name.ext",
    )
    .await
    .expect("the file moves");
    assert_eq!(target, scratch.downloads().join("name (2).ext"));
    let read = |name: &str| tokio::fs::read(scratch.downloads().join(name));
    assert_eq!(read("name.ext").await.expect("first"), b"one");
    assert_eq!(read("name (1).ext").await.expect("second"), b"two");
    assert_eq!(read("name (2).ext").await.expect("third"), b"three");
    assert!(missing(&scratch.root().join("name.ext")).await);
}

async fn calls_count(calls: &Mutex<Vec<Call>>) -> usize {
    tokio::time::sleep(Duration::from_millis(200)).await;
    calls.lock().expect("calls").len()
}

async fn serve_manager(
    bus: &glimpse_dbus::testing::PrivateBus,
    calls: Arc<Mutex<Vec<String>>>,
    outcome: Outcome,
) -> zbus::Connection {
    let server = bus.connection().await;
    server
        .object_server()
        .at(OBEX_MANAGER, ObexManager { calls, outcome })
        .await
        .expect("obex manager");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match glimpse_dbus::own_name(&server, OBEX_NAME).await {
            Ok(()) => break,
            Err(_) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(error) => panic!("own {OBEX_NAME}: {error}"),
        }
    }
    server
}

async fn wait_until_free(client: &zbus::Connection, name: &str) {
    let dbus = zbus::fdo::DBusProxy::new(client).await.expect("dbus");
    let name = zbus::names::BusName::try_from(name).expect("name");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let owned = dbus.name_has_owner(name.clone()).await.unwrap_or(true);
        if !owned {
            return;
        }
        assert!(Instant::now() < deadline, "{name} stayed owned");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_agent_registers_at_start_and_again_for_every_obexd_owner() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let server = serve_manager(&bus, Arc::clone(&calls), Outcome::Ok).await;
    let client = bus.connection().await;
    let config = Config {
        reactivate: Duration::from_millis(800),
        ..Config::default()
    };
    let task = tokio::spawn(super::spawn_with(client.clone(), client.clone(), config));
    assert_eq!(
        wait_registrations(&calls, 2).await,
        [format!("register:{PATH}"), format!("register:{PATH}"),]
    );

    drop(server);
    wait_until_free(&client, OBEX_NAME).await;
    let _server = serve_manager(&bus, Arc::clone(&calls), Outcome::Ok).await;
    assert_eq!(wait_registrations(&calls, 3).await.len(), 3);
    assert_eq!(
        wait_registrations(&calls, 4).await,
        [
            format!("register:{PATH}"),
            format!("register:{PATH}"),
            format!("register:{PATH}"),
            format!("register:{PATH}"),
        ]
    );
    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_existing_registration_counts_as_success() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let _server = serve_manager(&bus, Arc::clone(&calls), Outcome::AlreadyExists).await;
    let client = bus.connection().await;
    let task = tokio::spawn(super::spawn(client.clone(), client.clone()));
    assert_eq!(
        wait_registrations(&calls, 2).await,
        [format!("register:{PATH}"), format!("register:{PATH}"),]
    );
    client
        .object_server()
        .interface::<_, Agent>(PATH)
        .await
        .expect("the agent stays exported");
    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_obexd_the_agent_stays_quiet() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let client = bus.connection().await;
    let task = tokio::spawn(super::spawn(client.clone(), client.clone()));
    tokio::time::sleep(Duration::from_millis(300)).await;
    client
        .object_server()
        .interface::<_, Agent>(PATH)
        .await
        .expect("the agent stays exported");
    task.abort();
}
