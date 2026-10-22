use std::process::ExitCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    Ok = 0,
    Failed = 1,
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        Self::from(exit as u8)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{0} of the checks failed")]
pub struct ChecksFailed(pub usize);

pub fn exit(_error: &anyhow::Error) -> Exit {
    Exit::Failed
}
