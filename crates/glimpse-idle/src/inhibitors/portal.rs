use std::collections::HashMap;
use std::sync::Arc;

use glimpse_dbus::idle::{IdleInhibitorRecord, IdleInhibitorSource, InhibitionTargets};
use glimpse_dbus::login1::Login1ManagerProxy;
use zbus::Connection;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue};

use super::{Backend, Health, SharedRegistry, WHO_CAP, WHY_CAP, unix_now};

const BUS_NAME: &str = "me.aresa.Glimpse.Idle.Portal";
const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_RESPONSE_SUCCESS: u32 = 0;
const PORTAL_RESPONSE_OTHER: u32 = 2;
const SESSION_RUNNING: u32 = 1;

pub struct PortalInhibit {
    registry: Arc<SharedRegistry>,
    login1: Option<Login1ManagerProxy<'static>>,
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Inhibit")]
impl PortalInhibit {
    async fn inhibit(
        &self,
        handle: ObjectPath<'_>,
        app_id: String,
        _window: String,
        flags: u32,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] conn: &Connection,
    ) -> zbus::fdo::Result<()> {
        let app_id = glimpse_utils::clean(&app_id, WHO_CAP);
        let targets = InhibitionTargets::from_portal_flags(flags);
        let reason = options
            .get("reason")
            .and_then(|value| String::try_from(value.clone()).ok())
            .map(|reason| glimpse_utils::clean(&reason, WHY_CAP))
            .unwrap_or_default();

        let app_id_for_check = app_id.clone();
        let id = self
            .registry
            .mutate(move |registry| -> Result<u64, String> {
                registry.check_capacity(Some(&app_id_for_check))?;
                Ok(registry.mint_id())
            })
            .await
            .inspect_err(|reason| {
                tracing::warn!(%app_id, reason, "rejecting portal Inhibit");
            })
            .map_err(zbus::fdo::Error::LimitsExceeded)?;

        let what = login1_what(&targets);
        let logind_fd = if what.is_empty() {
            None
        } else {
            match self.login1.as_ref() {
                Some(login1) => login1
                    .inhibit(&what, "glimpse-idle", "Portal inhibit", "block")
                    .await
                    .inspect_err(|error| {
                        tracing::warn!(
                            %error,
                            %app_id,
                            "portal inhibit could not take a login1 hold; tracking without one"
                        );
                    })
                    .ok(),
                None => None,
            }
        };

        let handle_string = handle.to_string();
        let who = app_id.clone();
        let app_id_for_log = app_id.clone();
        let added_at_unix = unix_now();
        self.registry
            .mutate(move |registry| -> Result<(), String> {
                registry.check_capacity(None)?;
                let record = IdleInhibitorRecord {
                    id,
                    who,
                    why: reason,
                    bus_name: String::new(),
                    process_name: String::new(),
                    source: IdleInhibitorSource::portal(handle_string, app_id),
                    targets,
                    can_release: true,
                    added_at_unix,
                };
                registry.insert(record, logind_fd);
                Ok(())
            })
            .await
            .inspect_err(|reason| {
                tracing::warn!(
                    app_id = %app_id_for_log,
                    reason,
                    "rejecting portal Inhibit after login1 hold"
                );
            })
            .map_err(zbus::fdo::Error::LimitsExceeded)?;

        let owned_handle: OwnedObjectPath = handle.to_owned().into();
        let request = PortalRequest {
            registry: self.registry.clone(),
            id,
            handle: owned_handle.clone(),
        };
        match conn.object_server().at(owned_handle.clone(), request).await {
            Ok(true) => {}
            other => {
                tracing::warn!(
                    ?other,
                    handle = %owned_handle,
                    "portal Request path already taken; releasing the record it would have \
                     controlled"
                );
                self.registry
                    .mutate(move |registry| registry.release_record(id))
                    .await;
            }
        }

        Ok(())
    }

