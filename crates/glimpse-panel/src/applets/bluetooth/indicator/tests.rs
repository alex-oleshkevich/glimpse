use super::*;
use crate::applet::runtime::HostInput;
use glimpse_services::{Confirmation, Prompt};

async fn requests(inbox: &relm4::Receiver<HostInput>, ctx: &Ctx) -> usize {
    ctx.opener().wake();
    let mut seen = 0;
    while let Some(input) = inbox.recv().await {
        match input {
            HostInput::PopoverRequested => seen += 1,
            HostInput::Woken => break,
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

fn pairing(device: &str) -> BluetoothState {
    BluetoothState {
        pairing: Some(Prompt::Authorize(DeviceId::new(device))),
        ..BluetoothState::default()
    }
}

#[tokio::test]
async fn a_forget_confirmation_never_raises_a_popover() {
    let (mut service, handle) = glimpse_services::Running::<glimpse_services::Bluetooth>::spawn(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
        glimpse_services::BluetoothDependencies { agent: false },
    );
    let notifications =
        glimpse_dbus::notifications::NotificationsProvider::unavailable("no bus in tests").handle();
    let (sender, inbox) = relm4::channel();
    let ctx = Ctx::new("bluetooth".into(), None, sender);
    let mut bars: Vec<_> = (0..2)
        .map(|_| Bluetooth::start(handle.clone(), notifications.clone()))
        .collect();

    for bar in &mut bars {
        bar.state = forget();
        bar.raise(&ctx);
    }

    assert_eq!(requests(&inbox, &ctx).await, 0);
    service.stop().await;
}

#[tokio::test]
async fn a_pairing_prompt_raises_one_popover_and_is_claimed_again_once_it_clears() {
    let (mut service, handle) = glimpse_services::Running::<glimpse_services::Bluetooth>::spawn(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
        glimpse_services::BluetoothDependencies { agent: false },
    );
    let notifications =
        glimpse_dbus::notifications::NotificationsProvider::unavailable("no bus in tests").handle();
    let (sender, inbox) = relm4::channel();
    let ctx = Ctx::new("bluetooth".into(), None, sender);
    let mut bars: Vec<_> = (0..2)
        .map(|_| Bluetooth::start(handle.clone(), notifications.clone()))
        .collect();

    for bar in &mut bars {
        bar.state = pairing("/dev/1");
        bar.raise(&ctx);
    }
    assert_eq!(requests(&inbox, &ctx).await, 1);
    assert!(
        bars.iter().all(|bar| bar.raised.is_some()),
        "every bar can show the prompt when its popover is opened"
    );

    for bar in &mut bars {
        bar.state = BluetoothState::default();
        bar.raise(&ctx);
    }
    for bar in &mut bars {
        bar.state = pairing("/dev/1");
        bar.raise(&ctx);
    }
    assert_eq!(requests(&inbox, &ctx).await, 1);
    service.stop().await;
}
