use super::*;

#[test]
fn standalone_locks_at_once_touches_no_logind_state_and_exits_after_unlock() {
    let settings = Settings {
        standalone: true,
        ..DAEMON
    };
    let (mut machine, mut effects) = started(settings, false);
    assert_eq!(effects, vec![Effect::Lock(1)]);
    for input in [
        Input::Locked(1),
        Input::Painted(1),
        Input::PrepareForSleep(true),
        Input::PrepareForSleep(false),
        Input::UnlockRequested,
        Input::Submit,
        Input::AuthFinished {
            attempt: 1,
            verdict: UNLOCKED,
        },
    ] {
        effects.extend(machine.handle(input));
    }
    assert!(!effects.iter().any(is_logind), "{effects:?}");
    assert!(effects.contains(&Effect::Unlock));
    assert_eq!(machine.handle(Input::Unlocked(1)), vec![Effect::Exit(true)]);
}

#[test]
fn standalone_needs_pam_to_unlock() {
    let settings = Settings {
        standalone: true,
        ..DAEMON
    };
    let (mut machine, _) = started(settings, false);
    machine.handle(Input::Locked(1));
    machine.handle(Input::Submit);
    machine.handle(Input::AuthFinished {
        attempt: 1,
        verdict: refused(Message::WrongPassword),
    });
    assert_eq!(machine.handle(Input::Unlocked(1)), vec![Effect::Lock(2)]);
}

#[test]
fn standalone_with_a_failed_probe_refuses_and_exits() {
    let settings = Settings {
        standalone: true,
        refusal: Some(Refusal::CantVerify),
        ..DAEMON
    };
    let (_, effects) = started(settings, false);
    assert_eq!(
        effects,
        vec![
            Effect::Notify(Notice::NotLocked(Reason::CantVerify)),
            Effect::Exit(false)
        ]
    );
}
