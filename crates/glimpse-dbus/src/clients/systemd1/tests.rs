use zbus::zvariant::{Type, Value};

fn signature<T: Type + ?Sized>() -> String {
    T::SIGNATURE.to_string()
}

#[test]
fn manager_and_scope_signatures_match_introspection() {
    let name = signature::<&str>();
    let properties = signature::<&[(&str, Value<'_>)]>();
    let aux = signature::<&[(&str, &[(&str, Value<'_>)])]>();
    let states = signature::<&[&str]>();
    assert_eq!(format!("{name}{name}{properties}{aux}"), "ssa(sv)a(sa(sv))");
    assert_eq!(signature::<zbus::zvariant::OwnedObjectPath>(), "o");
    assert_eq!(format!("{states}{states}"), "asas");
    assert_eq!(
        signature::<Vec<super::Systemd1UnitEntry>>(),
        "a(ssssssouso)"
    );
    assert_eq!(signature::<Vec<(String, u32, String)>>(), "a(sus)");
    assert_eq!(format!("{name}{name}{}", signature::<i32>()), "ssi");
    assert_eq!(signature::<u64>(), "t");
    assert_eq!(
        format!(
            "{}{}{name}{name}",
            signature::<u32>(),
            signature::<zbus::zvariant::OwnedObjectPath>()
        ),
        "uoss"
    );
}

#[test]
fn transient_service_and_signal_signatures_match_introspection() {
    use zbus::zvariant::{Fd, OwnedObjectPath};
    assert_eq!(
        format!(
            "{}{}{}{}",
            signature::<u32>(),
            signature::<OwnedObjectPath>(),
            signature::<String>(),
            signature::<String>()
        ),
        "uoss"
    );
    assert_eq!(
        format!(
            "{}{}",
            signature::<String>(),
            signature::<OwnedObjectPath>()
        ),
        "so"
    );
    assert_eq!(signature::<Vec<(&str, Vec<&str>, bool)>>(), "a(sasb)");
    assert_eq!(signature::<Fd<'_>>(), "h");
    assert_eq!(signature::<Vec<&str>>(), "as");
}

mod live {
    use super::super::{Systemd1ManagerProxy, Systemd1ServiceProxy, Systemd1UnitProxy};
    use futures_util::StreamExt;
    use std::os::fd::OwnedFd;
    use std::time::{Duration, Instant};
    use zbus::zvariant::{Fd, OwnedObjectPath, Value};

    async fn start(
        manager: &Systemd1ManagerProxy<'_>,
        name: &str,
        stderr: &OwnedFd,
    ) -> zbus::Result<OwnedObjectPath> {
        let exec = vec![(
            "/usr/bin/sleep".to_owned(),
            vec!["sleep".to_owned(), "30".to_owned()],
            false,
        )];
        let props: Vec<(&str, Value<'_>)> = vec![
            ("Description", Value::from("glimpse spike")),
            ("Type", Value::from("exec")),
            ("ExecStart", Value::from(exec)),
            ("CollectMode", Value::from("inactive-or-failed")),
            ("Restart", Value::from("on-failure")),
            ("RestartUSec", Value::from(1_000_000u64)),
            ("RestartSteps", Value::from(6u32)),
            ("RestartMaxDelayUSec", Value::from(60_000_000u64)),
            ("StartLimitIntervalUSec", Value::from(0u64)),
            ("MemoryMax", Value::from(268_435_456u64)),
            ("Environment", Value::from(vec!["FOO=1"])),
            ("StandardErrorFileDescriptor", Value::from(Fd::from(stderr))),
        ];
        manager
            .start_transient_unit(name, "fail", &props, &[])
            .await
    }

    async fn run_unit(manager: &Systemd1ManagerProxy<'_>, name: &str, restart: bool) {
        let (_read, write) = std::io::pipe().unwrap();
        let write: OwnedFd = write.into();
        let mut jobs = manager.receive_job_removed().await.unwrap();
        let begun = Instant::now();
        let path = start(manager, name, &write).await.unwrap();
        println!(
            "{name}: StartTransientUnit returned {path} in {:?}",
            begun.elapsed()
        );
        let result = tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(signal) = jobs.next().await {
                let args = signal.args().unwrap();
                if args.unit == name {
                    return args.result;
                }
            }
            String::new()
        })
        .await
        .unwrap();
        println!(
            "{name}: JobRemoved result={result} after {:?}",
            begun.elapsed()
        );
        assert_eq!(result, "done");
        let unit_path = manager.get_unit(name).await.unwrap();
        let unit = Systemd1UnitProxy::builder(manager.inner().connection())
            .path(unit_path.clone())
            .unwrap()
            .build()
            .await
            .unwrap();
        let service = Systemd1ServiceProxy::builder(manager.inner().connection())
            .path(unit_path)
            .unwrap()
            .build()
            .await
            .unwrap();
        assert_eq!(unit.active_state().await.unwrap(), "active");
        assert_eq!(service.n_restarts().await.unwrap(), 0);
        println!(
            "{name}: active, MainPID={} Result={} MemoryCurrent={} ActiveEnterTimestamp={}",
            service.main_pid().await.unwrap(),
            service.result().await.unwrap(),
            service.memory_current().await.unwrap(),
            unit.active_enter_timestamp().await.unwrap()
        );
        if restart {
            manager.kill_unit(name, "all", 9).await.unwrap();
            let killed = Instant::now();
            while service.n_restarts().await.unwrap() != 1 {
                assert!(killed.elapsed() < Duration::from_secs(5), "no restart");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            println!("{name}: NRestarts=1 after {:?}", killed.elapsed());
        }
    }

    async fn stop(manager: &Systemd1ManagerProxy<'_>, name: &str) {
        let mut removed = manager.receive_unit_removed().await.unwrap();
        let begun = Instant::now();
        let _ = manager.stop_unit(name, "replace").await;
        let seen = tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(signal) = removed.next().await {
                if signal.args().unwrap().id == name {
                    return true;
                }
            }
            false
        })
        .await;
        println!(
            "{name}: UnitRemoved seen={seen:?} after {:?}",
            begun.elapsed()
        );
        let _ = manager.reset_failed_unit(name).await;
    }

    #[tokio::test]
    #[ignore]
    async fn session_bus_manager_runs_transient_units() {
        let connection = zbus::Connection::session().await.unwrap();
        let manager = Systemd1ManagerProxy::builder(&connection)
            .build()
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), manager.subscribe())
            .await
            .expect("Subscribe did not return")
            .unwrap();
        let pid = std::process::id();
        let first = format!("app-glimpse-spike-{pid}-1.service");
        let second = format!("app-glimpse-spike-{pid}-2.service");
        let task = {
            let manager = manager.clone();
            let (first, second) = (first.clone(), second.clone());
            tokio::spawn(async move {
                run_unit(&manager, &first, true).await;
                stop(&manager, &first).await;
                run_unit(&manager, &second, false).await;
            })
        };
        let result = task.await;
        let _ = manager.stop_unit(&first, "replace").await;
        let _ = manager.stop_unit(&second, "replace").await;
        result.unwrap();
    }
}
