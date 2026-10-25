use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use super::*;

#[test]
fn the_wayland_socket_belongs_to_the_selected_hyprland_instance() {
    let dir = Path::new("/run/user/1000/hypr/instance");
    assert_eq!(
        socket_path(dir, "1234\nwayland-7\n").unwrap(),
        Path::new("/run/user/1000/wayland-7")
    );
    assert_eq!(
        socket_path(dir, "1234\n/tmp/nested-wayland\n").unwrap(),
        Path::new("/tmp/nested-wayland")
    );
    for lock in [
        "",
        "1234\n",
        "0\nwayland-7",
        "1234\n../wayland-7",
        "1234\nwayland-7\nextra",
    ] {
        assert!(socket_path(dir, lock).is_err(), "{lock:?}");
    }
}

fn message(stream: &mut UnixStream, id: u32, opcode: u32, body: &[u8]) {
    let header = [id, ((body.len() as u32 + 8) << 16) | opcode];
    let mut bytes = words(&header);
    bytes.extend_from_slice(body);
    if let Err(error) = stream.write_all(&bytes) {
        assert!(
            id == 1
                && opcode == 1
                && matches!(
                    error.kind(),
                    std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
                ),
            "{error}"
        );
    }
}

fn words(values: &[u32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect()
}

fn string(value: &str) -> Vec<u8> {
    let mut bytes = words(&[value.len() as u32 + 1]);
    bytes.extend_from_slice(value.as_bytes());
    bytes.push(0);
    bytes.resize(bytes.len().next_multiple_of(4), 0);
    bytes
}

type Calls = Arc<Mutex<Vec<(u32, bool)>>>;
type Server = (
    UnixStream,
    Calls,
    Arc<Mutex<[bool; 3]>>,
    std::thread::JoinHandle<()>,
);

fn server(reply: u32, applies: bool) -> Server {
    server_with_behavior(reply, applies, false, false)
}

fn server_with_behavior(reply: u32, applies: bool, delay: bool, defer: bool) -> Server {
    let (client, mut server) = UnixStream::pair().unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let calls: Calls = Arc::default();
    let recorded = calls.clone();
    let saved = Arc::new(Mutex::new([false, false, true]));
    let pending = saved.clone();
    let task = std::thread::spawn(move || {
        let (mut registry, mut manager, mut config) = (0, 0, 0);
        let heads = [0xff00_0000, 0xff00_0001, 0xff00_0002];
        let mut enabled = [false, false, true];
        let mut changes = Vec::new();
        let mut sync_count = 0;
        let mut delayed_sync = 0;
        let mut apply_count = 0;
        loop {
            let mut header = [0; 8];
            if let Err(error) = server.read_exact(&mut header) {
                assert!(matches!(
                    error.kind(),
                    std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset
                ));
                return;
            }
            let id = u32::from_ne_bytes(header[..4].try_into().unwrap());
            let size_op = u32::from_ne_bytes(header[4..].try_into().unwrap());
            let opcode = size_op & 0xffff;
            let mut body = vec![0; (size_op >> 16) as usize - 8];
            server.read_exact(&mut body).unwrap();
            let args: Vec<u32> = body
                .chunks_exact(4)
                .map(|bytes| u32::from_ne_bytes(bytes.try_into().unwrap()))
                .collect();
            match (id, opcode) {
                (1, 1) => {
                    registry = args[0];
                    let mut global = words(&[1]);
                    global.extend(string("zwlr_output_manager_v1"));
                    global.extend(words(&[4]));
                    message(&mut server, registry, 0, &global);
                }
                (1, 0) => {
                    sync_count += 1;
                    if delay && sync_count > 1 {
                        if sync_count == 2 {
                            delayed_sync = args[0];
                        } else {
                            message(&mut server, delayed_sync, 0, &words(&[1]));
                            message(&mut server, 1, 1, &words(&[delayed_sync]));
                        }
                        continue;
                    }
                    message(&mut server, args[0], 0, &words(&[1]));
                    message(&mut server, 1, 1, &words(&[args[0]]));
                }
                (id, 0) if id == registry => {
                    manager = *args.last().unwrap();
                    for (i, name) in ["DP-3", "DP-4", "eDP-1"].iter().enumerate() {
                        message(&mut server, manager, 0, &words(&[heads[i]]));
                        message(&mut server, heads[i], 0, &string(name));
                        message(&mut server, heads[i], 4, &words(&[u32::from(enabled[i])]));
                    }
                    message(&mut server, manager, 1, &words(&[1]));
                }
                (id, 0) if id == manager => {
                    config = args[0];
                    changes.clear();
                }
                (id, 0) if id == config => changes.push((args[1], true)),
                (id, 1) if id == config => changes.push((args[0], false)),
                (id, 2) if id == config => {
                    apply_count += 1;
                    recorded.lock().unwrap().extend(changes.iter().copied());
                    for &(head, value) in &changes {
                        pending.lock().unwrap()[heads.iter().position(|id| *id == head).unwrap()] =
                            value;
                    }
                    if defer && apply_count > 1 {
                        continue;
                    }
                    if applies && reply == 0 {
                        for &(head, value) in &changes {
                            enabled[heads.iter().position(|id| *id == head).unwrap()] = value;
                            message(&mut server, head, 4, &words(&[u32::from(value)]));
                        }
                    }
                    message(&mut server, manager, 1, &words(&[2]));
                    message(&mut server, config, reply, &[]);
                }
                (id, 4) if id == config => {
                    if defer && apply_count == 2 {
                        message(&mut server, config, 0, &[]);
                    }
                    message(&mut server, 1, 1, &words(&[config]));
                }
                _ => {
                    panic!("unexpected request {id}:{opcode}; monitor settings must stay untouched")
                }
            }
        }
    });
    (client, calls, saved, task)
}

#[test]
fn enabling_an_output_preserves_its_native_rule_and_a_disabled_sibling() {
    let (socket, calls, _, task) = server(0, true);
    let mut manager = Manager::new(socket).unwrap();
    manager
        .set_enabled("DP-3", true, &["DP-4".to_owned()])
        .unwrap();
    manager
        .set_enabled("DP-3", false, &["DP-4".to_owned()])
        .unwrap();
    assert_eq!(
        *calls.lock().unwrap(),
        [
            (0xff00_0000, true),
            (0xff00_0001, false),
            (0xff00_0000, false),
            (0xff00_0001, false)
        ]
    );
    drop(manager);
    task.join().unwrap();
}

#[test]
fn an_unknown_connector_never_applies_a_configuration() {
    let (socket, calls, _, task) = server(0, true);
    let mut manager = Manager::new(socket).unwrap();
    assert!(matches!(
        manager.set_enabled("missing", true, &[]),
        Err(Error::Refused(_))
    ));
    assert!(calls.lock().unwrap().is_empty());
    drop(manager);
    task.join().unwrap();
}

#[test]
fn refused_and_cancelled_configurations_do_not_report_success() {
    for (reply, applies) in [(1, true), (2, true)] {
        let (socket, _, _, task) = server(reply, applies);
        let mut manager = Manager::new(socket).unwrap();
        assert!(matches!(
            manager.set_enabled("DP-3", true, &[]),
            Err(Error::Failed(_))
        ));
        drop(manager);
        task.join().unwrap();
    }
}

#[test]
fn an_unresponsive_compositor_has_a_bounded_wait() {
    let (client, _server) = UnixStream::pair().unwrap();
    let start = Instant::now();
    assert!(Manager::new(client).is_err());
    assert!(start.elapsed() < DEADLINE * 2);
}

#[test]
fn a_late_sync_reply_cannot_complete_the_next_command() {
    let (socket, calls, _, task) = server_with_behavior(0, true, true, false);
    let mut manager = Manager::new(socket).unwrap();
    assert!(manager.set_enabled("DP-3", true, &[]).is_err());
    assert!(manager.set_enabled("DP-3", true, &[]).is_err());
    assert!(calls.lock().unwrap().is_empty());
    drop(manager);
    task.join().unwrap();
}

#[tokio::test]
async fn native_monitor_state_verifies_changes_and_preserves_disabled_siblings() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use glimpse_compositors::backend::Backend;

    use super::super::Hyprland;
    use super::super::testing::FakeHyprland;

    for applied in [true, false] {
        let reads = AtomicUsize::new(0);
        let native = FakeHyprland::spawn(
            move |command| {
                assert_eq!(command, "j/monitors all");
                let disabled = reads.fetch_add(1, Ordering::SeqCst) == 0 || !applied;
                serde_json::json!([
                    { "id": 0, "name": "DP-3", "disabled": disabled },
                    { "id": 1, "name": "DP-4", "disabled": false },
                    { "id": 2, "name": "eDP-1", "disabled": true }
                ])
                .to_string()
            },
            Vec::new(),
        );
        let (socket, calls, _, task) = server(0, false);
        let backend = Hyprland::at(&native.dir);
        *backend.outputs.lock().await = Some(Manager::new(socket).unwrap());
        let result = backend.set_output_enabled("DP-3".to_owned(), true).await;
        if applied {
            result.unwrap();
        } else {
            assert!(
                matches!(result, Err(Error::Failed(reason)) if reason.contains("did not become enabled"))
            );
            assert!(backend.outputs.lock().await.is_none());
        }
        let mut expected = vec![(0xff00_0000, true), (0xff00_0002, false)];
        if !applied {
            expected.push((0xff00_0000, false));
        }
        assert_eq!(*calls.lock().unwrap(), expected);
        drop(backend);
        task.join().unwrap();
    }
}

