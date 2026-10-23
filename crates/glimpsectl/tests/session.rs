use std::sync::{Arc, Mutex};

use glimpse_dbus::testing::PrivateBus;
use glimpsectl::commands::{session::should_declare, session_action, session_suspend};

#[derive(Clone)]
struct Agents {
    calls: Arc<Mutex<Vec<String>>>,
    reject: bool,
}

#[zbus::interface(name = "me.aresa.Glimpse.Session")]
impl Agents {
    async fn request_action(&self, action: &str) -> zbus::fdo::Result<()> {
        self.calls.lock().unwrap().push(action.to_owned());
        if self.reject {
            Err(zbus::fdo::Error::AccessDenied("locked".to_owned()))
        } else {
            Ok(())
        }
    }
}

struct Login1(Arc<Mutex<Vec<String>>>);

#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl Login1 {
    async fn suspend(&self, interactive: bool) {
        self.0
            .lock()
            .unwrap()
            .push(format!("suspend:{interactive}"));
    }
}

#[test]
fn only_a_plain_niri_session_is_declared() {
    assert!(!should_declare(None));
    assert!(!should_declare(Some("glimpse:niri")));
    assert!(!should_declare(Some("Hyprland")));
    assert!(!should_declare(Some("niri:x")));
    assert!(!should_declare(Some("")));
    assert!(should_declare(Some("niri")));
}

#[tokio::test]
async fn reboot_and_power_off_go_through_agents_admission() {
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    let calls = Arc::new(Mutex::new(Vec::new()));
    server
        .object_server()
        .at(
            glimpse_dbus::session::SESSION_PATH,
            Agents {
                calls: calls.clone(),
                reject: false,
            },
        )
        .await
        .unwrap();
    glimpse_dbus::own_name(&server, glimpse_dbus::session::AGENTS_BUS_NAME)
        .await
        .unwrap();
    let client = bus.connection().await;

    session_action(&client, "reboot").await.unwrap();
    session_action(&client, "power-off").await.unwrap();

    assert_eq!(
        *calls.lock().unwrap(),
        ["reboot".to_owned(), "power-off".to_owned()]
    );
}

#[tokio::test]
async fn a_missing_or_rejecting_agents_service_returns_an_error() {
    let bus = PrivateBus::start();
    let client = bus.connection().await;
    assert!(session_action(&client, "reboot").await.is_err());

    let server = bus.connection().await;
    let calls = Arc::new(Mutex::new(Vec::new()));
    server
        .object_server()
        .at(
            glimpse_dbus::session::SESSION_PATH,
            Agents {
                calls: calls.clone(),
                reject: true,
            },
        )
        .await
        .unwrap();
    glimpse_dbus::own_name(&server, glimpse_dbus::session::AGENTS_BUS_NAME)
        .await
        .unwrap();

    assert!(session_action(&client, "power-off").await.is_err());
    assert_eq!(*calls.lock().unwrap(), ["power-off".to_owned()]);
}

#[tokio::test]
async fn suspend_calls_login1_directly() {
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    let calls = Arc::new(Mutex::new(Vec::new()));
    server
        .object_server()
        .at("/org/freedesktop/login1", Login1(calls.clone()))
        .await
        .unwrap();
    glimpse_dbus::own_name(&server, "org.freedesktop.login1")
        .await
        .unwrap();
    let client = bus.connection().await;

    session_suspend(&client).await.unwrap();

    assert_eq!(*calls.lock().unwrap(), ["suspend:true".to_owned()]);
}
