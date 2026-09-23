use std::collections::BTreeSet;
use std::time::Duration;

use crate::auth::{Message, Verdict};

const MAX_THREADS: usize = 2;
const SLEEP_MARGIN: Duration = Duration::from_millis(500);
const DEFAULT_INHIBIT_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    CantVerify,
    NoSessionLock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    CantVerify,
    NoSessionLock,
    LockFailed,
}

impl From<Refusal> for Reason {
    fn from(refusal: Refusal) -> Self {
        match refusal {
            Refusal::CantVerify => Self::CantVerify,
            Refusal::NoSessionLock => Self::NoSessionLock,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    NotLocked(Reason),
    SuspendingUnlocked(Reason),
    PasswordExpired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Acquiring,
    Locked,
    Unlocking,
}

#[derive(Debug)]
pub enum Input {
    Start {
        locked_hint: bool,
    },
    Configure {
        lock_on_request: bool,
        lock_before_sleep: bool,
    },
    LockRequested,
    UnlockRequested,
    PrepareForSleep(bool),
    Locked(u64),
    Failed(u64),
    Unlocked(u64),
    Painted(u64),
    Unpainted(u64),
    SleepDeadline(u64),
    Submit,
    AuthFinished {
        attempt: u64,
        verdict: Verdict,
    },
    AuthTimedOut(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Lock(u64),
    Unlock,
    SetLockedHint(bool),
    TakeInhibitor,
    ReleaseInhibitor,
    ArmSleepDeadline(u64),
    StartAttempt(u64),
    DiscardSubmit,
    Shake,
    Notify(Notice),
    Exit(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub standalone: bool,
    pub refusal: Option<Refusal>,
    pub lock_on_request: bool,
    pub lock_before_sleep: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptView {
    pub available: bool,
    pub busy: bool,
    pub message: Option<Message>,
}

pub struct Lifecycle {
    settings: Settings,
    phase: Phase,
    generation: u64,
    relock: bool,
    hint_set: bool,
    painted: bool,
    sleep: Option<u64>,
    sleeps: u64,
    attempts: u64,
    current: Option<u64>,
    in_flight: BTreeSet<u64>,
    message: Option<Message>,
}

pub fn sleep_wait(inhibit_delay_max_usec: Option<u64>) -> Duration {
    inhibit_delay_max_usec
        .map_or(DEFAULT_INHIBIT_DELAY, Duration::from_micros)
        .saturating_sub(SLEEP_MARGIN)
}

impl Lifecycle {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            phase: Phase::Idle,
            generation: 0,
            relock: false,
            hint_set: false,
            painted: false,
            sleep: None,
            sleeps: 0,
            attempts: 0,
            current: None,
            in_flight: BTreeSet::new(),
            message: None,
        }
    }

    #[cfg(test)]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn is_idle(&self) -> bool {
        self.phase == Phase::Idle
    }

    pub fn prompt(&self) -> PromptView {
        PromptView {
            available: self.settings.refusal.is_none(),
            busy: self.current.is_some() || self.in_flight.len() >= MAX_THREADS,
            message: self.message.clone(),
        }
    }

    pub fn handle(&mut self, input: Input) -> Vec<Effect> {
        let mut effects = Vec::new();
        match input {
            Input::Start { locked_hint } => self.start(locked_hint, &mut effects),
            Input::Configure {
                lock_on_request,
                lock_before_sleep,
            } => self.configure(lock_on_request, lock_before_sleep, &mut effects),
            Input::LockRequested => self.lock_requested(&mut effects),
            Input::UnlockRequested => {
                tracing::info!("ignoring logind Unlock: only local authentication ends a lock");
            }
            Input::PrepareForSleep(true) => self.prepare_for_sleep(&mut effects),
            Input::PrepareForSleep(false) => self.resumed(&mut effects),
            Input::Locked(generation) => self.locked(generation, &mut effects),
            Input::Failed(generation) => self.failed(generation, &mut effects),
            Input::Unlocked(generation) => self.unlocked(generation, &mut effects),
            Input::Painted(generation) => self.painted(generation, &mut effects),
            Input::Unpainted(generation) => {
                if generation == self.generation {
                    self.painted = false;
                }
            }
            Input::SleepDeadline(token) => {
                if self.sleep == Some(token) {
                    tracing::warn!("the lock did not paint before the sleep deadline");
                    self.sleep = None;
                    effects.push(Effect::ReleaseInhibitor);
                }
            }
            Input::Submit => self.submit(&mut effects),
            Input::AuthFinished { attempt, verdict } => {
                self.finished(attempt, verdict, &mut effects);
            }
            Input::AuthTimedOut(attempt) => {
                if self.current == Some(attempt) {
                    tracing::warn!(attempt, "authentication timed out; abandoning its thread");
                    self.current = None;
                    self.message = Some(Message::TimedOut);
                }
            }
        }
        effects
    }

    fn start(&mut self, locked_hint: bool, effects: &mut Vec<Effect>) {
        if self.settings.standalone {
            match self.settings.refusal {
                Some(refusal) => {
                    effects.push(Effect::Notify(Notice::NotLocked(refusal.into())));
                    effects.push(Effect::Exit(false));
                }
                None => self.lock(effects),
            }
            return;
        }
        if self.inhibits() {
            effects.push(Effect::TakeInhibitor);
        }
        if locked_hint && self.settings.refusal != Some(Refusal::NoSessionLock) {
            tracing::warn!("LockedHint is set at start; re-acquiring the lock");
            self.lock(effects);
        }
    }

    fn configure(
        &mut self,
        lock_on_request: bool,
        lock_before_sleep: bool,
        effects: &mut Vec<Effect>,
    ) {
        self.settings.lock_on_request = lock_on_request;
        if self.settings.lock_before_sleep == lock_before_sleep {
            return;
        }
        self.settings.lock_before_sleep = lock_before_sleep;
        if self.inhibits() {
            effects.push(Effect::TakeInhibitor);
        } else if !self.settings.standalone {
            self.sleep = None;
            effects.push(Effect::ReleaseInhibitor);
        }
    }

    fn lock_requested(&mut self, effects: &mut Vec<Effect>) {
        if !self.settings.lock_on_request {
            tracing::info!("ignoring logind Lock: [power] lock-on-request is off");
            return;
        }
        match self.phase {
            Phase::Idle => match self.settings.refusal {
                Some(refusal) => {
                    tracing::error!(?refusal, "refusing to lock");
                    effects.push(Effect::Notify(Notice::NotLocked(refusal.into())));
                }
                None => self.lock(effects),
            },
            Phase::Unlocking => self.relock = true,
            Phase::Acquiring | Phase::Locked => {}
        }
    }

    fn prepare_for_sleep(&mut self, effects: &mut Vec<Effect>) {
        if self.settings.standalone || !self.settings.lock_before_sleep {
            return;
        }
        if let Some(refusal) = self.settings.refusal {
            if self.phase == Phase::Idle {
                tracing::error!(?refusal, "suspending unlocked");
                effects.push(Effect::Notify(Notice::SuspendingUnlocked(refusal.into())));
            }
            return;
        }
        match self.phase {
            Phase::Locked if self.painted => {
                effects.push(Effect::ReleaseInhibitor);
                return;
            }
            Phase::Idle => self.lock(effects),
            Phase::Unlocking => self.relock = true,
            Phase::Acquiring | Phase::Locked => {}
        }
        self.sleeps += 1;
        self.sleep = Some(self.sleeps);
        effects.push(Effect::ArmSleepDeadline(self.sleeps));
    }

    fn resumed(&mut self, effects: &mut Vec<Effect>) {
        self.sleep = None;
        if self.inhibits() {
            effects.push(Effect::TakeInhibitor);
        }
    }

    fn locked(&mut self, generation: u64, effects: &mut Vec<Effect>) {
        if generation != self.generation || self.phase != Phase::Acquiring {
            return;
        }
        self.phase = Phase::Locked;
        self.set_hint(true, effects);
        if self.painted {
            self.release_sleep(effects);
        }
    }

    fn painted(&mut self, generation: u64, effects: &mut Vec<Effect>) {
        if generation != self.generation || !matches!(self.phase, Phase::Acquiring | Phase::Locked)
        {
            return;
        }
        self.painted = true;
        if self.phase == Phase::Locked {
            self.release_sleep(effects);
        }
    }

    fn failed(&mut self, generation: u64, effects: &mut Vec<Effect>) {
        if generation != self.generation || self.phase != Phase::Acquiring {
            return;
        }
        tracing::error!("the compositor refused the session lock");
        self.phase = Phase::Idle;
        self.current = None;
        self.set_hint(false, effects);
        effects.push(Effect::Notify(Notice::NotLocked(Reason::LockFailed)));
        self.release_sleep(effects);
        if self.settings.standalone {
            effects.push(Effect::Exit(false));
        }
    }

    fn unlocked(&mut self, generation: u64, effects: &mut Vec<Effect>) {
        if generation != self.generation {
            return;
        }
        match self.phase {
            Phase::Idle => {}
            Phase::Unlocking => {
                self.phase = Phase::Idle;
                self.set_hint(false, effects);
                if self.inhibits() {
                    effects.push(Effect::TakeInhibitor);
                }
                if self.settings.standalone {
                    effects.push(Effect::Exit(true));
                } else if std::mem::take(&mut self.relock) {
                    self.lock(effects);
                }
            }
            Phase::Acquiring | Phase::Locked => {
                tracing::error!("the session was unlocked without authentication; locking again");
                self.lock(effects);
            }
        }
    }

    fn submit(&mut self, effects: &mut Vec<Effect>) {
        let prompt = self.prompt();
        if self.phase != Phase::Locked || !prompt.available || prompt.busy {
            effects.push(Effect::DiscardSubmit);
            return;
        }
        self.attempts += 1;
        self.current = Some(self.attempts);
        self.in_flight.insert(self.attempts);
        self.message = None;
        effects.push(Effect::StartAttempt(self.attempts));
    }

    fn finished(&mut self, attempt: u64, verdict: Verdict, effects: &mut Vec<Effect>) {
        self.in_flight.remove(&attempt);
        if self.current != Some(attempt) {
            tracing::debug!(attempt, "discarding the result of a stale attempt");
            return;
        }
        self.current = None;
        match verdict {
            Verdict::Unlock { password_expired } if self.phase == Phase::Locked => {
                self.phase = Phase::Unlocking;
                effects.push(Effect::Unlock);
                if password_expired {
                    effects.push(Effect::Notify(Notice::PasswordExpired));
                }
            }
            Verdict::Unlock { .. } => {}
            Verdict::Refused { message, shake } => {
                self.message = Some(message);
                if shake {
                    effects.push(Effect::Shake);
                }
            }
        }
    }

    fn lock(&mut self, effects: &mut Vec<Effect>) {
        self.generation += 1;
        self.phase = Phase::Acquiring;
        self.painted = false;
        self.current = None;
        self.message =
            (self.settings.refusal == Some(Refusal::CantVerify)).then_some(Message::ConsoleOnly);
        effects.push(Effect::Lock(self.generation));
    }

    fn release_sleep(&mut self, effects: &mut Vec<Effect>) {
        if self.sleep.take().is_some() {
            effects.push(Effect::ReleaseInhibitor);
        }
    }

    fn set_hint(&mut self, locked: bool, effects: &mut Vec<Effect>) {
        if self.settings.standalone || self.hint_set == locked {
            return;
        }
        self.hint_set = locked;
        effects.push(Effect::SetLockedHint(locked));
    }

    fn inhibits(&self) -> bool {
        !self.settings.standalone
            && self.settings.lock_before_sleep
            && self.settings.refusal.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNLOCKED: Verdict = Verdict::Unlock {
        password_expired: false,
    };

    const DAEMON: Settings = Settings {
        standalone: false,
        refusal: None,
        lock_on_request: true,
        lock_before_sleep: true,
    };

    fn started(settings: Settings, locked_hint: bool) -> (Lifecycle, Vec<Effect>) {
        let mut machine = Lifecycle::new(settings);
        let effects = machine.handle(Input::Start { locked_hint });
        (machine, effects)
    }

    fn locked(settings: Settings) -> Lifecycle {
        let (mut machine, _) = started(settings, false);
        machine.handle(Input::LockRequested);
        let generation = machine.generation();
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
    fn a_lock_request_is_ignored_when_lock_on_request_is_off() {
        let (mut machine, _) = started(
            Settings {
                lock_on_request: false,
                ..DAEMON
            },
            false,
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
        assert_eq!(machine.generation(), 2, "an unrequested unlock re-locks");
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
}
