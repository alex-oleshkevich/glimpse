use super::*;

#[test]
fn a_negative_geometry_is_attached_to_region_and_nothing_else() {
    let args = |list: &[&str]| {
        attach_negative_geometry(list.iter().map(OsString::from))
            .into_iter()
            .map(|arg| arg.into_string().expect("utf-8"))
            .collect::<Vec<_>>()
    };

    assert_eq!(
        args(&["capture", "--region", "-100,50 400x300", "-o", "-"]),
        ["capture", "--region=-100,50 400x300", "-o", "-"]
    );
    assert_eq!(
        args(&["--region", "-o", "x.png"]),
        ["--region", "-o", "x.png"]
    );
    assert_eq!(args(&["-o", "-5.png"]), ["-o", "-5.png"]);
}

#[test]
fn every_tool_is_a_starting_tool_and_an_unknown_one_lists_them() {
    for (name, tool) in [
        ("crop", Tool::Crop),
        ("arrow", Tool::Arrow),
        ("line", Tool::Line),
        ("freehand", Tool::Freehand),
        ("highlighter", Tool::Highlighter),
        ("rect", Tool::Rect),
        ("filled-rect", Tool::FilledRect),
        ("oval", Tool::Oval),
        ("number", Tool::Number),
        ("text", Tool::Text),
        ("blur", Tool::Blur),
    ] {
        assert_eq!(gui(&["--tool", name]).tool, Some(tool), "{name}");
    }
    assert_eq!(gui(&["--window", "--tool", "crop"]).tool, Some(Tool::Crop));
    assert_eq!(gui(&[]).tool, None);
    let refused = parse(&["--tool", "pen"]).expect_err("refused");
    assert_eq!(refused.exit_code(), 2);
    let message = refused.to_string();
    for name in [
        "crop",
        "arrow",
        "line",
        "freehand",
        "highlighter",
        "filled-rect",
        "blur",
    ] {
        assert!(message.contains(name), "{message}");
    }
}

#[test]
fn the_argument_surface_is_well_formed() {
    Cli::command().debug_assert();
}

#[test]
fn the_hidden_service_flag_is_accepted_and_handed_to_gtk_alone() {
    let cli = parse(&["--gapplication-service"]).expect("parses");
    assert!(cli.command.is_none());
    assert!(cli.gui.gapplication_service);
    assert!(validate_gui(&cli.gui, PIPES).is_ok());
    let help = parse(&["--help"]).expect_err("help exits").to_string();
    assert!(!help.contains("gapplication"), "{help}");

    let argv = ["/usr/bin/glimpse-shot", "--gapplication-service"].map(OsString::from);
    assert_eq!(
        crate::gui::app::gtk_args(&cli.gui, argv.to_vec()),
        argv.to_vec()
    );
    let forwarded = ["glimpse-shot", "--screen", "--json"].map(OsString::from);
    assert_eq!(
        crate::gui::app::gtk_args(&gui(&["--screen", "--json"]), forwarded.to_vec()),
        forwarded.to_vec(),
        "a GUI invocation is forwarded whole to the primary instance"
    );
}

#[test]
fn clap_rows_exit_two() {
    assert_eq!(
        clap_code(&["capture", "--no-save", "--copy", "-o", "x.png"]),
        2
    );
    assert_eq!(clap_code(&["capture", "--no-save"]), 2);
    assert_eq!(clap_code(&["capture", "--screen", "--window"]), 2);
    assert_eq!(clap_code(&["--screen", "DP-2", "--region", "0,0 1x1"]), 2);
    assert_eq!(clap_code(&["shot.png", "--screen"]), 2);
    assert_eq!(clap_code(&["--window", "42", "shot.png"]), 2);
    assert_eq!(clap_code(&["--region", "0,0 1x1", "shot.png"]), 2);
    for flag in [
        "--flash",
        "--no-flash",
        "--notify",
        "--no-notify",
        "--editor=overlay",
        "--prompt=x",
        "--timeout=5",
    ] {
        assert_eq!(clap_code(&["capture", flag]), 2, "{flag}");
    }
    assert_eq!(clap_code(&["capture", "--delay", "61"]), 2);
    assert_eq!(clap_code(&["--delay", "61"]), 2);
}

