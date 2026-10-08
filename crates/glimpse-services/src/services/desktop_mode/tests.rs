use super::*;
use crate::Running;
use glimpse_dbus::Buses;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct Writer {
    writes: Arc<Mutex<Vec<Option<Mode>>>>,
    observed: watch::Sender<Result<Option<Mode>, String>>,
    failure: Arc<Mutex<Option<String>>>,
}

impl ModeWriter for Writer {
    fn write(&self, mode: Option<Mode>) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            if let Some(reason) = self.failure.lock().unwrap().clone() {
                return Err(reason);
            }
            self.writes.lock().unwrap().push(mode);
            let _ = self.observed.send_replace(Ok(mode));
            Ok(())
        })
    }
}

struct Fixture {
    runtime: Running<DesktopMode>,
    handle: DesktopModeHandle,
    config: glimpse_config::Config,
    solar: watch::Sender<Option<SolarStatus>>,
    writer: Writer,
}

impl Fixture {
    fn new(policy: AutoDark, observed: Option<Mode>, phase: Option<SolarPhase>) -> Self {
        let (solar, phases) = watch::channel(phase.map(|phase| SolarStatus {
            phase,
            next_change: None,
        }));
        let (observer, observed_rx) = watch::channel(Ok(observed));
        let writer = Writer {
            writes: Arc::default(),
            observed: observer,
            failure: Arc::default(),
        };
        let mut config = glimpse_config::Config::default();
        config.appearance.auto_dark = policy;
        let (runtime, handle) = Running::<DesktopMode>::spawn(
            &config,
            Buses::unavailable("test"),
            ModeDependencies {
                solar: phases,
                observed: observed_rx,
                writer: Box::new(writer.clone()),
            },
        );
        Self {
            runtime,
            handle,
            config,
            solar,
            writer,
        }
    }

    async fn observed(&self, mode: Option<Mode>) {
        timeout(
            Duration::from_secs(2),
            self.handle
                .subscribe()
                .wait_for(|status| status.observed == mode),
        )
        .await
        .unwrap()
        .unwrap();
    }

    async fn running(&self) {
        timeout(
            Duration::from_secs(2),
            self.handle
                .health()
                .wait_for(|health| *health == ServiceState::Running),
        )
        .await
        .unwrap()
        .unwrap();
    }

    fn writes(&self) -> Vec<Option<Mode>> {
        self.writer.writes.lock().unwrap().clone()
    }
}

#[tokio::test]
async fn solar_changes_apply_once_and_external_changes_do_not_feedback() {
    let mut fixture = Fixture::new(AutoDark::Auto, Some(Mode::Dark), Some(SolarPhase::Day));
    fixture.observed(Some(Mode::Light)).await;
    fixture.running().await;
    let _ = fixture.writer.observed.send_replace(Ok(Some(Mode::Dark)));
    fixture.observed(Some(Mode::Dark)).await;
    fixture.solar.send_replace(Some(SolarStatus {
        phase: SolarPhase::Day,
        next_change: Some(chrono::Utc::now()),
    }));
    fixture.handle.set_temporary(Mode::Light).await.unwrap();
    assert_eq!(fixture.writes(), [Some(Mode::Light), Some(Mode::Light)]);
    fixture.solar.send_replace(Some(SolarStatus {
        phase: SolarPhase::Night,
        next_change: None,
    }));
    fixture.handle.resume().await.unwrap();
    fixture.observed(Some(Mode::Dark)).await;
    assert_eq!(fixture.writes().last(), Some(&Some(Mode::Dark)));
    fixture.runtime.stop().await;
}

#[tokio::test]
async fn system_holds_keep_the_original_baseline_and_preserve_external_changes() {
    let mut fixture = Fixture::new(AutoDark::System, None, None);
    fixture.running().await;
    assert!(fixture.writes().is_empty());
    fixture.handle.set_temporary(Mode::Dark).await.unwrap();
    fixture.handle.set_temporary(Mode::Light).await.unwrap();
    fixture.handle.resume().await.unwrap();
    fixture.observed(None).await;
    assert_eq!(
        fixture.writes(),
        [Some(Mode::Dark), Some(Mode::Light), None]
    );
    fixture.handle.set_temporary(Mode::Dark).await.unwrap();
    let _ = fixture.writer.observed.send_replace(Ok(Some(Mode::Light)));
    fixture.observed(Some(Mode::Light)).await;
    fixture.handle.resume().await.unwrap();
    assert_eq!(fixture.writes().len(), 4);
    fixture.handle.set_temporary(Mode::Dark).await.unwrap();
    fixture.runtime.stop().await;
    assert_eq!(fixture.writes().last(), Some(&Some(Mode::Light)));
}

