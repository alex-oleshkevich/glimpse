fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, text).expect("writes");
    crate::load(Some(&path))
}

#[test]
fn an_absent_table_copies_hex_and_keeps_eight() {
    let picker = load("").expect("an absent table is fine").color_picker;

    assert_eq!(picker.format, super::ColorFormat::Hex);
    assert_eq!(picker.limit, 8);
    assert_eq!(picker.lens_radius, 106);
    assert_eq!(picker.max_zoom, 30);
}

#[test]
fn every_key_reads_kebab_case() {
    let picker =
        load("[color-picker]\nformat = \"oklch\"\nlimit = 3\nlens-radius = 150\nmax-zoom = 12\n")
            .expect("kebab-case keys")
            .color_picker;

    assert_eq!(picker.format, super::ColorFormat::Oklch);
    assert_eq!(picker.limit, 3);
    assert_eq!(picker.lens_radius, 150);
    assert_eq!(picker.max_zoom, 12);
}

#[test]
fn every_format_renders_the_known_values_of_a_reference_color() {
    let rendered: Vec<String> = super::ColorFormat::ALL
        .into_iter()
        .map(|format| format.render([0xE0, 0x56, 0x3F]))
        .collect();
    assert_eq!(
        rendered,
        [
            "#E0563F",
            "rgb(224 86 63)",
            "hsl(9 72% 56%)",
            "hsv(9 72% 88%)",
            "oklch(0.63 0.18 32)",
            "cmyk(0% 62% 72% 12%)",
        ]
    );
}

#[test]
fn black_and_white_have_no_hue_to_invent() {
    use super::ColorFormat;
    assert_eq!(
        ColorFormat::Oklch.render([255, 255, 255]),
        "oklch(1.00 0 0)"
    );
    assert_eq!(ColorFormat::Oklch.render([0, 0, 0]), "oklch(0.00 0 0)");
    assert_eq!(ColorFormat::Cmyk.render([0, 0, 0]), "cmyk(0% 0% 0% 100%)");
    assert_eq!(ColorFormat::Hsl.render([255, 255, 255]), "hsl(0 0% 100%)");
    assert_eq!(ColorFormat::Hsl.render([255, 0, 1]), "hsl(0 100% 50%)");
}

#[test]
fn every_format_name_parses_back_to_itself_and_matches_the_document() {
    use super::ColorFormat;
    for format in ColorFormat::ALL {
        assert_eq!(ColorFormat::parse(format.name()), Some(format));
        let written = format!("[color-picker]\nformat = \"{}\"\n", format.name());
        assert_eq!(
            load(&written).expect("a named format").color_picker.format,
            format
        );
    }
    assert_eq!(ColorFormat::parse("HEX"), None);
}

#[test]
fn an_unknown_format_is_refused_by_name() {
    let rendered = load("[color-picker]\nformat = \"lab\"\n")
        .expect_err("`lab` is not a format")
        .to_string();

    assert!(
        rendered.contains("lab"),
        "the error must name `lab`, got {rendered}"
    );
}
