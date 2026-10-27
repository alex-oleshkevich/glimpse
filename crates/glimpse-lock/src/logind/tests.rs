use super::*;

#[test]
fn only_a_release_drops_the_inhibitor() {
    let mut held = Some(7);
    assert_eq!(
        step(&mut held, Request::TakeInhibitor),
        None,
        "one is held already"
    );
    assert_eq!(held, Some(7));
    assert_eq!(step(&mut held, Request::ReleaseInhibitor), None);
    assert_eq!(held, None);
    assert_eq!(step(&mut held, Request::ReleaseInhibitor), None);
    assert_eq!(step(&mut held, Request::TakeInhibitor), Some(Call::Inhibit));
}

#[derive(Default)]
struct TestSnapshot {
    preparing: std::sync::atomic::AtomicBool,
    delayed: std::sync::atomic::AtomicBool,
    started: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

struct TestManager {
    bad_delay: bool,
    snapshot: std::sync::Arc<TestSnapshot>,
}

#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl TestManager {
    fn list_sessions(&self) -> Vec<(String, u32, String, String, zbus::zvariant::OwnedObjectPath)> {
        vec![(
            "qa".into(),
            glimpse_dbus::login1::current_uid(),
            "qa".into(),
            "seat0".into(),
            "/org/freedesktop/login1/session/qa".try_into().unwrap(),
        )]
    }

    #[zbus(property)]
    async fn preparing_for_sleep(&self) -> bool {
        let preparing = self
            .snapshot
            .preparing
            .load(std::sync::atomic::Ordering::SeqCst);
        let released = self.snapshot.release.notified();
        if self
            .snapshot
            .delayed
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            self.snapshot.started.notify_one();
            released.await;
        }
        preparing
    }

    #[zbus(property)]
    fn block_inhibited(&self) -> &str {
        ""
    }

    #[zbus(property, name = "InhibitDelayMaxUSec")]
    fn inhibit_delay(&self) -> zbus::fdo::Result<u64> {
        if self.bad_delay {
            Err(zbus::fdo::Error::Failed(
                "optional delay unavailable".into(),
            ))
        } else {
            Ok(8_000_000)
        }
    }
}

struct TestSession {
    mode: String,
}

#[zbus::interface(name = "org.freedesktop.login1.Session")]
impl TestSession {
    #[zbus(property)]
    fn active(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn class(&self) -> &str {
        "user"
    }

    #[zbus(property, name = "Type")]
    fn kind(&self) -> &str {
        "wayland"
    }

    #[zbus(property)]
    async fn locked_hint(&self) -> zbus::fdo::Result<bool> {
        match self.mode.as_str() {
            "error" => Err(zbus::fdo::Error::Failed("LockedHint unavailable".into())),
            "timeout" => std::future::pending().await,
            _ => Ok(true),
        }
    }
}

#[test]
fn mandatory_state_and_observation_gaps_use_an_isolated_bus() {
    if let Ok(mode) = std::env::var("GLIMPSE_LOCK_LOGIND_CASE") {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
            .block_on(isolated_case(&mode));
        return;
    }
    for mode in [
        "error",
        "timeout",
        "delay-error",
        "good",
        "missed",
        "newer",
        "owner",
        "ended",
    ] {
        let bus = glimpse_dbus::testing::PrivateBus::start();
        let status = std::process::Command::new("timeout")
            .args(["--kill-after=10s", "120s"])
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "logind::tests::mandatory_state_and_observation_gaps_use_an_isolated_bus",
                "--nocapture",
            ])
            .env("DBUS_SYSTEM_BUS_ADDRESS", bus.address())
            .env("GLIMPSE_LOCK_LOGIND_CASE", mode)
            .status()
            .unwrap();
        assert!(status.success(), "isolated login1 case {mode}: {status}");
    }
}

