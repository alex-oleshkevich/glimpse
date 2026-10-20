use super::*;

const PID_LOOP: &str = "open(sys.argv[1], 'a').write(str(os.getpid()) + '\\n')\nsend('{\"t\":\"hello\",\"v\":1}')\nfor line in r:\n    open(sys.argv[1] + '.in', 'a').write(line)\n";

#[tokio::test]
async fn dev_tree_requires_a_taken_key_and_tracks_restart() {
    let (_dir, entry, _output) = script(
        "import json\nsend('{\"t\":\"hello\",\"v\":1}')\nr.readline()\nsend(json.dumps({'t':'commit','ops':[{'op':'insert','parent':0,'node':{'id':1,'type':'indicator','props':{'tooltip':str(os.getpid())}}}]}))\nfor line in r:\n    pass\n",
    );
    let ((handle, _sender, cancel, task, _launcher), _config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry.clone())))));
    let key = handle
        .attach_dev("test", dev_request(dev_entry(&entry)))
        .await
        .unwrap();
    assert!(handle.dev_tree(&key).await.is_err());
    assert!(handle.dev_tree("test").await.is_err());
    handle.attach(1, "test", placement()).await.unwrap();
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    let mut states = handle.subscribe();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if states.borrow().slots[&1].tree.node(1).is_some() {
                break;
            }
            states.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    let first = handle.dev_tree(&key).await.unwrap();
    assert_eq!(first, handle.snapshot().slots[&1].tree.snapshot());
    assert!(first["children"][0]["props"]["tooltip"].as_str().is_some());
    handle.restart_dev(&key).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let current = handle.snapshot().slots[&1].tree.snapshot();
            if let Some(pid) = current["children"][0]["props"]["tooltip"].as_str()
                && Some(pid) != first["children"][0]["props"]["tooltip"].as_str()
            {
                break;
            }
            states.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    let restarted = handle.dev_tree(&key).await.unwrap();
    assert!(
        restarted["children"][0]["props"]["tooltip"]
            .as_str()
            .is_some()
    );
    assert_ne!(
        restarted["children"][0]["props"]["tooltip"],
        first["children"][0]["props"]["tooltip"]
    );
    handle.detach_dev(&key).await.unwrap();
    assert!(handle.dev_tree(&key).await.is_err());
    handle.detach(1).await.unwrap();
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn dev_takes_over_and_detach_returns_the_configured_applet() {
    let (_dir, entry, output) = script(PID_LOOP);
    let (_dev_dir, dev, dev_output) = script(PID_LOOP);
    let ((handle, _sender, cancel, task, _launcher), _config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    let configured_pid = file_lines(&output, 1).await[0].clone();

    let dev_handle = handle
        .attach_dev("test", dev_request(dev_entry(&dev)))
        .await
        .expect("dev attach");
    assert_eq!(dev_handle, "me.example.Test");
    let dev_state = status(&handle, 1, |s| matches!(s, Status::Running)).await;
    assert_eq!(dev_state.title, "Dev applet");
    assert!(handle.snapshot().dev.contains("me.example.Test"));
    let dev_pid = file_lines(&dev_output, 1).await[0].clone();
    assert_ne!(dev_pid, configured_pid);
    pid_gone(&configured_pid).await;

    handle.detach_dev(&dev_handle).await.expect("dev detach");
    assert!(!handle.snapshot().dev.contains("me.example.Test"));
    pid_gone(&dev_pid).await;
    let pids = file_lines(&output, 2).await;
    assert_ne!(pids[1], configured_pid);
    let back = status(&handle, 1, |s| matches!(s, Status::Running)).await;
    assert_eq!(back.title, "Test applet");
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn dev_restart_after_a_crash_starts_it_again() {
    let (_dir, entry, _output) = script(LOOP);
    let (_dev_dir, dev, dev_output) = script(
        "marker = sys.argv[1] + '.marker'\nopen(sys.argv[1], 'a').write(str(os.getpid()) + '\\n')\nsend('{\"t\":\"hello\",\"v\":1}')\nr.readline()\nif not os.path.exists(marker):\n    open(marker, 'w').close()\n    sys.exit(1)\nfor line in r:\n    pass\n",
    );
    let ((handle, _sender, cancel, task, _launcher), _config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    let dev_handle = handle
        .attach_dev("test", dev_request(dev_entry(&dev)))
        .await
        .expect("dev attach");
    let first = status(&handle, 1, |s| matches!(s, Status::Running)).await;
    status(&handle, 1, |s| {
        matches!(s, Status::Ended | Status::Failed(_))
    })
    .await;
    handle
        .restart_dev(&dev_handle)
        .await
        .expect("dev restart after crash");
    let again = next_generation(&handle, 1, first.generation).await;
    assert_eq!(again.title, "Dev applet");
    let pids = file_lines(&dev_output, 2).await;
    assert_ne!(pids[0], pids[1]);
    handle.detach_dev(&dev_handle).await.expect("dev detach");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn dev_reconnects_after_eof_and_unit_exit_is_terminal() {
    for fail in [false, true] {
        let (_dir, entry, _output) = script(LOOP);
        let (_dev_dir, dev, output) = script(
            "import json, time\nopen(sys.argv[1], 'w').write(str(os.getpid()))\nsend('{\"t\":\"hello\",\"v\":1}')\nfor line in r:\n    if json.loads(line).get('t') == 'event':\n        break\nr.close()\n_sock.close()\nwhile not os.path.exists(sys.argv[1] + '.resume'):\n    time.sleep(0.01)\n_sock = socket.socket(socket.AF_UNIX)\n_sock.connect(os.environ['GLIMPSE_APPLET_SOCKET'])\nr = _sock.makefile('r', encoding='utf-8')\nsend('{\"t\":\"hello\",\"v\":1}')\nfor line in r:\n    pass\n",
        );
        let ((handle, _sender, cancel, task, launcher), _config) =
            running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
        let dev_handle = handle
            .attach_dev("test", dev_request(dev_entry(&dev)))
            .await
            .expect("dev attach");
        handle.attach(1, "test", placement()).await.expect("attach");
        let first = status(&handle, 1, |s| matches!(s, Status::Running)).await;
        let pid = file_contains(&output, "").await;
        let units = launcher.started();
        handle
            .send_event(UserEvent {
                slot: 1,
                generation: first.generation,
                id: 1,
                name: "reconnect".to_owned(),
                args: Vec::new(),
                seq: None,
            })
            .await
            .expect("disconnect event");
        status(&handle, 1, |s| matches!(s, Status::Restarting { .. })).await;
        tokio::fs::write(output.with_extension("resume"), "")
            .await
            .expect("allow reconnect");
        let resumed = next_generation(&handle, 1, first.generation).await;
        assert_eq!(resumed.title, "Dev applet");
        assert_eq!(launcher.started(), units);
        if fail {
            launcher.kill(&units[0]).await;
            status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
        } else {
            launcher.stop(&units[0]).await;
            status(&handle, 1, |s| matches!(s, Status::Ended)).await;
        }
        pid_gone(&pid).await;
        assert_eq!(launcher.started(), units);
        handle.detach_dev(&dev_handle).await.expect("dev detach");
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }
}

#[tokio::test]
async fn dev_pending_attaches_when_its_widget_arrives() {
    let (_dir, entry, _output) = script(LOOP);
    let (_dev_dir, dev, dev_output) = script(PID_LOOP);
    let ((handle, _sender, cancel, task, _launcher), _config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    let dev_handle = handle
        .attach_dev("test", dev_request(dev_entry(&dev)))
        .await
        .expect("dev attach");
    assert!(handle.snapshot().dev.contains("me.example.Test"));
    handle.attach(1, "test", placement()).await.expect("attach");
    let state = status(&handle, 1, |s| matches!(s, Status::Running)).await;
    assert_eq!(state.title, "Dev applet");
    file_lines(&dev_output, 1).await;
    let second = handle
        .attach_dev("test", dev_request(dev_entry(&dev)))
        .await;
    assert!(second.is_err(), "a second dev attach for one id is refused");
    handle.detach_dev(&dev_handle).await.expect("dev detach");
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn dev_set_options_reach_the_running_slot() {
    let (_dir, entry, _output) = script(LOOP);
    let (_dev_dir, dev, dev_output) = script(PID_LOOP);
    let ((handle, _sender, cancel, task, _launcher), _config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    let dev_handle = handle
        .attach_dev("test", dev_request(dev_entry(&dev)))
        .await
        .expect("dev attach");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    handle
        .dev_set_options(
            &dev_handle,
            Map::from_iter([("color".to_owned(), Value::from("green"))]),
        )
        .await
        .expect("dev options");
    file_contains(&dev_output.with_extension("in"), "\"t\":\"options\"").await;
    handle.detach_dev(&dev_handle).await.expect("dev detach");
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

struct CapturingLauncher(Arc<Mutex<Vec<UnitSpec>>>);

impl Launcher for CapturingLauncher {
    fn start(&self, spec: UnitSpec) -> BoxFuture<'static, Result<(), String>> {
        self.0.lock().expect("captured").push(spec);
        Box::pin(async { Ok(()) })
    }
    fn stop(&self, _name: &str) -> BoxFuture<'static, ()> {
        Box::pin(async {})
    }
    fn kill(&self, _name: &str) -> BoxFuture<'static, ()> {
        Box::pin(async {})
    }
    fn reset_failed(&self, _name: &str) -> BoxFuture<'static, ()> {
        Box::pin(async {})
    }
    fn watch(&self, _name: &str) -> BoxStream<'static, UnitState> {
        Box::pin(futures_util::stream::pending())
    }
    fn sweep(&self, _prefix: &str) -> BoxFuture<'static, ()> {
        Box::pin(async {})
    }
}

#[tokio::test]
async fn a_dev_slot_runs_in_the_working_directory_its_entry_names() {
    let (_dir, entry, _output) = script(PID_LOOP);
    let working = tempfile::tempdir().expect("working directory");
    let dev = Entry {
        cwd: Some(working.path().to_path_buf()),
        ..dev_entry(&entry)
    };
    let captured = Arc::new(Mutex::new(Vec::new()));
    let test = configured();
    let (handle, _sender, cancel, task, _launcher) = running_with(
        test,
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
        Some(Arc::new(CapturingLauncher(Arc::clone(&captured)))),
    );
    handle
        .attach_dev("test", dev_request(dev))
        .await
        .expect("dev attach");
    handle.attach(1, "test", placement()).await.expect("attach");
    tokio::time::timeout(Duration::from_secs(5), async {
        while captured.lock().expect("captured").is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the unit starts");
    assert_eq!(
        captured.lock().expect("captured")[0].cwd.as_deref(),
        Some(working.path())
    );
    cancel.cancel();
    let _ = task.await;
}
