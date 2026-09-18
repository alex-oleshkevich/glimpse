use std::sync::Arc;

use anyhow::{Context as _, Result};
use futures_util::StreamExt;
use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_dbus::Exported;
use glimpse_dbus::idle::{
    BackendHealth, GLIMPSE_IDLE_BUS_NAME, GLIMPSE_IDLE_OBJECT_PATH, HealthKind,
};
use glimpse_dbus::login1::Login1ManagerProxy;
use glimpse_dbus::upower::UPowerProxy;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use zbus::Connection;
use zbus::proxy::PropertyStream;

use crate::idle::{Actor, Event, Handle, ShellRunner};
use crate::inhibitors::{Idle1Server, SharedRegistry, login1_observer, portal, screen_saver};
use crate::wayland_notify;

pub struct IdleServices {
    handle: Handle,
    actor_cancel: CancellationToken,
    wayland_cancel: CancellationToken,
    battery_cancel: CancellationToken,
    registry_cancel: CancellationToken,
    generation_cancel: CancellationToken,
    health_generation_cancel: CancellationToken,
    screen_saver_watch_cancel: CancellationToken,
    login1_observer_cancel: CancellationToken,
    actor_task: JoinHandle<()>,
    wayland_task: JoinHandle<()>,
    battery_task: JoinHandle<()>,
    registry_task: JoinHandle<()>,
    generation_task: JoinHandle<()>,
    health_generation_task: JoinHandle<()>,
    screen_saver_watch_task: JoinHandle<()>,
    login1_observer_task: JoinHandle<()>,
    idle1: Exported<Idle1Server>,
}

impl IdleServices {
    /// The D-Bus name is taken first, before any other task is spawned: `own_name` failing is
    /// fatal, matching every other glimpse provider's primary name, and taking it first means a
    /// duplicate `glimpse-idle` never touches the Wayland, UPower or registry resources the
    /// running one already holds. The system bus (and so `login1`) is independently optional —
    /// only `Hold` needs it, so its absence degrades that one method rather than the whole
    /// control interface.
    pub async fn start(document: &Config) -> Result<Self> {
        let buses = Buses::connect().await;
        let session = buses
            .session_bus()
            .cloned()
            .map_err(|reason| anyhow::anyhow!(reason.to_owned()))
            .context("the idle provider needs the session bus")?;

        let (shared_registry, any_idle_target_rx, generation_rx) = SharedRegistry::new();
        let (health_generation, health_generation_rx) = watch::channel(0);
        let login1 = connect_login1(&buses).await;

        let screen_saver_health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        let portal_health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        let login1_health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        let idle1 = Exported::start(
            session.clone(),
            GLIMPSE_IDLE_BUS_NAME,
            GLIMPSE_IDLE_OBJECT_PATH,
            Idle1Server::new(
                shared_registry.clone(),
                login1.clone(),
                screen_saver_health.clone(),
                portal_health.clone(),
                login1_health.clone(),
            ),
            std::future::pending::<()>(),
        )
        .await
        .context("another glimpse-idle already owns its D-Bus name")?;

        screen_saver::start(
            session.clone(),
            shared_registry.clone(),
            screen_saver_health,
        )
        .await;

        portal::start(
            session.clone(),
            shared_registry.clone(),
            login1.clone(),
            portal_health,
        )
        .await;

        let upower = connect_upower(&buses).await;
        let (on_battery, changes) = match &upower {
            Some(proxy) => {
                let changes = proxy.receive_on_battery_changed().await;
                let on_battery = proxy.on_battery().await.unwrap_or_else(|error| {
                    tracing::warn!(
                        %error,
                        "cannot read the UPower on-battery state; assuming AC power"
                    );
                    false
                });
                (on_battery, Some(changes))
            }
            None => (false, None),
        };

        let (actor, handle) = Actor::new(document.idle.clone(), on_battery, Arc::new(ShellRunner));
        let actor_cancel = CancellationToken::new();
        let actor_task = tokio::spawn(actor.run(actor_cancel.clone()));

        let wayland_cancel = CancellationToken::new();
        let wayland_task =
            tokio::spawn(wayland_notify::run(handle.clone(), wayland_cancel.clone()));

        let battery_cancel = CancellationToken::new();
        let battery_task = tokio::spawn(watch_battery(
            changes,
            handle.clone(),
            battery_cancel.clone(),
        ));

        let registry_cancel = CancellationToken::new();
        let registry_task = tokio::spawn(watch_registry(
            any_idle_target_rx,
            handle.clone(),
            registry_cancel.clone(),
        ));

        let generation_cancel = CancellationToken::new();
        let generation_task = tokio::spawn(watch_generation(
            generation_rx,
            session.clone(),
            generation_cancel.clone(),
        ));

        let health_generation_cancel = CancellationToken::new();
        let health_generation_task = tokio::spawn(watch_health_generation(
            health_generation_rx,
            session.clone(),
            health_generation_cancel.clone(),
        ));

        let screen_saver_watch_cancel = CancellationToken::new();
        let screen_saver_watch_task = tokio::spawn(screen_saver::watch_disconnects(
            session.clone(),
            shared_registry.clone(),
            screen_saver_watch_cancel.clone(),
        ));

        let login1_observer_cancel = CancellationToken::new();
        let login1_observer_task = tokio::spawn(login1_observer::start(
            login1,
            shared_registry,
            login1_health,
            health_generation,
            login1_observer_cancel.clone(),
        ));

        tracing::info!("idle service graph started");
        Ok(Self {
            handle,
            actor_cancel,
            wayland_cancel,
            battery_cancel,
            registry_cancel,
            generation_cancel,
            health_generation_cancel,
            screen_saver_watch_cancel,
            login1_observer_cancel,
            actor_task,
            wayland_task,
            battery_task,
            registry_task,
            generation_task,
            health_generation_task,
            screen_saver_watch_task,
            login1_observer_task,
            idle1,
        })
    }

