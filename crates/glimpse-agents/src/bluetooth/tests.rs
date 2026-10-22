use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zbus::zvariant::ObjectPath;

use super::{Agent, AgentError, Answer, Kind, PATH};
use crate::app::view_for;
use crate::ask::{Ask, Input, Prompt, Reply, Source};
use crate::dialog::{Field, Icon};

const DEVICE: &str = "/org/bluez/hci0/dev_F8_4E_17_BC_EE_D5";
const MISSING: &str = "/org/bluez/hci0/dev_00_00_00_00_00_00";
const SERVICE: &str = "0000110b-0000-1000-8000-00805f9b34fb";
const WAIT: Duration = Duration::from_secs(2);

struct Device {
    alias: String,
    bonded: AtomicBool,
    paired: AtomicBool,
    cancels: Arc<AtomicUsize>,
}

#[zbus::interface(name = "org.bluez.Device1")]
impl Device {
    #[zbus(property)]
    fn alias(&self) -> String {
        self.alias.clone()
    }

    #[zbus(property)]
    fn bonded(&self) -> bool {
        self.bonded.load(Ordering::Relaxed)
    }

    #[zbus(property)]
    fn paired(&self) -> bool {
        self.paired.load(Ordering::Relaxed)
    }

    fn cancel_pairing(&self) {
        self.cancels.fetch_add(1, Ordering::SeqCst);
    }
}

struct Manager {
    calls: Arc<Mutex<Vec<String>>>,
    refuse_default: bool,
}

#[zbus::interface(name = "org.bluez.AgentManager1")]
impl Manager {
    fn register_agent(&self, agent: ObjectPath<'_>, capability: &str) {
        self.calls
            .lock()
            .expect("calls")
            .push(format!("register:{}:{capability}", agent.as_str()));
    }

    fn request_default_agent(&self, agent: ObjectPath<'_>) -> zbus::fdo::Result<()> {
        self.calls
            .lock()
            .expect("calls")
            .push(format!("default:{}", agent.as_str()));
        match self.refuse_default {
            true => Err(zbus::fdo::Error::Failed("no".to_owned())),
            false => Ok(()),
        }
    }
}

struct Harness {
    _bus: glimpse_dbus::testing::PrivateBus,
    server: Option<zbus::Connection>,
    agent: Arc<Agent>,
    input: relm4::Receiver<Input>,
    cancels: Arc<AtomicUsize>,
}

impl Harness {
    async fn bare() -> Self {
        let bus = glimpse_dbus::testing::PrivateBus::start();
        let (sender, input) = relm4::channel();
        let agent = Arc::new(Agent::new(bus.connection().await, sender));
        Self {
            _bus: bus,
            server: None,
            agent,
            input,
            cancels: Arc::new(AtomicUsize::new(0)),
        }
    }

    async fn device(alias: &str, bonded: bool) -> Self {
        let bus = glimpse_dbus::testing::PrivateBus::start();
        let server = bus.connection().await;
        let cancels = Arc::new(AtomicUsize::new(0));
        server
            .object_server()
            .at(
                DEVICE,
                Device {
                    alias: alias.to_owned(),
                    bonded: AtomicBool::new(bonded),
                    paired: AtomicBool::new(false),
                    cancels: Arc::clone(&cancels),
                },
            )
            .await
            .expect("export the device");
        glimpse_dbus::own_name(&server, glimpse_dbus::bluez::SERVICE)
            .await
            .expect("own org.bluez");
        let (sender, input) = relm4::channel();
        let agent = Arc::new(Agent::new(bus.connection().await, sender));
        Self {
            _bus: bus,
            server: Some(server),
            agent,
            input,
            cancels,
        }
    }

    async fn set_bonded(&self, bonded: bool) {
        let server = self.server.as_ref().expect("a device");
        let interface = server
            .object_server()
            .interface::<_, Device>(DEVICE)
            .await
            .expect("device");
        interface
            .get_mut()
            .await
            .bonded
            .store(bonded, Ordering::Relaxed);
    }

