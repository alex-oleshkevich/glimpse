mod heartbeat;

use glimpse_config::AppletKind;

use crate::applet::Applet;
use crate::applet::runtime::Builder;
use heartbeat::Heartbeat;

pub fn resolve(name: &str) -> Option<Builder> {
    let Some(kind) = AppletKind::from_name(name) else {
        tracing::warn!(applet = name, "unknown applet, skipping");
        return None;
    };
    let builder = build(kind);
    if builder.is_none() {
        tracing::debug!(applet = name, "applet is not implemented yet, skipping");
    }
    builder
}

fn build(kind: AppletKind) -> Option<Builder> {
    match kind {
        AppletKind::Heartbeat => Some(|| Box::new(Heartbeat::start())),
        AppletKind::Audio
        | AppletKind::Battery
        | AppletKind::Brightness
        | AppletKind::Bluetooth
        | AppletKind::Display
        | AppletKind::Clipboard
        | AppletKind::Clock
        | AppletKind::Command
        | AppletKind::Exec
        | AppletKind::Idle
        | AppletKind::Keyboard
        | AppletKind::Mpris
        | AppletKind::Network
        | AppletKind::NextEvent
        | AppletKind::Notifications
        | AppletKind::Pager
        | AppletKind::Privacy
        | AppletKind::Printing
        | AppletKind::Removable
        | AppletKind::Session
        | AppletKind::Tray
        | AppletKind::Weather
        | AppletKind::Window
        | AppletKind::Workspace => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_the_panel_implements_resolves_to_a_builder() {
        assert!(resolve("heartbeat").is_some());
    }

    #[test]
    fn a_kind_without_an_implementation_is_not_the_same_as_a_typo() {
        assert!(
            AppletKind::from_name("clock").is_some(),
            "`clock` is a real kind, so skipping it is expected rather than a bad document"
        );
        assert!(build(AppletKind::Clock).is_none());
        assert!(AppletKind::from_name("nonesuch").is_none());
        assert!(resolve("nonesuch").is_none());
    }
}
