use std::io::Read as _;

use glimpse_dbus::idle::{IdleInhibitorRecord as TestRecord, InhibitionTargets as TestTargets};
use glimpse_dbus::testing::PrivateBus;
use zbus::zvariant::Value;

use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::*;
use crate::inhibitors::screen_saver::watch_disconnects;
use crate::inhibitors::test_support::{FakeLogin1, start_fake_login1, start_paused_fake_login1};

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
    assert_eq!(record.bus_name, client.unique_name().unwrap().as_str());
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
async fn portal_per_app_cap_holds() {
    let bus = PrivateBus::start();
    let (_login1_conn, _app, registry, fake_login1) = start_server(&bus).await;
    let client = bus.connection().await;
    let proxy = PortalInhibitTestProxy::new(&client).await.unwrap();

    for index in 0..crate::inhibitors::Registry::MAX_INHIBITORS_PER_BUS {
        let handle = ObjectPath::try_from(format!("/req/cap{index}")).unwrap();
        proxy
            .inhibit(&handle, "org.test.A", "", 8, HashMap::new())
            .await
            .unwrap();
    }

    let handle = ObjectPath::try_from("/req/cap_over").unwrap();
    let error = proxy
        .inhibit(&handle, "org.test.A", "", 4, HashMap::new())
        .await
        .unwrap_err();
    assert!(
        matches!(
            &error,
            zbus::Error::MethodError(name, _, _)
                if name.as_str() == "org.freedesktop.DBus.Error.LimitsExceeded"
        ),
        "unexpected error: {error:?}"
    );
    assert!(
        record_for_handle(&registry, "/req/cap_over")
            .await
            .is_none()
    );
    assert!(fake_login1.readers.lock().unwrap().is_empty());
    assert_eq!(
        registry.read(|registry| registry.snapshot().len()).await,
        crate::inhibitors::Registry::MAX_INHIBITORS_PER_BUS
    );

    let other = ObjectPath::try_from("/req/cap_other").unwrap();
    proxy
        .inhibit(&other, "org.test.B", "", 8, HashMap::new())
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_departed_frontend_releases_its_records() {
    let bus = PrivateBus::start();
    let (_login1_conn, app, registry, fake_login1) = start_server(&bus).await;
    let cancel = CancellationToken::new();
    let watch_task = tokio::spawn(watch_disconnects(
        app.clone(),
        registry.clone(),
        cancel.clone(),
    ));
    let client = bus.connection().await;
    let proxy = PortalInhibitTestProxy::new(&client).await.unwrap();
    let handle = ObjectPath::try_from("/req/frontend").unwrap();
    proxy
        .inhibit(&handle, "org.test.App", "", 8 | 4, HashMap::new())
        .await
        .unwrap();
    assert!(
        record_for_handle(&registry, "/req/frontend")
            .await
            .is_some()
    );
    assert_eq!(fake_login1.readers.lock().unwrap().len(), 1);

    drop(proxy);
    drop(client);
    tokio::time::timeout(Duration::from_secs(1), async {
        while record_for_handle(&registry, "/req/frontend")
            .await
            .is_some()
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the record is released once its frontend leaves the bus");

    let mut buf = [0u8; 1];
    assert_eq!(
        fake_login1.readers.lock().unwrap()[0]
            .read(&mut buf)
            .unwrap(),
        0
    );
    wait_until_request_is_gone(&bus, &handle).await;
    cancel.cancel();
    watch_task.await.unwrap();
}

async fn request_is_gone(observer: &Connection, handle: &ObjectPath<'_>) -> bool {
    zbus::fdo::IntrospectableProxy::builder(observer)
        .destination("me.aresa.Glimpse.Idle.Portal")
        .unwrap()
        .path(handle.clone())
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await
        .is_err()
}

async fn wait_until_request_is_gone(bus: &PrivateBus, handle: &ObjectPath<'_>) {
    let observer = bus.connection().await;
    tokio::time::timeout(Duration::from_secs(1), async {
        while !request_is_gone(&observer, handle).await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the Request object is removed with its record");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn releasing_a_portal_record_through_idle1_removes_its_request_object() {
    let bus = PrivateBus::start();
    let (_login1_conn, app, registry, _fake_login1) = start_server(&bus).await;
    let idle1 = crate::inhibitors::Idle1Server::new(registry.clone(), None, Health::new().0);
    app.object_server()
        .at(glimpse_dbus::idle::GLIMPSE_IDLE_OBJECT_PATH, idle1)
        .await
        .unwrap();
    glimpse_dbus::own_name(&app, glimpse_dbus::idle::GLIMPSE_IDLE_BUS_NAME)
        .await
        .unwrap();
    let client = bus.connection().await;
    let proxy = PortalInhibitTestProxy::new(&client).await.unwrap();
    let handle = ObjectPath::try_from("/req/idle1_release").unwrap();
    proxy
        .inhibit(&handle, "org.test.App", "", 8, HashMap::new())
        .await
        .unwrap();
    let id = record_for_handle(&registry, "/req/idle1_release")
        .await
        .unwrap()
        .id;

    glimpse_dbus::idle::Idle1Proxy::new(&client)
        .await
        .unwrap()
        .release(id)
        .await
        .unwrap();

    assert!(find_record(&registry, id).await.is_none());
    wait_until_request_is_gone(&bus, &handle).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_empty_app_id_is_named_after_the_daemon_rather_than_the_frontend() {
    let bus = PrivateBus::start();
    let (_login1_conn, _app, registry, _fake_login1) = start_server(&bus).await;
    let client = bus.connection().await;
    let proxy = PortalInhibitTestProxy::new(&client).await.unwrap();
    let handle = ObjectPath::try_from("/req/anonymous").unwrap();
    proxy
        .inhibit(&handle, "", "", 8, HashMap::new())
        .await
        .unwrap();

    let record = record_for_handle(&registry, "/req/anonymous")
        .await
        .unwrap();
    assert_eq!(record.who, "glimpse-idle");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_frontend_may_hold_more_records_than_the_per_source_limit_across_apps() {
    let bus = PrivateBus::start();
    let (_login1_conn, _app, registry, _fake_login1) = start_server(&bus).await;
    let client = bus.connection().await;
    let proxy = PortalInhibitTestProxy::new(&client).await.unwrap();

    let count = crate::inhibitors::Registry::MAX_INHIBITORS_PER_BUS + 1;
    for index in 0..count {
        let handle = ObjectPath::try_from(format!("/req/many{index}")).unwrap();
        proxy
            .inhibit(
                &handle,
                &format!("org.test.App{index}"),
                "",
                8,
                HashMap::new(),
            )
            .await
            .unwrap();
    }
    assert_eq!(
        registry.read(|registry| registry.snapshot().len()).await,
        count
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_during_the_login1_call_releases_the_record() {
    let bus = PrivateBus::start();
    let (_login1_conn, _app, registry, fake_login1, started, release) =
        start_server_with_paused_login1(&bus).await;
    let client = bus.connection().await;
    let inhibit_client = bus.connection().await;
    let handle = ObjectPath::try_from("/req/close_mid_call").unwrap();

    let inhibit = tokio::spawn({
        let handle = handle.clone();
        async move {
            let proxy = PortalInhibitTestProxy::new(&inhibit_client).await.unwrap();
            proxy
                .inhibit(&handle, "org.test.App", "", 4, HashMap::new())
                .await
        }
    });
    started.notified().await;
    let request_proxy = PortalRequestTestProxy::builder(&client)
        .path(handle.clone())
        .unwrap()
        .destination("me.aresa.Glimpse.Idle.Portal")
        .unwrap()
        .build()
        .await
        .unwrap();
    request_proxy
        .close()
        .await
        .expect("the Request object exists before logind answers");
    assert!(
        record_for_handle(&registry, "/req/close_mid_call")
            .await
            .is_none()
    );

    release.notify_one();
    inhibit.await.unwrap().unwrap();
    let mut buf = [0u8; 1];
    assert_eq!(
        fake_login1.readers.lock().unwrap()[0]
            .read(&mut buf)
            .unwrap(),
        0
    );
    assert!(
        record_for_handle(&registry, "/req/close_mid_call")
            .await
            .is_none()
    );
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
fn a_hold_is_named_after_the_caller_and_its_reason_when_given() {
    assert_eq!(
        hold_label("org.gnome.TextEditor", "glimpse-idle"),
        "org.gnome.TextEditor"
    );
    assert_eq!(hold_label("", "glimpse-idle"), "glimpse-idle");
    assert_eq!(hold_label("", "Portal inhibit"), "Portal inhibit");
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
