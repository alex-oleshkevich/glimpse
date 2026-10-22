use clap::CommandFactory;
use glimpsectl::cli::{
    AppletCommand, AppletLanguage, Cli, Command, ConfigCommand, NightLightCommand, duration,
    temperature,
};

#[test]
fn the_argument_surface_is_well_formed() {
    Cli::command().debug_assert();
}

#[test]
fn a_missing_subcommand_is_a_usage_error() {
    let error = Cli::try_parse_from(["glimpsectl"]).expect_err("a subcommand is required");
    assert_eq!(error.exit_code(), 2);
}

#[test]
fn json_is_accepted_after_the_subcommand() {
    let cli = Cli::try_parse_from(["glimpsectl", "night-light", "status", "--json"])
        .expect("a global flag follows its subcommand");
    assert!(cli.json);
}

#[test]
fn config_doctor_and_installed_applets_run_without_a_bus() {
    assert!(!Command::Doctor.needs_session_bus());
    assert!(!Command::Config(ConfigCommand::Path).needs_session_bus());
    assert!(
        !Command::Applet(AppletCommand::List {
            watch: false,
            installed: true
        })
        .needs_session_bus()
    );
    assert!(Command::NightLight(NightLightCommand::Pause).needs_session_bus());
}

#[test]
fn the_session_command_is_hidden_from_the_top_level_help() {
    let command = Cli::command();
    let session = command
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "session")
        .expect("the session subcommand still exists for parsing");
    assert!(session.is_hide_set());
}

#[test]
fn session_declare_still_parses() {
    let error = Cli::try_parse_from(["glimpsectl", "session", "declare", "--help"])
        .expect_err("--help exits through the error path");
    assert_eq!(error.exit_code(), 0);
}

fn run(args: &[&str]) -> Result<Cli, clap::Error> {
    Cli::try_parse_from(["glimpsectl", "applet", "run"].iter().chain(args))
}

#[test]
fn running_an_applet_needs_no_session_bus() {
    let cli = run(&["me.x.Applet"]).unwrap();
    assert!(!cli.command.needs_session_bus());
}

#[test]
fn new_applets_default_to_typescript_and_accept_every_language() {
    fn language(cli: Cli) -> AppletLanguage {
        match cli.command {
            Command::Applet(AppletCommand::New(args)) => args.language,
            _ => panic!("expected a new applet command"),
        }
    }
    assert_eq!(
        language(Cli::try_parse_from(["glimpsectl", "applet", "new", "--yes"]).unwrap()),
        AppletLanguage::Ts
    );
    for (value, expected) in [
        ("rs", AppletLanguage::Rs),
        ("ts", AppletLanguage::Ts),
        ("py", AppletLanguage::Py),
        ("go", AppletLanguage::Go),
    ] {
        assert_eq!(
            language(
                Cli::try_parse_from(["glimpsectl", "applet", "new", "--language", value, "--yes",])
                    .unwrap(),
            ),
            expected
        );
    }
    assert_eq!(
        language(Cli::try_parse_from(["glimpsectl", "applet", "new", "--yes"]).unwrap()),
        AppletLanguage::Ts
    );
    let error = Cli::try_parse_from(["glimpsectl", "applet", "new", "--language", "rust", "--yes"])
        .expect_err("only the four template names are accepted");
    assert_eq!(error.exit_code(), 2);
}

#[test]
fn zero_scroll_steps_are_a_usage_error() {
    let error = Cli::try_parse_from([
        "glimpsectl",
        "applet",
        "scroll",
        "clock",
        "down",
        "--steps",
        "0",
    ])
    .expect_err("scrolling must have at least one step");
    assert_eq!(error.exit_code(), 2);
}

#[test]
fn a_temperature_is_kelvin_or_clear_and_never_zero() {
    assert_eq!(temperature("4000"), Ok(4000));
    assert_eq!(temperature("clear"), Ok(0));
    assert!(temperature("0").is_err());
    assert!(temperature("warm").is_err());
}

