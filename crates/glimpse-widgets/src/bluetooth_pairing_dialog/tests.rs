use super::*;
use imp::{CANCEL, accepts};

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
