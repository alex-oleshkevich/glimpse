pub fn report_session_bus_loss() {
    match gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
        Ok(connection) => {
            eprintln!("PROBE: got session connection, exit_on_close={}", connection.is_exit_on_close());
            connection.connect_closed(|_, peer_vanished, error| {
                eprintln!("PROBE: closed fired vanished={peer_vanished} err={error:?}");
                tracing::error!(
                    peer_vanished,
                    reason = error.map(glib::Error::to_string).unwrap_or_default(),
                    "the session bus closed; GLib ends this process with SIGTERM"
                );
            });
        }
        Err(error) => eprintln!("PROBE: bus_get_sync FAILED: {error}"),
    }
}
