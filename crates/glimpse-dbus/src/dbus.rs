use zbus::Connection;

/// The session and system buses, each carrying either a connection or the reason there is none.
///
/// Connecting never fails: a session with no D-Bus still has a panel, a wallpaper and a lock
/// screen, so the daemon starts and each service that needs a bus reports its own `degraded`
/// carrying the reason. That reason is a `String` rather than an error type because `Buses` is
/// cloned into every service and the only thing anyone does with it is put it in that message.
#[derive(Clone)]
pub struct Buses {
    session: Result<Connection, String>,
    system: Result<Connection, String>,
}

impl Buses {
    pub async fn connect() -> Self {
        Self {
            session: bus("session", Connection::session().await),
            system: bus("system", Connection::system().await),
        }
    }

    /// Neither bus, for a test or for a caller that deliberately runs without D-Bus.
    pub fn unavailable(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            session: Err(reason.clone()),
            system: Err(reason),
        }
    }

    pub fn session_bus(&self) -> Result<&Connection, &str> {
        self.session.as_ref().map_err(String::as_str)
    }

    pub fn system_bus(&self) -> Result<&Connection, &str> {
        self.system.as_ref().map_err(String::as_str)
    }
}

fn bus(kind: &'static str, connected: zbus::Result<Connection>) -> Result<Connection, String> {
    match connected {
        Ok(connection) => {
            tracing::info!(bus = kind, "connected");
            Ok(connection)
        }
        Err(error) => {
            tracing::warn!(bus = kind, %error, "unreachable; services needing it will degrade");
            Err(error.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unavailable_bus_reports_why() {
        let buses = Buses::unavailable("no bus in tests");
        assert_eq!(buses.session_bus().unwrap_err(), "no bus in tests");
        assert_eq!(buses.system_bus().unwrap_err(), "no bus in tests");
    }
}