    async fn set_paired(&self, paired: bool) {
        let server = self.server.as_ref().expect("a device");
        let interface = server
            .object_server()
            .interface::<_, Device>(DEVICE)
            .await
            .expect("device");
        interface
            .get_mut()
            .await
            .paired
            .store(paired, Ordering::Relaxed);
        interface
            .get()
            .await
            .paired_changed(interface.signal_emitter())
            .await
            .expect("paired changed");
    }
}

fn path(device: &str) -> ObjectPath<'static> {
    ObjectPath::try_from(device).expect("path").into_owned()
}

async fn next_input(input: &relm4::Receiver<Input>) -> Input {
    tokio::time::timeout(WAIT, input.recv())
        .await
        .expect("timed out waiting for the agent")
        .expect("the agent dropped the sender")
}

async fn no_input(input: &relm4::Receiver<Input>) {
    let arrived = tokio::time::timeout(Duration::from_millis(30), input.recv()).await;
    assert!(arrived.is_err(), "the agent queued a prompt");
}

fn pairing(input: Input) -> (super::Request, Reply) {
    match input {
        Input::Ask(ask) => {
            let Prompt::Pairing(request) = ask.prompt else {
                panic!("the prompt was not a pairing");
            };
            assert_eq!(ask.source, Source::Bluetooth);
            assert_eq!(ask.key, request.device);
            (request, ask.reply)
        }
        Input::Withdraw { .. } => panic!("expected a prompt"),
        other => panic!("unexpected input: {other:?}"),
    }
}

fn reply(reply: Reply) -> tokio::sync::oneshot::Sender<Answer> {
    match reply {
        Reply::Pairing(reply) => reply,
        Reply::Shown => panic!("a prompt parked nothing to answer"),
        Reply::Polkit(_) | Reply::Secret(_) => panic!("wrong reply"),
    }
}

fn shown(reply: Reply) {
    assert!(matches!(reply, Reply::Shown));
}

