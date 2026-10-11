use super::*;

#[tokio::test]
async fn untaken_offer_expires_and_closes_its_pipes() {
    let (handle, _sender, cancel, task, _) = running_with(
        configured(),
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
            "not installed".to_owned()
        ))))),
    );
    let (pipes, _peer_writer, mut peer_reader) = attached_peer();
    handle
        .offer(
            "me.x.A",
            Offer {
                output: None,
                position: None,
                entry: attached_entry(),
                options: Map::new(),
                sandbox: glimpse_config::AppletSandbox::default(),
                pipes,
            },
        )
        .await
        .expect("offer");
    assert!(handle.snapshot().offers.contains("me.x.A"));
    tokio::time::timeout(
        Duration::from_secs(7),
        peer_reader.read_to_end(&mut Vec::new()),
    )
    .await
    .expect("host pipe EOF")
    .expect("host pipe read");
    assert!(handle.snapshot().offers.is_empty());
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn reoffer_revives_an_ended_attached_slot() {
    let (handle, _sender, cancel, task, _) = running_with(
        configured(),
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
            "not installed".to_owned()
        ))))),
    );
    let (pipes, mut peer_writer, mut peer_reader) = attached_peer();
    offer_and_attach(&handle, 1, "me.x.A", Map::new(), pipes).await;
    peer_writer
        .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
        .await
        .expect("hello");
    let mut line = String::new();
    peer_reader.read_line(&mut line).await.expect("host hello");
    let first = status(&handle, 1, |s| matches!(s, Status::Running)).await;
    drop(peer_writer);
    status(&handle, 1, |s| matches!(s, Status::Ended)).await;
    assert!(handle.snapshot().offers.is_empty());
    let (pipes, mut peer_writer, mut peer_reader) = attached_peer();
    handle
        .offer(
            "me.x.A",
            Offer {
                output: None,
                position: None,
                entry: attached_entry(),
                options: Map::new(),
                sandbox: glimpse_config::AppletSandbox::default(),
                pipes,
            },
        )
        .await
        .expect("reoffer");
    assert_eq!(handle.snapshot().slots.len(), 1);
    assert_eq!(
        handle.snapshot().slots.get(&1).expect("slot").status,
        Status::Starting
    );
    assert!(handle.snapshot().offers.contains("me.x.A"));
    peer_writer
        .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
        .await
        .expect("second hello");
    let mut line = String::new();
    peer_reader
        .read_line(&mut line)
        .await
        .expect("second host hello");
    next_generation(&handle, 1, first.generation).await;
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn offer_revives_a_not_configured_slot_left_by_a_rebuild() {
    let config = configured();
    let base = config.state_base.clone().expect("state base");
    let (handle, _sender, cancel, task, _) = running_with(
        config,
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
            "not installed".to_owned()
        ))))),
    );
    let (pipes, _peer_writer, _peer_reader) = attached_peer();
    offer_and_attach(&handle, 1, "me.x.A", Map::new(), pipes).await;
    handle
        .attach(2, "me.x.A", placement())
        .await
        .expect("replacement attach");
    assert_eq!(
        handle.snapshot().slots.get(&2).expect("replacement").status,
        Status::Failed("not configured".to_owned())
    );
    handle.detach(1).await.expect("old detach");
    let (pipes, mut peer_writer, mut peer_reader) = attached_peer();
    handle
        .offer(
            "me.x.A",
            Offer {
                output: None,
                position: None,
                entry: attached_entry(),
                options: Map::new(),
                sandbox: glimpse_config::AppletSandbox::default(),
                pipes,
            },
        )
        .await
        .expect("reoffer");
    assert_eq!(handle.snapshot().slots.len(), 1);
    assert_eq!(
        handle.snapshot().slots.get(&2).expect("replacement").status,
        Status::Starting
    );
    peer_writer
        .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
        .await
        .expect("hello");
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(2), peer_reader.read_line(&mut line))
        .await
        .expect("host hello")
        .expect("host pipe read");
    let hello: Value = serde_json::from_str(&line).expect("json");
    assert_eq!(hello["t"], "hello");
    assert_eq!(
        hello["state"],
        base.join("applets/me.x.A").display().to_string()
    );
    status(&handle, 2, |s| matches!(s, Status::Running)).await;
    handle.detach(2).await.expect("replacement detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn attach_takes_offer_for_its_output() {
    let (_dir, entry, _output) = script(
        "#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nwhile read -r line; do :; done\n",
    );
    let (handle, _sender, cancel, task, _) = running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    let (pipes, mut peer_writer, mut peer_reader) = attached_peer();
    handle
        .offer(
            "me.example.Test",
            Offer {
                output: Some("DP-1".to_owned()),
                position: None,
                entry: attached_entry(),
                options: Map::new(),
                sandbox: glimpse_config::AppletSandbox::default(),
                pipes,
            },
        )
        .await
        .expect("offer");
    let mut other = placement();
    other.output = Some("HDMI-1".to_owned());
    handle.attach(1, "test", other).await.expect("other attach");
    assert_eq!(
        status(&handle, 1, |s| matches!(s, Status::Running))
            .await
            .title,
        "Test applet"
    );
    assert!(handle.snapshot().offers.contains("me.example.Test"));
    handle
        .attach(2, "test", placement())
        .await
        .expect("matching attach");
    assert_eq!(handle.snapshot().slots.len(), 2);
    assert_eq!(
        handle.snapshot().slots.get(&2).expect("slot").status,
        Status::Starting
    );
    peer_writer
        .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
        .await
        .expect("hello");
    let mut line = String::new();
    peer_reader.read_line(&mut line).await.expect("host hello");
    assert_eq!(
        serde_json::from_str::<Value>(&line).expect("json")["t"],
        "hello"
    );
    assert_eq!(
        status(&handle, 2, |s| matches!(s, Status::Running))
            .await
            .title,
        "Attached applet"
    );
    handle.detach(1).await.expect("detach other");
    handle.detach(2).await.expect("detach match");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn second_offer_for_the_same_id_is_refused() {
    let (handle, _sender, cancel, task, _) = running_with(
        configured(),
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
            "not installed".to_owned()
        ))))),
    );
    for attached in [false, true] {
        let (pipes, _writer, _reader) = attached_peer();
        let result = handle
            .offer(
                "me.x.A",
                Offer {
                    output: None,
                    position: None,
                    entry: attached_entry(),
                    options: Map::new(),
                    sandbox: glimpse_config::AppletSandbox::default(),
                    pipes,
                },
            )
            .await;
        if attached {
            assert!(
                matches!(result, Err(CommandError::InvalidArgument(reason)) if reason == "me.x.A is already attached")
            );
        } else {
            result.expect("first offer");
        }
    }
    handle
        .attach(1, "me.x.A", placement())
        .await
        .expect("attach");
    let (pipes, _writer, _reader) = attached_peer();
    assert!(matches!(handle.offer("me.x.A", Offer {
        output: None, position: None, entry: attached_entry(), options: Map::new(),
        sandbox: glimpse_config::AppletSandbox::default(), pipes,
    }).await, Err(CommandError::InvalidArgument(reason)) if reason == "me.x.A is already attached"));
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn offer_takes_over_the_slot_on_its_panel_edge() {
    let (_dir, entry, _pid_file) = script(
        "#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nwhile read -r line; do :; done\n",
    );
    let (handle, _sender, cancel, task, _) = running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle
        .attach(1, "test", placement())
        .await
        .expect("attach top");
    let bottom = Placement {
        position: Edge::Bottom,
        ..placement()
    };
    handle
        .attach(2, "test", bottom)
        .await
        .expect("attach bottom");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    status(&handle, 2, |s| matches!(s, Status::Running)).await;
    let (pipes, _peer_writer, _peer_reader) = attached_peer();
    handle
        .offer(
            "me.example.Test",
            Offer {
                output: Some("DP-1".to_owned()),
                position: Some(Edge::Bottom),
                entry: attached_entry(),
                options: Map::new(),
                sandbox: glimpse_config::AppletSandbox::default(),
                pipes,
            },
        )
        .await
        .expect("take over");
    let snapshot = handle.snapshot();
    assert_ne!(
        snapshot.slots.get(&1).expect("top").title,
        "Attached applet"
    );
    assert_eq!(
        snapshot.slots.get(&2).expect("bottom").title,
        "Attached applet"
    );
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn offer_takes_over_spawned_slot_and_returns_it() {
    let (_dir, entry, pid_file) = script(
        "#!/bin/sh\necho $$ > \"$1\"\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nread -r line\nprintf '{\"t\":\"commit\",\"ops\":[{\"op\":\"insert\",\"parent\":0,\"node\":{\"id\":1,\"type\":\"indicator\"}}]}\\n'\nwhile read -r line; do :; done\n",
    );
    let (handle, _sender, cancel, task, _) = running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    let first = status(&handle, 1, |s| matches!(s, Status::Running)).await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while handle
            .snapshot()
            .slots
            .get(&1)
            .expect("slot")
            .tree
            .children(ROOT)
            != [1]
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("first tree");
    let pid = file_contains(&pid_file, "").await;
    let (pipes, mut peer_writer, mut peer_reader) = attached_peer();
    handle
        .offer(
            "me.example.Test",
            Offer {
                output: Some("DP-1".to_owned()),
                position: None,
                entry: attached_entry(),
                options: Map::new(),
                sandbox: glimpse_config::AppletSandbox::default(),
                pipes,
            },
        )
        .await
        .expect("take over");
    assert!(handle.snapshot().offers.contains("me.example.Test"));
    assert_eq!(handle.snapshot().slots.len(), 1);
    assert_eq!(
        handle.snapshot().slots.get(&1).expect("slot").title,
        "Attached applet"
    );
    assert_eq!(
        handle
            .snapshot()
            .slots
            .get(&1)
            .expect("slot")
            .tree
            .children(ROOT),
        [1]
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        while PathBuf::from(format!("/proc/{}", pid.trim())).exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("child stopped");
    peer_writer
        .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
        .await
        .expect("peer hello");
    let mut line = String::new();
    peer_reader.read_line(&mut line).await.expect("host hello");
    next_generation(&handle, 1, first.generation).await;
    drop(peer_writer);
    status(&handle, 1, |s| matches!(s, Status::Starting)).await;
    assert!(handle.snapshot().offers.is_empty());
    next_generation(&handle, 1, first.generation + 1).await;
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn detach_of_attached_slot_closes_pipes_and_drops_offer() {
    for fail in [false, true] {
        let (handle, _sender, cancel, task, _) = running_with(
            configured(),
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
                "not installed".to_owned()
            ))))),
        );
        let (pipes, mut peer_writer, mut peer_reader) = attached_peer();
        offer_and_attach(&handle, 1, "me.x.A", Map::new(), pipes).await;
        assert!(handle.snapshot().offers.contains("me.x.A"));
        peer_writer
            .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
            .await
            .expect("hello");
        let mut line = String::new();
        peer_reader.read_line(&mut line).await.expect("host hello");
        if fail {
            for _ in 0..EXEC_MAX_HELLOS {
                peer_writer
                    .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
                    .await
                    .expect("hello");
            }
            status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
        } else {
            handle.detach(1).await.expect("detach");
        }
        tokio::time::timeout(
            Duration::from_secs(1),
            peer_reader.read_to_end(&mut Vec::new()),
        )
        .await
        .expect("host pipe EOF")
        .expect("host pipe read");
        if fail {
            handle.detach(1).await.expect("detach");
        }
        assert!(handle.snapshot().offers.is_empty());
        drop(peer_writer);
        cancel.cancel();
        let _ = task.await;
    }
}

