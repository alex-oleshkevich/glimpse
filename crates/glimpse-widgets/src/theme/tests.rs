use super::{
    BUILTIN, BUILTIN_PRIORITY, DROPIN_DARK_PRIORITY, DROPIN_PRIORITY, SPEED_PRIORITY, Styles,
    THEME_DARK_PRIORITY, THEME_PRIORITY, effective_scheme, provider_scheme, requested_scheme,
    scaled, speed_sheet, usable_variant,
};

const OPEN: &str = ":root {";
const PREFIXES: &[&str] = &[
    "gl-",
    "applet-popover-",
    "popover-shell-",
    "popover-body-",
    "container-",
    "row-",
    "meter-row-",
];

fn carries_prefix(name: &str) -> bool {
    PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

fn split(sheet: &str) -> (&str, &str) {
    let open = sheet.find(OPEN).expect("a :root block");
    let close = open + sheet[open..].find('}').expect("the :root block closes");
    (&sheet[open + OPEN.len()..close], &sheet[close + 1..])
}

fn declared(block: &str) -> Vec<&str> {
    block
        .lines()
        .filter_map(|line| line.trim().strip_prefix("--"))
        .filter_map(|line| line.split(':').next())
        .collect()
}

fn referenced(text: &str) -> Vec<&str> {
    text.match_indices("var(--")
        .map(|(at, opener)| {
            let rest = &text[at + opener.len()..];
            let end = rest
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '-')
                .unwrap_or(rest.len());
            &rest[..end]
        })
        .collect()
}

