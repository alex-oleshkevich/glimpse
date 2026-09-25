use std::fmt;
use std::process::ExitCode;

mod codes {
    pub const FAILED: u8 = 1;
    pub const DENO_NOT_FOUND: u8 = 2;
}

#[derive(Debug)]
pub struct DenoNotFound;

impl fmt::Display for DenoNotFound {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("deno not found; install it or set GLIMPSE_DENO")
    }
}

impl std::error::Error for DenoNotFound {}

fn code(error: &anyhow::Error) -> u8 {
    if error.downcast_ref::<DenoNotFound>().is_some() {
        codes::DENO_NOT_FOUND
    } else {
        codes::FAILED
    }
}

pub fn exit_code(error: &anyhow::Error) -> ExitCode {
    ExitCode::from(code(error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_deno_has_its_own_code_through_context() {
        let error = anyhow::Error::new(DenoNotFound).context("launching applet");
        assert_eq!(code(&error), codes::DENO_NOT_FOUND);
        assert_eq!(code(&anyhow::anyhow!("failed")), codes::FAILED);
    }
}