#[tokio::test]
async fn offer_options_reach_running_attached_slot() {
    let config = configured();
    let base = config.state_base.clone().expect("state base");
    let (handle, sender, cancel, task, _) = running_with(
        config,
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
            "not installed".to_owned()
        ))))),
    );
    let (pipes, mut peer_writer, mut peer_reader) = attached_peer();
    let options = Map::from_iter([("color".to_owned(), Value::from("blue"))]);
    offer_and_attach(&handle, 1, "me.x.A", options, pipes).await;
    peer_writer
        .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
        .await
        .expect("hello");
    let mut line = String::new();
    peer_reader.read_line(&mut line).await.expect("host hello");
    let hello: Value = serde_json::from_str(&line).expect("json");
    assert_eq!(
        hello["state"],
        base.join("applets/me.x.A").display().to_string()
    );
    assert_eq!(hello["options"]["color"], "blue");
    let first = status(&handle, 1, |s| matches!(s, Status::Running)).await;
    assert_eq!(first.title, "Attached applet");
    assert_eq!(first.icon.as_deref(), Some("test-symbolic"));
    peer_writer.write_all(b"{\"t\":\"commit\",\"ops\":[{\"op\":\"insert\",\"parent\":0,\"node\":{\"id\":1,\"type\":\"indicator\"}}]}\n").await.expect("commit");
    tokio::time::timeout(Duration::from_secs(2), async {
        while handle
            .snapshot()
            .slots
            .get(&1)
            .is_none_or(|s| s.tree.children(ROOT) != [1])
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("tree");
    peer_writer
        .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
        .await
        .expect("second hello");
    line.clear();
    peer_reader
        .read_line(&mut line)
        .await
        .expect("second host hello");
    let second = next_generation(&handle, 1, first.generation).await;
    assert_eq!(second.tree.children(ROOT), [1]);
    assert_eq!(second.status, Status::Running);
    sender
        .try_send(Input::Config(Config {
            applets: BTreeMap::new(),
            state_base: Some(base),
        }))
        .expect("queued");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        handle.snapshot().slots.get(&1).expect("slot").status,
        Status::Running
    );
    handle
        .offer_options(
            "me.x.A",
            Map::from_iter([("color".to_owned(), Value::from("red"))]),
        )
        .await
        .expect("set options");
    line.clear();
    peer_reader.read_line(&mut line).await.expect("options");
    let options: Value = serde_json::from_str(&line).expect("json");
    assert_eq!(options["t"], "options");
    assert_eq!(options["options"]["color"], "red");
    assert!(
        matches!(handle.offer_options("unknown", Map::new()).await, Err(CommandError::InvalidArgument(reason)) if reason == "unknown applet")
    );
    drop(peer_writer);
    status(&handle, 1, |s| matches!(s, Status::Ended)).await;
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn attached_eof_is_terminal() {
    let (handle, _sender, cancel, task, _) = running_with(
        configured(),
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
            "not installed".to_owned()
        ))))),
    );
    let (pipes, mut peer_writer, mut peer_reader) = attached_peer();
    offer_and_attach(&handle, 1, "me.x.A", Map::new(), pipes).await;
    peer_writer
        .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
        .await
        .expect("hello");
    let mut line = String::new();
    peer_reader.read_line(&mut line).await.expect("host hello");
    drop(peer_writer);
    let ended = status(&handle, 1, |s| matches!(s, Status::Ended)).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        handle.snapshot().slots.get(&1).expect("slot").status,
        Status::Ended
    );
    assert_eq!(ended.generation, 1);
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn attached_read_write_fifo_is_refused() {
    let (handle, _sender, cancel, task, _) = running_with(
        configured(),
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
            "not installed".to_owned()
        ))))),
    );
    let dir = tempfile::tempdir().expect("tempdir");
    let fifo = dir.path().join("fifo");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )
    .expect("fifo");
    let from_applet = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&fifo)
        .expect("open fifo");
    let (_host_reader, to_applet) = std::io::pipe().expect("pipe");
    offer_and_attach(
        &handle,
        1,
        "me.x.A",
        Map::new(),
        Pipes {
            to_applet: to_applet.into(),
            from_applet: from_applet.into(),
        },
    )
    .await;
    let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
    assert_eq!(
        failed.status,
        Status::Failed("applet pipe must be opened read-only".to_owned())
    );
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn attached_hello_cap_and_missing_state_base() {
    let (handle, _sender, cancel, task, _) = running_with(
        configured(),
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
            "not installed".to_owned()
        ))))),
    );
    let (pipes, mut peer_writer, _peer_reader) = attached_peer();
    offer_and_attach(&handle, 1, "me.x.A", Map::new(), pipes).await;
    for _ in 0..=EXEC_MAX_HELLOS {
        peer_writer
            .write_all(b"{\"t\":\"hello\",\"v\":1}\n")
            .await
            .expect("hello");
    }
    let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
    assert_eq!(failed.status, Status::Failed("too many hellos".to_owned()));
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;

    let mut config = configured();
    config.state_base = None;
    let (handle, _sender, cancel, task, _) = running_with(
        config,
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Err(
            "not installed".to_owned()
        ))))),
    );
    let (pipes, _peer_writer, _peer_reader) = attached_peer();
    offer_and_attach(&handle, 1, "me.x.A", Map::new(), pipes).await;
    let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
    assert_eq!(
        failed.status,
        Status::Failed("cannot determine applet state directory".to_owned())
    );
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}
