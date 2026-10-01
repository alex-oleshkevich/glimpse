use std::process::ExitCode;

use crate::gamma::Unavailable;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    Ok = 0,
    Failed = 1,
    Gamma = 4,
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        Self::from(exit as u8)
    }
}

pub fn exit(error: &anyhow::Error) -> Exit {
    match error.downcast_ref::<Unavailable>() {
        Some(Unavailable::Unsupported) => Exit::Gamma,
        _ => Exit::Failed,
    }
}

#[cfg(test)]
mod tests;
