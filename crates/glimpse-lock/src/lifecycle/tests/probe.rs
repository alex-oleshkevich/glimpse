use super::*;

#[test]
fn a_failed_probe_refuses_every_lock_with_a_notice_and_takes_no_inhibitor() {
    let settings = Settings {
        refusal: Some(Refusal::CantVerify),
        ..DAEMON
    };
    let (mut machine, effects) = started(settings, false);
    assert_eq!(effects, vec![]);
    for _ in 0..2 {
        assert_eq!(
            machine.handle(Input::LockRequested),
            vec![Effect::Notify(Notice::NotLocked(Reason::CantVerify))]
        );
    }
    assert_eq!(
        machine.handle(Input::PrepareForSleep(true)),
        vec![Effect::Notify(Notice::SuspendingUnlocked(
            Reason::CantVerify
        ))]
    );
    assert_eq!(machine.handle(Input::PrepareForSleep(false)), vec![]);
}

#[test]
fn a_failed_probe_with_the_hint_set_locks_behind_an_unavailable_prompt() {
    let settings = Settings {
        refusal: Some(Refusal::CantVerify),
        ..DAEMON
    };
    let (mut machine, effects) = started(settings, true);
    assert_eq!(effects, vec![Effect::Lock(1)]);
    let prompt = machine.prompt();
    assert!(!prompt.available);
    assert_eq!(prompt.message, Some(Message::ConsoleOnly));
    machine.handle(Input::Locked(1));
    assert_eq!(machine.handle(Input::Submit), vec![Effect::DiscardSubmit]);
    assert_eq!(machine.handle(Input::PrepareForSleep(true)), vec![]);
}

#[test]
fn the_hint_at_start_relocks_when_the_probe_passes() {
    let (_, effects) = started(DAEMON, true);
    assert_eq!(effects, vec![Effect::TakeInhibitor, Effect::Lock(1)]);
}

#[test]
fn a_compositor_without_the_protocol_notifies_on_every_lock_and_stays() {
    let settings = Settings {
        refusal: Some(Refusal::NoSessionLock),
        ..DAEMON
    };
    let (mut machine, effects) = started(settings, true);
    assert_eq!(effects, vec![]);
    assert_eq!(
        machine.handle(Input::LockRequested),
        vec![Effect::Notify(Notice::NotLocked(Reason::NoSessionLock))]
    );
}

#[test]
fn a_failed_lock_notifies_and_the_next_lock_tries_again() {
    let (mut machine, _) = started(DAEMON, false);
    machine.handle(Input::LockRequested);
    assert_eq!(
        machine.handle(Input::Failed(1)),
        vec![Effect::Notify(Notice::NotLocked(Reason::LockFailed))]
    );
    assert_eq!(machine.handle(Input::Failed(1)), vec![]);
    assert_eq!(machine.handle(Input::LockRequested), vec![Effect::Lock(2)]);
}

#[test]
fn the_hint_is_cleared_only_when_this_process_set_it() {
    let (mut machine, effects) = started(DAEMON, true);
    assert!(effects.contains(&Effect::Lock(1)));
    assert!(
        !machine
            .handle(Input::Failed(1))
            .contains(&Effect::SetLockedHint(false)),
        "a hint a dead locker left is not ours to clear"
    );
    let mut machine = locked(DAEMON);
    machine.handle(Input::Unlocked(1));
    assert_eq!(
        machine.handle(Input::Failed(2)),
        vec![
            Effect::SetLockedHint(false),
            Effect::Notify(Notice::NotLocked(Reason::LockFailed))
        ],
        "a hint this process set is cleared when the lock it described is gone"
    );
}
