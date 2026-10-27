use super::*;

fn ready(initial_hint: Option<bool>, preparing_for_sleep: bool) -> Input {
    Input::BackendReady {
        initial_hint,
        preparing_for_sleep,
    }
}

fn verified(attempt: u64) -> Input {
    Input::AuthFinished {
        attempt,
        verdict: UNLOCKED,
    }
}

#[test]
fn late_initial_state_cannot_replace_an_owned_lock() {
    for hint in [false, true] {
        let mut machine = Lifecycle::new(DAEMON);
        machine.handle(Input::LockRequested);
        let generation = machine.generation;
        let effects = machine.handle(ready(Some(hint), false));
        assert_eq!(machine.phase, Phase::Acquiring);
        assert_eq!(machine.generation, generation);
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::Lock(_)))
        );
        assert!(!effects.contains(&Effect::SetLockedHint(false)));
        machine.handle(Input::Locked(generation));
        machine.handle(Input::Painted(generation));
        machine.handle(Input::Submit);
        let attempt = machine.current;
        let effects = machine.handle(ready(Some(hint), false));
        assert_eq!(machine.phase, Phase::Locked);
        assert_eq!(machine.generation, generation);
        assert_eq!(machine.current, attempt);
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::Lock(_)))
        );
        assert!(effects.contains(&Effect::SetLockedHint(true)));
    }
}

#[test]
fn backend_loss_does_not_lock_an_idle_session() {
    let (mut machine, _) = started(DAEMON, false);
    assert!(machine.handle(Input::BackendLost).is_empty());
    assert_eq!(machine.phase, Phase::Idle);
    assert!(machine.prompt().busy);
    let effects = machine.handle(ready(None, false));
    assert_eq!(machine.phase, Phase::Idle);
    assert!(!machine.prompt().busy);
    assert!(effects.contains(&Effect::TakeInhibitor));
    assert!(!effects.contains(&Effect::SetLockedHint(false)));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Lock(_)))
    );
}

#[test]
fn an_observation_gap_retires_authorization_even_if_both_sleep_edges_were_missed() {
    let mut machine = locked(DAEMON);
    let generation = machine.generation;
    machine.handle(Input::Submit);
    let old = machine.current.unwrap();
    assert!(machine.handle(Input::BackendLost).is_empty());
    assert_eq!(machine.phase, Phase::Locked);
    assert_eq!(machine.generation, generation);
    assert!(!machine.sleeping);
    assert!(machine.in_flight.contains(&old));
    assert_eq!(machine.handle(Input::Submit), vec![Effect::DiscardSubmit]);
    assert!(machine.prompt().busy);
    let effects = machine.handle(ready(None, false));
    assert!(effects.contains(&Effect::SetLockedHint(true)));
    assert!(effects.contains(&Effect::TakeInhibitor));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Lock(_)))
    );
    assert!(machine.handle(verified(old)).is_empty());
    assert_eq!(machine.phase, Phase::Locked);
    assert!(machine.in_flight.is_empty());
    assert!(!machine.prompt().busy);
    machine.handle(Input::Submit);
    let fresh = machine.current.unwrap();
    assert_ne!(fresh, old);
    assert_eq!(machine.handle(verified(fresh)), vec![Effect::Unlock]);
}

#[test]
fn authorization_cannot_succeed_while_the_backend_is_unknown() {
    let mut machine = locked(DAEMON);
    machine.handle(Input::Submit);
    let old = machine.current.unwrap();
    machine.handle(Input::BackendLost);
    assert!(machine.handle(verified(old)).is_empty());
    assert_eq!(machine.phase, Phase::Locked);
    assert!(machine.in_flight.is_empty());
    assert!(machine.prompt().busy);
}

#[test]
fn abandoned_pam_threads_still_count_toward_the_limit() {
    let mut machine = locked(DAEMON);
    machine.handle(Input::Submit);
    let first = machine.current.unwrap();
    machine.handle(Input::BackendLost);
    machine.handle(ready(None, false));
    machine.handle(Input::Submit);
    let second = machine.current.unwrap();
    machine.handle(Input::BackendLost);
    machine.handle(ready(None, false));
    assert_eq!(machine.in_flight.len(), MAX_THREADS);
    assert!(machine.prompt().busy);
    assert_eq!(machine.handle(Input::Submit), vec![Effect::DiscardSubmit]);
    machine.handle(verified(first));
    assert_eq!(machine.phase, Phase::Locked);
    assert_eq!(machine.in_flight.len(), 1);
    assert!(!machine.prompt().busy);
    machine.handle(verified(second));
    assert_eq!(machine.phase, Phase::Locked);
    assert!(machine.in_flight.is_empty());
}

