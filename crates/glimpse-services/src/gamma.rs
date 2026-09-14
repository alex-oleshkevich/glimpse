use std::sync::{Arc, Mutex};

/// Applying a color temperature to every output.
///
/// Declared here and implemented in `glimpse-sunset`, because this crate is linked into
/// `glimpse-panel` and `glimpsed`, and neither may gain a Wayland dependency.
///
/// Synchronous on purpose: the one real implementation is a Wayland roundtrip, which blocks, and it
/// knows to say so with `block_in_place` itself. An `async` signature here would be a promise the
/// backend cannot keep, and it would cost dyn-compatibility — which is what lets the night light be
/// one plain service rather than a generic one.
pub trait Gamma: Send + 'static {
    fn apply(&mut self, kelvin: u32) -> Result<(), String>;
    fn reset(&mut self) -> Result<(), String>;
}

/// The mock beside the declaration, so the night light's whole state machine is testable without a
/// compositor. Not `#[cfg(test)]`: `glimpse-sunset`'s own tests are a separate compilation unit.
///
/// A clone shares the record, because the service takes its backend by value and a test still has
/// to read what was applied to it.
#[derive(Debug, Clone, Default)]
pub struct FakeGamma {
    record: Arc<Mutex<Record>>,
}

#[derive(Debug, Default)]
struct Record {
    applied: Vec<u32>,
    resets: usize,
    failure: Option<String>,
}

impl FakeGamma {
    pub fn applied(&self) -> Vec<u32> {
        self.record().applied.clone()
    }

    pub fn resets(&self) -> usize {
        self.record().resets
    }

    pub fn fail(&self, reason: Option<&str>) {
        self.record().failure = reason.map(str::to_owned);
    }

    /// Poisoning only means a test panicked while holding this; the record is still readable and
    /// the panic is the failure worth reporting, not a second one from here.
    fn record(&self) -> std::sync::MutexGuard<'_, Record> {
        self.record.lock().unwrap_or_else(|held| held.into_inner())
    }
}

impl Gamma for FakeGamma {
    fn apply(&mut self, kelvin: u32) -> Result<(), String> {
        let mut record = self.record();
        match &record.failure {
            Some(reason) => Err(reason.clone()),
            None => {
                record.applied.push(kelvin);
                Ok(())
            }
        }
    }

    fn reset(&mut self) -> Result<(), String> {
        self.record().resets += 1;
        Ok(())
    }
}
