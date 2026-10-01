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
mod tests;
