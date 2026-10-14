use std::future::Future;
use std::pin::Pin;
use std::process::Stdio;

use super::history::Measurement;

const REASON: usize = 240;
const CANCELLED: i32 = 4;

pub type Measured = Pin<Box<dyn Future<Output = Result<Vec<Measurement>, String>> + Send>>;

pub trait Runner: Send + Sync + 'static {
    fn measure(&self) -> Measured;
}

pub struct ProcessRunner {
    program: String,
}

impl ProcessRunner {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }
}

impl Runner for ProcessRunner {
    fn measure(&self) -> Measured {
        let mut command = tokio::process::Command::new(&self.program);
        command
            .args(["--tool", "ruler"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let program = self.program.clone();
        Box::pin(async move {
            let output = command
                .output()
                .await
                .map_err(|error| format!("cannot run {program}: {error}"))?;
            answer(output.status.code(), &output.stdout, &output.stderr)
        })
    }
}

fn answer(code: Option<i32>, stdout: &[u8], stderr: &[u8]) -> Result<Vec<Measurement>, String> {
    match code {
        Some(0) => stdout
            .split(|&byte| byte == b'\n')
            .filter(|segment| !segment.is_empty())
            .map(|segment| {
                serde_json::from_slice::<Measurement>(segment)
                    .map_err(|error| format!("the ruler printed something unreadable: {error}"))
            })
            .collect(),
        Some(CANCELLED) => Ok(Vec::new()),
        _ => {
            let reason = String::from_utf8_lossy(stderr);
            let reason = glimpse_utils::clean(reason.trim(), REASON);
            Err(match reason.is_empty() {
                true => "the ruler failed".to_owned(),
                false => reason,
            })
        }
    }
}

#[cfg(test)]
mod tests;
