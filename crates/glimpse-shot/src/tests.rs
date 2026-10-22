use clap::Parser as _;

use super::*;

fn editor_exit(args: &[&str]) -> Exit {
    let cli = Cli::try_parse_from(std::iter::once("glimpse-shot").chain(args.iter().copied()))
        .expect("parses");
    let terminal = Terminal {
        stdin: false,
        stdout: false,
    };
    errors::exit(&open_editor(&cli.gui, terminal).expect_err("refused before any capture"))
}

#[test]
fn only_the_default_command_constructs_the_app() {
    let command = |args: &[&str]| {
        Cli::try_parse_from(std::iter::once("glimpse-shot").chain(args.iter().copied()))
            .expect("parses")
            .command
    };
    assert!(opens_app(command(&[]).as_ref()));
    assert!(opens_app(command(&["--screen", "--json"]).as_ref()));
    assert!(opens_app(command(&["--gapplication-service"]).as_ref()));
    for args in [
        &["capture"][..],
        &["list"][..],
        &["prime"][..],
        &["portal"][..],
        &["hold"][..],
    ] {
        assert!(!opens_app(command(args).as_ref()), "{args:?}");
    }
}

#[test]
fn default_command_usage_rows_exit_two_and_the_rest_exit_one() {
    assert_eq!(editor_exit(&["--json"]), Exit::Usage);
    assert_eq!(editor_exit(&["--prompt", "where?"]), Exit::Usage);
    assert_eq!(editor_exit(&["--timeout", "30"]), Exit::Usage);
    assert_eq!(editor_exit(&["--annotate", "marks.json"]), Exit::Usage);
    assert_eq!(
        editor_exit(&["--window", "--annotate", "marks.json"]),
        Exit::Usage
    );
}

#[test]
fn a_usage_error_exits_two() {
    let terminal = Terminal {
        stdin: false,
        stdout: false,
    };
    let exit_of = |args: &[&str]| {
        let cli = Cli::try_parse_from(std::iter::once("glimpse-shot").chain(args.iter().copied()))
            .expect("parses");
        let Some(Command::Capture(args)) = cli.command else {
            panic!("expected capture");
        };
        errors::exit(&capture(&args, terminal).expect_err("refused"))
    };

    assert_eq!(
        exit_of(&["capture", "--region", "garbage", "--json"]),
        Exit::Usage
    );
    assert_eq!(exit_of(&["capture", "--window", "firefox"]), Exit::Usage);
    assert_eq!(exit_of(&["capture", "-o", "-", "--json"]), Exit::Usage);
}
