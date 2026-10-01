use super::*;

#[test]
fn sleep_while_locked_and_painted_releases_at_once() {
    let mut machine = locked(DAEMON);
    assert_eq!(
        machine.handle(Input::PrepareForSleep(true)),
        vec![Effect::ReleaseInhibitor]
    );
    assert_eq!(
        machine.handle(Input::PrepareForSleep(false)),
        vec![Effect::TakeInhibitor]
    );
}

#[test]
fn sleep_while_idle_locks_and_releases_after_locked_and_painted() {
    let (mut machine, _) = started(DAEMON, false);
    assert_eq!(
        machine.handle(Input::PrepareForSleep(true)),
        vec![Effect::Lock(1), Effect::ArmSleepDeadline(1)]
    );
    assert_eq!(
        machine.handle(Input::Locked(1)),
        vec![Effect::SetLockedHint(true)]
    );
    assert_eq!(
        machine.handle(Input::Painted(1)),
        vec![Effect::ReleaseInhibitor]
    );
    assert_eq!(machine.handle(Input::SleepDeadline(1)), vec![]);
}

#[test]
fn sleep_while_acquiring_waits_for_the_lock_in_flight() {
    let (mut machine, _) = started(DAEMON, false);
    machine.handle(Input::LockRequested);
    machine.handle(Input::Painted(1));
    assert_eq!(
        machine.handle(Input::PrepareForSleep(true)),
        vec![Effect::ArmSleepDeadline(1)]
    );
    assert_eq!(
        machine.handle(Input::Locked(1)),
        vec![Effect::SetLockedHint(true), Effect::ReleaseInhibitor]
    );
}

#[test]
fn sleep_while_locked_but_unpainted_waits_for_the_paint() {
    let (mut machine, _) = started(DAEMON, false);
    machine.handle(Input::LockRequested);
    machine.handle(Input::Locked(1));
    assert_eq!(
        machine.handle(Input::PrepareForSleep(true)),
        vec![Effect::ArmSleepDeadline(1)]
    );
    assert_eq!(
        machine.handle(Input::Painted(1)),
        vec![Effect::ReleaseInhibitor]
    );
}

#[test]
fn the_sleep_wait_is_capped_by_the_deadline() {
    let (mut machine, _) = started(DAEMON, false);
    machine.handle(Input::PrepareForSleep(true));
    assert_eq!(machine.handle(Input::SleepDeadline(0)), vec![]);
    assert_eq!(
        machine.handle(Input::SleepDeadline(1)),
        vec![Effect::ReleaseInhibitor]
    );
    assert_eq!(
        machine.handle(Input::Locked(1)),
        vec![Effect::SetLockedHint(true)]
    );
    assert_eq!(machine.handle(Input::Painted(1)), vec![]);
}

#[test]
fn a_failed_lock_during_a_sleep_wait_releases_the_inhibitor() {
    let (mut machine, _) = started(DAEMON, false);
    machine.handle(Input::PrepareForSleep(true));
    assert_eq!(
        machine.handle(Input::Failed(1)),
        vec![
            Effect::Notify(Notice::NotLocked(Reason::LockFailed)),
            Effect::ReleaseInhibitor
        ]
    );
}

#[test]
fn sleep_during_an_unlock_locks_again_once_it_ends() {
    let mut machine = locked(DAEMON);
    machine.handle(Input::Submit);
    machine.handle(Input::AuthFinished {
        attempt: 1,
        verdict: UNLOCKED,
    });
    assert_eq!(
        machine.handle(Input::PrepareForSleep(true)),
        vec![Effect::ArmSleepDeadline(1)]
    );
    assert_eq!(
        machine.handle(Input::Unlocked(1)),
        vec![
            Effect::SetLockedHint(false),
            Effect::TakeInhibitor,
            Effect::Lock(2)
        ]
    );
}

#[test]
fn a_lock_request_during_an_unlock_locks_again_once_it_ends() {
    let mut machine = locked(DAEMON);
    machine.handle(Input::Submit);
    machine.handle(Input::AuthFinished {
        attempt: 1,
        verdict: UNLOCKED,
    });
    assert_eq!(machine.handle(Input::LockRequested), vec![]);
    assert_eq!(
        machine.handle(Input::Unlocked(1)),
        vec![
            Effect::SetLockedHint(false),
            Effect::TakeInhibitor,
            Effect::Lock(2)
        ]
    );
}