    pub fn reconfigure(&self, document: &Config) {
        if let Err(error) = self
            .handle
            .try_send(Event::ApplyConfig(document.idle.clone()))
        {
            tracing::warn!(%error, "failed to apply reloaded idle configuration");
        }
    }

    pub async fn shutdown(self) {
        tracing::info!("idle shutting down");
        self.idle1.shutdown().await;
        self.wayland_cancel.cancel();
        self.battery_cancel.cancel();
        self.registry_cancel.cancel();
        self.generation_cancel.cancel();
        self.health_generation_cancel.cancel();
        self.screen_saver_watch_cancel.cancel();
        self.login1_observer_cancel.cancel();
        self.actor_cancel.cancel();
        let _ = self.wayland_task.await;
        let _ = self.battery_task.await;
        let _ = self.registry_task.await;
        let _ = self.generation_task.await;
        let _ = self.health_generation_task.await;
        let _ = self.screen_saver_watch_task.await;
        let _ = self.login1_observer_task.await;
        let _ = self.actor_task.await;
        tracing::info!("idle stopped");
    }
}

async fn connect_login1(buses: &Buses) -> Option<Login1ManagerProxy<'static>> {
    let connection = match buses.system_bus() {
        Ok(connection) => connection,
        Err(reason) => {
            tracing::warn!(%reason, "no system bus; Idle1.Hold will be unavailable");
            return None;
        }
    };
    match Login1ManagerProxy::new(connection).await {
        Ok(proxy) => Some(proxy),
        Err(error) => {
            tracing::warn!(
                %error,
                "cannot reach org.freedesktop.login1; Idle1.Hold will be unavailable"
            );
            None
        }
    }
}

