use std::process::ExitCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    Ok = 0,
    Failed = 1,
    Config = 3,
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        Self::from(exit as u8)
    }
}

pub fn exit(error: &anyhow::Error) -> Exit {
    match error.downcast_ref::<glimpse_config::ConfigError>() {
        Some(_) => Exit::Config,
        None => Exit::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_failures_keep_the_generic_code() {
        assert_eq!(exit(&anyhow::anyhow!("failed")), Exit::Failed);
    }
}
