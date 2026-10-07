use super::*;

#[tokio::test]
async fn requests_require_a_recent_gesture() {
    let (_dir, entry, _output) = script(
        "#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nread -r line\nprintf '{\"t\":\"copy\",\"text\":\"blocked\"}\\n{\"t\":\"open-uri\",\"uri\":\"https://example.com/blocked\"}\\n'\nwhile read -r line; do printf '{\"t\":\"close-popover\"}\\n{\"t\":\"open-uri\",\"uri\":\"file:///x\"}\\n{\"t\":\"open-uri\",\"uri\":\"https://example.com/open\"}\\n{\"t\":\"copy\",\"text\":\"second\"}\\n'; done\n",
    );
    let (handle, _sender, cancel, task, selection) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    let state = status(&handle, 1, |s| matches!(s, Status::Running)).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(selection.offered().is_empty());
    assert!(
        handle
            .snapshot()
            .slots
            .get(&1)
            .expect("slot")
            .requests
            .is_empty()
    );
    handle
        .send_event(UserEvent {
            slot: 1,
            generation: state.generation,
            id: 1,
            name: "changed".to_owned(),
            args: Vec::new(),
            seq: None,
            gesture: false,
        })
        .await
        .expect("change");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(selection.offered().is_empty());
    handle
        .send_event(UserEvent {
            slot: 1,
            generation: state.generation,
            id: 1,
            name: "clicked".to_owned(),
            args: Vec::new(),
            seq: None,
            gesture: true,
        })
        .await
        .expect("click");
    let accepted = tokio::time::timeout(Duration::from_secs(2), async {
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
    assert_eq!(accepted.requests[0].1, BarRequest::ClosePopover);
    assert_eq!(
        accepted.requests[1].1,
        BarRequest::OpenUri("https://example.com/open".to_owned())
    );
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(selection.offered().is_empty());
    tokio::time::sleep(Duration::from_secs(3)).await;
    handle
        .send_event(UserEvent {
            slot: 1,
            generation: state.generation,
            id: 1,
            name: "changed".to_owned(),
            args: Vec::new(),
            seq: None,
            gesture: false,
        })
        .await
        .expect("late change");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        handle
            .snapshot()
            .slots
            .get(&1)
            .expect("slot")
            .requests
            .len(),
        2
    );
    assert!(selection.offered().is_empty());
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

#[tokio::test]
async fn rejects_bad_lines_trees_and_flood_without_stopping_sibling() {
    let bad_source = r##"#!/usr/bin/env python3
import json, sys, time
print('{"t":"hello","v":1}', flush=True)
sys.stdin.readline()
mode = sys.argv[2]
if mode == "garbage":
    print("garbage", flush=True)
elif mode == "huge":
    print("x" * (2 << 20), flush=True)
elif mode == "many":
    nodes = [{"id": i, "type": "box", "children": [{"id": i * 200 + j, "type": "label"} for j in range(1, 201)]} for i in range(2, 18)]
    print(json.dumps({"t": "commit", "ops": [{"op": "insert", "parent": 0, "node": {"id": 1, "type": "popover", "children": nodes}}]}), flush=True)
elif mode == "cycle":
    print(json.dumps({"t": "commit", "ops": [{"op": "insert", "parent": 0, "node": {"id": 1, "type": "popover", "children": [{"id": 2, "type": "box", "children": [{"id": 3, "type": "box"}]}]}}]}), flush=True)
    print(json.dumps({"t": "commit", "ops": [{"op": "move", "parent": 3, "id": 2}]}), flush=True)
time.sleep(30)
"##;
    let good_source = r##"#!/usr/bin/env python3
import json, sys, time
print('{"t":"hello","v":1}', flush=True)
sys.stdin.readline()
print(json.dumps({"t": "commit", "ops": [{"op": "insert", "parent": 0, "node": {"id": 1, "type": "indicator", "props": {"text": "alive"}}}]}), flush=True)
time.sleep(30)
"##;
    for mode in ["garbage", "huge", "many", "cycle"] {
        let (_bad_dir, mut bad, _bad_output) = script(bad_source);
        bad.argv.push(mode.to_owned());
        let (_good_dir, good, _good_output) = script(good_source);
        let mut config = configured();
        config.applets.insert(
            "other".to_owned(),
            glimpse_config::ExecAppletConfig {
                applet: "me.example.Other".to_owned(),
                options: Map::new(),
                sandbox: glimpse_config::AppletSandbox {
                    read: vec![std::env::temp_dir().display().to_string()],
                    write: vec![std::env::temp_dir().display().to_string()],
                    ..Default::default()
                },
            },
        );
        let (handle, _sender, cancel, task, _selection) =
            running_with(config, Arc::new(PairCatalog { bad, good }));
        handle
            .attach(1, "test", placement())
            .await
            .expect("bad attach");
        handle
            .attach(2, "other", placement())
            .await
            .expect("good attach");
        status(&handle, 1, |s| matches!(s, Status::Restarting { .. })).await;
        let sibling = tokio::time::timeout(Duration::from_secs(2), async {
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
        "#!/usr/bin/env python3\nimport time\nprint('{\"t\":\"hello\",\"v\":1}', flush=True)\ntime.sleep(30)\n",
    );
    let (handle, _sender, cancel, task, _selection) =
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
                args: vec![Value::String("x".repeat(1024))],
                seq: None,
                gesture: false,
            })
            .await
            .expect("event");
    }
    status(&handle, 1, |s| matches!(s, Status::Restarting { .. })).await;
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn ten_second_child_restarts_after_backoff() {
    let (_dir, entry, output) = script(
        "#!/usr/bin/env python3\nimport os, sys, time\nmarker = sys.argv[1] + '.marker'\nprint('{\"t\":\"hello\",\"v\":1}', flush=True)\nif not os.path.exists(marker):\n    open(marker, 'w').close()\n    time.sleep(10)\nelse:\n    sys.stdin.readline()\n    time.sleep(30)\n",
    );
    let (handle, _sender, cancel, task, _selection) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    let first = status(&handle, 1, |s| matches!(s, Status::Running)).await;
    let restarting = status(&handle, 1, |s| {
        matches!(s, Status::Restarting { attempt: 1 })
    })
    .await;
    assert!(matches!(
        restarting.status,
        Status::Restarting { attempt: 1 }
    ));
    let second = next_generation(&handle, 1, first.generation).await;
    assert!(second.generation > first.generation);
    assert!(output.with_extension("marker").exists());
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn a_second_hello_resets_the_tree_and_bumps_generation_without_respawn() {
    let (_dir, entry, output) = script(
        "#!/usr/bin/env python3\nimport json, os, sys, time\nopen(sys.argv[1] + '.pid', 'w').write(str(os.getpid()))\nprint('{\"t\":\"hello\",\"v\":1}', flush=True)\nsys.stdin.readline()\nprint(json.dumps({\"t\":\"commit\",\"ops\":[{\"op\":\"insert\",\"parent\":0,\"node\":{\"id\":1,\"type\":\"indicator\"}}]}), flush=True)\ntime.sleep(0.05)\nprint('{\"t\":\"hello\",\"v\":1}', flush=True)\nsys.stdin.readline()\ntime.sleep(30)\n",
    );
    let (handle, _sender, cancel, task, _selection) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    let first = tokio::time::timeout(Duration::from_secs(2), async {
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
    let pid = tokio::fs::read_to_string(format!("{}.pid", output.display()))
        .await
        .expect("pid");
    let second = next_generation(&handle, 1, first.generation).await;
    assert!(second.tree.children(ROOT).is_empty());
    assert_eq!(
        tokio::fs::read_to_string(format!("{}.pid", output.display()))
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
        "#!/usr/bin/env python3\nimport json, sys, time\nprint('{\"t\":\"hello\",\"v\":1}', flush=True)\nsys.stdin.readline()\nprint('{\"t\":\"dance\"}', flush=True)\nfor _ in range(300):\n    print('{\"t\":\"commit\",\"ops\":[]}', flush=True)\nprint(json.dumps({\"t\": \"commit\", \"ops\": [{\"op\": \"insert\", \"parent\": 0, \"node\": {\"id\": 1, \"type\": \"indicator\"}}]}), flush=True)\ntime.sleep(30)\n",
    );
    let (handle, _sender, cancel, task, _selection) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    let state = tokio::time::timeout(Duration::from_secs(6), async {
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

#[test]
fn only_web_and_mail_uris_are_allowed() {
    assert!(allowed_uri("https://example.com"));
    assert!(allowed_uri("MAILTO:me@example.com"));
    assert!(!allowed_uri("file:///x"));
    assert!(!allowed_uri("javascript:alert(1)"));
    assert!(!allowed_uri(&format!("https://{}", "a".repeat(2048))));
}

#[tokio::test]
async fn a_thousand_stderr_lines_cannot_block_hello() {
    let (_dir, entry, _output) = script(
        "#!/usr/bin/env python3\nimport sys, time\nfor i in range(1000):\n    sys.stderr.write(f'line {i}\\n')\nsys.stderr.write('y' * 10000 + '\\n')\nsys.stderr.flush()\nprint('{\"t\":\"hello\",\"v\":1}', flush=True)\nsys.stdin.readline()\ntime.sleep(30)\n",
    );
    let (handle, _sender, cancel, task, _selection) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn a_sixth_hello_within_ten_seconds_restarts_the_child() {
    let (_dir, entry, _output) = script(
        "#!/usr/bin/env python3\nimport time\nfor _ in range(6):\n    print('{\"t\":\"hello\",\"v\":1}', flush=True)\n    time.sleep(0.05)\ntime.sleep(30)\n",
    );
    let (handle, _sender, cancel, task, _selection) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Restarting { .. })).await;
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn a_vanished_entry_stops_the_running_child() {
    let (_dir, entry, output) = script(
        "#!/bin/sh\necho $$ > \"$1\"\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nwhile read -r line; do :; done\n",
    );
    let catalog = FakeCatalog(Arc::new(Mutex::new(Ok(entry))));
    let (handle, _sender, cancel, task, _selection) = running(catalog.clone());
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    let pid = tokio::fs::read_to_string(&output).await.expect("pid");
    *catalog.0.lock().expect("catalog") = Err("not installed".to_owned());
    let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
    assert_eq!(failed.status, Status::Failed("not installed".to_owned()));
    tokio::time::timeout(Duration::from_secs(2), async {
        while PathBuf::from(format!("/proc/{}", pid.trim())).exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("child killed");
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn stderr_output_cannot_block_hello() {
    let (_dir, entry, _output) = script(
        "#!/usr/bin/env python3\nimport sys, time\nsys.stderr.write('x' * (2 << 20))\nsys.stderr.flush()\nprint('{\"t\":\"hello\",\"v\":1}', flush=True)\nsys.stdin.readline()\ntime.sleep(30)\n",
    );
    let (handle, _sender, cancel, task, _selection) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle
        .attach(9193, "test", placement())
        .await
        .expect("attach");
    status(&handle, 9193, |s| matches!(s, Status::Running)).await;
    handle.detach(9193).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}
