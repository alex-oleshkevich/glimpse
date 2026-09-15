use glimpse_dbus::notifications::{
    DoNotDisturbWire, GLIMPSE_NOTIFICATIONS_BUS_NAME, GLIMPSE_NOTIFICATIONS_OBJECT_PATH,
    NotificationWire, NotificationsProvider, NotificationsSnapshot,
};
use glimpse_dbus::testing::PrivateBus;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;
use zbus::Connection;

#[derive(Clone)]
struct TestProvider {
    calls: Arc<Mutex<Vec<(bool, i64)>>>,
}

#[zbus::interface(name = "me.aresa.Glimpse.Notifications1")]
impl TestProvider {
    #[zbus(property)]
    fn snapshot(&self) -> NotificationsSnapshot {
        (Vec::new(), (false, 0), true, String::new())
    }

    async fn dismiss(&self, _id: u32) -> zbus::fdo::Result<()> {
        Ok(())
    }

    async fn remove(&self, _id: u32) -> zbus::fdo::Result<()> {
        Ok(())
    }

    async fn activate(&self, _id: u32, _activation_token: &str) -> zbus::fdo::Result<()> {
        Ok(())
    }

    async fn invoke_action(
        &self,
        _id: u32,
        _action_key: &str,
        _activation_token: &str,
    ) -> zbus::fdo::Result<()> {
        Ok(())
    }

    async fn clear_application(&self, _application_id: &str) -> zbus::fdo::Result<()> {
        Ok(())
    }

    async fn clear_all(&self) -> zbus::fdo::Result<()> {
        Ok(())
    }

    async fn set_do_not_disturb(&self, enabled: bool, until: i64) -> zbus::fdo::Result<()> {
        self.calls.lock().unwrap().push((enabled, until));
        Ok(())
    }
}

async fn serve(bus: &PrivateBus, calls: Arc<Mutex<Vec<(bool, i64)>>>) -> Connection {
    let connection = bus.connection().await;
    connection
        .object_server()
        .at(GLIMPSE_NOTIFICATIONS_OBJECT_PATH, TestProvider { calls })
        .await
        .unwrap();
    glimpse_dbus::own_name(&connection, GLIMPSE_NOTIFICATIONS_BUS_NAME)
        .await
        .unwrap();
    connection
}

async fn wait_until(
    receiver: &mut watch::Receiver<glimpse_dbus::notifications::NotificationsProviderState>,
    predicate: impl Fn(&glimpse_dbus::notifications::NotificationsProviderState) -> bool,
) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if predicate(&receiver.borrow()) {
                return;
            }
            receiver.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

fn complete(state: &glimpse_dbus::notifications::NotificationsProviderState) -> bool {
    state.view.as_ref().is_some_and(|view| {
        view.notifications.is_empty()
            && !view.do_not_disturb.enabled
            && view.do_not_disturb.until.is_none()
            && view.serving
            && view.reason.is_empty()
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn provider_reconnects_and_roundtrips_typed_methods() {
    let mut bus = PrivateBus::start();
    let client = bus.connection().await;
    let mut provider = NotificationsProvider::start(client);
    let handle = provider.handle();
    let mut state = handle.subscribe();
    assert!(handle.snapshot().view.is_none());

    let calls = Arc::new(Mutex::new(Vec::new()));
    let first = serve(&bus, calls.clone()).await;
    wait_until(&mut state, complete).await;
    assert!(complete(&handle.snapshot()));

    drop(first);
    wait_until(&mut state, |state| {
        state.view.is_none() && state.unavailable.is_some()
    })
    .await;
    assert!(handle.set_do_not_disturb(true, 7).await.is_err());

    let second = serve(&bus, calls.clone()).await;
    wait_until(&mut state, complete).await;
    handle.set_do_not_disturb(true, 7).await.unwrap();
    assert_eq!(calls.lock().unwrap().as_slice(), &[(true, 7)]);
    bus.kill();
    wait_until(&mut state, |state| {
        state.view.is_none() && state.unavailable.is_some()
    })
    .await;

    drop(second);
    provider.shutdown().await;
}

#[test]
fn contract_wire_types_remain_available_to_integration_consumers() {
    let _: NotificationWire = (
        1,
        String::new(),
        String::new(),
        0,
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        1,
        Vec::new(),
        -1.0,
        0,
        false,
        false,
    );
    let _: DoNotDisturbWire = (false, 0);
}
