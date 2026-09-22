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
mod tests {
    use super::*;

    fn status(status: StatusCode) -> IppError {
        IppError::StatusError(status)
    }

    #[test]
    fn cancelling_a_job_that_already_finished_is_not_a_failure() {
        assert_eq!(
            classify(Action::Cancel, &status(StatusCode::ClientErrorNotFound)),
            Ok(()),
            "a 2s poll racing the job's own completion is the normal case, not an error"
        );
    }

    #[test]
    fn pausing_a_missing_job_is_still_a_not_found_failure() {
        assert_eq!(
            classify(Action::Pause, &status(StatusCode::ClientErrorNotFound)),
            Err(Failure::NotFound),
            "the not-found allowance is specific to Cancel, not every action"
        );
    }

    #[test]
    fn releasing_an_already_released_job_is_not_a_failure() {
        assert_eq!(
            classify(Action::Resume, &status(StatusCode::ClientErrorNotPossible)),
            Ok(())
        );
    }

    #[test]
    fn pausing_a_job_that_cannot_be_held_is_still_refused() {
        assert_eq!(
            classify(Action::Pause, &status(StatusCode::ClientErrorNotPossible)),
            Err(Failure::Refused),
            "the not-possible allowance is specific to Resume, not every action"
        );
    }

    #[test]
    fn an_unauthorized_request_is_refused() {
        assert_eq!(
            classify(Action::Pause, &status(StatusCode::ClientErrorNotAuthorized)),
            Err(Failure::Refused)
        );
    }

    #[test]
    fn a_server_busy_status_is_unreachable_rather_than_a_local_problem() {
        assert_eq!(
            classify(Action::Resume, &status(StatusCode::ServerErrorBusy)),
            Err(Failure::Unreachable)
        );
    }

    #[test]
    fn an_unrecognised_status_is_unknown_rather_than_assumed_ok() {
        assert_eq!(
            classify(Action::Cancel, &status(StatusCode::ClientErrorBadRequest)),
            Err(Failure::Unknown)
        );
    }

    #[test]
    fn a_transport_error_with_no_http_status_is_unreachable() {
        assert_eq!(
            classify(Action::Cancel, &IppError::RequestError(500)),
            Err(Failure::Unreachable)
        );
    }

    #[test]
    fn an_http_layer_auth_rejection_is_refused_not_unreachable() {
        for code in [401, 403] {
            assert_eq!(
                classify(Action::Cancel, &IppError::RequestError(code)),
                Err(Failure::Refused),
                "cupsd's default policy needs auth for job ops on a job you do not own; \
                 that is a refusal, not a dead server"
            );
        }
    }

    #[test]
    fn an_http_layer_404_is_not_found() {
        assert_eq!(
            classify(Action::Pause, &IppError::RequestError(404)),
            Err(Failure::NotFound)
        );
    }
}
