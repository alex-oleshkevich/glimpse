use super::*;

fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, text).expect("writes");
    crate::load_files(std::slice::from_ref(&path))
}

#[test]
fn every_key_reads_kebab_case() {
    let screenshot = load(
        "[screenshot]\ndirectory = \"/tmp/shots\"\nfilename = \"shot %s.png\"\n\
             editor = \"window\"\ndelay = 5\nflash = false\ncursor = true\nnotify = false\n\
             color = \"blue\"\nstroke = \"thick\"\n",
    )
    .expect("every key")
    .screenshot;

    assert_eq!(screenshot.directory, Some(PathBuf::from("/tmp/shots")));
    assert_eq!(screenshot.filename.as_str(), "shot %s.png");
    assert_eq!(screenshot.editor, Editor::Window);
    assert_eq!(screenshot.delay, 5);
    assert!(!screenshot.flash && screenshot.cursor && !screenshot.notify);
    assert_eq!(screenshot.color, PaletteColor::Blue);
    assert_eq!(screenshot.stroke, Stroke::Thick);
}

#[test]
fn a_bad_filename_is_refused() {
    for (filename, reason) in [
        ("shot %Q.png", "strftime"),
        ("shots/%s.png", "`/`"),
        ("Shot %D.png", "`/`"),
        ("Shot %x.png", "`/`"),
        ("shot %s.jpg", "`.png`"),
    ] {
        let error = load(&format!("[screenshot]\nfilename = \"{filename}\"\n"))
            .expect_err("a bad filename is refused")
            .to_string();
        assert!(error.contains(reason), "{filename}: {error}");
    }
}

#[test]
fn a_template_whose_expansion_stays_one_component_is_accepted() {
    for filename in ["Screenshot from %Y-%m-%d %H-%M-%S.png", "Shot %F %H-%M.png"] {
        load(&format!("[screenshot]\nfilename = \"{filename}\"\n")).expect(filename);
    }
}

#[test]
fn a_delay_over_a_minute_is_clamped() {
    assert_eq!(
        load("[screenshot]\ndelay = 99\n")
            .expect("clamped")
            .screenshot
            .delay,
        60
    );
}

#[test]
fn an_unknown_key_is_refused_naming_the_table_and_the_key() {
    let rendered = load("[screenshot]\nbogus = 1\n")
        .expect_err("`bogus` is not a screenshot setting")
        .to_string();

    assert!(
        rendered.contains("screenshot") && rendered.contains("bogus"),
        "the error must name the table and the key, got {rendered}"
    );
}
