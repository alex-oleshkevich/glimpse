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
mod tests;
