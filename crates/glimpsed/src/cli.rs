use clap::Parser;
use glimpse_utils::{ConfigArg, LogArgs, SocketArg};
use shadow_rs::shadow;

shadow!(build);

#[derive(Debug, Parser)]
#[command(
    name = "glimpsed",
    about = "The glimpse session daemon.",
    version = build::VERSION
)]
pub struct Cli {
    #[command(flatten)]
    pub config: ConfigArg,

    #[command(flatten)]
    pub socket: SocketArg,

    #[command(flatten)]
    pub log: LogArgs,

    #[command(flatten)]
    pub color: colorchoice_clap::Color,

    #[arg(
        long,
        value_name = "SERVICES",
        value_delimiter = ',',
        value_parser = service_name,
        conflicts_with = "without",
        help = "Comma-separated allowlist; every other service stays unregistered"
    )]
    pub only: Vec<String>,

    #[arg(
        long,
        value_name = "SERVICES",
        value_delimiter = ',',
        value_parser = service_name,
        help = "Comma-separated denylist"
    )]
    pub without: Vec<String>,
}

// `--only ''` would otherwise read as "no filter" and start every service.
fn service_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("service name is empty".into());
    }
    Ok(name.to_owned())
}
