use super::*;
use wayland_client::Proxy;

#[test]
fn a_stopped_capture_finishes_even_when_another_frame_is_pending() {
    let (socket, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
    let connection = Connection::from_socket(socket).unwrap();
    let backend = connection.backend().downgrade();
    let mut state = State::default();
    for connector in ["DP-1", "DP-2"] {
        state.watch(
            connector.to_owned(),
            ExtImageCaptureSourceV1::inert(backend.clone()),
            ExtImageCopyCaptureSessionV1::inert(backend.clone()),
        );
    }
    assert!(!state.finished());
    assert!(!state.constrained());
    state.targets[1].stopped = true;
    assert!(state.finished());
    assert!(state.constrained());
    state.targets[1].stopped = false;
    state.targets[0].outcome = Some(true);
    assert!(!state.finished());
    state.targets[1].outcome = Some(true);
    assert!(state.finished());
    assert!(!state.constrained());
    state.targets[0].constrained = true;
    assert!(!state.constrained());
    state.targets[1].constrained = true;
    assert!(state.constrained());
}
