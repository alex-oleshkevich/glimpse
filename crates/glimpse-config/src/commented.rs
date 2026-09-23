use crate::load::DATA_DIR;
use crate::schema::{Applet, Config, Panel};

pub fn commented_document() -> String {
    let header = format!(
        "#:schema {DATA_DIR}/config.schema.json\n\
         # glimpse's configuration. Every setting that ships with a value is here at that value,\n\
         # and all of them are commented out, so this file changes nothing until you uncomment a\n\
         # line and edit it. Where the [table] header above a setting is commented out too,\n\
         # uncomment both — a key on its own lands in whichever table precedes it. A setting with\n\
         # no default is not listed; the schema named above has the complete set.\n\
         #\n\
         # With a TOML language server installed — taplo, or an editor extension built on it such\n\
         # as \"Even Better TOML\" — the first line above gives you completion for every table and\n\
         # key, the documentation for each setting on hover, and an error on a value the schema\n\
         # refuses.\n\
         #\n\
         # {DATA_DIR}/config.default.toml is this same document with nothing commented\n\
         # out, and it is the copy that stays current across upgrades. `[applets.<name>]` below\n\
         # is every applet the default panel carries, each at its own kind's defaults — an entry\n\
         # here is always commented, because any applet at all differs from the shipped panel's\n\
         # own list of names.\n"
    );
    let body = toml::to_string_pretty(&Config::default()).expect("Config::default() serializes");
    let mut lines: Vec<(&str, bool)> = body
        .lines()
        .map(|line| (line, line.trim().is_empty()))
        .collect();

    for index in 0..lines.len() {
        let line = lines[index].0;
        if !line.starts_with('[') || line.starts_with("[[") {
            continue;
        }
        lines[index].1 = true;
        if !is_the_shipped_defaults(&render(&lines)) {
            lines[index].1 = false;
        }
    }

    let mut body = render(&lines);
    assert!(
        is_the_shipped_defaults(&body),
        "the commented document has to load to Config::default()"
    );
    body.push_str(&applet_defaults());
    format!("{header}\n{body}")
}

/// The default panel's own applet names, each rendered as a fully commented `[applets.<name>]`
/// block at that kind's defaults — the settings a user would otherwise only find in the schema.
/// `command`, `exec` and `heartbeat` carry no sensible default (a program to run, a binary to
/// host) and are absent from the default panel for the same reason, so neither appears here.
fn applet_defaults() -> String {
    let panel = Panel::default();
    let mut rendered = String::new();
    for name in panel.left.iter().chain(&panel.center).chain(&panel.right) {
        let Some(applet) = Applet::from_name(name) else {
            continue;
        };
        let block = toml::to_string_pretty(&applet).expect("an applet's defaults serialize");
        rendered.push('\n');
        rendered.push_str(&format!("# [applets.{name}]\n"));
        for line in block.lines() {
            if let Some(rest) = line.strip_prefix("[[") {
                rendered.push_str(&format!("# [[applets.{name}.{rest}\n"));
            } else if let Some(rest) = line.strip_prefix('[') {
                rendered.push_str(&format!("# [applets.{name}.{rest}\n"));
            } else if line.trim().is_empty() {
                rendered.push('\n');
            } else {
                rendered.push_str(&format!("# {line}\n"));
            }
        }
    }
    rendered
}

fn render(lines: &[(&str, bool)]) -> String {
    let mut rendered = String::new();
    for (line, live) in lines {
        if !*live {
            rendered.push_str("# ");
        }
        rendered.push_str(line);
        rendered.push('\n');
    }
    rendered
}

fn is_the_shipped_defaults(text: &str) -> bool {
    config::Config::builder()
        .add_source(config::File::from_str(text, config::FileFormat::Toml))
        .build()
        .and_then(config::Config::try_deserialize::<Config>)
        .is_ok_and(|config| config == Config::default())
}

#[cfg(test)]
mod tests {
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
}
