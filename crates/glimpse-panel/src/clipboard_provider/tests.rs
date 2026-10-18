use glimpse_dbus::Buses;
use glimpse_services::{Clipboard, ClipboardConfig, ServiceRuntime};
use tokio_util::sync::CancellationToken;

use super::*;

#[test]
fn an_id_the_history_does_not_hold_is_no_such_entry() {
    let (_runtime, clipboard) = ServiceRuntime::<Clipboard>::new(
        ClipboardConfig::from(&glimpse_config::Config::default()),
        Buses::unavailable("no bus in tests"),
        CancellationToken::new(),
    );
    let provider = Provider { clipboard };

    assert!(provider.list().is_empty());
    assert!(matches!(provider.get(42), Err(Error::NoSuchEntry(_))));
}

#[tokio::test]
async fn empty_text_is_invalid_args() {
    let (_runtime, clipboard) = ServiceRuntime::<Clipboard>::new(
        ClipboardConfig::from(&glimpse_config::Config::default()),
        Buses::unavailable("no bus in tests"),
        CancellationToken::new(),
    );
    let provider = Provider { clipboard };

    assert!(matches!(
        provider.copy_text(String::new()).await,
        Err(Error::InvalidArgs(_))
    ));
}

#[test]
fn a_refused_id_maps_to_no_such_entry_and_anything_else_to_unavailable() {
    assert!(matches!(
        Error::from(CommandError::InvalidArgument("gone".to_owned())),
        Error::NoSuchEntry(_)
    ));
    assert!(matches!(
        Error::from(CommandError::Unavailable("stopped".to_owned())),
        Error::Unavailable(_)
    ));
}