async fn isolated_case(mode: &str) {
    let server = zbus::Connection::system().await.unwrap();
    let snapshot = std::sync::Arc::new(TestSnapshot::default());
    server
        .object_server()
        .at(
            "/org/freedesktop/login1",
            TestManager {
                bad_delay: mode == "delay-error",
                snapshot: snapshot.clone(),
            },
        )
        .await
        .unwrap();
    server
        .object_server()
        .at(
            "/org/freedesktop/login1/session/qa",
            TestSession { mode: mode.into() },
        )
        .await
        .unwrap();
    glimpse_dbus::own_name(&server, "org.freedesktop.login1")
        .await
        .unwrap();
    let result = connect(true).await;
    if matches!(mode, "error" | "timeout") {
        assert!(
            result.is_err(),
            "{mode} must not become an unlocked initial state"
        );
        return;
    }
    let (mut logind, ready) = result.unwrap();
    assert_eq!(ready.initial_hint, Some(true));
    assert!(!ready.preparing_for_sleep);
    assert_eq!(
        ready.sleep_wait,
        if mode == "delay-error" {
            Duration::from_millis(4_500)
        } else {
            Duration::from_millis(7_500)
        }
    );
    if !matches!(mode, "missed" | "newer" | "owner" | "ended") {
        return;
    }
    let (requests, mut receiver) = mpsc::unbounded_channel();
    let (events, mut observed) = mpsc::unbounded_channel();
    let (sleep_sender, sleep_receiver) = mpsc::unbounded_channel();
    if mode == "ended" {
        logind.owners = futures_util::stream::empty::<()>().boxed();
    } else if mode != "owner" {
        assert!(
            !logind
                .manager
                .inner()
                .get_property::<bool>("PreparingForSleep")
                .await
                .unwrap()
        );
        snapshot
            .preparing
            .store(mode == "missed", std::sync::atomic::Ordering::SeqCst);
        snapshot
            .delayed
            .store(true, std::sync::atomic::Ordering::SeqCst);
        logind.sleeps = futures_util::stream::unfold(sleep_receiver, |mut receiver| async {
            receiver.recv().await.map(|delivery| (delivery, receiver))
        })
        .boxed();
        sleep_sender.send(Delivery::Missed).unwrap();
    } else {
        server.release_name("org.freedesktop.login1").await.unwrap();
    }
    let mut following = std::pin::pin!(logind.follow(&mut receiver, move |event| {
        if let Event::Lifecycle(input) = event {
            let _ = events.send(input);
        }
    }));
    tokio::time::timeout(Duration::from_secs(3), async {
        if matches!(mode, "owner" | "ended") {
            let outcome = (&mut following).await;
            if mode == "owner" {
                assert!(matches!(outcome, super::Ended::OwnerChanged));
            } else {
                assert!(matches!(outcome, super::Ended::Lost));
            }
            assert!(matches!(observed.recv().await, Some(Input::BackendLost)));
        } else {
            tokio::select! {
                _ = &mut following => panic!("a missed observation must not reconnect"),
                () = async {
                    let mut machine = crate::lifecycle::Lifecycle::new(crate::lifecycle::Settings {
                        standalone: false,
                        refusal: None,
                    });
                    machine.handle(Input::BackendReady {
                        initial_hint: Some(false),
                        preparing_for_sleep: false,
                    });
                    machine.handle(Input::LockRequested);
                    machine.handle(Input::Locked(1));
                    machine.handle(Input::Painted(1));
                    machine.handle(Input::Submit);
                    let lost = observed.recv().await.unwrap();
                    assert!(matches!(lost, Input::BackendLost));
                    machine.handle(lost);
                    tokio::select! {
                        () = snapshot.started.notified() => {}
                        event = observed.recv() => panic!("re-enabled without an uncached snapshot: {event:?}"),
                    }
                    snapshot
                        .preparing
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                    sleep_sender.send(Delivery::Item(mode == "newer")).unwrap();
                    let stale = observed.recv().await.unwrap();
                    assert!(matches!(stale, Input::PrepareForSleep(preparing) if preparing == (mode == "newer")));
                    machine.handle(stale);
                    assert_eq!(
                        machine.handle(Input::Submit),
                        vec![crate::lifecycle::Effect::DiscardSubmit]
                    );
                    snapshot
                        .delayed
                        .store(false, std::sync::atomic::Ordering::SeqCst);
                    snapshot.release.notify_waiters();
                    let ready = observed.recv().await.unwrap();
                    assert!(matches!(ready, Input::BackendReady {
                        initial_hint: None,
                        preparing_for_sleep: true,
                    }));
                    machine.handle(ready);
                    assert_eq!(
                        machine.handle(Input::Submit),
                        vec![crate::lifecycle::Effect::DiscardSubmit]
                    );
                    assert!(
                        machine.handle(Input::AuthFinished {
                            attempt: 1,
                            verdict: crate::auth::Verdict::Unlock { password_expired: false },
                        }).is_empty()
                    );
                } => {}
            }
        }
    })
    .await
    .unwrap();
    assert!(requests.send(Request::TakeInhibitor).is_ok());
}
