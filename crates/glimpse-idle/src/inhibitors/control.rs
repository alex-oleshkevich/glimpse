use std::sync::Arc;
use std::time::Duration;

use glimpse_dbus::idle::{
    BackendHealth, IdleInhibitorRecord, IdleInhibitorSource, InhibitionTargets, InhibitorsHealth,
    Login1Mode,
};
use glimpse_dbus::login1::{Login1ManagerProxy, current_uid};
use zbus::message::Header;

use super::{SharedRegistry, unix_now};

const REASON_CAP: usize = 240;

pub struct Idle1Server {
    registry: Arc<SharedRegistry>,
    login1: Option<Login1ManagerProxy<'static>>,
    screen_saver_health: Arc<std::sync::Mutex<BackendHealth>>,
    portal_health: Arc<std::sync::Mutex<BackendHealth>>,
    login1_health: Arc<std::sync::Mutex<BackendHealth>>,
}

impl Idle1Server {
    pub fn new(
        registry: Arc<SharedRegistry>,
        login1: Option<Login1ManagerProxy<'static>>,
        screen_saver_health: Arc<std::sync::Mutex<BackendHealth>>,
        portal_health: Arc<std::sync::Mutex<BackendHealth>>,
        login1_health: Arc<std::sync::Mutex<BackendHealth>>,
    ) -> Self {
        Self {
            registry,
            login1,
            screen_saver_health,
            portal_health,
            login1_health,
        }
    }
}

#[zbus::interface(name = "me.aresa.Glimpse.Idle1")]
impl Idle1Server {
    #[zbus(property)]
    async fn inhibitors(&self) -> Vec<IdleInhibitorRecord> {
        self.registry.read(|registry| registry.snapshot()).await
    }

    #[zbus(property)]
    async fn health(&self) -> InhibitorsHealth {
        let screen_saver = self
            .screen_saver_health
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let login1 = self
            .login1_health
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let portal = self
            .portal_health
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        InhibitorsHealth {
            screen_saver,
            portal,
            login1,
        }
    }

    async fn hold(
        &self,
        seconds: u32,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<u64> {
        let Some(login1) = self.login1.as_ref() else {
            return Err(zbus::fdo::Error::NotSupported(
                "Idle1.Hold needs org.freedesktop.login1, which is unavailable on this system"
                    .to_owned(),
            ));
        };

        let bus_name = header
            .sender()
            .map(|name| name.as_str().to_owned())
            .unwrap_or_default();

        let bus_name_for_check = bus_name.clone();
        let id = self
            .registry
            .mutate(move |registry| -> Result<u64, String> {
                registry.check_capacity(Some(&bus_name_for_check))?;
                if !registry.check_rate(Some(&bus_name_for_check), std::time::Instant::now()) {
                    return Err("inhibit rate limit exceeded".to_owned());
                }
                Ok(registry.mint_id())
            })
            .await
            .map_err(zbus::fdo::Error::LimitsExceeded)?;

        let fd = login1
            .inhibit("idle:sleep", "glimpse-idle", "Manual hold", "block")
            .await
            .map_err(|error| {
                zbus::fdo::Error::Failed(glimpse_utils::clean(&error.to_string(), REASON_CAP))
            })?;

        let uid = current_uid_or_warn();
        let added_at_unix = unix_now();
        self.registry
            .mutate(move |registry| -> Result<(), String> {
                registry.check_capacity(Some(&bus_name))?;
                let record = IdleInhibitorRecord {
                    id,
                    who: "glimpse-idle".to_owned(),
                    why: "Manual hold".to_owned(),
                    bus_name,
                    process_name: String::new(),
                    source: IdleInhibitorSource::login1(std::process::id(), uid, Login1Mode::Block),
                    targets: InhibitionTargets::manual_hold(),
                    can_release: true,
                    added_at_unix,
                };
                registry.insert(record, Some(fd));
                Ok(())
            })
            .await
            .map_err(zbus::fdo::Error::LimitsExceeded)?;

        if seconds > 0 {
            spawn_auto_release(self.registry.clone(), id, seconds);
        }

        Ok(id)
    }

    async fn release(&self, id: u64) -> zbus::fdo::Result<()> {
        self.registry
            .mutate(|registry| match registry.can_release(id) {
                None => Err(zbus::fdo::Error::UnknownObject(format!(
                    "no such idle inhibitor: {id}"
                ))),
                Some(false) => Err(zbus::fdo::Error::NotSupported(
                    "this idle inhibitor is not releasable through Idle1".to_owned(),
                )),
                Some(true) => {
                    registry.release_record(id);
                    Ok(())
                }
            })
            .await
    }
}

fn spawn_auto_release(registry: Arc<SharedRegistry>, id: u64, seconds: u32) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(u64::from(seconds))).await;
        registry
            .mutate(|registry| registry.release_record(id))
            .await;
    });
}