#[test]
fn a_duration_reads_bare_seconds_and_the_three_units() {
    assert_eq!(duration("90"), Ok(90));
    assert_eq!(duration("90s"), Ok(90));
    assert_eq!(duration("15m"), Ok(900));
    assert_eq!(duration("2h"), Ok(7200));
    assert!(duration("0").is_err());
    assert!(duration("5d").is_err());
    assert!(duration("m").is_err());
    assert!(duration("9999999h").is_err());
}

#[test]
fn canonical_commands_parse_to_their_variants() {
    use glimpsectl::cli::AppearanceCommand;
    assert!(matches!(
        Cli::try_parse_from(["glimpsectl", "applet", "list"])
            .unwrap()
            .command,
        Command::Applet(AppletCommand::List {
            installed: false,
            ..
        })
    ));
    assert!(matches!(
        Cli::try_parse_from(["glimpsectl", "appearance", "set", "dark"])
            .unwrap()
            .command,
        Command::Appearance(AppearanceCommand::Set { .. })
    ));
    assert!(matches!(
        Cli::try_parse_from(["glimpsectl", "night-light", "temperature", "--clear"])
            .unwrap()
            .command,
        Command::NightLight(NightLightCommand::Temperature { clear: true, .. })
    ));
    assert!(Cli::try_parse_from(["glimpsectl", "night-light", "temperature", "0"]).is_err());
    assert!(
        Cli::try_parse_from([
            "glimpsectl",
            "night-light",
            "temperature",
            "4000",
            "--clear"
        ])
        .is_err()
    );
    assert!(Cli::try_parse_from(["glimpsectl", "clipboard", "remove", "1"]).is_ok());
}

#[test]
fn global_panel_and_bar_do_not_conflict() {
    let cli = Cli::try_parse_from([
        "glimpsectl",
        "applet",
        "run",
        "--bar",
        "2",
        "--panel",
        "me.test.Panel",
    ])
    .unwrap();
    assert_eq!(cli.panel_app_id, "me.test.Panel");
}

#[test]
fn canonical_help_and_completions_need_no_bus() {
    let mut command = Cli::command();
    let help = command.render_help().to_string();
    for noun in [
        "night-light",
        "appearance",
        "geolocation",
        "applet",
        "dev",
        "completions",
    ] {
        assert!(help.contains(noun), "{help}");
    }
    for shell in [
        clap_complete::Shell::Bash,
        clap_complete::Shell::Fish,
        clap_complete::Shell::Zsh,
    ] {
        let cli =
            Cli::try_parse_from(["glimpsectl", "completions", shell.to_string().as_str()]).unwrap();
        assert!(!cli.command.needs_session_bus());
        let mut output = Vec::new();
        clap_complete::generate(
            shell,
            &mut glimpsectl::cli::completion_command(),
            "glimpsectl",
            &mut output,
        );
        assert!(!output.is_empty());
        let generated = std::process::Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
            .args(["completions", shell.to_string().as_str()])
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
            .output()
            .unwrap();
        assert!(generated.status.success(), "{:?}", generated);
        let generated = String::from_utf8(generated.stdout).unwrap();
        for hidden in ["sunset", "session"] {
            assert!(!generated.contains(hidden), "{shell}: {hidden}");
        }
    }
}

#[test]
fn completion_tree_keeps_every_visible_command_and_argument() {
    fn compare(source: &clap::Command, completion: &clap::Command) {
        let arguments: Vec<_> = source
            .get_arguments()
            .filter(|arg| !arg.is_hide_set())
            .map(|arg| arg.get_id())
            .collect();
        assert_eq!(
            arguments,
            completion
                .get_arguments()
                .map(|arg| arg.get_id())
                .collect::<Vec<_>>()
        );
        let commands: Vec<_> = source
            .get_subcommands()
            .filter(|command| !command.is_hide_set())
            .collect();
        assert_eq!(commands.len(), completion.get_subcommands().count());
        for (source, completion) in commands.into_iter().zip(completion.get_subcommands()) {
            assert_eq!(source.get_name(), completion.get_name());
            compare(source, completion);
        }
    }
    let command = glimpsectl::cli::completion_command();
    compare(&Cli::command(), &command);
    command.debug_assert();
}
