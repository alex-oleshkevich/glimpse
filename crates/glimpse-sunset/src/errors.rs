use std::process::ExitCode;

use glimpse_ipc::NoRuntimeDir;

mod code {
    pub const SOCKET_IN_USE: u8 = 3;
    pub const NO_RUNTIME_DIR: u8 = 4;
}

pub fn exit_code(error: &anyhow::Error) -> ExitCode {
    ExitCode::from(code(error))
}

fn code(error: &anyhow::Error) -> u8 {
    match error.is::<NoRuntimeDir>() {
        true => code::NO_RUNTIME_DIR,
        false => code::SOCKET_IN_USE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_runtime_directory_is_its_own_code() {
        assert_eq!(code(&anyhow::Error::new(NoRuntimeDir)), 4);
    }

    #[test]
    fn a_context_layer_does_not_change_the_code() {
        let error = anyhow::Error::new(NoRuntimeDir).context("binding the socket");
        assert_eq!(code(&error), 4);
    }
}
