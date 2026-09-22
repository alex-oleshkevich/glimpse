#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Mount,
    Unmount,
    Eject,
    PowerOff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    NotMounted,
    Busy,
    MountedElsewhere,
    NotAuthorized,
    NotSupported,
    OptionNotPermitted,
    TimedOut,
    WouldWakeup,
    Canceled,
    Unknown,
}

pub fn classify(action: Action, error: &zbus::Error) -> Result<(), Failure> {
    let zbus::Error::MethodError(name, _detail, _) = error else {
        return Err(Failure::Unknown);
    };

    let name = name.as_str().rsplit('.').next().unwrap_or_default();

    match (action, name) {
        (Action::Mount, "AlreadyMounted") => return Ok(()),
        (Action::Unmount, "NotMounted" | "AlreadyUnmounting") => return Ok(()),
        (Action::Eject | Action::PowerOff, "AlreadyCancelled") => return Ok(()),
        _ => {}
    }

    Err(match name {
        "AlreadyMounted" => Failure::MountedElsewhere,
        "AlreadyUnmounting" | "DeviceBusy" => Failure::Busy,
        "MountedByOtherUser" => Failure::MountedElsewhere,
        "NotAuthorized" | "NotAuthorizedCanObtain" | "NotAuthorizedDismissed" => {
            Failure::NotAuthorized
        }
        "NotSupported" => Failure::NotSupported,
        "OptionNotPermitted" => Failure::OptionNotPermitted,
        "Timedout" => Failure::TimedOut,
        "WouldWakeup" => Failure::WouldWakeup,
        "AlreadyCancelled" | "Cancelled" => Failure::Canceled,
        "NotMounted" => Failure::NotMounted,
        _ => Failure::Unknown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn method(name: &str) -> zbus::Error {
        zbus::Error::MethodError(
            zbus::names::OwnedErrorName::try_from(format!("org.freedesktop.UDisks2.Error.{name}"))
                .expect("a well-formed error name"),
            None,
            zbus::message::Message::method_call("/org/freedesktop/UDisks2", "Whatever")
                .expect("a call")
                .build(&())
                .expect("a message"),
        )
    }

    #[test]
    fn mounting_an_already_mounted_volume_is_not_a_failure() {
        assert_eq!(classify(Action::Mount, &method("AlreadyMounted")), Ok(()));
    }

    #[test]
    fn the_same_name_is_still_a_failure_for_an_action_it_was_not_carved_out_for() {
        assert_eq!(
            classify(Action::Eject, &method("AlreadyMounted")),
            Err(Failure::MountedElsewhere)
        );
    }

    #[test]
    fn unmounting_an_unmounted_volume_is_not_a_failure() {
        assert_eq!(classify(Action::Unmount, &method("NotMounted")), Ok(()));
        assert_eq!(
            classify(Action::Unmount, &method("AlreadyUnmounting")),
            Ok(())
        );
    }

    #[test]
    fn a_stop_that_already_stopped_is_not_a_failure() {
        assert_eq!(classify(Action::Eject, &method("AlreadyCancelled")), Ok(()));
        assert_eq!(
            classify(Action::PowerOff, &method("AlreadyCancelled")),
            Ok(())
        );
    }

    #[test]
    fn every_reachable_name_classifies_into_its_own_variant() {
        let cases = [
            ("DeviceBusy", Failure::Busy),
            ("MountedByOtherUser", Failure::MountedElsewhere),
            ("NotAuthorized", Failure::NotAuthorized),
            ("NotAuthorizedCanObtain", Failure::NotAuthorized),
            ("NotAuthorizedDismissed", Failure::NotAuthorized),
            ("NotSupported", Failure::NotSupported),
            ("OptionNotPermitted", Failure::OptionNotPermitted),
            ("Timedout", Failure::TimedOut),
            ("WouldWakeup", Failure::WouldWakeup),
            ("Cancelled", Failure::Canceled),
            ("Failed", Failure::Unknown),
        ];
        for (name, expected) in cases {
            assert_eq!(
                classify(Action::PowerOff, &method(name)),
                Err(expected),
                "{name} must classify to its own variant"
            );
        }
    }

    #[test]
    fn a_transport_error_is_not_read_as_a_named_one() {
        assert_eq!(
            classify(Action::Mount, &zbus::Error::InvalidField),
            Err(Failure::Unknown)
        );
    }
}
