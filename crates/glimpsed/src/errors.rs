use std::process::ExitCode;

use glimpse_ipc::ServerError;

use crate::daemon::DaemonError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    Ok = 0,
    Failed = 1,
    // 2 is skipped: clap owns it, and that is where `--only` together with `--without` lands.
    AlreadyRunning = 3,
    Config = 4,
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        Self::from(exit as u8)
    }
}

/// `downcast_ref` sees through every `.context(…)` layer, which is what stops a message added
/// upstream from changing the code a supervisor reads.
pub fn exit(error: &anyhow::Error) -> Exit {
    if let Some(ServerError::AlreadyRunning(_)) = error.downcast_ref::<ServerError>() {
        return Exit::AlreadyRunning;
    }

    match error.downcast_ref::<DaemonError>() {
        Some(DaemonError::IpcServer(ServerError::AlreadyRunning(_))) => Exit::AlreadyRunning,
        Some(DaemonError::Config(_)) | Some(DaemonError::Socket(_)) => Exit::Config,
        _ => Exit::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The case a supervisor acts on: a second instance is not a crash loop to restart, it is a
    /// daemon that is already doing the job.
    #[test]
    fn a_second_instance_is_distinguishable_from_any_other_failure() {
        let busy = anyhow::Error::new(DaemonError::IpcServer(ServerError::AlreadyRunning(
            "/run/user/1000/glimpse/glimpsed.sock".into(),
        )))
        .context("while starting the daemon");

        assert_eq!(exit(&busy), Exit::AlreadyRunning);
        assert_eq!(exit(&anyhow::anyhow!("something else")), Exit::Failed);
    }

    #[test]
    fn a_broken_configuration_has_a_code_of_its_own() {
        let bad = anyhow::Error::new(DaemonError::Config("line 4: expected a table".into()));
        assert_eq!(exit(&bad), Exit::Config);
    }

    /// The message names the socket rather than the layer that failed; a code alone does not tell
    /// anyone which daemon is already there.
    #[test]
    fn the_already_running_message_names_the_socket() {
        let error = DaemonError::IpcServer(ServerError::AlreadyRunning("/tmp/some.sock".into()));
        assert!(error.to_string().contains("/tmp/some.sock"), "{error}");
    }
}