    async fn create_monitor(
        &self,
        _handle: ObjectPath<'_>,
        session_handle: ObjectPath<'_>,
        app_id: String,
        _window: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) -> zbus::fdo::Result<u32> {
        let owned: OwnedObjectPath = session_handle.to_owned().into();
        let session = PortalSession {
            handle: owned.clone(),
        };
        if let Err(error) = conn.object_server().at(owned.clone(), session).await {
            tracing::warn!(%error, session = %owned, "portal monitor session path already taken");
            return Ok(PORTAL_RESPONSE_OTHER);
        }
        tracing::info!(%app_id, session = %owned, "portal monitor session created");

        let _ = emitter.state_changed(owned.as_ref(), running_state()).await;
        Ok(PORTAL_RESPONSE_SUCCESS)
    }

    async fn query_end_response(&self, session_handle: ObjectPath<'_>) {
        tracing::debug!(session = %session_handle, "portal query-end acknowledged");
    }

    #[zbus(signal)]
    async fn state_changed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        session_handle: ObjectPath<'_>,
        state: HashMap<String, OwnedValue>,
    ) -> zbus::Result<()>;
}

fn running_state() -> HashMap<String, OwnedValue> {
    let mut state = HashMap::new();
    if let Ok(running) = OwnedValue::try_from(zbus::zvariant::Value::from(SESSION_RUNNING)) {
        state.insert("session-state".to_owned(), running);
    }
    state
}

struct PortalSession {
    handle: OwnedObjectPath,
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Session")]
impl PortalSession {
    async fn close(&self, #[zbus(object_server)] object_server: &zbus::ObjectServer) {
        let _ = object_server
            .remove::<PortalSession, _>(self.handle.clone())
            .await;
        tracing::debug!(session = %self.handle, "portal monitor session closed");
    }

    #[zbus(signal)]
    async fn closed(emitter: &zbus::object_server::SignalEmitter<'_>) -> zbus::Result<()>;
}

struct PortalRequest {
    registry: Arc<SharedRegistry>,
    id: u64,
    handle: OwnedObjectPath,
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Request")]
impl PortalRequest {
    async fn close(
        &self,
        #[zbus(object_server)] object_server: &zbus::ObjectServer,
    ) -> zbus::fdo::Result<()> {
        self.registry
            .mutate(|registry| registry.release_record(self.id))
            .await;
        let _ = object_server
            .remove::<PortalRequest, _>(self.handle.clone())
            .await;
        Ok(())
    }
}

fn login1_what(targets: &InhibitionTargets) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if targets.suspend {
        parts.push("sleep");
    }
    if targets.shutdown {
        parts.push("shutdown");
    }
    parts.join(":")
}

pub async fn start(
    connection: Connection,
    registry: Arc<SharedRegistry>,
    login1: Option<Login1ManagerProxy<'static>>,
    health: Health,
) {
    let server = PortalInhibit { registry, login1 };
    if let Err(error) = connection.object_server().at(OBJECT_PATH, server).await {
        tracing::warn!(%error, "failed to register org.freedesktop.impl.portal.Inhibit object");
    }

    match glimpse_dbus::own_name(&connection, BUS_NAME).await {
        Ok(()) => {
            tracing::info!(bus_name = BUS_NAME, "acquired me.aresa.Glimpse.Idle.Portal");
            health.ready(Backend::Portal);
        }
        Err(error) => {
            tracing::warn!(
                %error,
                "me.aresa.Glimpse.Idle.Portal already owned; running in degraded mode"
            );
            health.degraded(Backend::Portal, "Bus name already owned");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read as _;

    use glimpse_dbus::idle::{
        IdleInhibitorRecord as TestRecord, IdleInhibitorSource, InhibitionTargets as TestTargets,
        Login1Mode,
    };
    use glimpse_dbus::testing::PrivateBus;
    use zbus::zvariant::Value;

    use super::*;
    use crate::inhibitors::test_support::{
        FakeLogin1, start_fake_login1, start_paused_fake_login1,
    };

    #[zbus::proxy(
        interface = "org.freedesktop.impl.portal.Inhibit",
        default_service = "me.aresa.Glimpse.Idle.Portal",
        default_path = "/org/freedesktop/portal/desktop"
    )]
    trait PortalInhibitTest {
        fn inhibit(
            &self,
            handle: &ObjectPath<'_>,
            app_id: &str,
            window: &str,
            flags: u32,
            options: HashMap<&str, Value<'_>>,
        ) -> zbus::Result<()>;
    }

    #[zbus::proxy(interface = "org.freedesktop.impl.portal.Request")]
    trait PortalRequestTest {
        fn close(&self) -> zbus::Result<()>;
    }

    #[zbus::proxy(
        interface = "org.freedesktop.impl.portal.Inhibit",
        default_service = "me.aresa.Glimpse.Idle.Portal",
        default_path = "/org/freedesktop/portal/desktop"
    )]
    trait PortalMonitorTest {
        fn create_monitor(
            &self,
            handle: &ObjectPath<'_>,
            session_handle: &ObjectPath<'_>,
            app_id: &str,
            window: &str,
        ) -> zbus::Result<u32>;
        fn query_end_response(&self, session_handle: &ObjectPath<'_>) -> zbus::Result<()>;
    }

