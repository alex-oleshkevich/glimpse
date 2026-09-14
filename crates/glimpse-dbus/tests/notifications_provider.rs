use glimpse_dbus::notifications::{
    DoNotDisturbWire, GLIMPSE_NOTIFICATIONS_BUS_NAME, GLIMPSE_NOTIFICATIONS_OBJECT_PATH,
    NotificationWire, NotificationsProvider, NotificationsSnapshot,
};
use std::io::BufRead;
use std::process::{Child, Command, Stdio};
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

struct PrivateBus {
    child: Child,
    address: String,
}

impl PrivateBus {
    fn start() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args([
                "--session",
                "--nofork",
                "--print-address=1",
                "--print-pid=1",
            ])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.as_mut().unwrap();
        let mut lines = std::io::BufReader::new(stdout).lines();
        let address = lines.next().unwrap().unwrap();
        let _pid = lines.next().unwrap().unwrap();
        Self { child, address }
    }

    async fn connection(&self) -> Connection {
        zbus::connection::Builder::address(self.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn serve(bus: &PrivateBus, calls: Arc<Mutex<Vec<(bool, i64)>>>) -> Connection {
    let connection = bus.connection().await;
    connection
        .object_server()
        .at(GLIMPSE_NOTIFICATIONS_OBJECT_PATH, TestProvider { calls })
        .await
        .unwrap();
    connection
        .request_name(GLIMPSE_NOTIFICATIONS_BUS_NAME)
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
    state
        .snapshot
        .as_ref()
        .is_some_and(|(records, dnd, serving, reason)| {
            records.is_empty() && *dnd == (false, 0) && *serving && reason.is_empty()
        })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn provider_reconnects_and_roundtrips_typed_methods() {
    let mut bus = PrivateBus::start();
    let client = bus.connection().await;
    let mut provider = NotificationsProvider::start(client);
    let handle = provider.handle();
    let mut state = handle.subscribe();
    assert!(handle.snapshot().snapshot.is_none());

    let calls = Arc::new(Mutex::new(Vec::new()));
    let first = serve(&bus, calls.clone()).await;
    wait_until(&mut state, complete).await;
    assert!(complete(&handle.snapshot()));

    drop(first);
    wait_until(&mut state, |state| {
        state.snapshot.is_none() && state.unavailable.is_some()
    })
    .await;
    assert!(handle.set_do_not_disturb(true, 7).await.is_err());

    let second = serve(&bus, calls.clone()).await;
    wait_until(&mut state, complete).await;
    handle.set_do_not_disturb(true, 7).await.unwrap();
    assert_eq!(calls.lock().unwrap().as_slice(), &[(true, 7)]);

    let _ = bus.child.kill();
    let _ = bus.child.wait();
    wait_until(&mut state, |state| {
        state.snapshot.is_none() && state.unavailable.is_some()
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