fn outside_var_groups(line: &str) -> String {
    let mut rest = line;
    let mut kept = String::new();
    while let Some(at) = rest.find("var(") {
        kept.push_str(&rest[..at]);
        let mut depth = 1;
        let mut end = rest.len();
        for (index, character) in rest[at + 4..].char_indices() {
            match character {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = at + index + 5;
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = &rest[end..];
    }
    kept.push_str(rest);
    kept
}

#[test]
fn every_glimpse_token_the_stylesheet_reads_is_declared() {
    let (block, rules) = split(BUILTIN);
    let declared: Vec<&str> = declared(block).into_iter().chain(declared(rules)).collect();
    let read = referenced(block).into_iter().chain(referenced(rules));

    for name in read.filter(|name| carries_prefix(name)) {
        assert!(
            declared.contains(&name),
            "--{name} is read but never declared; GTK reports this nowhere"
        );
    }
}

#[test]
fn every_declared_token_carries_the_prefix() {
    let (block, _) = split(BUILTIN);
    for name in declared(block) {
        assert!(
            carries_prefix(name),
            "--{name} is declared without a shell prefix"
        );
    }
}

#[test]
fn no_rule_reads_an_adwaita_token_directly() {
    let (_, rules) = split(BUILTIN);
    for name in referenced(rules) {
        assert!(
            carries_prefix(name),
            "--{name} is a tier-1 token read from a rule; derive it in :root instead"
        );
    }
}

#[test]
fn no_rule_sets_a_pixel_font_size() {
    let (_, rules) = split(BUILTIN);
    for line in rules.lines().map(str::trim) {
        assert!(
            !line.starts_with("font-size:") || !line.contains("px"),
            "a font size in px cannot follow the user's font: {line}"
        );
    }
}

#[test]
fn only_hairlines_and_borders_are_measured_in_pixels() {
    let (_, rules) = split(BUILTIN);

    for line in rules.lines().map(str::trim) {
        assert!(
            pixel_lengths_allowed(line),
            "a length in px does not follow the user's text size: {line}"
        );
    }
}

fn pixel_lengths_allowed(line: &str) -> bool {
    let Some((property, value)) = line.split_once(':') else {
        return true;
    };
    value
        .split(|c: char| !c.is_ascii_alphanumeric() && !matches!(c, '.' | '-' | '+'))
        .filter(|length| length.ends_with("px"))
        .all(|length| match property.trim() {
            "border"
            | "border-top"
            | "border-right"
            | "border-bottom"
            | "border-left"
            | "border-width"
            | "border-top-width"
            | "border-right-width"
            | "border-bottom-width"
            | "border-left-width"
            | "outline"
            | "outline-width"
            | "outline-offset"
            | "box-shadow" => true,
            "border-radius" => length == "999px",
            _ => false,
        })
}

#[test]
fn pixel_lengths_are_rejected_in_scaled_properties_and_fallbacks() {
    for line in [
        "border-radius: 12px;",
        "padding: var(--gl-radius, 8px);",
        "-gtk-icon-size: var(--gl-text-body, 1px);",
        "padding: var(--gl-radius, var(--gl-space, 8px));",
        "border-radius: var(--gl-radius, 12px);",
        "border-radius: 999px 12px;",
        "border-top-left-radius: 12px;",
        "border-spacing: 1px;",
        "outline-radius: 1px;",
    ] {
        assert!(!pixel_lengths_allowed(line), "accepted {line}");
    }
}

#[test]
fn pixel_lengths_are_allowed_for_borders_outlines_shadows_and_pills() {
    for line in [
        "border: 1px solid var(--gl-border);",
        "border-top: 1px solid var(--gl-border);",
        "border-right: 1px solid var(--gl-border);",
        "border-bottom: 1px solid var(--gl-border);",
        "border-left: 1px solid var(--gl-border);",
        "border-width: 1px;",
        "border-top-width: 1px;",
        "border-right-width: 1px;",
        "border-bottom-width: 1px;",
        "border-left-width: 1px;",
        "outline: 1px solid var(--gl-border);",
        "outline-width: 1px;",
        "outline-offset: -1px;",
        "box-shadow: 0 6px 20px alpha(black, 0.4);",
        "box-shadow: var(--gl-elevation-floating, 0 6px 20px alpha(black, 0.4));",
        "border-radius: 999px;",
        "border-radius: var(--gl-radius, 999px);",
        "padding: var(--gl-radius, 0.5rem);",
    ] {
        assert!(pixel_lengths_allowed(line), "rejected {line}");
    }
}

#[test]
fn every_drop_shadow_reads_an_elevation_token() {
    let (_, rules) = split(BUILTIN);
    for line in rules.lines().map(str::trim) {
        let Some(value) = line.strip_prefix("box-shadow:") else {
            continue;
        };
        let value = value.trim();
        assert!(
            value == "none;"
                || value.starts_with("inset ")
                || value.starts_with("var(--gl-elevation-"),
            "a surface invents its own depth; read an elevation token: {line}"
        );
    }
}

#[test]
fn no_visual_rule_names_a_literal_color() {
    let (_, rules) = split(BUILTIN);
    for line in rules.lines() {
        let line = line.trim();
        if line.starts_with("--gl-") {
            continue;
        }
        let painted = outside_var_groups(line);
        assert!(
            !painted.contains('#') && !painted.contains("rgb(") && !painted.contains("rgba("),
            "a rule names a literal color, which cannot follow a theme: {line}"
        );
    }
}

#[test]
fn providers_follow_the_effective_scheme_without_using_default() {
    assert_eq!(provider_scheme(false), gtk4::InterfaceColorScheme::Light);
    assert_eq!(provider_scheme(true), gtk4::InterfaceColorScheme::Dark);
    assert_eq!(effective_scheme(false), "light");
    assert_eq!(effective_scheme(true), "dark");
    assert_eq!(requested_scheme(adw::ColorScheme::Default), "auto");
    assert_eq!(requested_scheme(adw::ColorScheme::ForceLight), "light");
    assert_eq!(requested_scheme(adw::ColorScheme::ForceDark), "dark");
}

#[test]
fn each_layer_is_refined_by_its_own_dark_sheet_and_the_user_has_the_last_word() {
    const {
        assert!(BUILTIN_PRIORITY < THEME_PRIORITY);
        assert!(THEME_PRIORITY < THEME_DARK_PRIORITY);
        assert!(THEME_DARK_PRIORITY < DROPIN_PRIORITY);
        assert!(DROPIN_PRIORITY < DROPIN_DARK_PRIORITY);
        assert!(DROPIN_DARK_PRIORITY < SPEED_PRIORITY);
    }
}

#[test]
fn speed_divides_every_duration_and_zero_stops_them() {
    assert_eq!(scaled(150, 1.0), 150);
    assert_eq!(scaled(150, 2.0), 75);
    assert_eq!(scaled(200, 0.5), 400);
    assert_eq!(scaled(150, 0.0), 0);
    assert_eq!(speed_sheet(0.0), ":root { --gl-duration: 0ms; }");
}

#[test]
fn a_variant_that_is_not_a_css_class_is_dropped_rather_than_written() {
    assert_eq!(usable_variant("nord"), "nord");
    assert_eq!(usable_variant("high-contrast_2"), "high-contrast_2");
    assert_eq!(usable_variant(""), "");
    assert_eq!(usable_variant("high contrast"), "");
    assert_eq!(usable_variant("2cool"), "");
    assert_eq!(usable_variant("nord, .panel"), "");
}

#[test]
fn every_block_in_the_builtin_sheet_closes() {
    let mut open: Vec<bool> = Vec::new();
    for (index, line) in BUILTIN.lines().enumerate() {
        let code = line.split("/*").next().unwrap_or_default();
        for character in code.chars() {
            match character {
                '{' => {
                    assert!(
                        open.last().is_none_or(|&at_rule| at_rule),
                        "glimpse.css:{} opens a rule inside an unclosed one, which GTK drops \
                             with everything after it",
                        index + 1
                    );
                    open.push(code.trim_start().starts_with('@'));
                }
                '}' => assert!(
                    open.pop().is_some(),
                    "glimpse.css:{} closes a block never opened",
                    index + 1
                ),
                _ => {}
            }
        }
    }
    assert!(open.is_empty(), "glimpse.css ends inside an unclosed block");
}

#[test]
fn inactive_rows_fade_each_element_once() {
    let (_, rules) = split(BUILTIN);
    for block in rules.split('}') {
        let Some((selector, declarations)) = block.split_once('{') else {
            continue;
        };
        if selector.contains("listrow")
            && (selector.contains(":disabled") || selector.contains("row--inactive"))
        {
            assert!(
                !declarations
                    .lines()
                    .any(|line| line.trim().starts_with("opacity:")),
                "row-level opacity compounds the theme disabled filter: {selector}"
            );
        }
    }
    assert!(!rules.contains("listrow:disabled label {"));
}

#[test]
#[ignore]
fn a_theme_sheet_reaches_its_provider_and_an_absent_one_clears_it() {
    adw::init().expect("a display");
    let styles = Styles::install(adw::ColorScheme::Default);

    styles.load(&glimpse_config::Theme {
        sheet: Some("label { color: red; }".into()),
        ..Default::default()
    });
    assert!(styles.theme.to_str().contains("label"));

    styles.load(&glimpse_config::Theme::default());
    assert!(styles.theme.to_str().is_empty());
}
