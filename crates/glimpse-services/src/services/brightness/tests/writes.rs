use super::*;

#[tokio::test]
async fn set_level_clamps_to_the_floor_rather_than_reaching_zero() {
    let mut harness = harness(vec![entry("panel", "raw", 400_000, 400_000, None)], 3).await;
    let (reply, result) = oneshot::channel();

    harness
        .feed(Input::Command(Command::SetLevel {
            id: "panel".to_owned(),
            value: 0,
            reply,
        }))
        .await;
    harness.settle_next().await;

    assert_eq!(harness.backend.writes(), vec![("panel".to_owned(), 12_000)]);
    assert!(result.await.expect("answered").is_ok());
}

#[tokio::test]
async fn adjust_level_clamps_at_the_floor_without_underflow() {
    let mut harness = harness(vec![entry("panel", "raw", 100, 100, None)], 3).await;
    let (reply, result) = oneshot::channel();

    harness
        .feed(Input::Command(Command::AdjustLevel {
            id: "panel".to_owned(),
            delta: -250,
            reply,
        }))
        .await;
    harness.settle_next().await;

    assert_eq!(harness.backend.writes(), vec![("panel".to_owned(), 3)]);
    assert!(result.await.expect("answered").is_ok());
}

#[tokio::test]
async fn a_middle_value_is_dropped_while_a_write_is_in_flight() {
    let mut harness = harness(vec![entry("panel", "raw", 50, 100, None)], 0).await;
    let (reply1, result1) = oneshot::channel();
    let (reply2, result2) = oneshot::channel();
    let (reply3, result3) = oneshot::channel();

    harness
        .feed(Input::Command(Command::SetLevel {
            id: "panel".to_owned(),
            value: 10,
            reply: reply1,
        }))
        .await;
    harness
        .feed(Input::Command(Command::SetLevel {
            id: "panel".to_owned(),
            value: 20,
            reply: reply2,
        }))
        .await;
    harness
        .feed(Input::Command(Command::SetLevel {
            id: "panel".to_owned(),
            value: 30,
            reply: reply3,
        }))
        .await;

    assert!(matches!(
        result2.await.expect("answered"),
        Err(CommandError::Unavailable(_))
    ));

    harness.settle_next().await;
    harness.settle_next().await;

    assert_eq!(
        harness.backend.writes(),
        vec![("panel".to_owned(), 10), ("panel".to_owned(), 30)]
    );
    assert!(result1.await.expect("answered").is_ok());
    assert!(result3.await.expect("answered").is_ok());
}

#[tokio::test]
async fn an_unknown_id_is_rejected_without_reaching_the_backend() {
    let mut harness = harness(vec![entry("panel", "raw", 100, 100, None)], 3).await;
    let (reply, result) = oneshot::channel();

    harness
        .feed(Input::Command(Command::SetLevel {
            id: "missing".to_owned(),
            value: 50,
            reply,
        }))
        .await;

    assert!(matches!(
        result.await.expect("answered"),
        Err(CommandError::InvalidArgument(_))
    ));
    assert!(harness.backend.writes().is_empty());
}

#[tokio::test]
async fn a_failed_write_answers_an_error_and_current_is_reconciled_back() {
    let mut harness = harness(vec![entry("panel", "raw", 50, 100, None)], 3).await;
    harness.backend.fail(Some("no backlight control"));
    let (reply, result) = oneshot::channel();

    harness
        .feed(Input::Command(Command::SetLevel {
            id: "panel".to_owned(),
            value: 80,
            reply,
        }))
        .await;
    assert_eq!(harness.state.borrow().sources[0].current, 80);

    harness.settle_next().await;

    assert!(matches!(
        result.await.expect("answered"),
        Err(CommandError::Internal(_))
    ));
    assert_eq!(harness.state.borrow().sources[0].current, 50);
}

#[tokio::test]
async fn a_success_after_a_failed_in_flight_write_lands_the_queued_value_in_current() {
    let mut harness = harness(vec![entry("panel", "raw", 50, 100, None)], 3).await;
    harness.backend.fail(Some("transient logind hiccup"));
    let (reply1, result1) = oneshot::channel();
    let (reply2, result2) = oneshot::channel();

    harness
        .feed(Input::Command(Command::SetLevel {
            id: "panel".to_owned(),
            value: 80,
            reply: reply1,
        }))
        .await;
    harness
        .feed(Input::Command(Command::SetLevel {
            id: "panel".to_owned(),
            value: 90,
            reply: reply2,
        }))
        .await;

    harness.settle_next().await;
    harness.backend.fail(None);
    harness.settle_next().await;

    assert_eq!(harness.state.borrow().sources[0].current, 90);
    assert!(matches!(
        result1.await.expect("answered"),
        Err(CommandError::Internal(_))
    ));
    assert!(result2.await.expect("answered").is_ok());
}

#[tokio::test]
async fn a_success_while_a_newer_value_is_queued_does_not_snap_current_backwards() {
    let mut harness = harness(vec![entry("panel", "raw", 50, 100, None)], 3).await;
    let (reply1, result1) = oneshot::channel();
    let (reply2, result2) = oneshot::channel();

    harness
        .feed(Input::Command(Command::SetLevel {
            id: "panel".to_owned(),
            value: 80,
            reply: reply1,
        }))
        .await;
    harness
        .feed(Input::Command(Command::SetLevel {
            id: "panel".to_owned(),
            value: 90,
            reply: reply2,
        }))
        .await;

    harness.settle_next().await;

    assert_eq!(harness.state.borrow().sources[0].current, 90);

    harness.settle_next().await;

    assert!(result1.await.expect("answered").is_ok());
    assert!(result2.await.expect("answered").is_ok());
}

#[tokio::test]
async fn raising_the_minimum_writes_up_a_source_left_below_the_new_floor() {
    let mut harness = harness(vec![entry("panel", "raw", 10, 100, None)], 3).await;

    harness.feed(Input::Config(Config { minimum: 50 })).await;
    harness.settle_next().await;

    assert_eq!(harness.backend.writes(), vec![("panel".to_owned(), 50)]);
    assert_eq!(harness.state.borrow().sources[0].current, 50);
}
