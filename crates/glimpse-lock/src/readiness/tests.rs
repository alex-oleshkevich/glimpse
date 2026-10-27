use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt as _;
use std::time::Duration;

use super::*;

#[test]
fn systemd_receives_readiness_at_a_filesystem_socket() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("notify");
    let receiver = UnixDatagram::bind(&path).unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    send_ready(path.as_os_str()).unwrap();
    let mut message = [0; 64];
    let length = receiver.recv(&mut message).unwrap();
    assert_eq!(&message[..length], b"READY=1");
}

#[test]
fn systemd_receives_readiness_at_an_abstract_socket() {
    let scratch = tempfile::tempdir().unwrap();
    let name = scratch.path().as_os_str().as_bytes();
    let address = SocketAddr::from_abstract_name(name).unwrap();
    let receiver = UnixDatagram::bind_addr(&address).unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let path = OsString::from_vec([b"@".as_slice(), name].concat());
    send_ready(&path).unwrap();
    let mut message = [0; 64];
    let length = receiver.recv(&mut message).unwrap();
    assert_eq!(&message[..length], b"READY=1");
}

#[test]
fn a_missing_systemd_socket_is_a_real_send_error() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("missing");
    assert_eq!(
        send_ready(path.as_os_str()).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
}
