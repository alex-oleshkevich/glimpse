//! One module per subcommand. Everything shared lives here: the session they run against, and the
//! one place stdout is written.

mod call;
mod config;
mod doctor;
mod get;
mod monitor;
mod services;
mod topics;
mod watch;

pub use call::call;
pub use config::{config_path, config_show, config_validate};
pub use doctor::doctor;
pub use get::get;
pub use monitor::monitor;
pub use services::services;
pub use topics::topics;
pub use watch::watch;

use std::io::{self, Write};

use anyhow::Result;
use glimpse_ipc::Client;

/// Stands in for a field with nothing in it, so a column is never blank about it.
const ABSENT: &str = "-";

pub struct Session {
    pub client: Client,
}

/// A declared topic with no value is a different answer from an unknown one, and exits 0. It says
/// so on stderr, leaving stdout empty, so a script reading the value sees nothing rather than a
/// sentence it would have to recognise.
fn absent(topic: &str, json: bool) -> Result<()> {
    match json {
        // A passthrough has to answer on stdout, and `null` is what the daemon means by no value.
        true => write_line("null").map(|_| ())?,
        false => anstream::eprintln!("glimpsectl: `{topic}` has no value yet"),
    }
    Ok(())
}

enum Flow {
    Continue,
    Stop,
}

fn write_line(text: &str) -> Result<Flow> {
    // `anstream` strips the styling when stdout is not a terminal, so a pipe gets plain text
    // without any command having to ask whether it is being piped.
    match writeln!(anstream::stdout(), "{text}") {
        Ok(()) => Ok(Flow::Continue),
        // Rust ignores SIGPIPE, so `glimpsectl watch … | head -1` arrives here rather than
        // panicking out of `println!` with exit 101, which no exit table contains.
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(Flow::Stop),
        Err(error) => Err(error.into()),
    }
}
