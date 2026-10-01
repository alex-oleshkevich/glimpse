use ipp::model::StatusCode;

use super::*;

fn response(status: StatusCode) -> IppRequestResponse {
    IppRequestResponse::new_response(IppVersion::v1_1(), status, 1).expect("a response")
}

#[test]
fn a_successful_status_is_accepted() {
    assert!(accept(response(StatusCode::SuccessfulOk)).is_ok());
}

#[test]
fn a_client_error_status_is_a_typed_failure_not_silently_accepted() {
    let outcome = accept(response(StatusCode::ClientErrorNotFound));
    assert!(matches!(
        outcome,
        Err(IppError::StatusError(StatusCode::ClientErrorNotFound))
    ));
}
