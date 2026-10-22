use crate::load::{DATA_DIR, Documents};
use crate::schema::{Applet, Config, Panel};

pub fn commented_document(documents: Documents) -> String {
    let name = documents.name();
    let panel = documents == Documents::ConfigAndPanel;
    let header = format!(
        "#:schema {DATA_DIR}/{name}.schema.json\n\
         # Every {name}.toml setting at its default, commented out. Uncomment a line, and its [table] line, to change it.\n\
         # {DATA_DIR}/{name}.default.toml holds the same settings uncommented; the schema lists them all.\n"
    );
    let body = crate::document_body(documents);
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
    if panel {
        body.push_str(&applet_defaults());
    }
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
mod tests;
