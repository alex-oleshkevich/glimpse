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
mod tests;
