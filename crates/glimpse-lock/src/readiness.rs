use std::ffi::OsStr;
use std::io;
use std::os::linux::net::SocketAddrExt as _;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::net::{SocketAddr, UnixDatagram};
use std::path::Path;

pub fn ready() {
    let Some(path) = std::env::var_os("NOTIFY_SOCKET") else {
        return;
    };
    if let Err(error) = send_ready(&path) {
        tracing::error!(%error, "cannot report startup readiness to systemd");
    }
}

fn send_ready(path: &OsStr) -> io::Result<()> {
    let address = match path.as_bytes().strip_prefix(b"@") {
        Some(name) => SocketAddr::from_abstract_name(name)?,
        None => SocketAddr::from_pathname(Path::new(path))?,
    };
    let socket = UnixDatagram::unbound()?;
    socket.send_to_addr(b"READY=1", &address)?;
    Ok(())
}

#[cfg(test)]
mod tests;
