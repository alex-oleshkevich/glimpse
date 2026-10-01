use super::*;

#[tokio::test]
async fn posting_without_an_exported_server_is_refused_rather_than_given_a_private_id() {
    let (handle, _sender, _state, cancel, running) = dnd_service().await;

    let refused = handle.post(incoming("glimpse", "Bluetooth")).await;
    cancel.cancel();
    running.await.expect("joined").expect("stopped");

    assert!(
        matches!(refused, Err(CommandError::Unavailable(_))),
        "ids come from the exported server's counter, so with no server there is no id to \
             hand out and inventing one would collide with the next Notify"
    );
}

#[tokio::test]
async fn do_not_disturb_with_an_expiry_lapses_without_anyone_turning_it_off() {
    let (handle, _sender, mut state, cancel, running) = dnd_service().await;

    handle
        .set_dnd(DoNotDisturb {
            enabled: true,
            until: Some(Utc::now() + chrono::TimeDelta::milliseconds(250)),
        })
        .await
        .expect("do not disturb is set");
    state.wait_for(enabled).await.expect("it is on");

    state
        .wait_for(|state| !enabled(state))
        .await
        .expect("the expiry passes and it turns itself off");

    let latest = handle.snapshot();
    cancel.cancel();
    running.await.expect("joined").expect("stopped");

    let dnd = latest.dnd.expect("do not disturb was published").dnd;
    assert!(!dnd.enabled);
    assert_eq!(dnd.until, None, "a spent expiry is not left behind");
}

#[tokio::test]
async fn do_not_disturb_without_an_expiry_has_nothing_that_could_lapse() {
    let (handle, sender, mut state, cancel, running) = dnd_service().await;

    handle
        .set_dnd(DoNotDisturb {
            enabled: true,
            until: None,
        })
        .await
        .expect("do not disturb is set");
    state.wait_for(enabled).await.expect("it is on");

    sender
        .send(Input::Event(Event::DoNotDisturbLapsed(Utc::now())))
        .await
        .expect("a deadline belonging to no window at all");
    handle
        .clear_all()
        .await
        .expect("a command answered after it proves the event was handled");

    let latest = handle.snapshot();
    cancel.cancel();
    running.await.expect("joined").expect("stopped");

    assert!(
        enabled(&latest),
        "nothing armed an expiry, so nothing can spend one"
    );
}

#[tokio::test]
async fn a_spent_expiry_cannot_cancel_the_window_that_replaced_it() {
    let (handle, sender, mut state, cancel, running) = dnd_service().await;
    let extended = Utc::now() + chrono::TimeDelta::hours(4);

    handle
        .set_dnd(DoNotDisturb {
            enabled: true,
            until: Some(extended),
        })
        .await
        .expect("do not disturb is set");
    state.wait_for(enabled).await.expect("it is on");

    sender
        .send(Input::Event(Event::DoNotDisturbLapsed(
            Utc::now() - chrono::TimeDelta::hours(1),
        )))
        .await
        .expect("a timer that was already torn down fires late");
    handle
        .clear_all()
        .await
        .expect("a command answered after it proves the event was handled");

    let latest = handle.snapshot();
    cancel.cancel();
    running.await.expect("joined").expect("stopped");

    let dnd = latest.dnd.expect("do not disturb was published").dnd;
    assert!(
        dnd.enabled,
        "the stale deadline belonged to a window that no longer exists"
    );
    assert_eq!(dnd.until, Some(extended));
}

#[tokio::test]
async fn an_expiry_that_has_already_passed_lapses_rather_than_sticking() {
    let (handle, _sender, mut state, cancel, running) = dnd_service().await;

    handle
        .set_dnd(DoNotDisturb {
            enabled: true,
            until: Some(Utc::now() - chrono::TimeDelta::hours(1)),
        })
        .await
        .expect("do not disturb is set");

    state
        .wait_for(|state| !enabled(state))
        .await
        .expect("an expiry in the past is due immediately");

    cancel.cancel();
    running.await.expect("joined").expect("stopped");
}
