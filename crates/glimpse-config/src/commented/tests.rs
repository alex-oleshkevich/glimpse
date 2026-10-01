use super::*;

#[test]
fn the_commented_document_loads_to_the_shipped_defaults() {
    assert!(is_the_shipped_defaults(&commented_document()));
}

#[test]
fn the_commented_document_holds_no_values_at_all() {
    let parsed: toml::Table = commented_document().parse().expect("valid TOML");
    for (name, value) in &parsed {
        let table = value
            .as_table()
            .unwrap_or_else(|| panic!("{name} is a value"));
        assert!(
            table.is_empty() || table.values().all(toml::Value::is_table),
            "{name}"
        );
    }
}

#[test]
fn an_array_of_tables_header_is_never_left_live() {
    for line in commented_document().lines() {
        assert!(!line.starts_with("[["), "{line}");
    }
}

#[test]
fn a_table_whose_emptiness_changes_nothing_stays_live() {
    let document = commented_document();
    for header in ["[appearance]", "[regional]", "[applets]"] {
        assert!(document.contains(&format!("\n{header}\n")), "{header}");
    }
}

#[test]
fn a_table_that_cannot_be_empty_is_commented_with_its_keys() {
    let document = commented_document();
    for header in ["[geolocation]", "[panels.margin]"] {
        assert!(document.contains(&format!("\n# {header}\n")), "{header}");
        assert!(!document.contains(&format!("\n{header}\n")), "{header}");
    }
}

#[test]
fn stripping_the_comment_markers_restores_the_shipped_body() {
    let body = toml::to_string_pretty(&Config::default()).expect("serializes");
    let mut restored = String::new();
    let mut started = false;
    for line in commented_document().lines() {
        if !started {
            started = line.starts_with("[appearance]") || line.starts_with("# [appearance]");
            if !started {
                continue;
            }
        }
        restored.push_str(line.strip_prefix("# ").unwrap_or(line));
        restored.push('\n');
        if line == "[applets]" {
            break;
        }
    }
    assert_eq!(restored, body);
}

#[test]
fn every_default_panel_applet_is_documented() {
    let document = commented_document();
    let panel = Panel::default();
    for name in panel.left.iter().chain(&panel.center).chain(&panel.right) {
        assert!(
            document.contains(&format!("\n# [applets.{name}]\n")),
            "{name}"
        );
    }
    for absent in ["command", "exec", "heartbeat"] {
        assert!(
            !document.contains(&format!("[applets.{absent}]")),
            "{absent} carries no sensible default and should not be documented"
        );
    }
}

#[test]
fn a_documented_applets_own_settings_strip_back_to_its_defaults() {
    let document = commented_document();
    let block = document
        .split_once("# [applets.clock]\n")
        .expect("the clock applet is documented")
        .1
        .split("\n\n")
        .next()
        .expect("a block ending at the next blank line");

    let mut stripped = "[applets.clock]\n".to_owned();
    for line in block.lines() {
        stripped.push_str(line.strip_prefix("# ").unwrap_or(line));
        stripped.push('\n');
    }

    let parsed: Config = toml::from_str(&stripped).expect("the documented defaults parse");
    assert_eq!(
        parsed.applets["clock"],
        Applet::from_name("clock").expect("clock is a known kind")
    );
}

#[test]
fn the_schema_directive_stays_on_the_first_line() {
    let first = commented_document()
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned();
    assert_eq!(first, format!("#:schema {DATA_DIR}/config.schema.json"));
}