fn current_uid_or_warn() -> u32 {
    match current_uid() {
        Ok(uid) => uid,
        Err(error) => {
            tracing::warn!(%error, "cannot read the current uid; recording 0");
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use glimpse_dbus::idle::{
        GLIMPSE_IDLE_BUS_NAME, GLIMPSE_IDLE_OBJECT_PATH, HealthKind, Idle1Proxy,
        IdleInhibitorSource as TestSource, InhibitionTargets as TestTargets,
    };
    use glimpse_dbus::testing::PrivateBus;
    use zbus::Connection;

    use super::*;
    use crate::inhibitors::test_support::{
        FakeLogin1, start_fake_login1, start_paused_fake_login1,
    };

    fn test_health_cell() -> Arc<std::sync::Mutex<BackendHealth>> {
        Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }))
    }

    async fn start_server(
        bus: &PrivateBus,
    ) -> (Connection, Connection, Arc<SharedRegistry>, FakeLogin1) {
        let (login1_conn, fake_login1) = start_fake_login1(bus).await;
        let app = bus.connection().await;
        let (registry, _any_idle_target, _generation) = SharedRegistry::new();
        let login1 = Login1ManagerProxy::new(&app).await.unwrap();
        let server = Idle1Server::new(
            registry.clone(),
            Some(login1),
            test_health_cell(),
            test_health_cell(),
            test_health_cell(),
        );
        app.object_server()
            .at(GLIMPSE_IDLE_OBJECT_PATH, server)
            .await
            .unwrap();
        glimpse_dbus::own_name(&app, GLIMPSE_IDLE_BUS_NAME)
            .await
            .unwrap();
        (login1_conn, app, registry, fake_login1)
    }

    async fn start_server_with_paused_login1(
        bus: &PrivateBus,
    ) -> (
        Connection,
        Connection,
        Arc<SharedRegistry>,
        FakeLogin1,
        Arc<tokio::sync::Notify>,
        Arc<tokio::sync::Notify>,
    ) {
        let (login1_conn, fake_login1, started, release) = start_paused_fake_login1(bus).await;
        let app = bus.connection().await;
        let (registry, _any_idle_target, _generation) = SharedRegistry::new();
        let login1 = Login1ManagerProxy::new(&app).await.unwrap();
        let server = Idle1Server::new(
            registry.clone(),
            Some(login1),
            test_health_cell(),
            test_health_cell(),
            test_health_cell(),
        );
        app.object_server()
            .at(GLIMPSE_IDLE_OBJECT_PATH, server)
            .await
            .unwrap();
        glimpse_dbus::own_name(&app, GLIMPSE_IDLE_BUS_NAME)
            .await
            .unwrap();
        (login1_conn, app, registry, fake_login1, started, release)
    }
    fn assert_fd_closed(fake_login1: &FakeLogin1, index: usize) {
        use std::io::Read as _;
        let mut readers = fake_login1.readers.lock().unwrap();
        let mut buf = [0u8; 1];
        let read = readers[index]
            .read(&mut buf)
            .expect("read from the fake login1 pipe");
        assert_eq!(
            read, 0,
            "EOF only appears once every fd referencing the write end is closed"
        );
    }
    async fn wait_until_released(registry: &SharedRegistry, id: u64) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let present = registry
                    .read(|registry| registry.snapshot().iter().any(|record| record.id == id))
                    .await;
                if !present {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("expected inhibitor {id} to have auto-released"));
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn hold_returns_an_id_with_manual_hold_targets_and_auto_releases() {
        let bus = PrivateBus::start();
        let (_login1_conn, _app, registry, fake_login1) = start_server(&bus).await;
        let client = bus.connection().await;
        let proxy = Idle1Proxy::new(&client).await.unwrap();

        let id = proxy.hold(1).await.unwrap();
        assert_ne!(id, 0, "0 is a sentinel, never a real Hold id");

        let inhibitors = proxy.inhibitors().await.unwrap();
        let record = inhibitors.iter().find(|record| record.id == id).unwrap();
        assert_eq!(record.targets, TestTargets::manual_hold());
        assert!(record.can_release);
        assert!(matches!(
            record.source.kind,
            glimpse_dbus::idle::SourceKind::Login1
        ));
        assert_eq!(record.who, "glimpse-idle");
        assert_eq!(record.why, "Manual hold");

        wait_until_released(&registry, id).await;
        assert_fd_closed(&fake_login1, 0);
    }
    #[tokio::test(start_paused = true)]
    async fn hold_zero_seconds_never_auto_releases_only_release_clears_it() {
        let bus = PrivateBus::start();
        let (_login1_conn, _app, registry, _fake_login1) = start_server(&bus).await;
        let client = bus.connection().await;
        let proxy = Idle1Proxy::new(&client).await.unwrap();

        let id = proxy.hold(0).await.unwrap();

        tokio::time::advance(Duration::from_secs(3600)).await;
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
        let present = registry
            .read(|registry| registry.snapshot().iter().any(|record| record.id == id))
            .await;
        assert!(present, "Hold(0) must never auto-release");

        proxy.release(id).await.unwrap();
        let present = registry
            .read(|registry| registry.snapshot().iter().any(|record| record.id == id))
            .await;
        assert!(!present, "an explicit Release must clear a Hold(0) record");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn hold_rechecks_capacity_after_logind_returns_and_closes_its_fd() {
        let bus = PrivateBus::start();
        let (_login1_conn, _app, registry, fake_login1, started, release) =
            start_server_with_paused_login1(&bus).await;
        let client = bus.connection().await;
        let proxy = Idle1Proxy::new(&client).await.unwrap();

        let hold = tokio::spawn(async move { proxy.hold(0).await });
        started.notified().await;
        registry
            .mutate(|registry| {
                for _ in 0..crate::inhibitors::Registry::MAX_INHIBITORS_TOTAL {
                    let id = registry.mint_id();
                    registry.insert(
                        IdleInhibitorRecord {
                            id,
                            who: "test".to_owned(),
                            why: String::new(),
                            bus_name: String::new(),
                            process_name: String::new(),
                            source: TestSource::login1(0, 0, Login1Mode::Block),
                            targets: TestTargets::NONE,
                            can_release: false,
                            added_at_unix: 0,
                        },
                        None,
                    );
                }
            })
            .await;
        release.notify_one();

        let error = hold.await.unwrap().unwrap_err();
        assert!(
            matches!(
                &error,
                zbus::Error::MethodError(name, _, _)
                    if name.as_str() == "org.freedesktop.DBus.Error.LimitsExceeded"
            ),
            "unexpected error: {error:?}"
        );
        assert_eq!(
            registry.read(|registry| registry.snapshot().len()).await,
            crate::inhibitors::Registry::MAX_INHIBITORS_TOTAL
        );
        assert_fd_closed(&fake_login1, 0);
    }
    #[tokio::test(start_paused = true)]
    async fn release_of_a_non_releasable_record_returns_not_supported() {
        let bus = PrivateBus::start();
        let (_login1_conn, _app, registry, _fake_login1) = start_server(&bus).await;
        let client = bus.connection().await;
        let proxy = Idle1Proxy::new(&client).await.unwrap();

        let id = registry
            .mutate(|registry| {
                let id = registry.mint_id();
                let record = glimpse_dbus::idle::IdleInhibitorRecord {
                    id,
                    who: "sleep.target".to_owned(),
                    why: "systemd-observed".to_owned(),
                    bus_name: String::new(),
                    process_name: String::new(),
                    source: TestSource::login1(1, 0, glimpse_dbus::idle::Login1Mode::Block),
                    targets: TestTargets::idle_only(),
                    can_release: false,
                    added_at_unix: 0,
                };
                registry.insert(record, None)
            })
            .await;

        let error = proxy.release(id).await.unwrap_err();
        assert!(
            matches!(
                &error,
                zbus::Error::MethodError(name, _, _)
                    if name.as_str() == "org.freedesktop.DBus.Error.NotSupported"
            ),
            "unexpected error: {error:?}"
        );

        let present = registry
            .read(|registry| registry.snapshot().iter().any(|record| record.id == id))
            .await;
        assert!(present, "a NotSupported release must not remove the record");
    }
    #[tokio::test(start_paused = true)]
    async fn release_of_an_unknown_id_is_unknown_object() {
        let bus = PrivateBus::start();
        let (_login1_conn, _app, _registry, _fake_login1) = start_server(&bus).await;
        let client = bus.connection().await;
        let proxy = Idle1Proxy::new(&client).await.unwrap();

        let error = proxy.release(999_999).await.unwrap_err();
        assert!(
            matches!(
                &error,
                zbus::Error::MethodError(name, _, _)
                    if name.as_str() == "org.freedesktop.DBus.Error.UnknownObject"
            ),
            "unexpected error: {error:?}"
        );
    }
    #[tokio::test(start_paused = true)]
    async fn hold_without_login1_returns_not_supported() {
        let bus = PrivateBus::start();
        let app = bus.connection().await;
        let (registry, _any_idle_target, _generation) = SharedRegistry::new();
        let server = Idle1Server::new(
            registry,
            None,
            test_health_cell(),
            test_health_cell(),
            test_health_cell(),
        );
        app.object_server()
            .at(GLIMPSE_IDLE_OBJECT_PATH, server)
            .await
            .unwrap();
        glimpse_dbus::own_name(&app, GLIMPSE_IDLE_BUS_NAME)
            .await
            .unwrap();

        let client = bus.connection().await;
        let proxy = Idle1Proxy::new(&client).await.unwrap();

        let error = proxy.hold(1).await.unwrap_err();
        assert!(
            matches!(
                &error,
                zbus::Error::MethodError(name, _, _)
                    if name.as_str() == "org.freedesktop.DBus.Error.NotSupported"
            ),
            "unexpected error: {error:?}"
        );
        assert!(proxy.inhibitors().await.unwrap().is_empty());
    }
    #[tokio::test]
    async fn health_reflects_the_shared_screen_saver_health_cell() {
        let (registry, _any_idle_target, _generation) = SharedRegistry::new();
        let health_cell = test_health_cell();
        let server = Idle1Server::new(
            registry,
            None,
            health_cell.clone(),
            test_health_cell(),
            test_health_cell(),
        );

        assert_eq!(
            server.health().await.screen_saver.kind,
            HealthKind::Unsupported
        );

        *health_cell.lock().unwrap() = BackendHealth {
            kind: HealthKind::Degraded,
            message: "Bus name already owned".to_owned(),
        };
        let health = server.health().await;
        assert_eq!(health.screen_saver.kind, HealthKind::Degraded);
        assert_eq!(health.screen_saver.message, "Bus name already owned");
    }
    #[tokio::test]
    async fn health_reflects_the_shared_login1_health_cell() {
        let (registry, _any_idle_target, _generation) = SharedRegistry::new();
        let login1_health_cell = test_health_cell();
        let server = Idle1Server::new(
            registry,
            None,
            test_health_cell(),
            test_health_cell(),
            login1_health_cell.clone(),
        );

        assert_eq!(server.health().await.login1.kind, HealthKind::Unsupported);

        *login1_health_cell.lock().unwrap() = BackendHealth {
            kind: HealthKind::Degraded,
            message: "cannot reach org.freedesktop.login1".to_owned(),
        };
        let health = server.health().await;
        assert_eq!(health.login1.kind, HealthKind::Degraded);
        assert_eq!(health.login1.message, "cannot reach org.freedesktop.login1");
    }
    #[tokio::test]
    async fn health_reflects_the_shared_portal_health_cell() {
        let (registry, _any_idle_target, _generation) = SharedRegistry::new();
        let portal_health_cell = test_health_cell();
        let server = Idle1Server::new(
            registry,
            None,
            test_health_cell(),
            portal_health_cell.clone(),
            test_health_cell(),
        );

        assert_eq!(server.health().await.portal.kind, HealthKind::Unsupported);

        *portal_health_cell.lock().unwrap() = BackendHealth {
            kind: HealthKind::Degraded,
            message: "Bus name already owned".to_owned(),
        };
        let health = server.health().await;
        assert_eq!(health.portal.kind, HealthKind::Degraded);
        assert_eq!(health.portal.message, "Bus name already owned");
    }
}
