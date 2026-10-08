mod cli;
mod errors;
mod lens;
mod marks;
mod measure;
mod pick;
mod session;

use std::io::Write as _;
use std::process::ExitCode;

use anyhow::{Context as _, Result};
use clap::Parser;
use cli::{Cli, Tool};
use glimpse_utils::{init_app_tracing, init_translations};

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

    let document = glimpse_config::load(cli.config.as_deref())?;
    init_translations(document.regional.language());

    match cli.tool {
        Some(Tool::Ruler) => {
            let ruler = &document.ruler;
            let settings = session::Settings {
                radius: cli.lens_radius.unwrap_or(ruler.lens_radius).clamp(40, 400) as f32,
                max_zoom: cli.max_zoom.unwrap_or(ruler.max_zoom).clamp(2, 64),
            };

            let segments = measure::measure(&document, settings)?;
            let lines = measure::ndjson(&segments).ok_or(errors::Cancelled)?;
            write!(std::io::stdout(), "{lines}").context("cannot write the measurements")?;
        }
        tool => {
            let picker = &document.color_picker;
            let format = cli.format.unwrap_or(picker.format);
            let settings = lens::Settings {
                format: tool.map(|_| format),
                radius: cli.lens_radius.unwrap_or(picker.lens_radius).clamp(40, 400) as f32,
                max_zoom: cli.max_zoom.unwrap_or(picker.max_zoom).clamp(2, 64),
            };

            let result = pick::pick(&document, settings);
            if tool.is_some() {
                let color = result?;
                let line = match cli.json {
                    true => pick::json(format, color),
                    false => format.render(color),
                };
                writeln!(std::io::stdout(), "{line}").context("cannot write the color")?;
            } else if let Err(error) = result
                && error.downcast_ref::<errors::Cancelled>().is_none()
            {
                return Err(error);
            }
        }
    }
    Ok(())
}
