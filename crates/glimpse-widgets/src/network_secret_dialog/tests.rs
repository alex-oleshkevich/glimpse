use super::*;
use imp::CANCEL;

#[test]
fn a_wireless_secret_is_held_to_the_passphrase_lengths_and_a_vpn_token_is_not() {
    assert!(!accepts("short", Entered::Passphrase));
    assert!(accepts("hunter2hunter2", Entered::Passphrase));
    assert!(
        accepts(&"a".repeat(100), Entered::Secret),
        "a VPN plugin's token obeys no Wi-Fi length"
    );
    assert!(!accepts("", Entered::Secret));
}

#[test]
fn cancelling_is_a_refusal_and_never_an_empty_password() {
    assert_eq!(
        answer(CANCEL, "", Entered::Passphrase),
        SecretAnswer::Refused
    );
    assert_eq!(
        answer(CANCEL, "hunter2hunter2", Entered::Passphrase),
        SecretAnswer::Refused,
        "a typed secret is discarded when the user says no"
    );
}

#[test]
fn connecting_returns_what_was_typed() {
    assert_eq!(
        answer(CONNECT, "hunter2hunter2", Entered::Passphrase),
        SecretAnswer::Secret("hunter2hunter2".to_owned())
    );
    assert_eq!(
        answer(CONNECT, "", Entered::Passphrase),
        SecretAnswer::Refused,
        "an empty secret must never reach NetworkManager"
    );
}

#[test]
fn a_hostile_network_name_is_capped_by_characters() {
    let name = "Kaffeehaus Freies WLAN Gäste-Zugang ".repeat(6);
    let capped = capped(&name);

    assert_eq!(capped.chars().count(), NAME_MAX);
    assert!(name.starts_with(&capped));
}
