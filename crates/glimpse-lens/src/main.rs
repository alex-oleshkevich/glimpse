mod cli;
mod clipboard;
mod errors;
mod lens;
mod marks;
mod measure;
mod overlay;
mod pick;
mod session;

use std::io::Write as _;
use std::process::ExitCode;

use anyhow::{Context as _, Result};
use clap::Parser;
use cli::{Cli, Tool};
use glimpse_utils::{init_app_tracing, init_translations};

const MIN_RADIUS: u32 = 10;
const MAX_RADIUS: u32 = 200;
const MIN_ZOOM: u32 = 1;
const MAX_ZOOM: u32 = 200;

fn main() -> ExitCode {
    let cli = Cli::parse();
    cli.color.write_global();
    if let Err(error) = cli.validate() {
        error.exit();
    }

    match run(cli) {
        Ok(()) => errors::Exit::Ok.into(),
        Err(error) => {
            anstream::eprintln!("glimpse-lens: {error:#}");
            errors::exit(&error).into()
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    init_app_tracing(&cli.log.log, cli.log.log_format);

    init_translations(None);

    match cli.tool {
        Some(Tool::Ruler) => {
            let settings = session::Settings {
                radius: cli.clamped_radius(),
                zoom: cli.clamped_zoom(),
            };

            let segments = measure::measure(settings)?;
            let lines = measure::ndjson(&segments).ok_or(errors::Cancelled)?;
            write!(std::io::stdout(), "{lines}").context("cannot write the measurements")?;
            if cli.copy
                && let Some(distance) = measure::last_distance(&segments)
            {
                clipboard::copy(&distance)?;
            }
        }
        tool => {
            let format = cli.format.unwrap_or(glimpse_config::ColorFormat::Hex);
            let settings = lens::Settings {
                format: tool.map(|_| format),
                radius: cli.clamped_radius(),
                zoom: cli.clamped_zoom(),
            };

            let result = pick::pick(settings);
            if tool.is_some() {
                let color = result?;
                let line = match cli.json {
                    true => pick::json(format, color),
                    false => format.render(color),
                };
                writeln!(std::io::stdout(), "{line}").context("cannot write the color")?;
                if cli.copy {
                    clipboard::copy(&line)?;
                }
            } else if let Err(error) = result
                && error.downcast_ref::<errors::Cancelled>().is_none()
            {
                return Err(error);
            }
        }
    }
    Ok(())
}
