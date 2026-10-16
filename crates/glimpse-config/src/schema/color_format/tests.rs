use super::ColorFormat;

#[test]
fn every_format_renders_the_known_values_of_a_reference_color() {
    let rendered: Vec<String> = ColorFormat::ALL
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
fn every_format_name_parses_back_to_itself() {
    for format in ColorFormat::ALL {
        assert_eq!(ColorFormat::parse(format.name()), Some(format));
        assert_eq!(ColorFormat::parse(format.label()), None);
    }
    assert_eq!(ColorFormat::parse("HEX"), None);
    assert_eq!(ColorFormat::parse("lab"), None);
}
