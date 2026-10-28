use super::*;
use std::cell::Cell;
use std::rc::Rc;

#[tokio::test(start_paused = true)]
async fn a_wall_clock_jump_moves_the_next_tick_to_the_next_boundary() {
    let jump = Rc::new(Cell::new(Duration::ZERO));
    let origin = tokio::time::Instant::now();
    let wall = {
        let jump = jump.clone();
        move || Duration::from_secs(10) + origin.elapsed() + jump.get()
    };
    let (host, inputs) = relm4::channel();
    let period = Duration::from_secs(60);
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let task = tokio::task::spawn_local(tick_loop(host, "test".into(), period, wall.clone()));

            assert!(matches!(
                inputs.recv().await,
                Some(runtime::HostInput::Ticked)
            ));
            assert_eq!(wall().as_secs(), 60);

            let before = wall();
            jump.set(Duration::from_secs(90));
            assert!(matches!(
                inputs.recv().await,
                Some(runtime::HostInput::Ticked)
            ));
            assert!(
                wall() - before - Duration::from_secs(90) <= Duration::from_secs(1),
                "a wall-clock jump during a long sleep ticks within a second, not at the old deadline"
            );

            assert!(matches!(
                inputs.recv().await,
                Some(runtime::HostInput::Ticked)
            ));
            assert_eq!(wall().as_secs(), 180);
            task.abort();
        })
        .await;
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
