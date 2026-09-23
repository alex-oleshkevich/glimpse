mod capture;
mod cli;
mod errors;
mod lens;
mod pick;

use std::io::Write as _;
use std::process::ExitCode;

use anyhow::{Context as _, Result};
use clap::Parser;
use cli::Cli;
use glimpse_utils::init_app_tracing;

fn main() -> ExitCode {
    let cli = Cli::parse();
    cli.color.write_global();
    match run(cli) {
        Ok(()) => errors::Exit::Ok.into(),
        Err(error) => {
            anstream::eprintln!("glimpse-picker: {error:#}");
            errors::exit(&error).into()
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    init_app_tracing(&cli.log.log, cli.log.log_format);
    let document = glimpse_config::load(cli.config.as_deref())?;
    let picker = &document.color_picker;
    let settings = lens::Settings {
        format: cli.format.unwrap_or(picker.format),
        radius: cli.lens_radius.unwrap_or(picker.lens_radius).clamp(40, 400) as f32,
        max_zoom: cli.max_zoom.unwrap_or(picker.max_zoom).clamp(2, 64),
    };
    let color = pick::pick(&document, settings)?;
    let line = match cli.json {
        true => pick::json(settings.format, color),
        false => settings.format.render(color),
    };
    writeln!(std::io::stdout(), "{line}").context("cannot write the color")?;
    Ok(())
}
