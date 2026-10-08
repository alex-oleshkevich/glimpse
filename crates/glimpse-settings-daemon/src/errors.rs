use std::process::ExitCode;

pub fn exit_code(_error: &anyhow::Error) -> ExitCode {
    ExitCode::FAILURE
}
