use glimpse_dbus::notifications::NotificationsProvider;
use glimpse_services::{KeyboardLayout, Running};

use super::*;
use crate::applet::runtime::HostInput;
use crate::applet::testing::{driven, run_in};

fn layout(code: &str) -> KeyboardLayout {
    KeyboardLayout {
        code: code.to_owned(),
        name: code.to_owned(),
    }
}

#[tokio::test]
async fn a_refused_layout_switch_puts_the_badge_back() {
    let (stopped, keyboard) = Running::<glimpse_services::Keyboard>::build(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    drop(stopped);
    let (sender, inbox) = relm4::channel();
    let ctx = Ctx::new("keyboard".into(), None, sender);
    let mut applet = Keyboard::start(
        keyboard.clone(),
        NotificationsProvider::unavailable("no bus in tests").handle(),
    );
    applet.layouts = Some(KeyboardLayouts {
        layouts: vec![layout("us"), layout("de")],
        current: Some(0),
        restored: false,
    });
    let context = glib::MainContext::new();

    run_in(&context, || {
        applet.handle(&ctx, &Input::Pointer(Pointer::Scroll(Direction::Down)))
    });
    assert_eq!(
        applet.layouts.as_ref().and_then(|layouts| layouts.current),
        Some(1),
        "the badge advances before the backend answers"
    );

    assert!(matches!(
        driven(&context, inbox.recv()).await,
        Some(HostInput::Woken)
    ));
    applet.handle(&ctx, &Input::Woken);
    assert_eq!(
        applet.layouts,
        Some(keyboard.snapshot()),
        "a refused switch leaves the chip on what the service reports"
    );
}
