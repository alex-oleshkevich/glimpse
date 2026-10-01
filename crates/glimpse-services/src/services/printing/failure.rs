use ipp::error::IppError;
use ipp::model::StatusCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Cancel,
    Pause,
    Resume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Unreachable,
    NotFound,
    Refused,
    Unknown,
}

pub fn classify(action: Action, error: &IppError) -> Result<(), Failure> {
    if let IppError::StatusError(status) = error {
        match (action, status) {
            (Action::Cancel, StatusCode::ClientErrorNotFound) => return Ok(()),
            (Action::Resume, StatusCode::ClientErrorNotPossible) => return Ok(()),
            _ => {}
        }
    }

    Err(match error {
        IppError::StatusError(status) => match status {
            StatusCode::ClientErrorNotFound => Failure::NotFound,
            StatusCode::ClientErrorForbidden
            | StatusCode::ClientErrorNotAuthorized
            | StatusCode::ClientErrorNotAuthenticated
            | StatusCode::ClientErrorNotPossible => Failure::Refused,
            StatusCode::ServerErrorServiceUnavailable
            | StatusCode::ServerErrorDeviceError
            | StatusCode::ServerErrorTemporaryError
            | StatusCode::ServerErrorBusy => Failure::Unreachable,
            _ => Failure::Unknown,
        },
        IppError::RequestError(401 | 403) => Failure::Refused,
        IppError::RequestError(404) => Failure::NotFound,
        IppError::RequestError(_) | IppError::IoError(_) => Failure::Unreachable,
        IppError::AsyncClientError(inner) if inner.is_connect() || inner.is_timeout() => {
            Failure::Unreachable
        }
        _ => Failure::Unknown,
    })
}

#[cfg(test)]
mod tests;
