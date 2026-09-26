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
    Discoverable,
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
    LocalSetup,
    Unknown,
}

const NO_DISCOVERY: &str = "No discovery started";

pub fn classify(action: Action, error: &zbus::Error) -> Result<(), Failure> {
    let zbus::Error::MethodError(name, detail, _) = error else {
        return Err(Failure::Unknown);
    };

    let name = name.as_str().rsplit('.').next().unwrap_or_default();
    let token = detail.as_deref().unwrap_or_default();

    match (action, name) {
        (Action::Scan, "InProgress") => {
            tracing::warn!(
                detail = token,
                "bluez says a discovery is already starting; if the adapter is not discovering, \
                 bluetoothd is stuck and nothing will be found until it is restarted"
            );
            return Ok(());
        }
        (Action::Connect, "AlreadyConnected")
        | (Action::Pair, "AlreadyExists")
        | (Action::Forget, "DoesNotExist") => return Ok(()),
        (Action::Scan, "Failed") if token == NO_DISCOVERY => return Ok(()),
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
        "ConnectionAttemptFailed" | "ConnectFailed" => match from_token(token) {
            Failure::Unknown => Failure::Unreachable,
            told => told,
        },
        _ => from_token(token),
    })
}

fn from_token(token: &str) -> Failure {
    let Some(reason) = token
        .strip_prefix("br-connection-")
        .or_else(|| token.strip_prefix("le-connection-"))
    else {
        return Failure::Unknown;
    };

    match reason {
        "page-timeout" | "timeout" | "sync-timeout" => Failure::Unreachable,
        "profile-unavailable" | "not-supported" | "not-suported" => Failure::NoService,
        "refused" => Failure::Refused,
        "key-missing" => Failure::BondBroken,
        "busy" | "too-many-connections" => Failure::Busy,
        "canceled" | "aborted-by-remote" => Failure::Dropped,
        "adapter-not-powered" => Failure::NotReady,
        "create-socket" | "bad-socket" | "memory-allocation" | "invalid-source"
        | "invalid-argument" | "invalid-arguments" => Failure::LocalSetup,
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
