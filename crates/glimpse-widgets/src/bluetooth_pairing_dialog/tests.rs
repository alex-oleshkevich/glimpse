use super::*;
use imp::{ACCEPT, CANCEL, OK, accepts};

#[test]
fn a_pin_is_one_to_sixteen_alphanumeric_characters() {
    assert!(accepts(Entry::Pin, "0000"));
    assert!(accepts(Entry::Pin, &"a".repeat(PIN_MAX)));
    assert!(!accepts(Entry::Pin, ""));
    assert!(!accepts(Entry::Pin, &"a".repeat(PIN_MAX + 1)));
    assert!(!accepts(Entry::Pin, "12 34"));
    assert!(!accepts(Entry::Pin, "пин"));
}

#[test]
fn a_passkey_stops_at_six_nines() {
    assert!(accepts(Entry::Passkey, "0"));
    assert!(accepts(Entry::Passkey, "999999"));
    assert!(!accepts(Entry::Passkey, "1000000"));
    assert!(!accepts(Entry::Passkey, ""));
    assert!(!accepts(Entry::Passkey, "-1"));
    assert!(!accepts(Entry::Passkey, "12a"));
}

#[test]
fn a_multibyte_name_is_capped_by_characters() {
    let name = "Наушники ".repeat(20);

    let capped = capped(&name);

    assert_eq!(capped.chars().count(), NAME_MAX);
    assert!(name.starts_with(&capped));
}

#[test]
fn a_typed_answer_is_read_back_from_the_response_and_its_value() {
    assert_eq!(answer(CANCEL, "", false), PairingAnswer::Deny);
    assert_eq!(answer(OK, "123456", true), PairingAnswer::Passkey(123_456));
    assert_eq!(
        answer(OK, "0000", false),
        PairingAnswer::Pin("0000".to_owned()),
        "a numeric PIN is a PIN; only the prompt knows which was asked for"
    );
    assert_eq!(
        answer(OK, "abcd", true),
        PairingAnswer::Deny,
        "the entry cannot enable OK on this, so it can only mean the value went missing"
    );
}

#[test]
fn a_prompt_without_an_entry_answers_with_a_bare_accept() {
    assert_eq!(answer(ACCEPT, "", false), PairingAnswer::Accept);
    assert_eq!(answer(CANCEL, "", false), PairingAnswer::Deny);
    assert!(accepts(Entry::Confirm, ""));
    assert!(!Entry::Confirm.typed() && Entry::Pin.typed() && Entry::Passkey.typed());
}

#[test]
#[ignore = "needs a display"]
fn each_prompt_kind_offers_exactly_its_own_buttons() {
    gtk4::init().expect("display");
    adw::init().expect("adwaita");
    crate::register_resources().expect("resources");

    let dialog = PairingDialog::new();
    let buttons = |kind, code: &str| {
        dialog.ask("/dev/1", "Pixel", kind, code);
        let has_ok = dialog.has_response(OK);
        (
            has_ok,
            dialog.response_label(CANCEL).to_string(),
            dialog.body().to_string(),
        )
    };

    assert_eq!(buttons(Entry::Pin, "").0, true);
    let confirm = buttons(Entry::Confirm, "123 456");
    assert!(confirm.0 && confirm.1 == "Cancel" && confirm.2.contains("123 456"));
    assert_eq!(dialog.response_label(OK), "Confirm");
    let authorize = buttons(Entry::Authorize, "");
    assert!(authorize.0 && authorize.1 == "Deny");
    assert_eq!(dialog.response_label(OK), "Allow");
    let shown = buttons(Entry::ShowPasskey, "123 456\n1 of 6 entered");
    assert!(!shown.0 && shown.1 == "Cancel" && shown.2.contains("1 of 6"));
    assert!(
        buttons(Entry::Pin, "").0,
        "typing a PIN again brings OK back"
    );
}

#[test]
#[ignore = "needs a display"]
fn a_dropped_pairing_dialog_is_finalized() {
    gtk4::init().expect("display");
    adw::init().expect("adwaita");
    crate::register_resources().expect("resources");

    let dialog = PairingDialog::new();
    let weak = dialog.downgrade();
    drop(dialog);

    assert!(weak.upgrade().is_none());
}