#[test]
fn a_relock_must_paint_again_before_a_sleep_is_released() {
    let mut machine = locked(DAEMON);
    machine.handle(Input::Unlocked(1));
    machine.handle(Input::Locked(2));
    assert_eq!(
        machine.handle(Input::PrepareForSleep(true)),
        vec![Effect::ArmSleepDeadline(1)],
        "the first generation's paint does not cover the second"
    );
    assert_eq!(
        machine.handle(Input::Painted(2)),
        vec![Effect::ReleaseInhibitor]
    );
}

#[test]
fn a_paint_from_an_old_generation_does_not_count() {
    let mut machine = locked(DAEMON);
    machine.handle(Input::Unlocked(1));
    machine.handle(Input::Painted(1));
    machine.handle(Input::Locked(2));
    assert_eq!(
        machine.handle(Input::PrepareForSleep(true)),
        vec![Effect::ArmSleepDeadline(1)]
    );
}

#[test]
fn a_surface_added_while_locked_holds_the_sleep_until_it_paints() {
    let mut machine = locked(DAEMON);
    machine.handle(Input::Unpainted(1));
    assert_eq!(
        machine.handle(Input::PrepareForSleep(true)),
        vec![Effect::ArmSleepDeadline(1)]
    );
    assert_eq!(
        machine.handle(Input::Painted(1)),
        vec![Effect::ReleaseInhibitor]
    );
    machine.handle(Input::Unpainted(0));
    assert_eq!(
        machine.handle(Input::PrepareForSleep(true)),
        vec![Effect::ReleaseInhibitor],
        "a stale unpaint changes nothing"
    );
}

#[test]
fn turning_lock_before_sleep_off_releases_the_inhibitor() {
    let (mut machine, _) = started(DAEMON, false);
    assert_eq!(
        machine.handle(Input::Configure {
            lock_on_request: true,
            lock_before_sleep: false
        }),
        vec![Effect::ReleaseInhibitor]
    );
    assert_eq!(machine.handle(Input::PrepareForSleep(true)), vec![]);
}

#[test]
fn a_current_locked_signal_outside_acquiring_changes_nothing() {
    let (mut machine, _) = started(DAEMON, false);
    machine.handle(Input::LockRequested);
    machine.handle(Input::Failed(1));
    assert_eq!(
        machine.handle(Input::Locked(1)),
        vec![],
        "a lock that failed is not locked by a late signal"
    );
    assert_eq!(machine.handle(Input::Submit), vec![Effect::DiscardSubmit]);

    let mut machine = locked(DAEMON);
    machine.handle(Input::Submit);
    machine.handle(Input::AuthFinished {
        attempt: 1,
        verdict: UNLOCKED,
    });
    assert_eq!(machine.handle(Input::Locked(1)), vec![]);
    assert_eq!(
        machine.handle(Input::Unlocked(1)),
        vec![Effect::SetLockedHint(false), Effect::TakeInhibitor],
        "an unlock in progress still ends as requested"
    );
}

#[test]
fn lock_before_sleep_off_takes_and_holds_nothing() {
    let settings = Settings {
        lock_before_sleep: false,
        ..DAEMON
    };
    let (mut machine, effects) = started(settings, false);
    assert_eq!(effects, vec![]);
    assert_eq!(machine.handle(Input::PrepareForSleep(true)), vec![]);
    assert_eq!(machine.handle(Input::PrepareForSleep(false)), vec![]);
    assert_eq!(
        machine.handle(Input::Configure {
            lock_on_request: true,
            lock_before_sleep: true
        }),
        vec![Effect::TakeInhibitor]
    );
}

#[test]
fn the_sleep_wait_is_the_logind_delay_minus_a_margin() {
    assert_eq!(sleep_wait(None), Duration::from_millis(4500));
    assert_eq!(sleep_wait(Some(10_000_000)), Duration::from_millis(9500));
    assert_eq!(sleep_wait(Some(100)), Duration::ZERO);
}
