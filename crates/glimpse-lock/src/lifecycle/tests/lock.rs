use super::*;

#[test]
fn a_lock_request_creates_surfaces_and_sets_the_hint_only_after_locked() {
    let (mut machine, effects) = started(DAEMON, false);
    assert_eq!(effects, vec![Effect::TakeInhibitor]);
    assert_eq!(machine.handle(Input::LockRequested), vec![Effect::Lock(1)]);
    assert_eq!(machine.handle(Input::Painted(1)), vec![]);
    assert_eq!(
        machine.handle(Input::Locked(1)),
        vec![Effect::SetLockedHint(true)]
    );
    assert_eq!(machine.handle(Input::LockRequested), vec![]);
}

#[test]
fn a_submit_while_acquiring_is_discarded_with_its_submitter() {
    let (mut machine, _) = started(DAEMON, false);
    machine.handle(Input::LockRequested);
    assert_eq!(machine.handle(Input::Submit), vec![Effect::DiscardSubmit]);
    machine.handle(Input::Locked(1));
    assert_eq!(machine.handle(Input::Submit), vec![Effect::StartAttempt(1)]);
}

#[test]
fn an_expired_password_unlocks_and_says_so() {
    let mut machine = locked(DAEMON);
    machine.handle(Input::Submit);
    assert_eq!(
        machine.handle(Input::AuthFinished {
            attempt: 1,
            verdict: Verdict::Unlock {
                password_expired: true
            }
        }),
        vec![Effect::Unlock, Effect::Notify(Notice::PasswordExpired)]
    );
}

#[test]
fn a_stale_locked_signal_sets_no_hint() {
    let (mut machine, _) = started(DAEMON, false);
    machine.handle(Input::LockRequested);
    assert_eq!(machine.handle(Input::Locked(0)), vec![]);
}

#[test]
fn a_current_success_unlocks_and_clears_the_hint_after_unlocked() {
    let mut machine = locked(DAEMON);
    assert_eq!(machine.handle(Input::Submit), vec![Effect::StartAttempt(1)]);
    assert!(machine.prompt().busy);
    assert_eq!(
        machine.handle(Input::AuthFinished {
            attempt: 1,
            verdict: UNLOCKED
        }),
        vec![Effect::Unlock]
    );
    assert_eq!(
        machine.handle(Input::Unlocked(1)),
        vec![Effect::SetLockedHint(false), Effect::TakeInhibitor],
        "a successful unlock also retries an inhibitor a resume failed to take"
    );
    assert_eq!(machine.handle(Input::Submit), vec![Effect::DiscardSubmit]);
}

#[test]
fn a_refusal_shows_its_message_and_shakes() {
    let mut machine = locked(DAEMON);
    machine.handle(Input::Submit);
    assert_eq!(
        machine.handle(Input::AuthFinished {
            attempt: 1,
            verdict: refused(Message::WrongPassword)
        }),
        vec![Effect::Shake]
    );
    let prompt = machine.prompt();
    assert_eq!(prompt.message, Some(Message::WrongPassword));
    assert!(!prompt.busy);
    assert_eq!(machine.handle(Input::Submit), vec![Effect::StartAttempt(2)]);
    assert_eq!(machine.prompt().message, None);
}

#[test]
fn a_result_for_another_attempt_is_discarded_in_the_same_cycle() {
    let mut machine = locked(DAEMON);
    machine.handle(Input::Submit);
    machine.handle(Input::AuthTimedOut(1));
    assert_eq!(machine.prompt().message, Some(Message::TimedOut));
    assert!(
        !machine.prompt().busy,
        "a timed-out attempt frees the prompt"
    );
    assert_eq!(machine.handle(Input::Submit), vec![Effect::StartAttempt(2)]);
    assert_eq!(
        machine.handle(Input::AuthFinished {
            attempt: 1,
            verdict: UNLOCKED
        }),
        vec![],
        "the abandoned attempt cannot unlock"
    );
    assert!(machine.prompt().busy, "attempt 2 is still in flight");
}

#[test]
fn a_result_from_an_earlier_cycle_is_discarded() {
    let mut machine = locked(DAEMON);
    machine.handle(Input::Submit);
    machine.handle(Input::Unlocked(1));
    assert_eq!(machine.generation, 2, "an unrequested unlock re-locks");
    machine.handle(Input::Locked(2));
    assert_eq!(
        machine.handle(Input::AuthFinished {
            attempt: 1,
            verdict: UNLOCKED
        }),
        vec![]
    );
}

#[test]
fn one_abandoned_and_one_in_flight_refuse_a_submit() {
    let mut machine = locked(DAEMON);
    machine.handle(Input::Submit);
    machine.handle(Input::AuthTimedOut(1));
    machine.handle(Input::Submit);
    machine.handle(Input::AuthTimedOut(2));
    assert_eq!(machine.prompt().message, Some(Message::TimedOut));
    assert!(machine.prompt().busy, "two threads are the cap");
    assert_eq!(machine.handle(Input::Submit), vec![Effect::DiscardSubmit]);
    machine.handle(Input::AuthFinished {
        attempt: 1,
        verdict: UNLOCKED,
    });
    assert!(!machine.prompt().busy, "one returning frees a slot");
    assert_eq!(machine.handle(Input::Submit), vec![Effect::StartAttempt(3)]);
}

#[test]
fn a_timeout_for_a_stale_attempt_changes_nothing() {
    let mut machine = locked(DAEMON);
    machine.handle(Input::Submit);
    machine.handle(Input::AuthFinished {
        attempt: 1,
        verdict: refused(Message::WrongPassword),
    });
    machine.handle(Input::AuthTimedOut(1));
    assert_eq!(machine.prompt().message, Some(Message::WrongPassword));
}

#[test]
fn an_unrequested_unlock_relocks_and_keeps_the_hint() {
    let mut machine = locked(DAEMON);
    let effects = machine.handle(Input::Unlocked(1));
    assert_eq!(effects, vec![Effect::Lock(2)]);
    assert!(!effects.contains(&Effect::SetLockedHint(false)));
    assert_eq!(machine.handle(Input::Locked(2)), vec![]);
}

#[test]
fn logind_unlock_unlocks_nothing() {
    let mut machine = locked(DAEMON);
    assert_eq!(machine.handle(Input::UnlockRequested), vec![]);
    assert_eq!(machine.handle(Input::Unlocked(0)), vec![]);
    assert_eq!(machine.handle(Input::Submit), vec![Effect::StartAttempt(1)]);
}
