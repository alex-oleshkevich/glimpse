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

use anyhow::Result;
use glimpse_ipc::Client;

use crate::render;

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
        true => render::print("null").map(|_| ())?,
        false => anstream::eprintln!("glimpsectl: `{topic}` has no value yet"),
    }
    Ok(())
}