#[tokio::test]
async fn only_policy_reload_clears_a_hold_and_failures_are_visible() {
    let mut fixture = Fixture::new(AutoDark::System, Some(Mode::Light), Some(SolarPhase::Night));
    fixture.running().await;
    fixture.handle.set_temporary(Mode::Light).await.unwrap();
    fixture.config.appearance.theme = "test".into();
    fixture.runtime.reconfigure(&fixture.config);
    fixture.handle.set_temporary(Mode::Light).await.unwrap();
    assert_eq!(fixture.handle.snapshot().temporary, Some(Mode::Light));
    fixture.config.appearance.auto_dark = AutoDark::Auto;
    fixture.runtime.reconfigure(&fixture.config);
    fixture.observed(Some(Mode::Dark)).await;
    assert_eq!(fixture.handle.snapshot().temporary, None);
    *fixture.writer.failure.lock().unwrap() = Some("write refused".into());
    assert!(fixture.handle.set_temporary(Mode::Light).await.is_err());
    assert!(matches!(
        fixture.handle.health().borrow().clone(),
        ServiceState::Degraded { .. }
    ));
    fixture.runtime.stop().await;
}

#[tokio::test]
async fn missing_solar_does_not_prevent_a_temporary_mode_or_system_policy() {
    let mut fixture = Fixture::new(AutoDark::Auto, None, None);
    fixture.handle.set_temporary(Mode::Dark).await.unwrap();
    fixture.running().await;
    assert!(fixture.handle.resume().await.is_err());
    fixture.observed(Some(Mode::Dark)).await;
    fixture.config.appearance.auto_dark = AutoDark::System;
    fixture.runtime.reconfigure(&fixture.config);
    fixture.running().await;
    let _ = fixture
        .writer
        .observed
        .send_replace(Err("backend absent".into()));
    timeout(
        Duration::from_secs(2),
        fixture
            .handle
            .health()
            .wait_for(|state| matches!(state, ServiceState::Degraded { .. })),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(fixture.handle.set_temporary(Mode::Light).await.is_err());
    fixture.runtime.stop().await;
}

struct Delayed(
    tokio::sync::mpsc::UnboundedSender<(Option<Mode>, oneshot::Sender<()>)>,
    watch::Sender<Result<Option<Mode>, String>>,
);

impl ModeWriter for Delayed {
    fn write(&self, mode: Option<Mode>) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            let (release, ready) = oneshot::channel();
            self.0.send((mode, release)).unwrap();
            ready.await.unwrap();
            let _ = self.1.send_replace(Ok(mode));
            Ok(())
        })
    }
}

#[tokio::test]
async fn shutdown_restores_the_baseline_while_replacing_a_hold() {
    let (requests, mut writes) = tokio::sync::mpsc::unbounded_channel();
    let (_solar, phases) = watch::channel(None);
    let (observer, observed) = watch::channel(Ok(None));
    let (mut runtime, handle) = Running::<DesktopMode>::spawn(
        &glimpse_config::Config::default(),
        Buses::unavailable("test"),
        ModeDependencies {
            solar: phases,
            observed,
            writer: Box::new(Delayed(requests, observer)),
        },
    );
    let first = handle.clone();
    let reply = tokio::spawn(async move { first.set_temporary(Mode::Dark).await });
    let (mode, release) = writes.recv().await.unwrap();
    assert_eq!(mode, Some(Mode::Dark));
    release.send(()).unwrap();
    reply.await.unwrap().unwrap();
    let reply = tokio::spawn(async move { handle.set_temporary(Mode::Light).await });
    let (mode, _release) = writes.recv().await.unwrap();
    assert_eq!(mode, Some(Mode::Light));
    let stopped = tokio::spawn(async move { runtime.stop().await });
    let (mode, release) = timeout(Duration::from_secs(2), writes.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(mode, None);
    release.send(()).unwrap();
    stopped.await.unwrap();
    assert!(reply.await.unwrap().is_err());
}

#[tokio::test]
async fn a_slow_write_cannot_overwrite_a_newer_policy_or_its_health() {
    let (requests, mut writes) = tokio::sync::mpsc::unbounded_channel();
    let (_solar, phases) = watch::channel(Some(SolarStatus {
        phase: SolarPhase::Day,
        next_change: None,
    }));
    let (observer, observed) = watch::channel(Ok(None));
    let mut config = glimpse_config::Config::default();
    config.appearance.auto_dark = AutoDark::System;
    let (mut runtime, handle) = Running::<DesktopMode>::spawn(
        &config,
        Buses::unavailable("test"),
        ModeDependencies {
            solar: phases,
            observed,
            writer: Box::new(Delayed(requests, observer)),
        },
    );
    let first = handle.clone();
    let reply = tokio::spawn(async move { first.set_temporary(Mode::Dark).await });
    let (mode, release) = timeout(Duration::from_secs(2), writes.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(mode, Some(Mode::Dark));
    config.appearance.auto_dark = AutoDark::Auto;
    runtime.reconfigure(&config);
    timeout(
        Duration::from_secs(2),
        handle
            .subscribe()
            .wait_for(|status| status.automatic && status.temporary.is_none()),
    )
    .await
    .unwrap()
    .unwrap();
    release.send(()).unwrap();
    assert!(reply.await.unwrap().is_err());
    let (mode, release) = timeout(Duration::from_secs(2), writes.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(mode, Some(Mode::Light));
    release.send(()).unwrap();
    timeout(
        Duration::from_secs(2),
        handle
            .subscribe()
            .wait_for(|status| status.observed == Some(Mode::Light)),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(handle.snapshot().requested, Some(Mode::Light));
    runtime.stop().await;
}
