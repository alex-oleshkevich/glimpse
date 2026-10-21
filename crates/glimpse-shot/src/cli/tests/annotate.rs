use super::*;

#[test]
fn annotate_picker_requires_no_image_but_preserves_an_explicit_one() {
    for argv in [
        &["annotate", "--pick"][..],
        &["annotate", "--pick", "image.png"],
        &["annotate", "image.png", "--pick"],
    ] {
        let Some(Command::Annotate(args)) = parse(argv).expect("picker").command else {
            panic!("annotate")
        };
        assert!(validate_annotate(&args, PIPES).is_ok());
        let expected = (argv.len() == 3).then_some(Path::new("image.png"));
        assert_eq!(args.image.as_deref(), expected);
        let gui = args.into_gui();
        assert_eq!(gui.file.as_deref(), expected);
        assert!(gui.pick);
        assert!(crate::opens_app(
            parse(argv).expect("picker").command.as_ref()
        ));
    }
    assert_eq!(
        clap_code(&[
            "annotate",
            "--pick",
            "--headless",
            "--annotate",
            "marks.json"
        ]),
        2
    );
    let mut args = annotate(&[]);
    args.image = None;
    assert!(validate_annotate(&args, PIPES).is_err());
    args.pick = true;
    assert!(validate_annotate(&args, PIPES).is_ok());
    args.headless = true;
    args.annotate = Some("marks.json".into());
    assert!(validate_annotate(&args, PIPES).is_err());
}

fn annotate(args: &[&str]) -> AnnotateArgs {
    match parse(&[&["annotate", "image.png"], args].concat())
        .expect("parses")
        .command
    {
        Some(Command::Annotate(args)) => args,
        _ => panic!("annotate"),
    }
}

#[test]
fn annotate_validates_modes_stdio_and_editor_options() {
    for args in [
        &[][..],
        &[
            "--annotate",
            "marks.json",
            "--tool",
            "number",
            "-o",
            "out.png",
        ],
        &["--json", "--prompt", "Circle <b>this</b>", "--timeout", "0"],
        &["--annotate", "-", "--headless", "-o", "-"],
        &[
            "--annotate",
            "marks.json",
            "--headless",
            "--copy",
            "--no-save",
            "--json",
        ],
    ] {
        assert!(
            validate_annotate(&annotate(args), PIPES).is_ok(),
            "{args:?}"
        );
    }
    for (args, hint) in [
        (&["--headless"][..], "--annotate"),
        (&["--copy"][..], "add --headless"),
        (&["--no-save"][..], "add --headless"),
        (
            &["--headless", "--annotate", "-", "--tool", "arrow"][..],
            "omit --headless",
        ),
        (
            &["--headless", "--annotate", "-", "--prompt=test"][..],
            "omit --headless",
        ),
        (
            &["--headless", "--annotate", "-", "--timeout", "0"][..],
            "omit --headless",
        ),
        (
            &["--headless", "--annotate", "-", "--no-save"][..],
            "--copy",
        ),
        (
            &[
                "--headless",
                "--annotate",
                "-",
                "--copy",
                "--no-save",
                "-o",
                "out.png",
            ][..],
            "cannot be used",
        ),
        (&["--editor", "window"][..], "--editor"),
    ] {
        let error = parse(&[&["annotate", "image.png"], args].concat()).expect_err("usage");
        assert_eq!(error.exit_code(), 2);
        let text = error.to_string();
        assert!(text.contains(hint), "{error}");
        assert!(!text.contains("error: error:"), "{error}");
        assert_eq!(text.matches("Usage:").count(), 1, "{error}");
    }
    for (args, terminal) in [
        (&["-o", "-"][..], PIPES),
        (
            &[
                "--headless",
                "--annotate",
                "marks.json",
                "-o",
                "-",
                "--json",
            ][..],
            PIPES,
        ),
        (
            &["--headless", "--annotate", "marks.json", "-o", "-"][..],
            TTY,
        ),
        (&["--annotate", "-"][..], TTY),
    ] {
        assert!(
            validate_annotate(&annotate(args), terminal).is_err(),
            "{args:?}"
        );
    }
    let mut args = annotate(&[]);
    args.copy = true;
    args.no_save = true;
    assert!(
        validate_annotate(&args, PIPES)
            .expect_err("constructed GUI flags")
            .contains("add --headless")
    );
    args.headless = true;
    assert!(validate_annotate(&args, PIPES).is_err());
    args.annotate = Some("marks.json".into());
    assert!(validate_annotate(&args, PIPES).is_ok());
    args.prompt = Some("where?".into());
    assert!(
        validate_annotate(&args, PIPES)
            .expect_err("constructed conflict")
            .contains("omit --headless")
    );
}