#[test]
fn global_options_go_before_or_after_the_subcommand() {
    for args in [
        &["--config", "/x.toml", "capture", "--screen", "DP-9"][..],
        &["capture", "--config", "/x.toml", "--screen", "DP-9"][..],
        &[
            "-c", "/x.toml", "--log", "debug", "--color", "never", "capture", "--screen", "DP-9",
        ][..],
    ] {
        let cli = parse(args).unwrap_or_else(|error| panic!("{args:?}: {error}"));
        assert_eq!(
            cli.config.as_deref(),
            Some(Path::new("/x.toml")),
            "{args:?}"
        );
        match cli.command {
            Some(Command::Capture(capture)) => {
                assert_eq!(capture.target.screen.as_deref(), Some("DP-9"))
            }
            other => panic!("{args:?}: expected capture, got {other:?}"),
        }
    }

    let cli = parse(&["--log", "debug", "list", "--json"]).expect("parses");
    assert_eq!(cli.log.log, "debug");
    assert!(matches!(cli.command, Some(Command::List { json: true })));

    let cli = parse(&["--log-format", "json", "hold"]).expect("parses");
    assert!(matches!(cli.command, Some(Command::Hold)));
}

#[test]
fn without_a_subcommand_the_default_command_still_takes_its_file_and_target() {
    let cli = parse(&["--config", "/x.toml", "shot.png"]).expect("parses");
    assert!(cli.command.is_none());
    assert_eq!(cli.gui.file.as_deref(), Some(Path::new("shot.png")));

    let gui = gui(&["--screen", "--json"]);
    assert_eq!(gui.target.screen.as_deref(), Some("current"));
    assert!(gui.json);
}

#[test]
fn a_bare_word_is_a_subcommand_or_a_png_and_nothing_else() {
    assert_eq!(clap_code(&["captur"]), 2);
    assert_eq!(clap_code(&["notes.txt"]), 2);
    let cli = parse(&["Shot.PNG"]).expect("parses");
    assert!(cli.command.is_none());
    assert_eq!(cli.gui.file.as_deref(), Some(Path::new("Shot.PNG")));
}

#[test]
fn a_default_command_option_before_a_subcommand_is_refused() {
    assert_eq!(clap_code(&["--json", "capture"]), 2);
    assert_eq!(clap_code(&["--screen", "DP-2", "list"]), 2);
    assert_eq!(clap_code(&["-o", "x.png", "capture"]), 2);
}

#[test]
fn an_editor_only_option_given_to_capture_says_why_and_what_to_run() {
    let command = Cli::command();
    let capture = command.find_subcommand("capture").expect("capture exists");
    let own: Vec<&str> = capture
        .get_arguments()
        .filter_map(|arg| arg.get_long())
        .collect();
    let editor_only: Vec<(String, bool)> = command
        .get_arguments()
        .filter(|arg| !arg.is_global_set() && !arg.is_hide_set())
        .filter_map(|arg| {
            let long = arg.get_long()?;
            (!own.contains(&long)).then(|| (long.to_owned(), arg.get_action().takes_values()))
        })
        .collect();
    for flag in ["editor", "prompt", "timeout", "flash", "no-flash", "tool"] {
        assert!(
            editor_only.iter().any(|(long, _)| long == flag),
            "--{flag} is not editor-only: {editor_only:?}"
        );
    }
    for (long, takes_value) in &editor_only {
        let flag = format!("--{long}");
        let mut args = vec!["capture", flag.as_str()];
        if *takes_value {
            args.push("window");
        }
        let refused = parse(&args).expect_err("refused");
        assert_eq!(refused.exit_code(), 2, "{flag}");
        let message = refused.to_string();
        assert!(
            message.contains(&format!(
                "{flag} is an editor option; `capture` never opens a window"
            )),
            "{message}"
        );
        assert!(message.contains("Run `glimpse-shot --screen"), "{message}");
    }
    let message = parse(&["capture", "--window", "--editor", "window"])
        .expect_err("refused")
        .to_string();
    assert!(
        message.contains("Run `glimpse-shot --window --editor window` instead."),
        "{message}"
    );
}

#[test]
fn the_last_of_a_pair_of_flags_wins() {
    assert!(!capture(&["--cursor", "--no-cursor"]).cursor(true));
    assert!(capture(&["--no-cursor", "--cursor"]).cursor(false));
    assert!(capture(&[]).cursor(true));
    assert!(!capture(&[]).cursor(false));
}

#[test]
fn capture_takes_its_own_options() {
    let args = capture(&[
        "--region",
        "-100,50 400x300",
        "-o",
        "-",
        "--scale",
        "logical",
        "-d",
        "3",
    ]);

    assert_eq!(args.output.as_deref(), Some(Path::new("-")));
    assert_eq!(args.scale, ScaleMode::Logical);
    assert_eq!(args.delay, Some(3));
    assert_eq!(
        args.target.resolve(true),
        Ok(Target::Region(Rect {
            x: Logical(-100.0),
            y: Logical(50.0),
            width: Logical(400.0),
            height: Logical(300.0),
        }))
    );
}

