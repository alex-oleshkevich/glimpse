use super::*;

#[tokio::test]
async fn requests_are_served_without_a_gesture() {
    let (_dir, entry, _output) = script(
        "import time\nsend('{\"t\":\"hello\",\"v\":1}')\nr.readline()\nsend('{\"t\":\"copy\",\"text\":\"copied\"}')\nsend('{\"t\":\"open-uri\",\"uri\":\"javascript:x\"}')\nsend('{\"t\":\"open-uri\",\"uri\":\"https://example.com/open\"}')\nsend('{\"t\":\"launch\",\"id\":\"org.example.App\"}')\ntime.sleep(30)\n",
    );
    let ((handle, _sender, cancel, task, selection, _launcher), _config) =
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
    assert_eq!(selection.offered().len(), 1);
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[test]
fn note_uses_the_entry_name_and_icon() {
    let request = FromApplet::Notify {
        summary: "Done".to_owned(),
        body: "Saved".to_owned(),
        icon: None,
        urgency: Urgency::Normal,
    };
    let entry = Entry {
        argv: Vec::new(),
        cwd: None,
        name: "Pomodoro".to_owned(),
        icon: Some("timer-symbolic".to_owned()),
        path: PathBuf::new(),
        exec: String::new(),
    };
    let note = note(&entry, "me.example.Pomodoro", &request).expect("note");
    assert_eq!(note.name, "Pomodoro");
    assert_eq!(note.id, "me.example.Pomodoro");
    assert_eq!(note.icon, "timer-symbolic");
    assert_eq!(note.summary, "Done");
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
        let (handle, _sender, cancel, task, _selection, launcher) =
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
    let (_dir, entry, _output) =
        script("import time\nsend('{\"t\":\"hello\",\"v\":1}')\ntime.sleep(30)\n");
    let ((handle, _sender, cancel, task, _selection, launcher), _config) =
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
async fn a_second_hello_resets_the_tree_and_bumps_generation_without_respawn() {
    let (_dir, entry, output) = script(
        "import json, time\nopen(sys.argv[1] + '.pid', 'w').write(str(os.getpid()))\nsend('{\"t\":\"hello\",\"v\":1}')\nr.readline()\nsend(json.dumps({'t':'commit','ops':[{'op':'insert','parent':0,'node':{'id':1,'type':'indicator'}}]}))\ntime.sleep(0.05)\nsend('{\"t\":\"hello\",\"v\":1}')\nr.readline()\ntime.sleep(30)\n",
    );
    let ((handle, _sender, cancel, task, _selection, _launcher), _config) =
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
    let ((handle, _sender, cancel, task, _selection, _launcher), _config) =
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
async fn a_sixth_hello_within_ten_seconds_kills_the_unit() {
    let (_dir, entry, _output) = script(
        "import time\nfor _ in range(6):\n    send('{\"t\":\"hello\",\"v\":1}')\n    time.sleep(0.05)\ntime.sleep(30)\n",
    );
    let ((handle, _sender, cancel, task, _selection, launcher), _config) =
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
    let ((handle, _sender, cancel, task, _selection, _launcher), _config) =
        running(catalog.clone());
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