#[test]
fn reconnect_reconciles_a_missed_resume_without_replacing_surfaces() {
    let mut machine = locked(DAEMON);
    let generation = machine.generation;
    machine.handle(Input::PrepareForSleep(true));
    machine.handle(Input::BackendLost);
    let effects = machine.handle(ready(None, false));
    assert!(!machine.sleeping);
    assert_eq!(machine.phase, Phase::Locked);
    assert_eq!(machine.generation, generation);
    assert!(effects.contains(&Effect::TakeInhibitor));
    assert!(effects.contains(&Effect::SetLockedHint(true)));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Lock(_)))
    );
    assert!(
        machine
            .handle(Input::Submit)
            .iter()
            .any(|effect| matches!(effect, Effect::StartAttempt(_)))
    );
}

#[test]
fn reconnect_preserves_the_original_unpainted_sleep_deadline() {
    let (mut machine, _) = started(DAEMON, false);
    machine.handle(Input::PrepareForSleep(true));
    let generation = machine.generation;
    let token = machine.sleep.unwrap();
    machine.handle(Input::BackendLost);
    let effects = machine.handle(ready(None, true));
    assert_eq!(machine.phase, Phase::Acquiring);
    assert_eq!(machine.generation, generation);
    assert_eq!(machine.sleep, Some(token));
    assert!(effects.contains(&Effect::TakeInhibitor));
    assert!(!effects.contains(&Effect::SetLockedHint(false)));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Lock(_) | Effect::ArmSleepDeadline(_)))
    );
    machine.handle(Input::Locked(generation));
    assert_eq!(
        machine.handle(Input::Painted(generation)),
        vec![Effect::ReleaseInhibitor]
    );
}

#[test]
fn reconnect_does_not_hold_an_inhibitor_after_a_painted_sleep() {
    let mut machine = locked(DAEMON);
    machine.handle(Input::PrepareForSleep(true));
    machine.handle(Input::BackendLost);
    let effects = machine.handle(ready(None, true));
    assert!(effects.contains(&Effect::ReleaseInhibitor));
    assert!(!effects.contains(&Effect::TakeInhibitor));
    assert_eq!(machine.phase, Phase::Locked);
}

#[test]
fn reconnect_cannot_clear_an_inherited_hint_after_a_refused_restore() {
    let (mut machine, _) = started(DAEMON, true);
    let generation = machine.generation;
    let effects = machine.handle(Input::Failed(generation));
    assert_eq!(machine.phase, Phase::Idle);
    assert!(!effects.contains(&Effect::SetLockedHint(false)));
    machine.handle(Input::BackendLost);
    let effects = machine.handle(ready(None, false));
    assert!(
        !effects.contains(&Effect::SetLockedHint(false)),
        "this process never acquired the orphaned lock or wrote its hint"
    );
}

#[test]
fn reconnect_replays_the_false_hint_written_after_our_own_unlock() {
    let mut machine = locked(DAEMON);
    machine.handle(Input::Submit);
    let attempt = machine.current.unwrap();
    machine.handle(verified(attempt));
    let effects = machine.handle(Input::Unlocked(machine.generation));
    assert!(effects.contains(&Effect::SetLockedHint(false)));
    machine.handle(Input::BackendLost);
    let effects = machine.handle(ready(None, false));
    assert!(effects.contains(&Effect::SetLockedHint(false)));
}

#[test]
fn reconnect_replays_our_true_hint_after_an_unexpected_relock_is_acknowledged() {
    let mut machine = locked(DAEMON);
    let previous = machine.generation;
    machine.handle(Input::Unlocked(previous));
    let generation = machine.generation;
    assert_ne!(generation, previous);
    machine.handle(Input::BackendLost);
    let effects = machine.handle(ready(None, false));
    assert_eq!(machine.phase, Phase::Acquiring);
    assert_eq!(machine.generation, generation);
    assert!(!effects.contains(&Effect::SetLockedHint(false)));
    let effects = machine.handle(Input::Locked(generation));
    assert!(
        effects.contains(&Effect::SetLockedHint(true)),
        "the new backend needs our owned hint after the new lock is acknowledged"
    );
}
