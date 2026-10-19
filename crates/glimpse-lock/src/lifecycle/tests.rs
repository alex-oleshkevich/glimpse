use super::*;

const UNLOCKED: Verdict = Verdict::Unlock {
    password_expired: false,
};

const DAEMON: Settings = Settings {
    standalone: false,
    refusal: None,
};

fn started(settings: Settings, locked_hint: bool) -> (Lifecycle, Vec<Effect>) {
    let mut machine = Lifecycle::new(settings);
    let effects = machine.handle(Input::Start { locked_hint });
    (machine, effects)
}

fn locked(settings: Settings) -> Lifecycle {
    let (mut machine, _) = started(settings, false);
    machine.handle(Input::LockRequested);
    let generation = machine.generation;
    machine.handle(Input::Locked(generation));
    machine.handle(Input::Painted(generation));
    machine
}

fn refused(message: Message) -> Verdict {
    Verdict::Refused {
        message,
        shake: true,
    }
}

fn is_logind(effect: &Effect) -> bool {
    matches!(
        effect,
        Effect::SetLockedHint(_)
            | Effect::TakeInhibitor
            | Effect::ReleaseInhibitor
            | Effect::ArmSleepDeadline(_)
    )
}

mod lock;
mod probe;
mod sleep;
mod standalone;
