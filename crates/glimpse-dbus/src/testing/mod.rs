pub mod geoclue;
pub mod tray;

use std::io::BufRead as _;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use zbus::Connection;

/// A `dbus-daemon` of this test's own, killed when the value is dropped.
pub struct PrivateBus {
    child: Child,
    address: String,
    scratch: PathBuf,
}

impl PrivateBus {
    pub fn start() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let scratch =
            std::env::temp_dir().join(format!("glimpse-private-bus-{}-{n}", std::process::id()));
        std::fs::create_dir(&scratch).expect("a scratch directory");
        let services = scratch.join("services");
        std::fs::create_dir(&services).expect("an empty service directory");
        let socket = scratch.join("bus");
        let config_path = scratch.join("bus.conf");
        let config = format!(
            "<busconfig>\n\
               <type>session</type>\n\
               <listen>unix:path={socket}</listen>\n\
               <servicedir>{services}</servicedir>\n\
               <policy context='default'>\n\
                 <allow send_destination='*' eavesdrop='true'/>\n\
                 <allow eavesdrop='true'/>\n\
                 <allow own='*'/>\n\
               </policy>\n\
             </busconfig>\n",
            socket = socket.display(),
            services = services.display(),
        );
        std::fs::write(&config_path, config).expect("the bus config");
        let mut child = Command::new("dbus-daemon")
            .arg("--config-file")
            .arg(&config_path)
            .args(["--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-daemon starts");
        Self {
            address: printed_address(&mut child),
            child,
            scratch,
        }
    }

    pub async fn connection(&self) -> Connection {
        zbus::connection::Builder::address(self.address.as_str())
            .expect("a valid address")
            .build()
            .await
            .expect("a connection")
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn service_dir(&self) -> PathBuf {
        self.scratch.join("services")
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
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

fn printed_address(child: &mut Child) -> String {
    let stdout = child.stdout.as_mut().expect("a pipe");
    std::io::BufReader::new(stdout)
        .lines()
        .next()
        .expect("an address")
        .expect("readable")
}
