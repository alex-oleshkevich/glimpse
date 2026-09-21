use std::sync::Arc;
use std::time::Instant;

use futures_util::StreamExt;
use glimpse_dbus::idle::{
    BackendHealth, HealthKind, IdleInhibitorRecord, IdleInhibitorSource, InhibitionTargets,
};
use tokio_util::sync::CancellationToken;
use zbus::Connection;
use zbus::fdo::DBusProxy;
use zbus::message::Header;
use zbus::names::BusName;

use super::{SharedRegistry, WHO_CAP, WHY_CAP, clamp_label, unix_now};

const BUS_NAME: &str = "org.freedesktop.ScreenSaver";
const OBJECT_PATH: &str = "/org/freedesktop/ScreenSaver";
const LEGACY_OBJECT_PATH: &str = "/ScreenSaver";

#[derive(Clone)]
pub struct ScreenSaverServer {
    registry: Arc<SharedRegistry>,
    dbus: DBusProxy<'static>,
}

impl ScreenSaverServer {
    pub fn new(registry: Arc<SharedRegistry>, dbus: DBusProxy<'static>) -> Self {
        Self { registry, dbus }
    }
}

#[zbus::interface(name = "org.freedesktop.ScreenSaver")]
impl ScreenSaverServer {
    async fn inhibit(
        &self,
        application_name: String,
        reason_for_inhibit: String,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<u32> {
        let application_name = clamp_label(&application_name, WHO_CAP);
        let reason_for_inhibit = clamp_label(&reason_for_inhibit, WHY_CAP);
        let bus_name = header
            .sender()
            .map(|name| name.as_str().to_owned())
            .unwrap_or_default();

        let bus_name_for_record = bus_name.clone();
        let added_at_unix = unix_now();
        let (id, cookie) = self
            .registry
            .mutate(move |registry| -> Result<(u64, u32), String> {
                registry.check_capacity(Some(&bus_name_for_record))?;
                if !registry.check_rate(Some(&bus_name_for_record), Instant::now()) {
                    return Err("inhibit rate limit exceeded".to_owned());
                }
                let id = registry.mint_id();
                let cookie = registry.mint_cookie();
                let record = IdleInhibitorRecord {
                    id,
                    who: application_name,
                    why: reason_for_inhibit,
                    bus_name: bus_name_for_record,
                    process_name: String::new(),
                    source: IdleInhibitorSource::screen_saver(cookie),
                    targets: InhibitionTargets::idle_only(),
                    can_release: true,
                    added_at_unix,
                };
                registry.insert(record, None);
                Ok((id, cookie))
            })
            .await
            .inspect_err(|reason| {
                tracing::warn!(%bus_name, reason, "rejecting ScreenSaver Inhibit");
            })
            .map_err(zbus::fdo::Error::LimitsExceeded)?;

        if !bus_name.is_empty() {
            tokio::spawn(backfill_process_name(
                self.registry.clone(),
                self.dbus.clone(),
                bus_name,
                id,
            ));
        }

        Ok(cookie)
    }

    async fn un_inhibit(
        &self,
        cookie: u32,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        let bus_name = header
            .sender()
            .map(|name| name.as_str().to_owned())
            .unwrap_or_default();
        let bus_name_for_request = bus_name.clone();
        self.registry
            .mutate(move |registry| -> Result<(), String> {
                match registry.lookup_by_cookie(cookie) {
                    Some(id) if registry.record_is_owned_by(id, &bus_name_for_request) => {
                        registry.release_record(id);
                    }
                    Some(_) => {
                        tracing::debug!(cookie, bus_name = %bus_name_for_request, "UnInhibit for another client's cookie")
                    }
                    None => tracing::debug!(cookie, "UnInhibit for an unknown cookie"),
                }
                Ok(())
            })
            .await
            .inspect_err(|reason| {
                tracing::warn!(%bus_name, reason, "rejecting ScreenSaver UnInhibit");
            })
            .map_err(zbus::fdo::Error::LimitsExceeded)
    }
}

async fn backfill_process_name(
    registry: Arc<SharedRegistry>,
    dbus: DBusProxy<'static>,
    bus_name: String,
    id: u64,
) {
    let Ok(name) = BusName::try_from(bus_name.as_str()) else {
        return;
    };
    let Ok(pid) = dbus.get_connection_unix_process_id(name).await else {
        return;
    };
    let Ok(comm) = tokio::fs::read_to_string(format!("/proc/{pid}/comm")).await else {
        return;
    };
    let process_name = clamp_label(&comm, WHO_CAP);
    registry
        .mutate(move |registry| registry.set_process_name(id, process_name))
        .await;
}

pub async fn start(
    connection: Connection,
    registry: Arc<SharedRegistry>,
    health: Arc<std::sync::Mutex<BackendHealth>>,
) {
    let dbus = match DBusProxy::new(&connection).await {
        Ok(dbus) => dbus,
        Err(error) => {
            tracing::warn!(%error, "cannot reach org.freedesktop.DBus; ScreenSaver will not start");
            *health
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = BackendHealth {
                kind: HealthKind::Degraded,
                message: "cannot reach org.freedesktop.DBus".to_owned(),
            };
            return;
        }
    };

    let server = ScreenSaverServer::new(registry, dbus);
    for path in [OBJECT_PATH, LEGACY_OBJECT_PATH] {
        if let Err(error) = connection.object_server().at(path, server.clone()).await {
            tracing::warn!(%error, path, "failed to register org.freedesktop.ScreenSaver object");
        }
    }

    let result = match glimpse_dbus::own_name(&connection, BUS_NAME).await {
        Ok(()) => {
            tracing::info!(bus_name = BUS_NAME, "acquired org.freedesktop.ScreenSaver");
            BackendHealth {
                kind: HealthKind::Ready,
                message: String::new(),
            }
        }
        Err(error) => {
            tracing::warn!(
                %error,
                "org.freedesktop.ScreenSaver already owned; running in degraded mode"
            );
            BackendHealth {
                kind: HealthKind::Degraded,
                message: "Bus name already owned".to_owned(),
            }
        }
    };
    *health
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = result;
}

pub async fn watch_disconnects(
    connection: Connection,
    registry: Arc<SharedRegistry>,
    cancel: CancellationToken,
) {
    let dbus = match DBusProxy::new(&connection).await {
        Ok(dbus) => dbus,
        Err(error) => {
            tracing::warn!(
                %error,
                "cannot observe NameOwnerChanged; disconnected inhibitors will not auto-release"
            );
            return;
        }
    };
    let mut owners = match dbus.receive_name_owner_changed().await {
        Ok(owners) => owners,
        Err(error) => {
            tracing::warn!(%error, "cannot subscribe to NameOwnerChanged");
            return;
        }
    };

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            signal = owners.next() => match signal {
                Some(signal) => {
                    let Ok(args) = signal.args() else { continue };
                    if args.old_owner().is_some() && args.new_owner().is_none() {
                        let name = args.name().to_string();
                        registry
                            .mutate(move |registry| {
                                registry.release_by_bus_name(&name);
                            })
                            .await;
                    }
                }
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use glimpse_dbus::testing::PrivateBus;

    use super::*;

    #[zbus::proxy(
        interface = "org.freedesktop.ScreenSaver",
        default_service = "org.freedesktop.ScreenSaver",
        default_path = "/ScreenSaver"
    )]
    trait ScreenSaver {
        fn inhibit(&self, application_name: &str, reason_for_inhibit: &str) -> zbus::Result<u32>;
        fn un_inhibit(&self, cookie: u32) -> zbus::Result<()>;
    }