fn withdraw(input: Input, key: Option<&str>) {
    match input {
        Input::Withdraw {
            source: Source::Bluetooth,
            key: got,
        } => assert_eq!(got.as_deref(), key),
        other => panic!("expected a withdraw, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_numeric_comparison_shows_the_code_and_confirms() {
    let harness = Harness::device("Pixel 9", false).await;
    let agent = Arc::clone(&harness.agent);
    let asking =
        tokio::spawn(async move { agent.request_confirmation(path(DEVICE), 123_456).await });

    let (request, answer) = pairing(next_input(&harness.input).await);
    assert_eq!(request.device, DEVICE);
    assert_eq!(request.name, "Pixel 9");
    assert_eq!(request.kind, Kind::Confirm { passkey: 123_456 });
    let view = view_for(&Prompt::Pairing(request), None);
    assert_eq!(view.icon, Icon::Bluetooth);
    assert_eq!(view.title, "Pair with “Pixel 9”?");
    assert_eq!(view.body, "Check that the device shows the same code");
    assert_eq!(view.code.as_deref(), Some("123 456"));
    assert_eq!(view.confirm.as_deref(), Some("Pair"));
    assert_eq!(view.cancel, "Cancel");
    assert!(view.secret.is_none());
    reply(answer).send(Answer::Confirm).expect("answer");
    assert!(asking.await.expect("task").is_ok());

    let agent = Arc::clone(&harness.agent);
    let asking = tokio::spawn(async move { agent.request_confirmation(path(DEVICE), 1).await });
    let (_, answer) = pairing(next_input(&harness.input).await);
    reply(answer).send(Answer::Deny).expect("answer");
    assert!(matches!(
        asking.await.expect("task"),
        Err(AgentError::Rejected(_))
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_keyboard_code_is_displayed_with_only_cancel() {
    let harness = Harness::device("MX Keys", false).await;
    let returned = tokio::time::timeout(
        WAIT,
        harness
            .agent
            .display_pin_code(path(DEVICE), "1234\n<script>".to_owned()),
    )
    .await
    .expect("display pin waited");
    assert!(returned.is_ok());
    let (request, answer) = pairing(next_input(&harness.input).await);
    shown(answer);
    assert_eq!(
        request.kind,
        Kind::ShowPin {
            pin: "1234 <script>".to_owned()
        }
    );
    let view = view_for(&Prompt::Pairing(request), None);
    assert_eq!(view.title, "Pair with “MX Keys”");
    assert_eq!(view.body, "Type this code on the device, then press Enter");
    assert_eq!(view.code.as_deref(), Some("1234 <script>"));
    assert!(view.confirm.is_none());
    assert_eq!(view.cancel, "Cancel");

    tokio::time::timeout(
        WAIT,
        harness.agent.display_passkey(path(DEVICE), 730_519, 2),
    )
    .await
    .expect("display passkey waited");
    let (request, answer) = pairing(next_input(&harness.input).await);
    shown(answer);
    assert_eq!(
        request.kind,
        Kind::ShowPasskey {
            passkey: 730_519,
            entered: 2
        }
    );
    let view = view_for(&Prompt::Pairing(request), None);
    assert_eq!(view.code.as_deref(), Some("730 519"));
    assert!(view.confirm.is_none());
    assert_eq!(view.cancel, "Cancel");

    harness.agent.cancel().await;
    withdraw(next_input(&harness.input).await, None);

    let closed = tokio::task::spawn_blocking(|| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .build()
            .expect("runtime");
        runtime.block_on(show_view_closes_on_its_timer())
    })
    .await
    .expect("timer runtime");
    assert!(closed, "the show-view did not close on its timer");

    let server = harness.server.clone().expect("a device");
    let (sender, receiver) = relm4::channel();
    let mut app = crate::app::App::headless(
        tokio::runtime::Handle::current(),
        sender.clone(),
        server,
        None,
    );
    app.drive(Input::Ask(show_ask(DEVICE)), &sender);
    app.drive(
        Input::Dialog {
            source: Source::Bluetooth,
            key: DEVICE.to_owned(),
            response: crate::dialog::Response::Cancel,
        },
        &sender,
    );
    let started = Instant::now();
    while harness.cancels.load(Ordering::SeqCst) == 0 && started.elapsed() < WAIT {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        harness.cancels.load(Ordering::SeqCst),
        1,
        "cancel closes the pairing on the device"
    );

    let (reply, mut answer) = tokio::sync::oneshot::channel();
    app.drive(
        Input::Ask(Ask {
            source: Source::Bluetooth,
            key: DEVICE.to_owned(),
            prompt: Prompt::Pairing(super::Request {
                device: DEVICE.to_owned(),
                name: "MX Keys".to_owned(),
                kind: Kind::Confirm { passkey: 1 },
            }),
            reply: Reply::Pairing(reply),
        }),
        &sender,
    );
    harness.set_paired(true).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(200), receiver.recv())
            .await
            .is_err(),
        "a finished show-view withdrew the next request"
    );
    assert!(matches!(
        answer.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    ));
}

fn show_ask(device: &str) -> Ask {
    Ask {
        source: Source::Bluetooth,
        key: device.to_owned(),
        prompt: Prompt::Pairing(super::Request {
            device: device.to_owned(),
            name: "MX Keys".to_owned(),
            kind: Kind::ShowPin {
                pin: "1234".to_owned(),
            },
        }),
        reply: Reply::Shown,
    }
}

async fn show_view_closes_on_its_timer() -> bool {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let system = bus.connection().await;
    let (sender, receiver) = relm4::channel();
    let mut app = crate::app::App::headless(
        tokio::runtime::Handle::current(),
        sender.clone(),
        system,
        None,
    );
    app.drive(Input::Ask(show_ask(MISSING)), &sender);
    tokio::time::sleep(crate::app::SHOW_CLOSE).await;
    tokio::task::yield_now().await;
    match tokio::time::timeout(Duration::from_millis(1), receiver.recv()).await {
        Ok(Some(input)) => {
            withdraw(input, Some(MISSING));
            true
        }
        _ => false,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pin_request_returns_the_typed_digits() {
    let harness = Harness::device("Headset", false).await;
    let agent = Arc::clone(&harness.agent);
    let asking = tokio::spawn(async move { agent.request_pin_code(path(DEVICE)).await });
    let (request, answer) = pairing(next_input(&harness.input).await);
    assert_eq!(request.kind, Kind::Pin);
    let view = view_for(&Prompt::Pairing(request), None);
    assert_eq!(view.secret, Some(("PIN".to_owned(), Field::Pin)));
    assert_eq!(
        view.body,
        "Enter the PIN from the device's manual — often 0000"
    );
    assert_eq!(view.confirm.as_deref(), Some("Pair"));
    reply(answer)
        .send(Answer::Pin("0000".to_owned()))
        .expect("answer");
    assert_eq!(asking.await.expect("task").expect("pin"), "0000");

    let agent = Arc::clone(&harness.agent);
    let asking = tokio::spawn(async move { agent.request_pin_code(path(DEVICE)).await });
    let (_, answer) = pairing(next_input(&harness.input).await);
    reply(answer).send(Answer::Deny).expect("answer");
    assert!(matches!(
        asking.await.expect("task"),
        Err(AgentError::Rejected(_))
    ));

    let agent = Arc::clone(&harness.agent);
    let asking = tokio::spawn(async move { agent.request_pin_code(path(DEVICE)).await });
    let (_, answer) = pairing(next_input(&harness.input).await);
    reply(answer).send(Answer::Passkey(1)).expect("answer");
    assert!(matches!(
        asking.await.expect("task"),
        Err(AgentError::Rejected(_))
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_passkey_request_returns_a_number() {
    let harness = Harness::device("Keyboard", false).await;
    let agent = Arc::clone(&harness.agent);
    let asking = tokio::spawn(async move { agent.request_passkey(path(DEVICE)).await });
    let (request, answer) = pairing(next_input(&harness.input).await);
    assert_eq!(request.kind, Kind::Passkey);
    let view = view_for(&Prompt::Pairing(request), None);
    assert_eq!(view.secret, Some(("Code".to_owned(), Field::Passkey)));
    assert_eq!(view.body, "Enter the code shown on the device");
    assert!(!Field::Passkey.accepts("12a"));
    assert!(!Field::Passkey.accepts("1000000"));
    assert!(Field::Passkey.accepts("123456"));
    reply(answer)
        .send(Answer::Passkey(123_456))
        .expect("answer");
    assert_eq!(asking.await.expect("task").expect("passkey"), 123_456);

    let agent = Arc::clone(&harness.agent);
    let asking = tokio::spawn(async move { agent.request_passkey(path(DEVICE)).await });
    let (_, answer) = pairing(next_input(&harness.input).await);
    reply(answer).send(Answer::Deny).expect("answer");
    assert!(matches!(
        asking.await.expect("task"),
        Err(AgentError::Rejected(_))
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn service_authorization_is_automatic_only_for_bonded_devices() {
    let harness = Harness::device("Headset", true).await;
    let agent = Arc::clone(&harness.agent);
    let asking = tokio::spawn(async move { agent.request_authorization(path(DEVICE)).await });
    let (request, answer) = pairing(next_input(&harness.input).await);
    assert_eq!(request.name, "Headset");
    assert_eq!(request.kind, Kind::Authorize);
    let view = view_for(&Prompt::Pairing(request), None);
    assert_eq!(view.title, "“Headset” wants to connect");
    assert_eq!(view.body, "It has never been paired with this computer");
    assert_eq!(view.confirm.as_deref(), Some("Allow"));
    assert_eq!(view.cancel, "Deny");
    reply(answer).send(Answer::Confirm).expect("answer");
    assert!(asking.await.expect("task").is_ok());

    harness
        .agent
        .authorize_service(path(DEVICE), SERVICE.to_owned())
        .await
        .expect("bonded");
    no_input(&harness.input).await;

    harness.set_bonded(false).await;
    assert!(matches!(
        harness
            .agent
            .authorize_service(path(DEVICE), SERVICE.to_owned())
            .await,
        Err(AgentError::Rejected(_))
    ));
    no_input(&harness.input).await;

    let bare = Harness::bare().await;
    let agent = Arc::clone(&bare.agent);
    let asking = tokio::spawn(async move { agent.request_authorization(path(MISSING)).await });
    let (request, answer) = pairing(next_input(&bare.input).await);
    assert!(request.name.is_empty());
    let view = view_for(&Prompt::Pairing(request), None);
    assert_eq!(view.title, "“this device” wants to connect");
    reply(answer).send(Answer::Deny).expect("answer");
    assert!(matches!(
        asking.await.expect("task"),
        Err(AgentError::Rejected(_))
    ));
    assert!(matches!(
        bare.agent
            .authorize_service(path(MISSING), SERVICE.to_owned())
            .await,
        Err(AgentError::Rejected(_))
    ));
    no_input(&bare.input).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_pairing_leaves_the_screen() {
    let harness = Harness::bare().await;
    let agent = Arc::clone(&harness.agent);
    let asking = tokio::spawn(async move { agent.request_confirmation(path(DEVICE), 42).await });
    let (_, answer) = pairing(next_input(&harness.input).await);
    drop(reply(answer));
    assert!(matches!(
        asking.await.expect("task"),
        Err(AgentError::Canceled(_))
    ));

    harness.agent.cancel().await;
    withdraw(next_input(&harness.input).await, None);
    harness.agent.release().await;
    withdraw(next_input(&harness.input).await, None);

    let agent = Arc::clone(&harness.agent);
    let asking = tokio::spawn(async move { agent.request_pin_code(path(DEVICE)).await });
    let (_, answer) = pairing(next_input(&harness.input).await);
    let answer = reply(answer);
    asking.abort();
    let _ = asking.await;
    assert!(answer.send(Answer::Pin("1111".to_owned())).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_agent_registers_as_default_and_again_after_bluetoothd_restarts() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let server = serve_manager(&bus, Arc::clone(&calls), false).await;
    let client = bus.connection().await;
    let (sender, _input) = relm4::channel();
    let task = tokio::spawn(super::spawn(client.clone(), sender));
    assert_eq!(
        wait_calls(&calls, 2).await,
        [
            format!("register:{PATH}:KeyboardDisplay"),
            format!("default:{PATH}"),
        ]
    );

    drop(server);
    wait_until_free(&client).await;
    let _server = serve_manager(&bus, Arc::clone(&calls), false).await;
    assert_eq!(
        wait_calls(&calls, 4).await,
        [
            format!("register:{PATH}:KeyboardDisplay"),
            format!("default:{PATH}"),
            format!("register:{PATH}:KeyboardDisplay"),
            format!("default:{PATH}"),
        ]
    );
    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_default_request_keeps_the_registration() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let _server = serve_manager(&bus, Arc::clone(&calls), true).await;
    let client = bus.connection().await;
    let (sender, _input) = relm4::channel();
    let task = tokio::spawn(super::spawn(client.clone(), sender));
    assert_eq!(
        wait_calls(&calls, 2).await,
        [
            format!("register:{PATH}:KeyboardDisplay"),
            format!("default:{PATH}"),
        ]
    );
    client
        .object_server()
        .interface::<_, Agent>(PATH)
        .await
        .expect("the agent stays exported");
    task.abort();
}

async fn serve_manager(
    bus: &glimpse_dbus::testing::PrivateBus,
    calls: Arc<Mutex<Vec<String>>>,
    refuse_default: bool,
) -> zbus::Connection {
    let server = bus.connection().await;
    server
        .object_server()
        .at(
            "/org/bluez",
            Manager {
                calls,
                refuse_default,
            },
        )
        .await
        .expect("export the manager");
    own_when_free(&server).await;
    server
}

async fn own_when_free(connection: &zbus::Connection) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match glimpse_dbus::own_name(connection, glimpse_dbus::bluez::SERVICE).await {
            Ok(()) => return,
            Err(_) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(error) => panic!("own org.bluez: {error}"),
        }
    }
}

async fn wait_until_free(client: &zbus::Connection) {
    let dbus = zbus::fdo::DBusProxy::new(client).await.expect("dbus");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let owned = dbus
            .name_has_owner(glimpse_dbus::bluez::SERVICE.try_into().expect("name"))
            .await
            .unwrap_or(true);
        if !owned {
            return;
        }
        assert!(Instant::now() < deadline, "org.bluez stayed owned");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn wait_calls(calls: &Mutex<Vec<String>>, count: usize) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let seen = calls.lock().expect("calls").clone();
        if seen.len() >= count || Instant::now() >= deadline {
            return seen;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
