use super::*;

#[test]
fn a_compositor_without_the_protocol_has_its_own_code() {
    let error = anyhow::Error::new(Unavailable::Unsupported)
        .context("cannot take gamma control")
        .context("while starting");

    assert_eq!(exit(&error), Exit::Gamma);
}

#[test]
fn a_compositor_that_is_merely_not_up_yet_stays_retryable() {
    let error = anyhow::Error::new(Unavailable::Unreachable(
        "No such file or directory".to_owned(),
    ))
    .context("cannot take gamma control");

    assert_eq!(exit(&error), Exit::Failed);
}
