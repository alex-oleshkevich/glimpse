use std::io::BufRead as _;
use std::process::{Child, Command, Stdio};

use zbus::Connection;

pub struct PrivateBus {
    pub child: Child,
    address: String,
}

impl PrivateBus {
    pub fn start() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args([
                "--session",
                "--nofork",
                "--print-address=1",
                "--print-pid=1",
            ])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.as_mut().unwrap();
        let mut lines = std::io::BufReader::new(stdout).lines();
        let address = lines.next().unwrap().unwrap();
        let _pid = lines.next().unwrap().unwrap();
        Self { child, address }
    }

    pub async fn connection(&self) -> Connection {
        zbus::connection::Builder::address(self.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
