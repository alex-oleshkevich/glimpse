use clap::CommandFactory;

use super::*;

use crate::errors::Exit;

use glimpse_config::PaletteColor;

use glimpse_shot::doc::units::{Logical, Rect};

use glimpse_shot::doc::wire::{self, AnnotationV1, MAX_MARKS, MAX_PATH_POINTS, MAX_TEXT_CHARS};

const PIPES: Terminal = Terminal {
    stdin: false,
    stdout: false,
};

const TTY: Terminal = Terminal {
    stdin: true,
    stdout: true,
};

fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
    super::parse(
        std::iter::once("glimpse-shot")
            .chain(args.iter().copied())
            .map(OsString::from),
    )
}

fn clap_code(args: &[&str]) -> i32 {
    parse(args).expect_err("a usage error").exit_code()
}

fn capture(args: &[&str]) -> CaptureArgs {
    match parse(&[&["capture"], args].concat())
        .expect("parses")
        .command
    {
        Some(Command::Capture(capture)) => capture,
        other => panic!("expected capture, got {other:?}"),
    }
}

fn gui(args: &[&str]) -> GuiArgs {
    let cli = parse(args).expect("parses");
    assert!(cli.command.is_none());
    cli.gui
}

mod annotate;
mod guide;
mod parse;