#[test]
fn targets_resolve_per_the_targets_table() {
    let resolve = |args: &[&str], headless: bool| {
        if headless {
            capture(args).target.resolve(true)
        } else {
            gui(args).target.resolve(false)
        }
    };

    assert_eq!(resolve(&[], false), Ok(Target::App));
    assert_eq!(resolve(&[], true), Ok(Target::CurrentScreen));
    assert_eq!(resolve(&["--screen"], true), Ok(Target::CurrentScreen));
    assert_eq!(
        resolve(&["--screen", "current"], false),
        Ok(Target::CurrentScreen)
    );
    assert_eq!(
        resolve(&["--screen", "DP-2"], true),
        Ok(Target::Screen("DP-2".to_owned()))
    );
    assert!(resolve(&["--screen", "all"], true).is_err());
    assert_eq!(resolve(&["--window"], false), Ok(Target::PickWindow));
    assert_eq!(resolve(&["--window"], true), Ok(Target::FocusedWindow));
    assert_eq!(
        resolve(&["--window", "focused"], false),
        Ok(Target::FocusedWindow)
    );
    assert_eq!(resolve(&["--window", "42"], true), Ok(Target::Window(42)));
    assert!(resolve(&["--window", "firefox"], true).is_err());
    assert_eq!(resolve(&["--region"], false), Ok(Target::PickRegion));
    assert!(resolve(&["--region"], true).is_err());
    assert!(resolve(&["--region", "10,10"], true).is_err());
}

#[test]
fn usage_rows_are_refused_before_any_capture() {
    let refused = |args: CaptureArgs, terminal| validate_capture(&args, terminal).is_err();

    assert!(refused(capture(&["-o", "-", "--json"]), PIPES));
    assert!(refused(capture(&["-o", "-"]), TTY));
    assert!(!refused(capture(&["-o", "-"]), PIPES));
    assert!(refused(capture(&["--annotate", "-"]), TTY));
    assert!(!refused(capture(&["--annotate", "-"]), PIPES));
    assert!(refused(capture(&["--region"]), PIPES));
    assert!(refused(capture(&["--region", "garbage"]), PIPES));
    assert!(!refused(capture(&["--region", "0,0 10x10", "--json"]), TTY));

    let refused = |args: GuiArgs| validate_gui(&args, PIPES).is_err();
    assert!(refused(gui(&["--annotate", "marks.json"])));
    assert!(refused(gui(&["--window", "--annotate", "marks.json"])));
    assert!(refused(gui(&["--region", "--annotate", "marks.json"])));
    assert!(!refused(gui(&[
        "--window",
        "42",
        "--annotate",
        "marks.json"
    ])));
    assert!(refused(gui(&["--json"])));
    assert!(refused(gui(&["--prompt", "where is the bug?"])));
    assert!(refused(gui(&["--timeout", "30"])));
    assert!(!refused(gui(&[
        "--screen",
        "--json",
        "--prompt",
        "x",
        "--timeout",
        "30"
    ])));

    let window = |extra: &[&str]| {
        let mut args = vec!["--screen", "--editor", "window"];
        args.extend_from_slice(extra);
        validate_gui(&gui(&args), PIPES)
    };
    assert!(window(&[]).is_ok());
    assert!(window(&["-o", "shot.png"]).is_ok());
    for conflict in [&["--prompt", "x"][..], &["--timeout", "30"], &["-o", "-"]] {
        let problem = window(conflict).expect_err("refused");
        assert!(problem.contains("--editor window"), "{problem}");
    }
    assert!(validate_gui(&gui(&["--screen", "--editor", "overlay", "-o", "-"]), PIPES).is_ok());
}

#[test]
fn every_problem_is_reported_at_once() {
    let problems = validate_capture(&capture(&["--region", "-o", "-", "--json"]), TTY)
        .expect_err("three problems");

    assert!(problems.contains("--region needs a geometry"));
    assert!(problems.contains("both want stdout"));
    assert!(problems.contains("refusing to write PNG to a terminal"));
}

#[test]
fn only_an_explicit_output_directory_must_exist() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let present = dir.path().join("shot.png");
    let absent = dir.path().join("missing").join("shot.png");

    assert_eq!(missing_directory(None), None);
    assert_eq!(missing_directory(Some(Path::new("-"))), None);
    assert_eq!(missing_directory(Some(Path::new("shot.png"))), None);
    assert_eq!(missing_directory(Some(&present)), None);
    assert_eq!(
        missing_directory(Some(&absent)),
        Some(dir.path().join("missing"))
    );
}
