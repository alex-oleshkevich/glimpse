use std::sync::{Arc, Mutex};

use zbus::proxy::Defaults;

use super::*;
use crate::testing::PrivateBus;

#[test]
fn the_proxy_defaults_are_the_agents_session_name() {
    assert_eq!(AGENTS_BUS_NAME, "me.aresa.Glimpse.Agents");
    assert_eq!(SESSION_PATH, "/me/aresa/Glimpse/Session");
    assert_eq!(SESSION_INTERFACE, "me.aresa.Glimpse.Session");
    assert_eq!(
        SessionProxy::DESTINATION.as_ref().map(|name| name.as_str()),
        Some(AGENTS_BUS_NAME)
    );
    assert_eq!(
        SessionProxy::PATH.as_ref().map(|path| path.as_str()),
        Some(SESSION_PATH)
    );
    assert_eq!(
        SessionProxy::INTERFACE.as_ref().map(|name| name.as_str()),
        Some(SESSION_INTERFACE)
    );
}

struct Admitted(Arc<Mutex<Vec<String>>>);

#[zbus::interface(name = "me.aresa.Glimpse.Session")]
impl Admitted {
    async fn request_action(&self, action: &str) {
        self.0.lock().expect("calls").push(action.to_owned());
    }
}

#[tokio::test]
async fn request_action_reaches_the_agents_session_object() {
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    let calls = Arc::new(Mutex::new(Vec::new()));
    server
        .object_server()
        .at(SESSION_PATH, Admitted(calls.clone()))
        .await
        .expect("export");
    crate::own_name(&server, AGENTS_BUS_NAME)
        .await
        .expect("name");
    let client = bus.connection().await;
    let proxy = SessionProxy::builder(&client).build().await.expect("proxy");

    proxy.request_action("reboot").await.expect("reboot");
    proxy.request_action("power-off").await.expect("power-off");

    assert_eq!(
        calls.lock().expect("calls").as_slice(),
        ["reboot", "power-off"]
    );
}

#[tokio::test]
async fn a_missing_agents_name_is_an_error() {
    let bus = PrivateBus::start();
    let client = bus.connection().await;
    let proxy = SessionProxy::builder(&client).build().await.expect("proxy");

    assert!(proxy.request_action("reboot").await.is_err());
}
