use anyhow::{Result, bail};

use super::Session;

pub async fn monitor(_session: &Session) -> Result<()> {
    bail!("monitor is not implemented yet")
}
