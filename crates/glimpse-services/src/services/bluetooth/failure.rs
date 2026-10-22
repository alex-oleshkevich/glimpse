use glimpse_dbus::bluez::DisconnectReason;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Connect,
    Disconnect,
    Pair,
    Forget,
    Trust,
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
        "busy" | "too-many-connections" | "create-socket" => Failure::Busy,
        "canceled" | "aborted-by-remote" => Failure::Dropped,
        "adapter-not-powered" => Failure::NotReady,
        "bad-socket" | "memory-allocation" | "invalid-source" | "invalid-argument"
        | "invalid-arguments" => Failure::LocalSetup,
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
mod tests;