#[test]
fn timed_out_changes_are_replaced_without_losing_prior_overrides() {
    let (socket, calls, saved, task) = server_with_behavior(0, true, false, true);
    let mut manager = Manager::new(socket).unwrap();
    manager
        .set_enabled("DP-4", true, &["DP-3".to_owned()])
        .unwrap();
    manager.confirm("DP-4".to_owned(), true, vec!["DP-3".to_owned()]);
    assert!(
        manager
            .set_enabled("eDP-1", false, &["DP-3".to_owned()])
            .is_err()
    );
    manager.restore_enabled("eDP-1", true).unwrap();
    assert_eq!(*saved.lock().unwrap(), [false, true, true]);
    assert_eq!(
        *calls.lock().unwrap(),
        [
            (0xff00_0000, false),
            (0xff00_0001, true),
            (0xff00_0000, false),
            (0xff00_0002, false),
            (0xff00_0002, true)
        ]
    );
    assert!(manager.state.result.is_none());
    let (socket, replacement_calls, _, replacement_task) = server(0, true);
    let replacement = Manager::replacement(socket, &manager).unwrap();
    assert_eq!(
        *replacement_calls.lock().unwrap(),
        [(0xff00_0000, false), (0xff00_0001, true)]
    );
    drop(manager);
    task.join().unwrap();
    assert!(!replacement.committed.contains_key("eDP-1"));
    drop(replacement);
    replacement_task.join().unwrap();
}
