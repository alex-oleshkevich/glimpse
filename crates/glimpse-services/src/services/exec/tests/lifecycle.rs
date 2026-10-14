use super::*;

#[tokio::test]
async fn hello_is_answered() {
    let (_dir, entry, output) = script(LOOP);
    let ((handle, _sender, cancel, task, _selection, _launcher), _config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    assert!(matches!(
        status(&handle, 1, |s| matches!(s, Status::Running))
            .await
            .status,
        Status::Running
    ));
    let line = file_contains(&output, "\"t\":\"hello\"").await;
    let reply: Outgoing = serde_json::from_str(line.lines().next().expect("line")).expect("reply");
    assert!(
        matches!(reply, Outgoing::Hello { v: 1, options, placement: p, .. } if options.get("color") == Some(&Value::from("blue")) && p == placement())
    );
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn placement_change_does_not_respawn() {
    let (_dir, entry, output) = script(&format!(
        "open(sys.argv[1] + '.pid', 'w').write(str(os.getpid()))\n{LOOP}"
    ));
    let ((handle, _sender, cancel, task, _selection, _launcher), _config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    file_contains(&output, "\"t\":\"hello\"").await;
    let pid = tokio::fs::read_to_string(output.with_extension("pid"))
        .await
        .expect("pid");
    let mut changed = placement();
    changed.size = 48;
    handle.place(1, changed).await.expect("place");
    file_contains(&output, "\"t\":\"placement\"").await;
    assert_eq!(
        tokio::fs::read_to_string(output.with_extension("pid"))
            .await
            .expect("pid"),
        pid
    );
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn options_change_does_not_respawn() {
    let (_dir, entry, output) = script(&format!(
        "open(sys.argv[1] + '.pid', 'w').write(str(os.getpid()))\n{LOOP}"
    ));
    let ((handle, sender, cancel, task, _selection, _launcher), mut config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    file_contains(&output, "\"t\":\"hello\"").await;
    let pid = tokio::fs::read_to_string(output.with_extension("pid"))
        .await
        .expect("pid");
    config
        .applets
        .get_mut("test")
        .expect("config")
        .options
        .insert("color".to_owned(), Value::from("red"));
    sender.reconfigure(config);
    file_contains(&output, "\"t\":\"options\"").await;
    assert_eq!(
        tokio::fs::read_to_string(output.with_extension("pid"))
            .await
            .expect("pid"),
        pid
    );
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn options_changed_while_starting_follow_hello() {
    let (_dir, entry, output) = script(
        "import time\ntime.sleep(0.3)\nsend('{\"t\":\"hello\",\"v\":1}')\nopen(sys.argv[1], 'w').write(r.readline())\ntime.sleep(5)\n",
    );
    let ((handle, sender, cancel, task, _selection, _launcher), mut config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    config
        .applets
        .get_mut("test")
        .expect("config")
        .options
        .insert("color".to_owned(), Value::from("red"));
    sender.reconfigure(config);
    let content = file_contains(&output, "\"t\":\"hello\"").await;
    assert!(content.contains("\"red\""));
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn removed_instance_stays_failed_without_respawning() {
    let (_dir, entry, output) = script(
        "open(sys.argv[1], 'a').write(str(os.getpid()) + '\\n')\nsend('{\\\"t\\\":\\\"hello\\\",\\\"v\\\":1}')\nfor line in r:\n    pass\n",
    );
    let ((handle, sender, cancel, task, _selection, _launcher), mut config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    let pid = file_lines(&output, 1).await[0].clone();
    config.applets.clear();
    sender.reconfigure(config);
    tokio::time::sleep(Duration::from_secs(6)).await;
    assert!(
        matches!(handle.snapshot().slots.get(&1).map(|s| &s.status), Some(Status::Failed(reason)) if reason == "not configured")
    );
    assert_eq!(
        tokio::fs::read_to_string(&output)
            .await
            .expect("spawns")
            .lines()
            .count(),
        1
    );
    pid_gone(&pid).await;
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn crash_reconnects_and_runs_again() {
    let (_dir, entry, output) = script(
        "import time\nmarker = sys.argv[1] + '.marker'\nopen(sys.argv[1], 'a').write(str(os.getpid()) + '\\n')\nsend('{\"t\":\"hello\",\"v\":1}')\nr.readline()\nif not os.path.exists(marker):\n    open(marker, 'w').close()\n    sys.exit(1)\nfor line in r:\n    pass\n",
    );
    let ((handle, _sender, cancel, task, _selection, _launcher), _config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    let first = status(&handle, 1, |s| matches!(s, Status::Running)).await;
    status(&handle, 1, |s| matches!(s, Status::Restarting { .. })).await;
    let second = next_generation(&handle, 1, first.generation).await;
    assert!(second.generation > first.generation);
    let pids = file_lines(&output, 2).await;
    assert_ne!(pids[0], pids[1]);
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn exits_before_hello_keeps_restarting() {
    let (_dir, entry, output) = raw_script(
        "#!/usr/bin/env python3\nimport os, sys\nopen(sys.argv[1], 'a').write(str(os.getpid()) + '\\n')\nsys.exit(3)\n",
    );
    let catalog = FakeCatalog(Arc::new(Mutex::new(Ok(entry))));
    let ((handle, sender, cancel, task, _selection, launcher), mut config) =
        running(catalog.clone());
    handle.attach(1, "test", placement()).await.expect("attach");
    file_lines(&output, 2).await;
    assert!(
        !matches!(
            handle.snapshot().slots.get(&1).map(|s| &s.status),
            Some(Status::Failed(_))
        ),
        "crash before hello restarts on the curve, never Failed"
    );
    let (_good_dir, good, _good_output) = script(LOOP);
    *catalog.0.lock().expect("catalog") = Ok(good);
    let started_before = launcher.started().len();
    config
        .applets
        .get_mut("test")
        .expect("config")
        .env
        .insert("FIXED".to_owned(), "1".to_owned());
    sender.reconfigure(config);
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    assert!(launcher.started().len() > started_before);
    let names: std::collections::BTreeSet<_> = launcher.started().into_iter().collect();
    assert!(names.len() > 1, "each start gets a fresh unit name");
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn retry_recovers() {
    let (_dir, entry, _output) = script(LOOP);
    let catalog = FakeCatalog(Arc::new(Mutex::new(Err("not installed".to_owned()))));
    let ((handle, _sender, cancel, task, _selection, _launcher), _config) =
        running(catalog.clone());
    handle.attach(1, "test", placement()).await.expect("attach");
    assert!(
        matches!(status(&handle, 1, |s| matches!(s, Status::Failed(_))).await.status, Status::Failed(reason) if reason == "not installed")
    );
    *catalog.0.lock().expect("catalog") = Ok(entry);
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn stale_generation_is_dropped_and_detach_kills_child() {
    let (_dir, entry, output) = script(&format!(
        "open(sys.argv[1] + '.pid', 'w').write(str(os.getpid()))\n{LOOP}"
    ));
    let ((handle, _sender, cancel, task, _selection, _launcher), _config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    let state = status(&handle, 1, |s| matches!(s, Status::Running)).await;
    file_contains(&output, "\"t\":\"hello\"").await;
    handle
        .send_event(UserEvent {
            slot: 1,
            generation: state.generation.saturating_sub(1),
            id: 4,
            name: "press".to_owned(),
            args: Vec::new(),
            seq: None,
        })
        .await
        .expect("send");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !tokio::fs::read_to_string(&output)
            .await
            .expect("messages")
            .contains("\"t\":\"event\"")
    );
    let pid = tokio::fs::read_to_string(output.with_extension("pid"))
        .await
        .expect("pid");
    handle.detach(1).await.expect("detach");
    assert!(!handle.snapshot().slots.contains_key(&1));
    pid_gone(&pid).await;
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn detach_stops_the_unit_and_unlinks_the_socket() {
    let (_dir, entry, output) = script(&format!(
        "open(sys.argv[1] + '.pid', 'w').write(str(os.getpid()))\n{LOOP}"
    ));
    let test = configured();
    let socket = test.config.socket_path(1);
    let (handle, _sender, cancel, task, _selection, _launcher) = running_with(
        test,
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
        None,
    );
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    assert!(socket.exists(), "slot socket is bound before start");
    let pid = tokio::fs::read_to_string(output.with_extension("pid"))
        .await
        .expect("pid");
    handle.detach(1).await.expect("detach");
    pid_gone(&pid).await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while socket.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("socket unlinked");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn missing_systemd_fails_the_slot() {
    let (_dir, entry, _output) = script(LOOP);
    let (handle, _sender, cancel, task, _selection, _launcher) = running_with(
        configured(),
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
        Some(Arc::new(FailingLauncher)),
    );
    handle.attach(1, "test", placement()).await.expect("attach");
    let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
    assert_eq!(
        failed.status,
        Status::Failed("needs the systemd user manager".to_owned())
    );
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn unresolvable_program_fails_the_slot() {
    let entry = Entry {
        argv: vec!["definitely-not-a-program-xyz".to_owned()],
        cwd: None,
        name: "Missing".to_owned(),
        icon: None,
        path: PathBuf::from("/nonexistent/missing.desktop"),
        exec: "definitely-not-a-program-xyz".to_owned(),
    };
    let ((handle, _sender, cancel, task, _selection, _launcher), _config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
    assert_eq!(
        failed.status,
        Status::Failed("definitely-not-a-program-xyz not found".to_owned())
    );
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn start_sweeps_the_panels_units_and_prunes_dead_sockets() {
    let (_dir, entry, _output) = script(LOOP);
    let test = configured();
    let sockets = test.config.sockets_dir().expect("sockets dir");
    std::fs::create_dir_all(&sockets).expect("sockets dir");
    let dead = sockets.join("4194999-1.sock");
    std::fs::write(&dead, "").expect("dead socket");
    let mine = launcher::unit_name(&test.config.panel, "x", 9, 1);
    let other = launcher::unit_name("me.aresa.OtherPanel", "x", 9, 1);
    let process = Arc::new(ProcessLauncher::new());
    for name in [&mine, &other] {
        process
            .start(UnitSpec {
                name: name.clone(),
                description: "sleeper".to_owned(),
                argv: vec!["/bin/sleep".to_owned(), "30".to_owned()],
                cwd: None,
                env: Vec::new(),
                restart: false,
                stderr: None,
            })
            .await
            .expect("start sleeper");
    }
    let (handle, _sender, cancel, task, _selection, _launcher) = running_with(
        test,
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
        Some(Arc::clone(&process) as Arc<dyn Launcher>),
    );
    let mut watch = process.watch(&mine);
    let gone = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(state) = futures_util::StreamExt::next(&mut watch).await {
            if state == UnitState::Gone {
                return true;
            }
        }
        false
    })
    .await
    .expect("sweep finished");
    assert!(gone, "own unit swept");
    assert!(!dead.exists(), "dead socket pruned");
    let mut other_watch = process.watch(&other);
    assert!(
        tokio::time::timeout(
            Duration::from_millis(200),
            futures_util::StreamExt::next(&mut other_watch)
        )
        .await
        .is_err(),
        "another panel's unit untouched"
    );
    process.stop(&other).await;
    drop(handle);
    cancel.cancel();
    let _ = task.await;
}