async fn watch_registry(
    mut any_idle_target: watch::Receiver<bool>,
    handle: Handle,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            changed = any_idle_target.changed() => match changed {
                Ok(()) => {
                    let value = *any_idle_target.borrow();
                    handle.send(Event::RegistryChanged(value)).await;
                }
                Err(_) => break,
            }
        }
    }
}

async fn watch_generation(
    mut generation: watch::Receiver<u64>,
    connection: Connection,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            changed = generation.changed() => match changed {
                Ok(()) => {
                    let interface = match connection
                        .object_server()
                        .interface::<_, Idle1Server>(GLIMPSE_IDLE_OBJECT_PATH)
                        .await
                    {
                        Ok(interface) => interface,
                        Err(error) => {
                            tracing::warn!(%error, "idle control object unavailable for an Inhibitors change signal");
                            continue;
                        }
                    };
                    if let Err(error) = interface
                        .get()
                        .await
                        .inhibitors_changed(interface.signal_emitter())
                        .await
                    {
                        tracing::warn!(%error, "failed to emit Idle1.Inhibitors change");
                    }
                }
                Err(_) => break,
            }
        }
    }
}

async fn watch_health_generation(
    mut generation: watch::Receiver<u64>,
    connection: Connection,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            changed = generation.changed() => match changed {
                Ok(()) => {
                    let interface = match connection
                        .object_server()
                        .interface::<_, Idle1Server>(GLIMPSE_IDLE_OBJECT_PATH)
                        .await
                    {
                        Ok(interface) => interface,
                        Err(error) => {
                            tracing::warn!(%error, "idle control object unavailable for a Health change signal");
                            continue;
                        }
                    };
                    if let Err(error) = interface
                        .get()
                        .await
                        .health_changed(interface.signal_emitter())
                        .await
                    {
                        tracing::warn!(%error, "failed to emit Idle1.Health change");
                    }
                }
                Err(_) => break,
            }
        }
    }
}

async fn connect_upower(buses: &Buses) -> Option<UPowerProxy<'static>> {
    let connection = match buses.system_bus() {
        Ok(connection) => connection,
        Err(reason) => {
            tracing::warn!(%reason, "no system bus; idle will not follow AC/battery changes");
            return None;
        }
    };
    match UPowerProxy::new(connection).await {
        Ok(proxy) => Some(proxy),
        Err(error) => {
            tracing::warn!(%error, "cannot reach org.freedesktop.UPower; assuming AC power");
            None
        }
    }
}

