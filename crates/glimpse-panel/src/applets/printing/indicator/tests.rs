use super::*;

#[test]
fn wording_never_leaks_a_raw_failure_token() {
    let told = wording(&PrintingError::Failed(
        glimpse_services::PrintingFailure::Refused,
    ))
    .expect("a failure always has words");
    assert!(!told.is_empty());
}
