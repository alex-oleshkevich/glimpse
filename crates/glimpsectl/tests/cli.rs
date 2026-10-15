use clap::CommandFactory;
use glimpsectl::cli::{
    AppletsCommand, Cli, Command, ConfigCommand, Mode, SunsetCommand, duration, temperature,
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
    let cli = Cli::try_parse_from(["glimpsectl", "sunset", "status", "--json"])
        .expect("a global flag follows its subcommand");
    assert!(cli.json);
}

#[test]
fn settings_accepts_status_two_modes_and_resume() {
    for args in [
        vec!["status", "--json"],
        vec!["mode", "light"],
        vec!["mode", "dark"],
        vec!["resume"],
    ] {
        let cli = Cli::try_parse_from(["glimpsectl", "settings"].into_iter().chain(args)).unwrap();
        assert!(cli.command.needs_session_bus());
    }
    for mode in ["auto", "system", "schedule"] {
        assert!(Cli::try_parse_from(["glimpsectl", "settings", "mode", mode]).is_err());
    }
}

#[test]
fn every_mode_spells_itself_the_way_the_document_does() {
    assert_eq!(Mode::Off.as_str(), "off");
    assert_eq!(Mode::Automatic.as_str(), "automatic");
    assert_eq!(Mode::Schedule.as_str(), "schedule");
}

#[test]
fn config_doctor_and_applets_run_without_a_bus() {
    assert!(!Command::Doctor.needs_session_bus());
    assert!(!Command::Config(ConfigCommand::Path).needs_session_bus());
    assert!(!Command::Applets(AppletsCommand::List).needs_session_bus());
    assert!(Command::Sunset(SunsetCommand::Status).needs_session_bus());
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
    Cli::try_parse_from(["glimpsectl", "applets", "run"].iter().chain(args))
}

#[test]
fn running_an_applet_needs_no_session_bus() {
    let cli = run(&["me.x.Applet"]).unwrap();
    assert!(!cli.command.needs_session_bus());
}

#[test]
fn zero_scroll_steps_are_a_usage_error() {
    let error = Cli::try_parse_from([
        "glimpsectl",
        "panel",
        "scroll",
        "DP-1",
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
fn canonical_commands_and_legacy_adapters_have_distinct_semantics() {
    use glimpsectl::cli::{AppearanceCommand, AppletCommand, ClipboardCommand, NightLightCommand};
    assert!(matches!(
        Cli::try_parse_from(["glimpsectl", "applet", "list"])
            .unwrap()
            .command,
        Command::Applet(AppletCommand::List {
            installed: false,
            ..
        })
    ));
    let old = Cli::try_parse_from(["glimpsectl", "applets", "list"]).unwrap();
    assert!(old.deprecation().is_some());
    assert!(matches!(
        old.command,
        Command::Applets(AppletsCommand::List)
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
    let old = Cli::try_parse_from(["glimpsectl", "clipboard", "forget", "1"]).unwrap();
    assert!(old.deprecation().is_some());
    assert!(matches!(
        old.command,
        Command::Clipboard(ClipboardCommand::Forget(_))
    ));
    assert!(
        Cli::try_parse_from(["glimpsectl", "clipboard", "remove", "1"])
            .unwrap()
            .deprecation()
            .is_none()
    );
}

#[test]
fn global_panel_and_legacy_numeric_panel_do_not_conflict() {
    for args in [
        vec![
            "--panel",
            "me.test.Panel",
            "applets",
            "run",
            ".",
            "--panel",
            "2",
        ],
        vec![
            "applets",
            "run",
            ".",
            "--panel=2",
            "--panel",
            "me.test.Panel",
        ],
        vec!["applets", "--panel", "me.test.Panel", "run", "--panel", "2"],
    ] {
        let cli = Cli::try_parse_from(["glimpsectl"].into_iter().chain(args)).unwrap();
        assert_eq!(cli.panel_app_id, "me.test.Panel");
        assert!(
            matches!(cli.command, Command::Applets(AppletsCommand::Run(args)) if args.panel == 2)
        );
    }
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
    assert!(
        Cli::try_parse_from(["glimpsectl", "applets", "run", "--panel", "999999999999999"])
            .is_err()
    );
}

#[test]
fn compatibility_normalization_respects_values_and_double_dash() {
    let cli =
        Cli::try_parse_from(["glimpsectl", "applets", "run", "--applet-config=--panel=2"]).unwrap();
    assert!(
        matches!(cli.command, Command::Applets(AppletsCommand::Run(args)) if args.applet_config == Some("--panel=2".into()) && args.panel == 0)
    );
    let cli = Cli::try_parse_from(["glimpsectl", "applets", "run", "--", "--panel=2"]).unwrap();
    assert!(
        matches!(cli.command, Command::Applets(AppletsCommand::Run(args)) if args.target == "--panel=2" && args.panel == 0)
    );
}

#[test]
fn all_retained_old_paths_parse_and_warn() {
    for args in [
        vec!["sunset", "mode", "automatic"],
        vec!["sunset", "temperature", "clear"],
        vec!["settings", "status"],
        vec!["settings", "mode", "dark"],
        vec!["settings", "resume"],
        vec!["applets", "new", "--yes"],
        vec!["applets", "inspect", "me.test.Applet"],
        vec!["applets", "restart", "me.test.Applet"],
        vec!["panel", "--app-id", "me.test.Panel", "applets"],
        vec!["panel", "click", "DP-1", "clock"],
        vec!["panel", "scroll", "DP-1", "clock", "down"],
        vec!["panel", "popover", "open", "DP-1", "clock"],
        vec!["panel", "popovers", "close"],
    ] {
        let cli = Cli::try_parse_from(["glimpsectl"].into_iter().chain(args)).unwrap();
        assert!(cli.deprecation().is_some());
    }
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
    for hidden in [
        "sunset",
        "settings",
        "applets",
        "screenshot",
        "audio",
        "brightness",
        "media",
        "logs",
        "restart",
    ] {
        assert!(
            !command
                .get_subcommands()
                .any(|subcommand| subcommand.get_name() == hidden && !subcommand.is_hide_set())
        );
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
        clap_complete::generate(shell, &mut Cli::command(), "glimpsectl", &mut output);
        assert!(!output.is_empty());
    }
}
