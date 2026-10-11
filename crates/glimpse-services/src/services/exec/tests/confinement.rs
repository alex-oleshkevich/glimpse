use super::*;

#[tokio::test]
async fn sandbox_denies_ungranted_file() {
    if !landlock_available() {
        return;
    }
    let denied = tempfile::tempdir().expect("denied dir");
    let secret = denied.path().join("secret");
    std::fs::write(&secret, "private\n").expect("secret");
    for granted in [false, true] {
        let (_dir, mut entry, output) = script(
            r#"#!/bin/sh
if read -r value < "$2"; then printf 'allowed' > "$1"; else printf 'denied' > "$1"; fi
printf '{"t":"hello","v":1}\n'
read -r line
sleep 30
"#,
        );
        let mut config = confined(&entry);
        if granted {
            config
                .applets
                .get_mut("test")
                .expect("test applet")
                .sandbox
                .read
                .push(denied.path().display().to_string());
        }
        entry.argv.push(secret.display().to_string());
        let (handle, _, cancel, task, _) = running_with(
            config,
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
        );
        handle.attach(1, "test", placement()).await.expect("attach");
        let wanted = if granted { "allowed" } else { "denied" };
        assert_eq!(file_contains(&output, wanted).await, wanted);
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }
}

#[tokio::test]
async fn symlink_grant_to_root_fails() {
    if !landlock_available() {
        return;
    }
    let (dir, entry, _) = script("#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\n");
    let link = dir.path().join("root-link");
    std::os::unix::fs::symlink("/", &link).expect("symlink");
    let mut config = confined(&entry);
    config
        .applets
        .get_mut("test")
        .expect("test applet")
        .sandbox
        .read
        .push(link.display().to_string());
    let (handle, _, cancel, task, _) = running_with(
        config,
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
    );
    handle.attach(1, "test", placement()).await.expect("attach");
    let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
    assert!(
        matches!(failed.status, Status::Failed(reason) if reason.contains(&link.display().to_string()))
    );
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn sandbox_denies_ungranted_unix_socket() {
    if !landlock_available() {
        return;
    }
    let denied = tempfile::tempdir().expect("denied dir");
    let socket = denied.path().join("socket");
    let _listener = std::os::unix::net::UnixListener::bind(&socket).expect("socket");
    let (_dir, mut entry, output) = script(
        r#"#!/usr/bin/env python3
import errno, socket, sys, time
try:
    socket.socket(socket.AF_UNIX).connect(sys.argv[2])
    result = "allowed"
except OSError as error:
    result = "denied" if error.errno == errno.EACCES else f"other:{error.errno}"
open(sys.argv[1], "w").write(result)
print('{"t":"hello","v":1}', flush=True)
sys.stdin.readline()
time.sleep(30)
"#,
    );
    let config = confined(&entry);
    entry.argv.push(socket.display().to_string());
    let (handle, _, cancel, task, _) = running_with(
        config,
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
    );
    handle.attach(1, "test", placement()).await.expect("attach");
    assert_eq!(file_contains(&output, "denied").await, "denied");
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn sandbox_denies_signal_to_panel() {
    if !landlock_available() {
        return;
    }
    let (_dir, mut entry, output) = script(
        r#"#!/bin/sh
if kill -0 "$2"; then printf 'allowed' > "$1"; else printf 'denied' > "$1"; fi
printf '{"t":"hello","v":1}\n'
read -r line
sleep 30
"#,
    );
    let config = confined(&entry);
    entry.argv.push(std::process::id().to_string());
    let (handle, _, cancel, task, _) = running_with(
        config,
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
    );
    handle.attach(1, "test", placement()).await.expect("attach");
    assert_eq!(file_contains(&output, "denied").await, "denied");
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn network_grant_controls_tcp_connect() {
    if !landlock_available() {
        return;
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
    let port = listener.local_addr().expect("address").port();
    let udp = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("UDP listener");
    let udp_port = udp.local_addr().expect("UDP address").port();
    for (network, wanted) in [(false, "denied,denied"), (true, "connected,sent")] {
        let (_dir, mut entry, output) = script(
            r#"#!/usr/bin/env python3
import errno, socket, sys, time
try:
    socket.create_connection(("127.0.0.1", int(sys.argv[2])), timeout=1).close()
    tcp = "connected"
except OSError as error:
    tcp = "denied" if error.errno == errno.EACCES else f"other:{error.errno}"
try:
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.sendto(b"test", ("127.0.0.1", int(sys.argv[3])))
    udp = "sent"
except OSError as error:
    udp = "denied" if error.errno == errno.EACCES else f"other:{error.errno}"
open(sys.argv[1], "w").write(f"{tcp},{udp}")
print('{"t":"hello","v":1}', flush=True)
sys.stdin.readline()
time.sleep(30)
"#,
        );
        let mut config = confined(&entry);
        config
            .applets
            .get_mut("test")
            .expect("test applet")
            .sandbox
            .network = network;
        entry.argv.push(port.to_string());
        entry.argv.push(udp_port.to_string());
        let (handle, _, cancel, task, _) = running_with(
            config,
            Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
        );
        handle.attach(1, "test", placement()).await.expect("attach");
        assert_eq!(file_contains(&output, wanted).await, wanted);
        if network {
            let mut received = [0; 4];
            let (length, _) =
                tokio::time::timeout(Duration::from_secs(2), udp.recv_from(&mut received))
                    .await
                    .expect("UDP datagram")
                    .expect("UDP receive");
            assert_eq!(&received[..length], b"test");
        }
        handle.detach(1).await.expect("detach");
        cancel.cancel();
        let _ = task.await;
    }
}

#[tokio::test]
async fn hello_state_is_private_writable_and_persistent() {
    if !landlock_available() {
        return;
    }
    let (_dir, entry, output) = script(
        r#"#!/usr/bin/env python3
import json, pathlib, sys, time
print('{"t":"hello","v":1}', flush=True)
hello = json.loads(sys.stdin.readline())
state = pathlib.Path(hello["state"])
with (state / "marker").open("a") as marker:
    marker.write("x")
pathlib.Path(sys.argv[1]).write_text(hello["state"])
time.sleep(30)
"#,
    );
    let config = confined(&entry);
    let state = config
        .state_base
        .as_ref()
        .expect("state base")
        .join("applets/me.example.Test");
    let marker = state.join("marker");
    let (handle, _, cancel, task, _) = running_with(
        config,
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
    );
    handle.attach(1, "test", placement()).await.expect("attach");
    file_contains(&output, &state.display().to_string()).await;
    file_contains(&marker, "x").await;
    assert_eq!(
        std::fs::metadata(&state)
            .expect("state dir")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    handle.detach(1).await.expect("detach");
    handle
        .attach(1, "test", placement())
        .await
        .expect("reattach");
    file_contains(&marker, "xx").await;
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn ungranted_executable_fails_with_sandbox_reason() {
    if !landlock_available() {
        return;
    }
    let (_dir, entry, _) = script("#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\n");
    let mut config = confined(&entry);
    let sandbox = &mut config.applets.get_mut("test").expect("test applet").sandbox;
    sandbox.read.clear();
    sandbox.write.clear();
    let (handle, _, cancel, task, _) = running_with(
        config,
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
    );
    handle.attach(1, "test", placement()).await.expect("attach");
    let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
    assert!(
        matches!(failed.status, Status::Failed(reason) if reason.contains("not granted by sandbox"))
    );
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn missing_state_directory_fails_before_spawn() {
    let (_dir, entry, _) = script("#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\n");
    let mut config = confined(&entry);
    config.state_base = None;
    let (handle, _, cancel, task, _) = running_with(
        config,
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
    );
    handle.attach(1, "test", placement()).await.expect("attach");
    let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
    assert!(
        matches!(failed.status, Status::Failed(reason) if reason == "cannot determine applet state directory")
    );
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn non_utf8_state_directory_fails_before_spawn() {
    use std::os::unix::ffi::OsStringExt;

    let (_dir, entry, output) = script("#!/bin/sh\nprintf x > \"$1\"\n");
    let mut config = confined(&entry);
    config.state_base = Some(
        config
            .state_dir
            .path()
            .join(std::ffi::OsString::from_vec(vec![0xff])),
    );
    let (handle, _, cancel, task, _) = running_with(
        config,
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
    );
    handle.attach(1, "test", placement()).await.expect("attach");
    let failed = status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
    assert!(
        matches!(failed.status, Status::Failed(reason) if reason == "cannot determine applet state directory")
    );
    assert!(!output.exists());
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn sandbox_change_respawns() {
    let (_dir, entry, output) = script(
        r#"#!/bin/sh
printf x >> "$1"
printf '{"t":"hello","v":1}\n'
read -r line
sleep 30
"#,
    );
    let config = confined(&entry);
    let mut updated = config.config.clone();
    let (handle, sender, cancel, task, _) = running_with(
        config,
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
    );
    handle.attach(1, "test", placement()).await.expect("attach");
    file_contains(&output, "x").await;
    updated
        .applets
        .get_mut("test")
        .expect("test applet")
        .sandbox
        .read
        .push("/missing-glimpse-test".to_owned());
    sender.try_send(Input::Config(updated)).expect("queued");
    file_contains(&output, "xx").await;
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn unconfined_runs_ungranted_executable() {
    let (_dir, entry, output) = script(
        "#!/bin/sh\nprintf '{\"t\":\"hello\",\"v\":1}\\n'\nread -r line\nprintf '%s' \"$line\" > \"$1\"\nsleep 30\n",
    );
    let mut config = confined(&entry);
    let sandbox = &mut config.applets.get_mut("test").expect("test applet").sandbox;
    sandbox.read.clear();
    sandbox.write.clear();
    sandbox.unconfined = true;
    let (handle, _, cancel, task, _) = running_with(
        config,
        Arc::new(FakeCatalog(Arc::new(Mutex::new(Ok(entry))))),
    );
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    file_contains(&output, "\"state\"").await;
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}
