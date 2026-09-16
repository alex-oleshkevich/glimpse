use glimpse_dbus::bluez::DisconnectReason;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Connect,
    Disconnect,
    Pair,
    Forget,
    Trust,
    CancelPairing,
    Scan,
    Power,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Unreachable,
    Refused,
    NoService,
    WrongKey,
    PairingRejected,
    PairingTimeout,
    PairingCanceled,
    NoAgent,
    Busy,
    NotReady,
    Dropped,
    BondBroken,
    Unknown,
}

pub fn classify(action: Action, error: &zbus::Error) -> Result<(), Failure> {
    let zbus::Error::MethodError(name, detail, _) = error else {
        return Err(Failure::Unknown);
    };

    let name = name.as_str().rsplit('.').next().unwrap_or_default();
    let token = detail.as_deref().unwrap_or_default();

    match (action, name) {
        (Action::Connect, "AlreadyConnected")
        | (Action::Pair, "AlreadyExists")
        | (Action::Scan, "InProgress")
        | (Action::Forget, "DoesNotExist") => return Ok(()),
        _ => {}
    }

    Err(match name {
        "NotReady" => Failure::NotReady,
        "InProgress" => Failure::Busy,
        "AuthenticationFailed" => Failure::WrongKey,
        "AuthenticationRejected" => Failure::PairingRejected,
        "AuthenticationTimeout" => Failure::PairingTimeout,
        "AuthenticationCanceled" => Failure::PairingCanceled,
        "ProfileUnavailable" | "NotAvailable" | "NotSupported" => Failure::NoService,
        _ => from_token(token),
    })
}

fn from_token(token: &str) -> Failure {
    match token {
        "br-connection-page-timeout" | "le-connection-timeout" => Failure::Unreachable,
        "br-connection-profile-unavailable" => Failure::NoService,
        "br-connection-refused" => Failure::Refused,
        "br-connection-key-missing" => Failure::BondBroken,
        "br-connection-busy" | "le-connection-busy" => Failure::Busy,
        "br-connection-canceled" | "le-connection-canceled" => Failure::Dropped,
        _ => Failure::Unknown,
    }
}

pub fn dropped(reason: DisconnectReason) -> Option<Failure> {
    match reason {
        DisconnectReason::Local | DisconnectReason::Suspend => None,
        DisconnectReason::Timeout | DisconnectReason::Remote => Some(Failure::Dropped),
        DisconnectReason::Authentication => Some(Failure::BondBroken),
        DisconnectReason::Unknown => Some(Failure::Unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn the_four_non_failures_are_not_failures() {
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
}
