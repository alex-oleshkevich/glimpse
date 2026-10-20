use super::*;
use crate::app::AppInput;
use glimpse_services::Confirmation;

async fn raised(dialog: &relm4::Sender<AppInput>, inbox: &relm4::Receiver<AppInput>) -> usize {
    let _ = dialog.send(AppInput::MonitorsChanged);
    let mut seen = 0;
    while let Some(input) = inbox.recv().await {
        match input {
            AppInput::Raise("bluetooth") => seen += 1,
            AppInput::MonitorsChanged => break,
            _ => {}
        }
    }
    seen
}

fn forget() -> BluetoothState {
    BluetoothState {
        confirm: Some(Confirmation::Forget {
            device: DeviceId::new("/dev/1"),
            connected: false,
        }),
        ..BluetoothState::default()
    }
}

async fn applets(
    count: usize,
) -> (
    glimpse_services::Running<glimpse_services::Bluetooth>,
    Vec<Bluetooth>,
    relm4::Receiver<AppInput>,
    relm4::Sender<AppInput>,
) {
    let (service, handle) = glimpse_services::Running::<glimpse_services::Bluetooth>::spawn(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
        glimpse_services::BluetoothDependencies { agent: false },
    );
    let notifications =
        glimpse_dbus::notifications::NotificationsProvider::unavailable("no bus in tests").handle();
    let (dialog, inbox) = relm4::channel();
    let bars = (0..count)
        .map(|_| Bluetooth::start(handle.clone(), notifications.clone(), dialog.clone()))
        .collect();
    (service, bars, inbox, dialog)
}

#[tokio::test]
async fn a_forget_confirmation_never_raises_a_popover() {
    let (mut service, mut bars, inbox, dialog) = applets(2).await;

    for bar in &mut bars {
        bar.state = forget();
        bar.raise();
    }

    assert_eq!(raised(&dialog, &inbox).await, 0);
    service.stop().await;
}

#[tokio::test]
async fn a_pairing_prompt_asks_the_panel_to_raise_it() {
    let (mut service, mut bars, inbox, dialog) = applets(1).await;

    bars[0].state.pairing = Some(glimpse_services::Prompt::Authorize(DeviceId::new("/dev/1")));
    bars[0].raise();

    assert_eq!(raised(&dialog, &inbox).await, 1);
    service.stop().await;
}