    #[zbus::proxy(interface = "org.freedesktop.impl.portal.Session")]
    trait PortalSessionTest {
        fn close(&self) -> zbus::Result<()>;
    }

    async fn start_server(
        bus: &PrivateBus,
    ) -> (Connection, Connection, Arc<SharedRegistry>, FakeLogin1) {
        let (login1_conn, fake_login1) = start_fake_login1(bus).await;
        let app = bus.connection().await;
        let (registry, _any_idle_target, _generation) = SharedRegistry::new();
        let login1 = Login1ManagerProxy::new(&app).await.unwrap();
        let health = Health::new().0;
        start(app.clone(), registry.clone(), Some(login1), health).await;
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
        let registry = SharedRegistry::new().0;
        let login1 = Login1ManagerProxy::new(&app).await.unwrap();
        let health = Health::new().0;
        start(app.clone(), registry.clone(), Some(login1), health).await;
        (login1_conn, app, registry, fake_login1, started, release)
    }

    async fn find_record(registry: &SharedRegistry, id: u64) -> Option<TestRecord> {
        registry
            .read(|registry| {
                registry
                    .snapshot()
                    .into_iter()
                    .find(|record| record.id == id)
            })
            .await
    }

    async fn record_for_handle(registry: &SharedRegistry, handle: &str) -> Option<TestRecord> {
        registry
            .read(|registry| {
                registry
                    .snapshot()
                    .into_iter()
                    .find(|record| record.source.request_handle == handle)
            })
            .await
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn inhibit_with_idle_and_suspend_flags_lands_a_matching_record() {
        let bus = PrivateBus::start();
        let (_login1_conn, _app, registry, _fake_login1) = start_server(&bus).await;
        let client = bus.connection().await;
        let proxy = PortalInhibitTestProxy::new(&client).await.unwrap();

        let handle = ObjectPath::try_from("/req/1").unwrap();
        proxy
            .inhibit(&handle, "org.test.App", "", 12, HashMap::new())
            .await
            .unwrap();

        let record = record_for_handle(&registry, "/req/1")
            .await
            .expect("a record for the registered handle");
        assert_eq!(
            record.targets,
            TestTargets {
                idle: true,
                suspend: true,
                ..TestTargets::NONE
            }
        );
        assert_eq!(record.who, "org.test.App");
        assert_eq!(record.bus_name, "");
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn close_releases_the_record_and_a_second_close_does_not_crash_the_server() {
        let bus = PrivateBus::start();
        let (_login1_conn, _app, registry, _fake_login1) = start_server(&bus).await;
        let client = bus.connection().await;
        let inhibit_proxy = PortalInhibitTestProxy::new(&client).await.unwrap();

        let handle = ObjectPath::try_from("/req/2").unwrap();
        inhibit_proxy
            .inhibit(&handle, "org.test.App", "", 8, HashMap::new())
            .await
            .unwrap();
        let id = record_for_handle(&registry, "/req/2").await.unwrap().id;

        let request_proxy = PortalRequestTestProxy::builder(&client)
            .path(handle.clone())
            .unwrap()
            .destination("me.aresa.Glimpse.Idle.Portal")
            .unwrap()
            .build()
            .await
            .unwrap();
        request_proxy.close().await.unwrap();
        assert!(find_record(&registry, id).await.is_none());

        let second = request_proxy.close().await;
        assert!(second.is_err());

        let handle2 = ObjectPath::try_from("/req/2b").unwrap();
        inhibit_proxy
            .inhibit(&handle2, "org.test.App", "", 8, HashMap::new())
            .await
            .unwrap();
        assert!(record_for_handle(&registry, "/req/2b").await.is_some());
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_reused_handle_releases_the_second_records_rather_than_leaking_it() {
        let bus = PrivateBus::start();
        let (_login1_conn, _app, registry, _fake_login1) = start_server(&bus).await;
        let client = bus.connection().await;
        let proxy = PortalInhibitTestProxy::new(&client).await.unwrap();

        let handle = ObjectPath::try_from("/req/dup").unwrap();
        proxy
            .inhibit(&handle, "org.first.App", "", 8, HashMap::new())
            .await
            .unwrap();
        let first_id = record_for_handle(&registry, "/req/dup").await.unwrap().id;

        proxy
            .inhibit(&handle, "org.second.App", "", 8, HashMap::new())
            .await
            .unwrap();

        let count = registry
            .read(|registry| {
                registry
                    .snapshot()
                    .iter()
                    .filter(|record| record.source.request_handle == "/req/dup")
                    .count()
            })
            .await;
        assert_eq!(
            count, 1,
            "the second Inhibit's record must be released, not left dangling with no Request \
             object routing to it"
        );

        let request_proxy = PortalRequestTestProxy::builder(&client)
            .path(handle.clone())
            .unwrap()
            .destination("me.aresa.Glimpse.Idle.Portal")
            .unwrap()
            .build()
            .await
            .unwrap();
        request_proxy.close().await.unwrap();
        assert!(find_record(&registry, first_id).await.is_none());
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn login1_fd_is_taken_only_when_targets_include_suspend_or_shutdown() {
        let bus = PrivateBus::start();
        let (_login1_conn, _app, registry, fake_login1) = start_server(&bus).await;
        let client = bus.connection().await;
        let proxy = PortalInhibitTestProxy::new(&client).await.unwrap();

        let idle_only = ObjectPath::try_from("/req/idle").unwrap();
        proxy
            .inhibit(&idle_only, "org.test.App", "", 8, HashMap::new())
            .await
            .unwrap();
        assert!(record_for_handle(&registry, "/req/idle").await.is_some());
        assert_eq!(
            fake_login1.readers.lock().unwrap().len(),
            0,
            "an idle-only target must never call login1.Inhibit"
        );

        let suspend = ObjectPath::try_from("/req/suspend").unwrap();
        proxy
            .inhibit(&suspend, "org.test.App", "", 4, HashMap::new())
            .await
            .unwrap();
        assert!(record_for_handle(&registry, "/req/suspend").await.is_some());
        assert_eq!(
            fake_login1.readers.lock().unwrap().len(),
            1,
            "a suspend-targeting Inhibit must take exactly one login1 fd"
        );

        let shutdown_and_suspend = ObjectPath::try_from("/req/both").unwrap();
        proxy
            .inhibit(&shutdown_and_suspend, "org.test.App", "", 5, HashMap::new())
            .await
            .unwrap();
        assert_eq!(fake_login1.readers.lock().unwrap().len(), 2);

        assert_eq!(
            *fake_login1.whats.lock().unwrap(),
            vec!["sleep".to_owned(), "sleep:shutdown".to_owned()],
            "the exact wire string login1.Inhibit received for each call"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn inhibit_rechecks_capacity_after_logind_returns_and_closes_its_fd() {
        let bus = PrivateBus::start();
        let (_login1_conn, _app, registry, fake_login1, started, release) =
            start_server_with_paused_login1(&bus).await;
        let client = bus.connection().await;
        let proxy = PortalInhibitTestProxy::new(&client).await.unwrap();

        let inhibit = tokio::spawn(async move {
            let handle = ObjectPath::try_from("/req/late_capacity").unwrap();
            proxy
                .inhibit(&handle, "org.test.App", "", 4, HashMap::new())
                .await
        });
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
                            source: IdleInhibitorSource::login1(0, 0, Login1Mode::Block),
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

        let error = inhibit.await.unwrap().unwrap_err();
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
        let mut readers = fake_login1.readers.lock().unwrap();
        let mut buf = [0u8; 1];
        assert_eq!(readers[0].read(&mut buf).unwrap(), 0);
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn create_monitor_serves_a_session_that_closes_and_query_end_is_accepted() {
        let bus = PrivateBus::start();
        let (_login1_conn, _app, _registry, _fake) = start_server(&bus).await;
        let client = bus.connection().await;
        let proxy = PortalMonitorTestProxy::new(&client).await.unwrap();

        let handle = ObjectPath::try_from("/req/monitor").unwrap();
        let session = ObjectPath::try_from("/session/monitor").unwrap();
        assert_eq!(
            proxy
                .create_monitor(&handle, &session, "org.test.App", "")
                .await
                .expect("xdg-desktop-portal routes this here because glimpse claims the interface"),
            0
        );

        proxy
            .query_end_response(&session)
            .await
            .expect("the acknowledgement is accepted even though glimpse never asks for one");

        let session_proxy = PortalSessionTestProxy::builder(&client)
            .destination("me.aresa.Glimpse.Idle.Portal")
            .unwrap()
            .path(session.clone())
            .unwrap()
            .build()
            .await
            .unwrap();
        session_proxy.close().await.expect("the session closes");
        assert!(
            session_proxy.close().await.is_err(),
            "the object is gone after the first close rather than lingering"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn app_id_and_reason_are_clamped_to_their_own_caps() {
        let bus = PrivateBus::start();
        let (_login1_conn, _app, registry, _fake_login1) = start_server(&bus).await;
        let client = bus.connection().await;
        let proxy = PortalInhibitTestProxy::new(&client).await.unwrap();

        let app_id = "é".repeat(200);
        let reason = "é".repeat(300);
        let mut options = HashMap::new();
        options.insert("reason", Value::from(reason.as_str()));

        let handle = ObjectPath::try_from("/req/clamp").unwrap();
        proxy
            .inhibit(&handle, &app_id, "", 8, options)
            .await
            .unwrap();

        let record = record_for_handle(&registry, "/req/clamp").await.unwrap();
        assert!(
            record.who.chars().count() <= WHO_CAP + 1,
            "{:?}",
            record.who
        );
        assert!(
            record.why.chars().count() <= WHY_CAP + 1,
            "{:?}",
            record.why
        );
        assert!(record.who.ends_with('…'));
        assert!(record.why.ends_with('…'));
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_second_acquisition_attempt_degrades_health_but_keeps_serving() {
        let bus = PrivateBus::start();
        let first = bus.connection().await;
        let (first_registry, _any_idle_target, _generation) = SharedRegistry::new();
        let first_health = Health::new().0;
        start(
            first.clone(),
            first_registry.clone(),
            None,
            first_health.clone(),
        )
        .await;
        assert_eq!(
            first_health.kind(Backend::Portal),
            glimpse_dbus::idle::HealthKind::Ready
        );

        let second = bus.connection().await;
        let (second_registry, _any_idle_target, _generation) = SharedRegistry::new();
        let second_health = Health::new().0;
        start(
            second.clone(),
            second_registry.clone(),
            None,
            second_health.clone(),
        )
        .await;

        let health = second_health.snapshot();
        assert_eq!(health.portal.kind, glimpse_dbus::idle::HealthKind::Degraded);
        assert_eq!(health.portal.message, "Bus name already owned");

        assert!(
            second
                .object_server()
                .interface::<_, PortalInhibit>(OBJECT_PATH)
                .await
                .is_ok(),
            "the un-owning second server must still register its own object"
        );

        let client = bus.connection().await;
        let proxy = PortalInhibitTestProxy::new(&client).await.unwrap();
        let handle = ObjectPath::try_from("/req/still_alive").unwrap();
        proxy
            .inhibit(&handle, "org.test.App", "", 8, HashMap::new())
            .await
            .unwrap();
        assert!(
            first_registry
                .read(|registry| registry
                    .snapshot()
                    .iter()
                    .any(|record| record.source.request_handle == "/req/still_alive"))
                .await,
            "the first, name-owning server must keep serving Inhibit"
        );
    }

    #[test]
    fn login1_what_orders_sleep_then_shutdown() {
        let targets = TestTargets {
            suspend: true,
            shutdown: true,
            ..TestTargets::NONE
        };
        assert_eq!(login1_what(&targets), "sleep:shutdown");
    }

    #[test]
    fn login1_what_is_empty_for_idle_only() {
        assert_eq!(login1_what(&TestTargets::idle_only()), "");
    }
}
