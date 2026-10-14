use super::*;

const PID_LOOP: &str = "open(sys.argv[1], 'a').write(str(os.getpid()) + '\\n')\nsend('{\"t\":\"hello\",\"v\":1}')\nfor line in r:\n    open(sys.argv[1] + '.in', 'a').write(line)\n";

#[tokio::test]
async fn dev_takes_over_and_detach_returns_the_configured_applet() {
    let (_dir, entry, output) = script(PID_LOOP);
    let (_dev_dir, dev, dev_output) = script(PID_LOOP);
    let ((handle, _sender, cancel, task, _selection, _launcher), _config) =
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
    let ((handle, _sender, cancel, task, _selection, _launcher), _config) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    let dev_handle = handle
        .attach_dev("test", dev_request(dev_entry(&dev)))
        .await
        .expect("dev attach");
    let first = status(&handle, 1, |s| matches!(s, Status::Running)).await;
    status(&handle, 1, |s| matches!(s, Status::Ended)).await;
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
async fn dev_pending_attaches_when_its_widget_arrives() {
    let (_dir, entry, _output) = script(LOOP);
    let (_dev_dir, dev, dev_output) = script(PID_LOOP);
    let ((handle, _sender, cancel, task, _selection, _launcher), _config) =
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
    let ((handle, _sender, cancel, task, _selection, _launcher), _config) =
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
