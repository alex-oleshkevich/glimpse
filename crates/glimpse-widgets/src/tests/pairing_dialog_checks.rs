use super::*;

pub(super) fn check() {
    let dialog = PairingDialog::new();
    let answers = Rc::new(RefCell::new(Vec::new()));
    dialog.connect_answered({
        let answers = Rc::clone(&answers);
        move |_, answer| answers.borrow_mut().push(answer)
    });

    let rewrites = Rc::new(Cell::new(0u32));
    dialog.connect_heading_notify({
        let rewrites = Rc::clone(&rewrites);
        move |_| rewrites.set(rewrites.get() + 1)
    });

    const KEYBOARD: &str = "/org/bluez/hci0/dev_K3";

    dialog.ask(KEYBOARD, "Keychron K3", PairingEntry::Pin, "");
    assert_eq!(rewrites.get(), 1);
    dialog.ask(KEYBOARD, "Keychron K3", PairingEntry::Pin, "");
    assert_eq!(
        rewrites.get(),
        1,
        "an unchanged heading must not be written again"
    );
    assert!(dialog.imp().entry.get_visible());
    assert!(
        !dialog.is_response_enabled("ok"),
        "an empty PIN cannot be sent"
    );
    dialog.imp().entry.set_text("0000");
    assert!(dialog.is_response_enabled("ok"));
    dialog.ask(KEYBOARD, "Keychron K3 Keyboard", PairingEntry::Pin, "");
    assert_eq!(
        dialog.imp().entry.text(),
        "0000",
        "bluez resolving the device name re-asks, and must not wipe a half-typed PIN"
    );
    dialog.ask(
        "/org/bluez/hci0/dev_OTHER",
        "Pixel 9 Pro",
        PairingEntry::Pin,
        "",
    );
    assert!(
        dialog.imp().entry.text().is_empty(),
        "a second device asking must not inherit the PIN typed for the first"
    );
    assert!(!dialog.is_response_enabled("ok"));
    dialog.ask(KEYBOARD, "Keychron K3", PairingEntry::Pin, "");
    dialog.imp().entry.set_text("0000");
    dialog.imp().entry.set_max_length(0);
    dialog.imp().entry.set_text(&"a".repeat(PIN_MAX + 1));
    assert!(
        !dialog.is_response_enabled("ok"),
        "seventeen characters is one past what BlueZ takes"
    );
    assert!(dialog.imp().entry.has_css_class("error"));

    dialog.ask(
        "/org/bluez/hci0/dev_BO_SE",
        "Bose QuietComfort 45",
        PairingEntry::Passkey,
        "",
    );
    assert_eq!(rewrites.get(), 2, "a different prompt does write it");
    dialog.imp().entry.set_max_length(0);
    dialog.imp().entry.set_text("1000000");
    assert!(
        !dialog.is_response_enabled("ok"),
        "a passkey above 999999 cannot be sent"
    );
    dialog.imp().entry.set_text("123456");
    assert!(dialog.is_response_enabled("ok"));
    assert_eq!(dialog.close_response(), "cancel");

    dialog.emit_by_name::<()>("response", &[&"ok".to_owned()]);
    assert_eq!(*answers.borrow(), [PairingAnswer::Passkey(123_456)]);

    answers.borrow_mut().clear();
    dialog.emit_by_name::<()>("response", &[&dialog.close_response().to_string()]);
    assert_eq!(
        *answers.borrow(),
        [PairingAnswer::Deny],
        "Esc closes with the close response, which is a refusal"
    );
}
