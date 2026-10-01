use super::*;

#[tokio::test]
async fn a_stalled_timer_skips_what_it_missed_rather_than_firing_all_of_it() {
    let ticks = ticker(tokio::time::Instant::now(), Duration::from_secs(1));

    assert_eq!(
        ticks.missed_tick_behavior(),
        tokio::time::MissedTickBehavior::Skip,
        "tokio defaults to Burst, which after a suspend would deliver one tick per second \
             slept, all in one pass; Skip is also the only behaviour that keeps the phase"
    );
}

#[tokio::test]
async fn a_typed_state_change_wakes_the_applet() {
    let (state, receiver) = watch::channel(1_u8);
    let (host, inputs) = relm4::channel();
    let task = tokio::spawn(watch_changes(receiver, host));

    state.send(2).expect("the receiver is live");
    assert!(matches!(
        inputs.recv().await,
        Some(runtime::HostInput::Woken)
    ));

    drop(state);
    task.await.expect("the watch task stops");
}

#[test]
fn a_tick_lands_on_the_boundary_rather_than_where_the_panel_happened_to_start() {
    let period = Duration::from_secs(60);
    let started = Duration::from_millis(12_400);

    assert_eq!(
        until_boundary(started, period),
        Duration::from_millis(47_600),
        "starting 12.4s into a minute must wait out the rest of it, not a whole minute"
    );
    assert_eq!(
        (started + until_boundary(started, period)).as_nanos() % period.as_nanos(),
        0
    );
}

#[test]
fn a_tick_exactly_on_the_boundary_waits_a_whole_period() {
    let period = Duration::from_secs(60);

    assert_eq!(
        until_boundary(Duration::from_secs(120), period),
        period,
        "waiting zero would render the same value twice in one instant"
    );
}

#[test]
fn a_second_long_period_aligns_to_the_second() {
    assert_eq!(
        until_boundary(Duration::from_millis(1_250), Duration::from_secs(1)),
        Duration::from_millis(750)
    );
}

#[test]
fn a_zero_period_cannot_be_aligned_and_asks_for_no_wait() {
    assert_eq!(
        until_boundary(Duration::from_secs(5), Duration::ZERO),
        Duration::ZERO,
        "Ctx::interval refuses a zero period before it gets here"
    );
}

#[test]
fn a_pointer_button_keeps_its_gdk_code_when_it_has_no_name() {
    assert_eq!(Button::from_code(1), Button::Left);
    assert_eq!(Button::from_code(2), Button::Middle);
    assert_eq!(Button::from_code(3), Button::Right);
    assert_eq!(Button::from_code(8), Button::Other(8));
}
