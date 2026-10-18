use super::*;

#[tokio::test]
async fn requests_are_served_without_a_gesture() {
    let (_dir, entry, _output) = script(
        "import time\nsend('{\"t\":\"hello\",\"v\":1}')\nr.readline()\nsend('{\"t\":\"copy\",\"text\":\"copied\"}')\nsend('{\"t\":\"open-uri\",\"uri\":\"javascript:x\"}')\nsend('{\"t\":\"open-uri\",\"uri\":\"https://example.com/open\"}')\nsend('{\"t\":\"launch\",\"id\":\"org.example.App\"}')\ntime.sleep(30)\n",
    );
    let ((handle, _sender, cancel, task, _launcher), _config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    let accepted = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let state = handle.snapshot();
            if let Some(slot) = state.slots.get(&1)
                && slot.requests.len() == 2
            {
                return slot.clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("requests");
    assert_eq!(
        accepted.requests[0].1,
        BarRequest::OpenUri("https://example.com/open".to_owned())
    );
    assert_eq!(
        accepted.requests[1].1,
        BarRequest::Launch("org.example.App".to_owned())
    );
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[test]
fn only_web_mail_and_file_uris_are_allowed() {
    assert!(allowed_uri("https://example.com"));
    assert!(allowed_uri("MAILTO:me@example.com"));
    assert!(allowed_uri("file:///home/user/notes.txt"));
    assert!(!allowed_uri("javascript:alert(1)"));
    assert!(!allowed_uri(&format!("https://{}", "a".repeat(2048))));
}

#[tokio::test]
async fn rejects_bad_lines_trees_and_flood_without_stopping_sibling() {
    let bad_body = "import json, time\nmode = sys.argv[2]\nsend('{\"t\":\"hello\",\"v\":1}')\nr.readline()\nif mode == 'garbage':\n    send('garbage')\nelif mode == 'huge':\n    send('x' * (2 << 20))\nelif mode == 'many':\n    nodes = [{'id': i, 'type': 'box', 'children': [{'id': i * 200 + j, 'type': 'label'} for j in range(1, 201)]} for i in range(2, 18)]\n    send(json.dumps({'t': 'commit', 'ops': [{'op': 'insert', 'parent': 0, 'node': {'id': 1, 'type': 'popover', 'children': nodes}}]}))\nelif mode == 'cycle':\n    send(json.dumps({'t': 'commit', 'ops': [{'op': 'insert', 'parent': 0, 'node': {'id': 1, 'type': 'popover', 'children': [{'id': 2, 'type': 'box', 'children': [{'id': 3, 'type': 'box'}]}]}}]}))\n    send(json.dumps({'t': 'commit', 'ops': [{'op': 'move', 'parent': 3, 'id': 2}]}))\ntime.sleep(30)\n";
    let good_body = "import json, time\nsend('{\"t\":\"hello\",\"v\":1}')\nr.readline()\nsend(json.dumps({'t': 'commit', 'ops': [{'op': 'insert', 'parent': 0, 'node': {'id': 1, 'type': 'indicator', 'props': {'text': 'alive'}}}]}))\ntime.sleep(30)\n";
    for mode in ["garbage", "huge", "many", "cycle"] {
        let (_bad_dir, mut bad, _bad_output) = script(bad_body);
        bad.argv.push(mode.to_owned());
        let (_good_dir, good, _good_output) = script(good_body);
        let mut test = configured();
        test.config.applets.insert(
            "other".to_owned(),
            glimpse_config::ExecAppletConfig {
                applet: "me.example.Other".to_owned(),
                options: Map::new(),
                env: BTreeMap::new(),
            },
        );
        let (handle, _sender, cancel, task, launcher) =
            running_with(test, Arc::new(PairCatalog { bad, good }), None);
        handle
            .attach(1, "test", placement())
            .await
            .expect("bad attach");
        handle
            .attach(2, "other", placement())
            .await
            .expect("good attach");
        status(&handle, 1, |s| matches!(s, Status::Restarting { .. })).await;
        assert!(
            launcher.kill_count() >= 1,
            "{mode}: misbehaving unit killed"
        );
        let sibling = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(state) = handle.snapshot().slots.get(&2).filter(|s| {
                    matches!(s.status, Status::Running) && s.tree.children(ROOT).len() == 1
                }) {
                    return state.clone();
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("sibling update");
        assert_eq!(sibling.tree.children(ROOT), &[1], "{mode}");
        handle.detach(1).await.expect("bad detach");
        handle.detach(2).await.expect("good detach");
        cancel.cancel();
        let _ = task.await;
    }
}

#[tokio::test]
async fn full_queue_kills_child() {
    let (_dir, entry, _output) = script(
        "send('{\"t\":\"hello\",\"v\":1}')\nr.readline()\nwhile True:\n    send('{\"t\":\"commit\",\"ops\":[]}')\n",
    );
    let ((handle, _sender, cancel, task, launcher), _config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    let state = status(&handle, 1, |s| matches!(s, Status::Running)).await;
    for _ in 0..200 {
        handle
            .send_event(UserEvent {
                slot: 1,
                generation: state.generation,
                id: 1,
                name: "changed".to_owned(),
                args: vec![Value::String("x".repeat(65536))],
                seq: None,
            })
            .await
            .expect("event");
    }
    status(&handle, 1, |s| matches!(s, Status::Restarting { .. })).await;
    assert!(launcher.kill_count() >= 1);
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn connection_cancellation_precedes_buffered_commits() {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let (_dir, entry, _output) =
        raw_script("#!/usr/bin/env python3\nimport time\ntime.sleep(30)\n");
    let test = configured();
    let socket = test.config.socket_path(1);
    let launcher = Arc::new(ProcessLauncher::new());
    let source = spawn::source(
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
        spawn::SourceArgs {
            slot: 1,
            spawn: 1,
            id: "me.example.Test".to_owned(),
            env: BTreeMap::new(),
            state_base: test.config.state_base.clone(),
            entry: None,
            cwd: None,
            restart: false,
            stderr: None,
            unit: launcher::unit_name(&test.config.panel, "me.example.Test", 1, 1),
            socket: socket.clone(),
            launcher: Arc::clone(&launcher) as Arc<dyn Launcher>,
            stop: CancellationToken::new(),
        },
    )
    .await;
    futures_util::pin_mut!(source);
    assert!(matches!(source.next().await, Some(Event::Started { .. })));
    let mut client = tokio::net::UnixStream::connect(socket)
        .await
        .expect("connect");
    client
        .write_all(b"{\"t\":\"hello\",\"v\":1}\n{\"t\":\"commit\",\"ops\":[]}\n")
        .await
        .expect("buffered messages");
    let Some(Event::Connected { link, .. }) = source.next().await else {
        panic!("connection accepted");
    };
    assert!(matches!(source.next().await, Some(Event::Hello { .. })));
    link.queue_full
        .store(true, std::sync::atomic::Ordering::Relaxed);
    link.stop.cancel();
    assert!(matches!(
        source.next().await,
        Some(Event::Misbehaved { reason, .. }) if reason == "not reading its socket"
    ));
    assert_eq!(launcher.kill_count(), 1);
}

#[tokio::test]
async fn a_second_hello_resets_the_tree_and_bumps_generation_without_respawn() {
    let (_dir, entry, output) = script(
        "import json, time\nopen(sys.argv[1] + '.pid', 'w').write(str(os.getpid()))\nsend('{\"t\":\"hello\",\"v\":1}')\nr.readline()\nsend(json.dumps({'t':'commit','ops':[{'op':'insert','parent':0,'node':{'id':1,'type':'indicator'}}]}))\ntime.sleep(0.05)\nsend('{\"t\":\"hello\",\"v\":1}')\nr.readline()\ntime.sleep(30)\n",
    );
    let ((handle, _sender, cancel, task, _launcher), _config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    let first = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(state) = handle
                .snapshot()
                .slots
                .get(&1)
                .filter(|s| s.tree.children(ROOT).len() == 1)
            {
                return state.clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("first tree");
    let pid = tokio::fs::read_to_string(output.with_extension("pid"))
        .await
        .expect("pid");
    let second = next_generation(&handle, 1, first.generation).await;
    assert!(second.tree.children(ROOT).is_empty());
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
async fn an_unknown_message_and_a_flood_do_not_stop_the_child() {
    let (_dir, entry, _output) = script(
        "import json, time\nsend('{\"t\":\"hello\",\"v\":1}')\nr.readline()\nsend('{\"t\":\"dance\"}')\nfor _ in range(300):\n    send('{\"t\":\"commit\",\"ops\":[]}')\nsend(json.dumps({'t':'commit','ops':[{'op':'insert','parent':0,'node':{'id':1,'type':'indicator'}}]}))\ntime.sleep(30)\n",
    );
    let ((handle, _sender, cancel, task, _launcher), _config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    let state = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(state) = handle
                .snapshot()
                .slots
                .get(&1)
                .filter(|s| s.tree.children(ROOT).len() == 1)
            {
                return state.clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("tree after the flood");
    assert_eq!(state.status, Status::Running);
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn removed_service_requests_do_not_stop_the_child() {
    let (_dir, entry, _output) = script(
        "import json, time\nsend('{\"t\":\"hello\",\"v\":1}')\nr.readline()\nsend('{\"t\":\"notify\",\"summary\":\"x\"}')\nsend('{\"t\":\"copy\",\"text\":\"x\"}')\nsend('{\"t\":\"mpris\",\"player\":\"p\",\"op\":\"control\",\"action\":\"play\"}')\nsend('{\"t\":\"compositor\",\"op\":\"close-window\",\"id\":1}')\nsend(json.dumps({'t':'commit','ops':[{'op':'insert','parent':0,'node':{'id':1,'type':'indicator'}}]}))\ntime.sleep(30)\n",
    );
    let ((handle, _sender, cancel, task, launcher), _config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    let state = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(state) = handle
                .snapshot()
                .slots
                .get(&1)
                .filter(|s| s.tree.children(ROOT) == [1])
            {
                return state.clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("commit after removed requests");
    assert_eq!(state.status, Status::Running);
    assert_eq!(launcher.kill_count(), 0);
    assert_eq!(launcher.started().len(), 1);
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn a_sixth_hello_within_ten_seconds_kills_the_unit() {
    let (_dir, entry, _output) = script(
        "import time\nfor _ in range(6):\n    send('{\"t\":\"hello\",\"v\":1}')\n    time.sleep(0.05)\ntime.sleep(30)\n",
    );
    let ((handle, _sender, cancel, task, launcher), _config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Restarting { .. })).await;
    assert!(launcher.kill_count() >= 1);
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn a_vanished_entry_stops_the_running_child() {
    let (_dir, entry, output) = script(&format!(
        "open(sys.argv[1], 'w').write(str(os.getpid()))\n{LOOP}"
    ));
    let catalog = FakeCatalog(Arc::new(Mutex::new(Ok(entry))));
    let ((handle, _sender, cancel, task, _launcher), _config) = running(catalog.clone());
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    let pid = file_lines(&output, 1).await[0].clone();
    *catalog.0.lock().expect("catalog") = Err("not installed".to_owned());
    let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
    assert_eq!(failed.status, Status::Failed("not installed".to_owned()));
    pid_gone(&pid).await;
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}
