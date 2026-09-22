pub mod geoclue;
pub mod tray;

use std::io::BufRead as _;
use std::process::{Child, Command, Stdio};

use zbus::Connection;

/// A `dbus-daemon` of this test's own, killed when the value is dropped.
pub struct PrivateBus {
    child: Child,
    address: String,
}

impl PrivateBus {
    pub fn start() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon starts");
        let stdout = child.stdout.as_mut().expect("a pipe");
        let address = std::io::BufReader::new(stdout)
            .lines()
            .next()
            .expect("an address")
            .expect("readable");
        Self { child, address }
    }

    pub async fn connection(&self) -> Connection {
        zbus::connection::Builder::address(self.address.as_str())
            .expect("a valid address")
            .build()
            .await
            .expect("a connection")
    }

    /// Stop the daemon early, for a test that wants to see a provider lose the bus.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        self.kill();
    }
}