    async fn settle() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    async fn start_server(bus: &PrivateBus) -> (Connection, Arc<SharedRegistry>) {
        let app = bus.connection().await;
        let (registry, _any_idle_target, _generation) = SharedRegistry::new();
        let health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        start(app.clone(), registry.clone(), health).await;
        (app, registry)
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn inhibit_at_either_path_returns_a_cookie_with_idle_only_targets() {
        let bus = PrivateBus::start();
        let (_app, registry) = start_server(&bus).await;

        for path in [OBJECT_PATH, LEGACY_OBJECT_PATH] {
            let client = bus.connection().await;
            let proxy = ScreenSaverProxy::builder(&client)
                .path(path)
                .unwrap()
                .build()
                .await
                .unwrap();

            let cookie = proxy.inhibit("test", "smoke").await.unwrap();
            assert_ne!(cookie, 0);

            let found = registry
                .read(|registry| {
                    registry
                        .snapshot()
                        .iter()
                        .any(|record| record.source.cookie == cookie && record.targets.idle)
                })
                .await;
            assert!(found, "no idle-targeting record for path {path}");
        }
    }
    #[tokio::test]
    async fn un_inhibit_of_an_unknown_cookie_succeeds() {
        let bus = PrivateBus::start();
        let (_app, _registry) = start_server(&bus).await;
        let client = bus.connection().await;
        let proxy = ScreenSaverProxy::new(&client).await.unwrap();

        proxy.un_inhibit(999_999).await.unwrap();
    }

    #[tokio::test]
    async fn un_inhibit_cannot_release_another_clients_cookie() {
        let bus = PrivateBus::start();
        let (_app, registry) = start_server(&bus).await;
        let owner = bus.connection().await;
        let owner_proxy = ScreenSaverProxy::new(&owner).await.unwrap();
        let cookie = owner_proxy.inhibit("test", "smoke").await.unwrap();
        let other = bus.connection().await;
        let other_proxy = ScreenSaverProxy::new(&other).await.unwrap();

        other_proxy.un_inhibit(cookie).await.unwrap();
        assert!(
            registry
                .read(|registry| registry.lookup_by_cookie(cookie).is_some())
                .await
        );

        owner_proxy.un_inhibit(cookie).await.unwrap();
        assert!(
            !registry
                .read(|registry| registry.lookup_by_cookie(cookie).is_some())
                .await
        );
    }

    #[tokio::test]
    async fn un_inhibit_is_never_rate_limited() {
        let bus = PrivateBus::start();
        let (_app, registry) = start_server(&bus).await;
        let client = bus.connection().await;
        let proxy = ScreenSaverProxy::new(&client).await.unwrap();

        for _ in 0..8 {
            proxy.un_inhibit(999_999).await.expect("a release is free");
        }

        let cookie = proxy.inhibit("player", "playing").await.unwrap();
        for _ in 0..8 {
            let _ = proxy.inhibit("player", "playing").await;
        }
        proxy.un_inhibit(cookie).await.expect(
            "a player that inhibits on play and releases on pause exhausts the bucket;              refusing the release strands the record and suppresses every idle listener",
        );
        assert!(
            registry
                .read(|registry| registry.lookup_by_cookie(cookie))
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn inhibit_clamps_oversized_multi_byte_labels_to_their_own_caps() {
        let bus = PrivateBus::start();
        let (_app, registry) = start_server(&bus).await;
        let client = bus.connection().await;
        let proxy = ScreenSaverProxy::new(&client).await.unwrap();

        let who_input = "é".repeat(200);
        let why_input = "é".repeat(300);
        let cookie = proxy.inhibit(&who_input, &why_input).await.unwrap();

        let (who, why) = registry
            .read(|registry| {
                let record = registry
                    .snapshot()
                    .into_iter()
                    .find(|record| record.source.cookie == cookie)
                    .unwrap();
                (record.who, record.why)
            })
            .await;
        assert!(who.chars().count() <= WHO_CAP + 1, "who: {who:?}");
        assert!(why.chars().count() <= WHY_CAP + 1, "why: {why:?}");
        assert!(who.ends_with('…'));
        assert!(why.ends_with('…'));
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn disconnecting_client_auto_releases_its_records() {
        let bus = PrivateBus::start();
        let (app, registry) = start_server(&bus).await;
        let cancel = CancellationToken::new();
        let watch_task = tokio::spawn(watch_disconnects(
            app.clone(),
            registry.clone(),
            cancel.clone(),
        ));

        let client = bus.connection().await;
        let proxy = ScreenSaverProxy::new(&client).await.unwrap();
        let cookie = proxy.inhibit("test", "smoke").await.unwrap();
        assert!(
            registry
                .read(|registry| registry.lookup_by_cookie(cookie).is_some())
                .await
        );

        drop(proxy);
        drop(client);
        settle().await;

        assert!(
            !registry
                .read(|registry| registry.lookup_by_cookie(cookie).is_some())
                .await,
            "a disconnected client's record must be auto-released"
        );

        cancel.cancel();
        let _ = watch_task.await;
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn process_name_backfills_after_the_initial_reply() {
        let bus = PrivateBus::start();
        let (_app, registry) = start_server(&bus).await;
        let client = bus.connection().await;
        let proxy = ScreenSaverProxy::new(&client).await.unwrap();

        let cookie = proxy.inhibit("test", "smoke").await.unwrap();

        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let process_name = registry
                    .read(|registry| {
                        registry
                            .snapshot()
                            .into_iter()
                            .find(|record| record.source.cookie == cookie)
                            .map(|record| record.process_name)
                    })
                    .await;
                if process_name.as_deref().is_some_and(|name| !name.is_empty()) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("process_name to backfill");
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn inhibit_reports_capacity_rejection_as_limits_exceeded() {
        let bus = PrivateBus::start();
        let (_app, registry) = start_server(&bus).await;
        registry
            .mutate(|registry| {
                for i in 0..super::super::Registry::MAX_INHIBITORS_TOTAL {
                    let id = registry.mint_id();
                    let record = IdleInhibitorRecord {
                        id,
                        who: "who".into(),
                        why: "why".into(),
                        bus_name: format!(":1.filler.{i}"),
                        process_name: String::new(),
                        source: IdleInhibitorSource::screen_saver(0),
                        targets: InhibitionTargets::idle_only(),
                        can_release: true,
                        added_at_unix: 0,
                    };
                    registry.insert(record, None);
                }
            })
            .await;

        let client = bus.connection().await;
        let proxy = ScreenSaverProxy::new(&client).await.unwrap();
        let error = proxy.inhibit("test", "smoke").await.unwrap_err();
        assert!(
            matches!(
                &error,
                zbus::Error::MethodError(name, _, _)
                    if name.as_str() == "org.freedesktop.DBus.Error.LimitsExceeded"
            ),
            "unexpected error: {error:?}"
        );
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_second_acquisition_attempt_degrades_health_but_keeps_serving() {
        let bus = PrivateBus::start();
        let first = bus.connection().await;
        let (first_registry, _any_idle_target, _generation) = SharedRegistry::new();
        let first_health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        start(first.clone(), first_registry.clone(), first_health.clone()).await;
        assert_eq!(first_health.lock().unwrap().kind, HealthKind::Ready);

        let second = bus.connection().await;
        let (second_registry, _any_idle_target, _generation) = SharedRegistry::new();
        let second_health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        start(
            second.clone(),
            second_registry.clone(),
            second_health.clone(),
        )
        .await;

        let health = second_health.lock().unwrap().clone();
        assert_eq!(health.kind, HealthKind::Degraded);
        assert_eq!(health.message, "Bus name already owned");

        // The second instance never won the well-known name, so a client dialing the default
        // destination still reaches the first owner — reachability by full address is the
        // acknowledged edge case. What must hold is that starting the second instance neither
        // panicked nor left it unregistered: its own object server still carries the interface.
        assert!(
            second
                .object_server()
                .interface::<_, ScreenSaverServer>(OBJECT_PATH)
                .await
                .is_ok(),
            "the un-owning second server must still register its own object"
        );

        // And the first owner is unaffected: it still answers on the well-known name.
        let client = bus.connection().await;
        let proxy = ScreenSaverProxy::new(&client).await.unwrap();
        let cookie = proxy.inhibit("test", "smoke").await.unwrap();
        assert!(
            first_registry
                .read(|registry| registry.lookup_by_cookie(cookie).is_some())
                .await,
            "the first, name-owning server must keep serving Inhibit"
        );
    }
}
