mod capture;
mod cli;
mod errors;
mod marks;
mod measure;
mod session;

use std::io::Write as _;
use std::process::ExitCode;

use anyhow::{Context as _, Result};
use clap::Parser;
use cli::Cli;
use glimpse_utils::{init_app_tracing, init_translations};

fn main() -> ExitCode {
    let cli = Cli::parse();
    cli.color.write_global();
    match run(cli) {
        Ok(()) => errors::Exit::Ok.into(),
        Err(error) => {
            anstream::eprintln!("glimpse-ruler: {error:#}");
            errors::exit(&error).into()
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    init_app_tracing(&cli.log.log, cli.log.log_format);
    let document = glimpse_config::load(cli.config.as_deref())?;
    init_translations(document.regional.language());
    let ruler = &document.ruler;
    let settings = session::Settings {
        radius: cli.lens_radius.unwrap_or(ruler.lens_radius).clamp(40, 400) as f32,
        max_zoom: cli.max_zoom.unwrap_or(ruler.max_zoom).clamp(2, 64),
    };
    let segments = measure::measure(&document, settings)?;
    let lines = measure::ndjson(&segments).ok_or(errors::Cancelled)?;
    write!(std::io::stdout(), "{lines}").context("cannot write the measurements")?;
    Ok(())
}
