use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use zbus::zvariant::OwnedObjectPath;

use super::*;

fn request(action: Action) -> (Request, oneshot::Receiver<fdo::Result<()>>) {
    let (reply, answer) = oneshot::channel();
    (Request { action, reply }, answer)
}

#[test]
fn admission_fails_closed_and_never_queues_busy_or_abandoned_requests() {
    let mut state = State::default();
    let (ask, mut answer) = request(Action::Reboot);
    assert!(state.admit(ask, false).is_none());
    assert!(matches!(answer.try_recv(), Ok(Err(fdo::Error::Failed(_)))));
    state.locked = Some(true);
    let (ask, mut answer) = request(Action::Reboot);
    assert!(state.admit(ask, false).is_none());
    assert!(matches!(
        answer.try_recv(),
        Ok(Err(fdo::Error::AccessDenied(_)))
    ));
    state.locked = Some(false);
    let (ask, mut answer) = request(Action::PowerOff);
    assert!(state.admit(ask, true).is_none());
    assert!(matches!(
        answer.try_recv(),
        Ok(Err(fdo::Error::LimitsExceeded(_)))
    ));
    let (ask, answer) = request(Action::PowerOff);
    drop(answer);
    assert!(state.admit(ask, false).is_none());
    assert!(state.pending.is_none());
    let (ask, mut answer) = request(Action::Reboot);
    let (id, _) = state.admit(ask, false).expect("admitted");
    assert!(answer.try_recv().expect("answer").is_ok());
    let (ask, mut answer) = request(Action::PowerOff);
    assert!(state.admit(ask, false).is_none());
    assert!(matches!(
        answer.try_recv(),
        Ok(Err(fdo::Error::LimitsExceeded(_)))
    ));
    assert!(state.take(id + 1).is_none());
    assert_eq!(state.take(id).expect("pending").action, Action::Reboot);
    assert!(state.take(id).is_none());
    state.executing = true;
    let (ask, mut answer) = request(Action::PowerOff);
    assert!(state.admit(ask, false).is_none());
    assert!(matches!(
        answer.try_recv(),
        Ok(Err(fdo::Error::LimitsExceeded(_)))
    ));
}

#[test]
fn inhibitors_are_filtered_before_the_cap_and_delay_is_distinct() {
    let entry = |what: &str, who: &str, mode: &str| {
        (
            what.to_owned(),
            who.to_owned(),
            "why".to_owned(),
            mode.to_owned(),
            1000,
            1,
        )
    };
    let mut inhibitors = vec![entry("sleep", "unrelated", "block"); 12];
    inhibitors.push(entry("shutdown:sleep", "delay app", "delay"));
    inhibitors.push(entry("shutdown", "block app", "block-weak"));
    inhibitors.push(entry("shutdown", "ignored", "unknown"));
    let rows = details(&inhibitors);
    assert_eq!(rows.len(), 2);
    assert!(rows[0].0.contains("Delay") && rows[0].0.contains("delay app"));
    assert!(rows[1].0.contains("Block") && rows[1].0.contains("block app"));
    inhibitors.extend(vec![entry("shutdown", &"x".repeat(300), "block"); 20]);
    let rows = details(&inhibitors);
    assert_eq!(rows.len(), INHIBITORS_MAX);
    assert!(rows.iter().all(|(label, _)| label.chars().count() <= 100));
}

#[test]
fn power_cards_use_the_approved_copy_without_a_countdown() {
    let reboot = Action::Reboot.view(Vec::new());
    assert_eq!(reboot.icon, Icon::Reboot);
    assert_eq!(reboot.title, "Restart?");
    assert_eq!(reboot.confirm.as_deref(), Some("Restart"));
    let off = Action::PowerOff.view(Vec::new());
    assert_eq!(off.icon, Icon::PowerOff);
    assert_eq!(off.title, "Power off?");
    assert_eq!(off.confirm.as_deref(), Some("Power off"));
    assert_eq!(off.body, "Unsaved work in open apps may be lost");
    assert!(off.secret.is_none() && off.code.is_none());
}

#[tokio::test]
async fn exported_interface_rejects_invalid_actions_and_awaits_admission() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let server = bus.connection().await;
    let (input, inbox) = relm4::channel();
    server
        .object_server()
        .at(glimpse_dbus::session::SESSION_PATH, Server::new(input))
        .await
        .expect("export");
    glimpse_dbus::own_name(&server, glimpse_dbus::session::AGENTS_BUS_NAME)
        .await
        .expect("name");
    let client = bus.connection().await;
    let proxy = glimpse_dbus::session::SessionProxy::new(&client)
        .await
        .expect("proxy");
    let error = proxy.request_action("suspend").await.expect_err("invalid");
    assert!(
        matches!(error, zbus::Error::MethodError(name, _, _) if name.as_str() == "org.freedesktop.DBus.Error.InvalidArgs")
    );
    let admission = async {
        let Input::SessionRequest(request) = inbox.recv().await.expect("request") else {
            panic!("session request");
        };
        request
            .reply
            .send(Err(fdo::Error::LimitsExceeded("busy".into())))
            .expect("reply");
    };
    let (result, ()) = tokio::join!(proxy.request_action("reboot"), admission);
    assert!(
        matches!(result, Err(zbus::Error::MethodError(name, _, _)) if name.as_str() == "org.freedesktop.DBus.Error.LimitsExceeded")
    );
}

#[derive(Clone)]
pub(crate) struct Backend {
    pub locked: Arc<AtomicBool>,
    pub capability: Arc<Mutex<String>>,
    pub calls: Arc<Mutex<Vec<(String, bool)>>>,
    pub fail: Arc<AtomicBool>,
    pub lock_on_capability: Arc<AtomicBool>,
}