async fn watch_battery(
    changes: Option<PropertyStream<'static, bool>>,
    handle: Handle,
    cancel: CancellationToken,
) {
    let Some(mut changes) = changes else {
        return;
    };
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            change = changes.next() => match change {
                Some(change) => match change.get().await {
                    Ok(on_battery) => handle.send(Event::OnBattery(on_battery)).await,
                    Err(error) => tracing::warn!(%error, "failed to read the on-battery property change"),
                },
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use glimpse_config::{Idle, IdleListener, IdleProfile, IdleProfiles};
    use glimpse_dbus::idle::{
        IdleInhibitorRecord, IdleInhibitorSource, InhibitionTargets, Login1Mode,
    };

    use super::*;

    /// A `CommandRunner` that never shells out — `ShellRunner` would actually run `on_idle`
    /// through `/bin/sh -c`, and this test only needs to observe `fired_listeners`, not exercise
    /// real command execution (that belongs to `idle.rs`'s own tests).
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

    /// F5: `shared.rs` proves the watch channel republishes and `idle.rs` proves the actor reacts
    /// to `Event::RegistryChanged` directly; this is the only test that proves `watch_registry` —
    /// the glue between the two — actually forwards one into the other.
    #[tokio::test]
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

    /// Proves `watch_generation` is the crate's single `Inhibitors`-changed emitter: a
    /// `SharedRegistry::mutate` call that actually changes state reaches a live
    /// `Idle1.Inhibitors` `PropertiesChanged` signal, with no emission code in the caller.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn watch_generation_emits_inhibitors_changed_on_every_mutation() {
        use glimpse_dbus::idle::Idle1Proxy;
        use glimpse_dbus::testing::PrivateBus;

        let bus = PrivateBus::start();
        let app = bus.connection().await;
        let (shared_registry, _any_idle_target_rx, generation_rx) = SharedRegistry::new();
        let health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        let login1_health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        let portal_health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        let server = Idle1Server::new(
            shared_registry.clone(),
            None,
            health,
            portal_health,
            login1_health,
        );
        app.object_server()
            .at(GLIMPSE_IDLE_OBJECT_PATH, server)
            .await
            .unwrap();
        glimpse_dbus::own_name(&app, GLIMPSE_IDLE_BUS_NAME)
            .await
            .unwrap();

        let cancel = CancellationToken::new();
        let task = tokio::spawn(watch_generation(generation_rx, app.clone(), cancel.clone()));

        let client = bus.connection().await;
        let proxy = Idle1Proxy::new(&client).await.unwrap();
        let mut changes = proxy.receive_inhibitors_changed().await;

        shared_registry
            .mutate(|registry| {
                let id = registry.mint_id();
                registry.insert(idle_targeting_record(id), None)
            })
            .await;

        tokio::time::timeout(Duration::from_secs(5), changes.next())
            .await
            .expect("an Inhibitors PropertiesChanged signal within the timeout")
            .expect("a live property-change stream item");

        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn watch_health_generation_refreshes_a_cached_health_property() {
        use glimpse_dbus::idle::{Idle1Proxy, InhibitorsHealth};
        use glimpse_dbus::testing::PrivateBus;
        use zbus::proxy::CacheProperties;

        let bus = PrivateBus::start();
        let app = bus.connection().await;
        let (shared_registry, _any_idle_target_rx, _generation_rx) = SharedRegistry::new();
        let screen_saver_health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        let portal_health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        let login1_health = Arc::new(std::sync::Mutex::new(BackendHealth {
            kind: HealthKind::Unsupported,
            message: String::new(),
        }));
        let server = Idle1Server::new(
            shared_registry,
            None,
            screen_saver_health,
            portal_health,
            login1_health.clone(),
        );
        app.object_server()
            .at(GLIMPSE_IDLE_OBJECT_PATH, server)
            .await
            .unwrap();
        glimpse_dbus::own_name(&app, GLIMPSE_IDLE_BUS_NAME)
            .await
            .unwrap();

        let (health_generation, health_generation_rx) = watch::channel(0);
        let cancel = CancellationToken::new();
        let task = tokio::spawn(watch_health_generation(
            health_generation_rx,
            app.clone(),
            cancel.clone(),
        ));

        let client = bus.connection().await;
        let proxy = Idle1Proxy::builder(&client)
            .cache_properties(CacheProperties::Yes)
            .build()
            .await
            .unwrap();
        assert_eq!(
            proxy.cached_health().unwrap(),
            Some(InhibitorsHealth {
                screen_saver: BackendHealth {
                    kind: HealthKind::Unsupported,
                    message: String::new(),
                },
                portal: BackendHealth {
                    kind: HealthKind::Unsupported,
                    message: String::new(),
                },
                login1: BackendHealth {
                    kind: HealthKind::Unsupported,
                    message: String::new(),
                },
            })
        );

        let mut changes = proxy.receive_health_changed().await;
        login1_observer::set_health(
            &login1_health,
            &health_generation,
            BackendHealth {
                kind: HealthKind::Degraded,
                message: "cannot reach org.freedesktop.login1".to_owned(),
            },
        );
        tokio::time::timeout(Duration::from_secs(5), changes.next())
            .await
            .expect("a Health PropertiesChanged signal within the timeout")
            .expect("a live property-change stream item");
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if proxy
                    .cached_health()
                    .unwrap()
                    .is_some_and(|health| health.login1.kind == HealthKind::Degraded)
                {
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
}
