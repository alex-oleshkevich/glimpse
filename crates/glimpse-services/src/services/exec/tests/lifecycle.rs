use super::*;

#[tokio::test]
async fn hello_is_answered() {
    let (_dir, entry, output) = script(
        "#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nread -r line\nprintf '%s\\n' \"$line\" > \"$1\"\nwhile read -r line; do printf '%s\\n' \"$line\" >> \"$1\"; done\n",
    );
    let (handle, _sender, cancel, task, _selection) =
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
    let (_dir, entry, output) = script(
        "#!/bin/sh\necho $$ > \"$1.pid\"\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nwhile read -r line; do printf '%s\\n' \"$line\" >> \"$1\"; done\n",
    );
    let (handle, _sender, cancel, task, _selection) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    file_contains(&output, "\"t\":\"hello\"").await;
    let pid = tokio::fs::read_to_string(format!("{}.pid", output.display()))
        .await
        .expect("pid");
    let mut changed = placement();
    changed.size = 48;
    handle.place(1, changed).await.expect("place");
    file_contains(&output, "\"t\":\"placement\"").await;
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
async fn options_change_does_not_respawn() {
    let (_dir, entry, output) = script(
        "#!/bin/sh\necho $$ > \"$1.pid\"\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nwhile read -r line; do printf '%s\\n' \"$line\" >> \"$1\"; done\n",
    );
    let (handle, sender, cancel, task, _selection) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    file_contains(&output, "\"t\":\"hello\"").await;
    let pid = tokio::fs::read_to_string(format!("{}.pid", output.display()))
        .await
        .expect("pid");
    let mut config = configured();
    config
        .applets
        .get_mut("test")
        .expect("config")
        .options
        .insert("color".to_owned(), Value::from("red"));
    sender.reconfigure(config.config);
    file_contains(&output, "\"t\":\"options\"").await;
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
async fn options_changed_while_starting_follow_hello() {
    let (_dir, entry, output) = script(
        "#!/bin/sh\nsleep 0.3\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nread -r line\nprintf '%s\\n' \"$line\" > \"$1\"\nsleep 1\n",
    );
    let (handle, sender, cancel, task, _selection) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    let mut config = configured();
    config
        .applets
        .get_mut("test")
        .expect("config")
        .options
        .insert("color".to_owned(), Value::from("red"));
    sender.reconfigure(config.config);
    file_contains(&output, "\"t\":\"hello\"").await;
    assert!(
        tokio::fs::read_to_string(output)
            .await
            .expect("first line")
            .contains("\"red\"")
    );
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn removed_instance_stays_failed_without_respawning() {
    let (_dir, entry, output) = script(
        "#!/bin/sh\necho $$ >> \"$1\"\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nwhile read -r line; do :; done\n",
    );
    let (handle, sender, cancel, task, _selection) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    let pid = tokio::fs::read_to_string(&output).await.expect("pid");
    sender.reconfigure(Config {
        applets: BTreeMap::new(),
        state_base: None,
        bus_proxy: std::path::PathBuf::from(super::super::PROXY_PROGRAM),
    });
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
    assert!(!PathBuf::from(format!("/proc/{}", pid.trim())).exists());
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn dropping_live_child_stream_reaps_process() {
    let (_dir, entry, output) =
        script("#!/bin/sh\necho $$ > \"$1\"\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nsleep 30\n");
    let (handle, _sender, cancel, task, _selection) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    let pid = tokio::fs::read_to_string(output).await.expect("pid");
    cancel.cancel();
    let _ = task.await;
    tokio::time::timeout(Duration::from_secs(1), async {
        while PathBuf::from(format!("/proc/{}", pid.trim())).exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("child reaped");
}

#[tokio::test]
async fn exits_before_hello_fails() {
    let (_dir, entry, output) = script("#!/bin/sh\necho $$ >> \"$1\"\nexit 3\n");
    let catalog = FakeCatalog(Arc::new(Mutex::new(Ok(entry))));
    let (handle, _sender, cancel, task, _selection) = running(catalog.clone());
    handle.attach(1, "test", placement()).await.expect("attach");
    let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
    assert_eq!(
        failed.status,
        Status::Failed("exited with status 3".to_owned())
    );
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            if tokio::fs::read_to_string(&output)
                .await
                .is_ok_and(|spawns| spawns.lines().count() >= 2)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("respawned on backoff with an unchanged entry");
    let (_good_dir, good, _good_output) = script(
        "#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nwhile read -r line; do :; done\n",
    );
    *catalog.0.lock().expect("catalog") = Ok(good);
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[test]
fn backoff_table() {
    assert_eq!(backoff(0), Duration::from_secs(1));
    assert_eq!(backoff(1), Duration::from_secs(2));
    assert_eq!(backoff(5), Duration::from_secs(32));
    assert_eq!(backoff(6), Duration::from_secs(60));
    assert_eq!(backoff(30), Duration::from_secs(60));
}

#[tokio::test]
async fn retry_recovers() {
    let (_dir, entry, _output) = script(
        "#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nwhile read -r line; do :; done\n",
    );
    let catalog = FakeCatalog(Arc::new(Mutex::new(Err("not installed".to_owned()))));
    let (handle, _sender, cancel, task, _selection) = running(catalog.clone());
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
    let (_dir, entry, output) = script(
        "#!/bin/sh\necho $$ > \"$1.pid\"\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nwhile read -r line; do printf '%s\\n' \"$line\" >> \"$1\"; done\n",
    );
    let (handle, _sender, cancel, task, _selection) =
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
            gesture: true,
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
    let pid: u32 = tokio::fs::read_to_string(format!("{}.pid", output.display()))
        .await
        .expect("pid")
        .trim()
        .parse()
        .expect("numeric pid");
    handle.detach(1).await.expect("detach");
    assert!(!handle.snapshot().slots.contains_key(&1));
    tokio::time::timeout(Duration::from_secs(2), async {
        while std::path::Path::new(&format!("/proc/{pid}")).exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("child reaped");
    cancel.cancel();
    let _ = task.await;
}