#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl Backend {
    fn list_sessions(&self) -> Vec<glimpse_dbus::login1::Login1SessionEntry> {
        vec![(
            "1".into(),
            glimpse_dbus::login1::current_uid(),
            "user".into(),
            "seat0".into(),
            OwnedObjectPath::try_from("/session").expect("path"),
        )]
    }

    fn can_reboot(&self) -> String {
        if self.lock_on_capability.load(Ordering::SeqCst) {
            self.locked.store(true, Ordering::SeqCst);
        }
        self.capability.lock().expect("capability").clone()
    }
    fn can_power_off(&self) -> String {
        self.can_reboot()
    }
    fn list_inhibitors(&self) -> Vec<Login1InhibitorEntry> {
        Vec::new()
    }
    fn reboot(&self, interactive: bool) -> fdo::Result<()> {
        self.record("reboot", interactive)
    }
    fn power_off(&self, interactive: bool) -> fdo::Result<()> {
        self.record("power-off", interactive)
    }
}

impl Backend {
    fn record(&self, action: &str, interactive: bool) -> fdo::Result<()> {
        self.calls
            .lock()
            .expect("calls")
            .push((action.into(), interactive));
        if self.fail.load(Ordering::SeqCst) {
            Err(fdo::Error::Failed("backend failed".into()))
        } else {
            Ok(())
        }
    }
}

struct Session(Backend);

#[zbus::interface(name = "org.freedesktop.login1.Session")]
impl Session {
    #[zbus(property)]
    fn active(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn class(&self) -> String {
        "user".into()
    }
    #[zbus(property, name = "Type")]
    fn kind(&self) -> String {
        "wayland".into()
    }
    #[zbus(property, name = "LockedHint")]
    fn locked_hint(&self) -> bool {
        self.0.locked.load(Ordering::SeqCst)
    }
}

pub(crate) async fn backend(server: &zbus::Connection) -> Backend {
    let backend = Backend {
        locked: Arc::new(AtomicBool::new(false)),
        capability: Arc::new(Mutex::new("yes".into())),
        calls: Arc::default(),
        fail: Arc::default(),
        lock_on_capability: Arc::default(),
    };
    server
        .object_server()
        .at("/org/freedesktop/login1", backend.clone())
        .await
        .expect("manager");
    server
        .object_server()
        .at("/session", Session(backend.clone()))
        .await
        .expect("session");
    glimpse_dbus::own_name(server, "org.freedesktop.login1")
        .await
        .expect("login1");
    backend
}

#[tokio::test]
async fn execution_rechecks_lock_capability_and_preserves_backend_errors() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let server = bus.connection().await;
    let backend = backend(&server).await;
    let client = bus.connection().await;
    backend.locked.store(true, Ordering::SeqCst);
    assert!(execute(&client, Action::Reboot).await.is_err());
    backend.locked.store(false, Ordering::SeqCst);
    *backend.capability.lock().expect("capability") = "inhibited".into();
    assert!(execute(&client, Action::Reboot).await.is_err());
    assert!(backend.calls.lock().expect("calls").is_empty());
    *backend.capability.lock().expect("capability") = "challenge".into();
    backend.lock_on_capability.store(true, Ordering::SeqCst);
    assert!(execute(&client, Action::Reboot).await.is_err());
    assert!(backend.calls.lock().expect("calls").is_empty());
    backend.lock_on_capability.store(false, Ordering::SeqCst);
    backend.locked.store(false, Ordering::SeqCst);
    execute(&client, Action::Reboot).await.expect("reboot");
    backend.fail.store(true, Ordering::SeqCst);
    assert!(
        execute(&client, Action::PowerOff)
            .await
            .expect_err("failure")
            .contains("backend failed")
    );
    assert_eq!(
        *backend.calls.lock().expect("calls"),
        [("reboot".into(), true), ("power-off".into(), true)]
    );
}

struct Notes(Arc<Mutex<Vec<(String, String)>>>);

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Notes {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        _app_name: &str,
        _replaces_id: u32,
        _app_icon: &str,
        summary: &str,
        body: &str,
        _actions: Vec<String>,
        _hints: HashMap<String, zbus::zvariant::OwnedValue>,
        _expire_timeout: i32,
    ) -> u32 {
        self.0
            .lock()
            .expect("notes")
            .push((summary.into(), body.into()));
        1
    }
}

#[tokio::test]
async fn admitted_backend_failure_posts_a_bounded_notification_and_clears_busy() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let server = bus.connection().await;
    let notes = Arc::new(Mutex::new(Vec::new()));
    server
        .object_server()
        .at("/org/freedesktop/Notifications", Notes(notes.clone()))
        .await
        .expect("notes");
    glimpse_dbus::own_name(&server, "org.freedesktop.Notifications")
        .await
        .expect("notification name");
    let (input, receiver) = relm4::channel();
    let mut app = crate::app::App::headless(
        tokio::runtime::Handle::current(),
        input.clone(),
        bus.connection().await,
        None,
    );
    let reason = format!("<b>failed & unsafe</b>{}", "x".repeat(1000));
    app.drive(Input::SessionDone(Err(reason.clone())), &input);
    tokio::time::timeout(DEADLINE, async {
        while notes.lock().expect("notes").is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("notification deadline");
    let notes = notes.lock().expect("notes");
    assert_eq!(notes[0].0, "The power action failed");
    assert_eq!(
        notes[0].1,
        gtk4::glib::markup_escape_text(&clean(&reason, 240)).as_str()
    );
    assert!(
        notes[0]
            .1
            .starts_with("&lt;b&gt;failed &amp; unsafe&lt;/b&gt;")
    );
    drop(receiver);
}