#[test]
fn annotate_preserves_gui_fields_globals_and_original_forwarded_argv() {
    for args in [
        &["--config", "/scratch/config.toml", "annotate", "image.dat"][..],
        &["annotate", "image.dat", "--config", "/scratch/config.toml"][..],
    ] {
        let cli = parse(args).expect("global config");
        assert_eq!(
            cli.config.as_deref(),
            Some(Path::new("/scratch/config.toml"))
        );
    }
    let args = [
        "annotate",
        "image.png",
        "--annotate",
        "-",
        "--json",
        "--prompt",
        "where?",
        "--timeout",
        "3",
        "--tool",
        "number",
        "-o",
        "out.png",
    ];
    let cli = parse(&args).expect("parses");
    assert!(cli.json());
    let Some(Command::Annotate(args)) = cli.command else {
        panic!("annotate")
    };
    let gui = args.into_gui();
    assert_eq!(gui.file.as_deref(), Some(Path::new("image.png")));
    assert_eq!(gui.annotate.as_deref(), Some(Path::new("-")));
    assert_eq!(gui.output.as_deref(), Some(Path::new("out.png")));
    assert_eq!(gui.prompt.as_deref(), Some("where?"));
    assert_eq!(gui.timeout, Some(3));
    assert_eq!(gui.tool, Some(Tool::Number));
    let argv = ["glimpse-shot", "annotate", "image.png", "--annotate", "-"]
        .map(OsString::from)
        .to_vec();
    assert_eq!(crate::gui::app::gtk_args(&gui, argv.clone()), argv);
    assert_eq!(clap_code(&["--json", "annotate", "image.png"]), 2);
}

#[test]
fn annotate_refuses_canonical_output_aliases_without_changing_the_source() {
    let dir = tempfile::tempdir().expect("scratch");
    let image = dir.path().join("карціна.png");
    let alias = dir.path().join("alias.png");
    std::fs::write(&image, b"source bytes").expect("writes");
    std::os::unix::fs::symlink(&image, &alias).expect("symlink");
    let mut args = annotate(&[]);
    args.image = Some(image.clone());
    for output in [&image, &alias, &dir.path().join("./карціна.png")] {
        args.output = Some(output.clone());
        assert!(validate_annotate(&args, PIPES).is_err());
        assert_eq!(std::fs::read(&image).expect("unchanged"), b"source bytes");
    }
    args.output = Some(dir.path().join("new.png"));
    assert!(validate_annotate(&args, PIPES).is_ok());
}

#[test]
fn an_image_or_pick_is_a_known_target_but_still_validated() {
    for (file, pick) in [(Some("image.png".into()), false), (None, true)] {
        let mut gui = GuiArgs {
            file,
            pick,
            annotate: Some("-".into()),
            json: true,
            prompt: Some("text".into()),
            timeout: Some(5),
            ..Default::default()
        };
        assert!(validate_gui(&gui, PIPES).is_ok());
        assert_eq!(gui.target.resolve(false), Ok(Target::App));
        gui.editor = Some(Editor::Window);
        assert!(validate_gui(&gui, PIPES).is_err());
    }
}

#[test]
fn annotate_without_image_or_pick_points_at_both() {
    let Some(Command::Annotate(args)) = parse(&["annotate"]).expect("parses").command else {
        panic!("annotate");
    };
    let message = validate_annotate(&args, PIPES).expect_err("usage");
    assert!(
        message.contains("IMAGE") && message.contains("--pick"),
        "{message}"
    );
}
