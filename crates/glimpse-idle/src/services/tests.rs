use std::time::Duration;

use glimpse_config::{Idle, IdleListener, IdleProfile, IdleProfiles};
use glimpse_dbus::idle::{IdleInhibitorRecord, IdleInhibitorSource, InhibitionTargets, Login1Mode};

use super::*;

struct NoopRunner;

impl crate::idle::CommandRunner for NoopRunner {
    fn run<'a>(
        &'a self,
        _command: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }
}

fn one_listener_config() -> Idle {
    Idle {
        enabled: true,
        respect_inhibitors: true,
        profiles: IdleProfiles {
            ac: IdleProfile {
                listeners: vec![IdleListener {
                    timeout: 10,
                    on_idle: "idle".into(),
                    on_resume: "resume".into(),
                    respect_inhibitors: None,
                }],
            },
            battery: IdleProfile { listeners: vec![] },
        },
    }
}

fn idle_targeting_record(id: u64) -> IdleInhibitorRecord {
    IdleInhibitorRecord {
        id,
        who: "who".into(),
        why: "why".into(),
        bus_name: String::new(),
        process_name: String::new(),
        source: IdleInhibitorSource::login1(0, 0, Login1Mode::Block),
        targets: InhibitionTargets::idle_only(),
        can_release: true,
        added_at_unix: 0,
    }
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(50)).await;
}
#[tokio::test(start_paused = true)]
async fn watch_registry_forwards_shared_registry_changes_into_the_actor() {
    let (shared_registry, any_idle_target_rx, _generation_rx) = SharedRegistry::new();
    let (actor, handle) = Actor::new(one_listener_config(), false, Arc::new(NoopRunner));
    let actor_cancel = CancellationToken::new();
    let actor_task = tokio::spawn(actor.run(actor_cancel.clone()));
    let registry_cancel = CancellationToken::new();
    let registry_task = tokio::spawn(watch_registry(
        any_idle_target_rx,
        handle.clone(),
        registry_cancel.clone(),
    ));

    let id = shared_registry
        .mutate(|registry| {
            let id = registry.mint_id();
            registry.insert(idle_targeting_record(id), None)
        })
        .await;
    settle().await;

    handle
        .send(Event::ListenerIdle {
            generation: 0,
            id: 0,
        })
        .await;
    settle().await;
    assert!(
        handle.snapshot().fired_listeners.is_empty(),
        "an idle-targeting record inserted into SharedRegistry must reach the actor through \
             watch_registry and suppress a respecting listener"
    );

    shared_registry
        .mutate(|registry| registry.release_record(id))
        .await;
    settle().await;
    assert_eq!(
        handle.snapshot().fired_listeners,
        vec![0],
        "releasing the last idle-targeting record must reach the actor through \
             watch_registry and fire the suppressed listener"
    );

    registry_cancel.cancel();
    actor_cancel.cancel();
    let _ = registry_task.await;
    let _ = actor_task.await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn emit_changes_sends_inhibitors_changed_on_every_mutation() {
    use glimpse_dbus::idle::Idle1Proxy;
    use glimpse_dbus::testing::PrivateBus;

    let bus = PrivateBus::start();
    let app = bus.connection().await;
    let (shared_registry, _any_idle_target_rx, generation_rx) = SharedRegistry::new();
    let server = Idle1Server::new(shared_registry.clone(), None, Health::new().0);
    app.object_server()
        .at(GLIMPSE_IDLE_OBJECT_PATH, server)
        .await
        .unwrap();
    glimpse_dbus::own_name(&app, GLIMPSE_IDLE_BUS_NAME)
        .await
        .unwrap();

    let cancel = CancellationToken::new();
    let task = tokio::spawn(emit_changes(
        generation_rx,
        app.clone(),
        cancel.clone(),
        Signal::Inhibitors,
    ));

    let client = bus.connection().await;
    let proxy = Idle1Proxy::new(&client).await.unwrap();
    let mut changes = proxy.receive_inhibitors_changed().await;
    let mut expiry_changes = proxy.receive_hold_expiries_changed().await;

    shared_registry
        .mutate(|registry| {
            let id = registry.mint_id();
            registry.insert(idle_targeting_record(id), None);
            registry.set_hold_expiry(id, 1_900);
        })
        .await;

    tokio::time::timeout(Duration::from_secs(5), changes.next())
        .await
        .expect("an Inhibitors PropertiesChanged signal within the timeout")
        .expect("a live property-change stream item");

    let changed = tokio::time::timeout(Duration::from_secs(5), expiry_changes.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        changed
            .get()
            .await
            .unwrap()
            .values()
            .copied()
            .collect::<Vec<_>>(),
        [1_900]
    );
    shared_registry
        .mutate(|registry| registry.release_record(1))
        .await;
    let changed = tokio::time::timeout(Duration::from_secs(5), expiry_changes.next())
        .await
        .unwrap()
        .unwrap();
    assert!(changed.get().await.unwrap().is_empty());

    cancel.cancel();
    let _ = task.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn emit_changes_refreshes_a_cached_health_property() {
    use glimpse_dbus::idle::{Idle1Proxy, InhibitorsHealth};
    use glimpse_dbus::testing::PrivateBus;
    use zbus::proxy::CacheProperties;

    let bus = PrivateBus::start();
    let app = bus.connection().await;
    let (shared_registry, _any_idle_target_rx, _generation_rx) = SharedRegistry::new();
    let (health, health_changes) = Health::new();
    let server = Idle1Server::new(shared_registry, None, health.clone());
    app.object_server()
        .at(GLIMPSE_IDLE_OBJECT_PATH, server)
        .await
        .unwrap();
    glimpse_dbus::own_name(&app, GLIMPSE_IDLE_BUS_NAME)
        .await
        .unwrap();

    let cancel = CancellationToken::new();
    let task = tokio::spawn(emit_changes(
        health_changes,
        app.clone(),
        cancel.clone(),
        Signal::Health,
    ));

    let client = bus.connection().await;
    let proxy = Idle1Proxy::builder(&client)
        .cache_properties(CacheProperties::Yes)
        .build()
        .await
        .unwrap();
    assert_eq!(
        proxy.cached_health().unwrap(),
        Some(InhibitorsHealth::default()),
        "nothing has reported yet, so every backend reads unknown"
    );

    let mut changes = proxy.receive_health_changed().await;
    health.degraded(
        crate::inhibitors::Backend::Login1,
        "cannot reach org.freedesktop.login1",
    );
    tokio::time::timeout(Duration::from_secs(5), changes.next())
        .await
        .expect("a Health PropertiesChanged signal within the timeout")
        .expect("a live property-change stream item");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if proxy.cached_health().unwrap().is_some_and(|health| {
                health.login1.kind == glimpse_dbus::idle::HealthKind::Degraded
            }) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the cached Health property refreshes after PropertiesChanged");

    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn reconfigure_waits_for_a_full_queue_and_applies_the_newest_config() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let connection = bus.connection().await;
    let (registry, _, _) = SharedRegistry::new();
    let idle1 = Exported::start(
        connection,
        GLIMPSE_IDLE_BUS_NAME,
        GLIMPSE_IDLE_OBJECT_PATH,
        Idle1Server::new(registry, None, Health::new().0),
        std::future::pending::<()>(),
    )
    .await
    .unwrap();
    let (actor, handle) = Actor::new(one_listener_config(), false, Arc::new(NoopRunner));
    while handle
        .command_tx
        .try_send(Event::RegistryChanged(false))
        .is_ok()
    {}
    let cancel = CancellationToken::new();
    let mut services = IdleServices {
        handle,
        cancel: cancel.clone(),
        tasks: vec![],
        idle1,
        actions: None,
    };
    let mut config = Config {
        idle: one_listener_config(),
        ..Config::default()
    };
    config.idle.profiles.ac.listeners[0].timeout = 20;
    let mut pending = Box::pin(services.reconfigure(&config));
    assert!(futures_util::poll!(&mut pending).is_pending());
    let task = tokio::spawn(actor.run(cancel));
    pending.await;
    config.idle.profiles.ac.listeners[0].timeout = 30;
    services.reconfigure(&config).await;
    let mut state = services.handle.subscribe();
    while state.borrow_and_update().listeners[0].timeout != 30 {
        state.changed().await.unwrap();
    }
    assert_eq!(state.borrow().listeners[0].timeout, 30);
    services.tasks.push(task);
    services.shutdown().await;
}
