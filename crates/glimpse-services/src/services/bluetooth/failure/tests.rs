use super::*;

#[test]
fn a_failed_attempt_reads_its_token_first_and_falls_back_to_out_of_range() {
    assert_eq!(
        classify(Action::Pair, &method("ConnectionAttemptFailed", None)),
        Err(Failure::Unreachable),
        "BlueZ raises this with no token when it never reached the device, and \
             `Unknown` told the viewer nothing at all"
    );
    assert_eq!(
        classify(
            Action::Connect,
            &method("ConnectionAttemptFailed", Some("br-connection-refused")),
        ),
        Err(Failure::Refused),
        "a token that names the reason still outranks the fallback"
    );
    assert_eq!(
        classify(Action::Connect, &method("ConnectFailed", None)),
        Err(Failure::Unreachable)
    );
}

fn method(name: &str, detail: Option<&str>) -> zbus::Error {
    zbus::Error::MethodError(
        zbus::names::OwnedErrorName::try_from(format!("org.bluez.Error.{name}"))
            .expect("a well-formed error name"),
        detail.map(ToOwned::to_owned),
        zbus::message::Message::method_call("/org/bluez", "Whatever")
            .expect("a call")
            .build(&())
            .expect("a message"),
    )
}

#[test]
fn the_five_non_failures_are_not_failures() {
    assert_eq!(
        classify(Action::Connect, &method("AlreadyConnected", None)),
        Ok(())
    );
    assert_eq!(
        classify(Action::Pair, &method("AlreadyExists", None)),
        Ok(())
    );
    assert_eq!(classify(Action::Scan, &method("InProgress", None)), Ok(()));
    assert_eq!(
        classify(Action::Forget, &method("DoesNotExist", None)),
        Ok(())
    );
    assert_eq!(
        classify(Action::Scan, &method("Failed", Some(NO_DISCOVERY))),
        Ok(()),
        "the deadline stops the scan first, and the popover closing stops it again"
    );
}

#[test]
fn a_failed_scan_that_names_no_stopped_discovery_is_still_a_failure() {
    assert_eq!(
        classify(Action::Scan, &method("Failed", Some("br-connection-busy"))),
        Err(Failure::Busy)
    );
}

#[test]
fn a_non_failure_for_one_action_is_still_a_failure_for_another() {
    assert_eq!(
        classify(Action::Connect, &method("InProgress", None)),
        Err(Failure::Busy)
    );
}

#[test]
fn a_page_timeout_says_the_device_is_away_rather_than_naming_the_token() {
    assert_eq!(
        classify(
            Action::Connect,
            &method("Failed", Some("br-connection-page-timeout"))
        ),
        Err(Failure::Unreachable)
    );
}

#[test]
fn every_token_classifies_into_a_variant_that_has_its_own_wording() {
    for token in [
        "br-connection-profile-unavailable",
        "br-connection-refused",
        "br-connection-busy",
        "br-connection-canceled",
        "le-connection-timeout",
        "something-bluez-invents-next-year",
    ] {
        let failure =
            classify(Action::Connect, &method("Failed", Some(token))).expect_err("a failure");
        assert!(
            !format!("{failure:?}").contains(token),
            "{token} survived classification"
        );
    }
}

#[test]
fn a_named_pairing_error_outranks_its_token() {
    assert_eq!(
        classify(
            Action::Pair,
            &method("AuthenticationFailed", Some("br-connection-refused"))
        ),
        Err(Failure::WrongKey)
    );
}

#[test]
fn a_missing_link_key_says_the_pairing_is_gone_rather_than_refused() {
    assert_eq!(
        classify(
            Action::Connect,
            &method("Failed", Some("br-connection-key-missing"))
        ),
        Err(Failure::BondBroken),
        "the remote forgot the bond; refusing is a different thing and a different fix"
    );
}

#[test]
fn a_profile_the_device_rejects_says_busy_rather_than_blaming_this_computer() {
    assert_eq!(
        classify(
            Action::Connect,
            &method("Failed", Some("br-connection-create-socket"))
        ),
        Err(Failure::Busy),
        "bluez reports any EIO from the profile connect this way, and headphones held by \
             another host reject the stream with exactly that"
    );
}

#[test]
fn a_local_disconnect_is_not_a_failure_and_a_remote_one_is() {
    assert_eq!(dropped(DisconnectReason::Local), None);
    assert_eq!(dropped(DisconnectReason::Suspend), None);
    assert_eq!(dropped(DisconnectReason::Timeout), Some(Failure::Dropped));
    assert_eq!(
        dropped(DisconnectReason::Authentication),
        Some(Failure::BondBroken)
    );
}

#[test]
fn a_transport_error_is_not_read_as_a_named_one() {
    assert_eq!(
        classify(Action::Connect, &zbus::Error::InvalidField),
        Err(Failure::Unknown)
    );
}
