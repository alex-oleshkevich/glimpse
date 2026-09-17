use glimpse_dbus::network_manager as nm;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Connect,
    Disconnect,
    Forget,
    Scan,
    Radio,
    Autoconnect,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    NeedSecrets,
    WrongKey,
    NotFound,
    Timeout,
    ConfigFailed,
    Dropped,
    Removed,
    NoDevice,
    Refused,
    Unknown,
}

pub fn from_device(reason: u32, wireless: bool) -> Result<(), Failure> {
    Err(match reason {
        0 | 1 | 2 | 39 | 40 | 60 => return Ok(()),
        4 | 6 => Failure::ConfigFailed,
        7 => Failure::NeedSecrets,
        8 => Failure::Dropped,
        9 | 10 => Failure::WrongKey,
        11 => Failure::Timeout,
        38 => Failure::Removed,
        53 if wireless => Failure::NotFound,
        _ => Failure::Unknown,
    })
}

pub fn from_active(reason: u32) -> Result<(), Failure> {
    Err(match reason {
        0..=3 => return Ok(()),
        4 | 12 => Failure::Dropped,
        5 | 8 => Failure::ConfigFailed,
        6 | 7 => Failure::Timeout,
        9 => Failure::NeedSecrets,
        10 => Failure::WrongKey,
        11 => Failure::Removed,
        13 | 14 => Failure::NoDevice,
        _ => Failure::Unknown,
    })
}

pub fn classify(action: Action, error: &zbus::Error) -> Result<(), Failure> {
    let zbus::Error::MethodError(name, detail, _) = error else {
        return Err(Failure::Unknown);
    };

    let name = name.as_str().rsplit('.').next().unwrap_or_default();
    let detail = detail.as_deref().unwrap_or_default();

    match (action, name) {
        (Action::Scan, "NotAllowed") => return Ok(()),
        (Action::Scan, _)
            if detail.contains("too soon") || detail.contains("Scanning not allowed") =>
        {
            return Ok(());
        }
        (Action::Disconnect, "NotActive") | (Action::Forget, "UnknownConnection") => return Ok(()),
        _ => {}
    }

    Err(match name {
        "NoSecrets" | "SecretsRequired" => Failure::NeedSecrets,
        "UnknownDevice" | "DeviceNotFound" => Failure::NoDevice,
        "UnknownConnection" => Failure::Removed,
        "NotAllowed" | "PermissionDenied" => Failure::Refused,
        "InvalidProperty" | "MissingProperty" | "InvalidSetting" => Failure::ConfigFailed,
        _ => Failure::Unknown,
    })
}

pub fn from_vpn(state: nm::VpnState) -> Result<(), Failure> {
    match state {
        nm::VpnState::Failed => Err(Failure::ConfigFailed),
        nm::VpnState::NeedAuth => Err(Failure::NeedSecrets),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn method(name: &str, detail: Option<&str>) -> zbus::Error {
        zbus::Error::MethodError(
            zbus::names::OwnedErrorName::try_from(format!("org.freedesktop.NetworkManager.{name}"))
                .expect("a well-formed error name"),
            detail.map(ToOwned::to_owned),
            zbus::message::Message::method_call("/", "x")
                .and_then(|builder| builder.build(&()))
                .expect("a message"),
        )
    }

    #[test]
    fn a_user_requested_device_disconnect_is_not_a_failure() {
        assert_eq!(
            from_device(39, true),
            Ok(()),
            "39 is USER_REQUESTED; _old mapped it as a dependency failure and reported it"
        );
        assert_eq!(from_device(40, true), Ok(()), "40 is CARRIER");
        assert_eq!(from_device(60, true), Ok(()), "60 is NEW_ACTIVATION");
    }

    #[test]
    fn a_device_reason_we_do_not_recognise_is_still_reported() {
        assert_eq!(
            from_device(9999, true),
            Err(Failure::Unknown),
            "the caller only asks once the device failed, so silence would lose the failure"
        );
    }

    #[test]
    fn leaving_a_network_is_never_a_failure_however_the_leaving_is_spelled() {
        assert_eq!(from_active(2), Ok(()), "2 is USER_DISCONNECTED");
        assert_eq!(
            from_active(3),
            Ok(()),
            "3 is DEVICE_DISCONNECTED, which is what switching networks produces"
        );
        assert_eq!(
            from_active(4),
            Err(Failure::Dropped),
            "4 is SERVICE_STOPPED, which nothing the user did explains"
        );
    }

    #[test]
    fn a_device_that_is_gone_is_not_a_connection_that_dropped() {
        assert_eq!(from_active(13), Err(Failure::NoDevice), "REALIZE_FAILED");
        assert_eq!(from_active(14), Err(Failure::NoDevice), "DEVICE_REMOVED");
    }

    #[test]
    fn a_wrong_password_reaches_the_user_from_either_reason_source() {
        assert_eq!(from_device(9, true), Err(Failure::WrongKey));
        assert_eq!(from_device(10, true), Err(Failure::WrongKey));
        assert_eq!(from_active(10), Err(Failure::WrongKey));
    }

    #[test]
    fn missing_secrets_are_distinct_from_a_wrong_one() {
        assert_eq!(from_device(7, true), Err(Failure::NeedSecrets));
        assert_eq!(from_active(9), Err(Failure::NeedSecrets));
    }

    #[test]
    fn an_ssid_that_is_not_there_is_only_a_wireless_failure() {
        assert_eq!(from_device(53, true), Err(Failure::NotFound));
        assert_eq!(
            from_device(53, false),
            Err(Failure::Unknown),
            "53 is not an SSID problem on a wired device, but the device still failed"
        );
    }

    #[test]
    fn both_timeout_reasons_are_timeouts() {
        assert_eq!(from_device(11, true), Err(Failure::Timeout));
        assert_eq!(from_active(6), Err(Failure::Timeout));
        assert_eq!(from_active(7), Err(Failure::Timeout));
    }

    #[test]
    fn a_rate_limited_scan_is_routine_and_never_reported() {
        assert_eq!(classify(Action::Scan, &method("NotAllowed", None)), Ok(()));
        assert_eq!(
            classify(
                Action::Scan,
                &method(
                    "Failed",
                    Some("Scanning not allowed immediately following previous scan")
                )
            ),
            Ok(()),
            "NM throttles RequestScan; asking too soon is not a failure"
        );
    }

    #[test]
    fn a_rate_limit_error_on_anything_else_is_still_a_failure() {
        assert_eq!(
            classify(Action::Connect, &method("NotAllowed", None)),
            Err(Failure::Refused),
            "only a scan gets the benefit of the doubt"
        );
    }

    #[test]
    fn disconnecting_something_already_down_and_forgetting_something_gone_both_succeed() {
        assert_eq!(
            classify(Action::Disconnect, &method("NotActive", None)),
            Ok(())
        );
        assert_eq!(
            classify(Action::Forget, &method("UnknownConnection", None)),
            Ok(())
        );
    }

    #[test]
    fn a_transport_error_that_is_not_a_method_error_is_unknown_rather_than_a_panic() {
        assert_eq!(
            classify(Action::Connect, &zbus::Error::InvalidField),
            Err(Failure::Unknown)
        );
    }

    #[test]
    fn a_vpn_needing_auth_is_not_the_same_as_one_that_failed() {
        assert_eq!(from_vpn(nm::VpnState::NeedAuth), Err(Failure::NeedSecrets));
        assert_eq!(from_vpn(nm::VpnState::Failed), Err(Failure::ConfigFailed));
        assert_eq!(from_vpn(nm::VpnState::Activated), Ok(()));
    }
}
