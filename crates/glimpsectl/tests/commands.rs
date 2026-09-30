use glimpsectl::commands::{ABSENT, reason_or_absent, within, yes_no};

#[tokio::test(start_paused = true)]
async fn a_request_that_never_answers_is_given_up_on() {
    let never = std::future::pending::<Result<(), zbus::Error>>();

    let error = within(never).await.expect_err("the deadline passes");

    assert!(
        error
            .downcast_ref::<glimpsectl::errors::TimedOut>()
            .is_some(),
        "a hung provider used to hang the command line instead: {error}"
    );
    assert_eq!(
        glimpsectl::errors::exit(&error),
        glimpsectl::errors::Exit::Timeout
    );
}

#[tokio::test(start_paused = true)]
async fn a_request_that_answers_is_not_disturbed_by_the_bound() {
    let answered = within(std::future::ready(Ok::<u8, zbus::Error>(7)))
        .await
        .expect("it answered well inside the deadline");

    assert_eq!(answered, 7);
}

#[test]
fn a_healthy_provider_has_no_reason_to_show() {
    assert_eq!(reason_or_absent(""), ABSENT);
    assert_eq!(reason_or_absent("no location fix"), "no location fix");
}

#[test]
fn a_flag_reads_as_a_word() {
    assert_eq!(yes_no(true), "yes");
    assert_eq!(yes_no(false), "no");
}
