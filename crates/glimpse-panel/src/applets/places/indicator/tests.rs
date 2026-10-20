use super::*;
use crate::applet::runtime::HostInput;
use gtk4::glib::object::ObjectExt;

#[test]
#[ignore = "needs a display"]
fn opening_a_place_closes_the_popover() {
    gtk4::init().expect("display");
    glimpse_widgets::register_resources().expect("resources");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let _guard = runtime.enter();
    let (mut service, handle) = glimpse_services::Running::<glimpse_services::Places>::spawn(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
        (),
    );
    let notifications =
        glimpse_dbus::notifications::NotificationsProvider::unavailable("no bus in tests").handle();
    let mut applet = Places::start(handle, notifications);
    applet.places_state.borrow_mut().bookmarks = vec![glimpse_services::Place {
        id: "bookmark".to_owned(),
        name: "Projects".to_owned(),
        path: "/tmp".into(),
        kind: glimpse_services::PlacesKind::Bookmark,
    }];
    let (host, inbox) = relm4::channel();
    let seat = Seat::new(host);
    let popover = applet
        .popover(&seat)
        .expect("the applet opens a popover")
        .downcast::<PlacesPopover>()
        .expect("the places applet's popover is its own widget");

    popover.emit_by_name::<()>("activated", &[&"bookmark".to_owned()]);

    seat.opener().wake();
    let mut lowered = false;
    while let Some(input) = inbox.recv_sync() {
        match input {
            HostInput::PopoverLowered => lowered = true,
            HostInput::Woken => break,
            _ => {}
        }
    }
    assert!(lowered);
    drop(_guard);
    runtime.block_on(service.stop());
}
