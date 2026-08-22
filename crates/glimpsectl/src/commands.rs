use std::path::PathBuf;

use anyhow::Result;
use glimpse_config::ConfigError;
use glimpse_ipc::Client;

pub async fn get(client: &Client, topic: String, field: Option<String>, json: bool) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub async fn watch(client: &Client, pattern: String, count: Option<u64>, json: bool) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub async fn call(
    client: &Client,
    method: String,
    args: Vec<(String, String)>,
    json: bool,
) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub async fn topics(client: &Client, pattern: Option<String>, json: bool) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub async fn services(client: &Client, json: bool) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub async fn doctor(client: &Client, json: bool) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub async fn monitor(client: &Client) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub fn config_show(override_path: Option<PathBuf>, json: bool) -> Result<()> {
    match glimpse_config::load(override_path.as_deref()) {
        Ok(config) => {
            if json {
                anstream::println!("{}", serde_json::to_string_pretty(&config)?);
            } else {
                anstream::print!("{}", toml::to_string_pretty(&config)?);
            }
            Ok(())
        }
        Err(problems) => Err(report_problems(&problems, json)),
    }
}

pub fn config_validate(path: Option<PathBuf>, json: bool) -> Result<()> {
    match glimpse_config::load(path.as_deref()) {
        Ok(_) => {
            if json {
                anstream::println!(r#"{{"valid":true}}"#);
            } else {
                anstream::println!("{GREEN}config is valid{GREEN:#}");
            }
            Ok(())
        }
        Err(problems) => Err(report_problems(&problems, json)),
    }
}

pub fn config_path(config: Option<PathBuf>, json: bool) -> Result<()> {
    let files = glimpse_config::resolved_files(config.as_deref())?;
    let rows: Vec<(PathBuf, bool)> = files
        .into_iter()
        .map(|path| {
            let exists = path.try_exists().unwrap_or(false);
            (path, exists)
        })
        .collect();

    if json {
        anstream::println!("{}", render_stack_json(&rows));
    } else {
        anstream::print!("{}", render_stack(&rows));
    }
    Ok(())
}

const GREEN: anstyle::Style =
    anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Green)));
const DIM: anstyle::Style = anstyle::Style::new().effects(anstyle::Effects::DIMMED);

fn render_stack(rows: &[(PathBuf, bool)]) -> String {
    let width = rows
        .iter()
        .map(|(path, _)| path.as_os_str().len())
        .max()
        .unwrap_or(0);

    let mut out = String::new();
    for (path, exists) in rows {
        let path = path.display();
        if *exists {
            out.push_str(&format!("{path:<width$}  {GREEN}found{GREEN:#}\n"));
        } else {
            out.push_str(&format!("{path:<width$}  {DIM}missing{DIM:#}\n"));
        }
    }
    out
}

fn render_stack_json(rows: &[(PathBuf, bool)]) -> String {
    let payload: Vec<_> = rows
        .iter()
        .map(|(path, exists)| {
            serde_json::json!({ "path": path.to_string_lossy(), "exists": exists })
        })
        .collect();
    serde_json::to_string_pretty(&payload).expect("strings and booleans always serialize")
}

fn render_problems(problems: &[ConfigError]) -> String {
    problems
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_problems_json(problems: &[ConfigError]) -> String {
    let payload = serde_json::json!({
        "valid": false,
        "problems": problems.iter().map(ToString::to_string).collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&payload).expect("a ConfigError's Display always serializes")
}

fn report_problems(problems: &[ConfigError], json: bool) -> anyhow::Error {
    if json {
        anstream::println!("{}", render_problems_json(problems));
    } else {
        anstream::println!("{}", render_problems(problems));
    }
    anyhow::anyhow!(
        "{} problem{} found",
        problems.len(),
        if problems.len() == 1 { "" } else { "s" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_show_prints_a_valid_override_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[appearance]\npack = \"catppuccin\"\n").expect("fixture");

        config_show(Some(path), false).expect("a valid file shows cleanly");
    }

    #[test]
    fn config_show_fails_on_a_broken_override_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[panle]\n").expect("fixture");

        let error = config_show(Some(path), false).expect_err("an unknown table is refused");
        assert_eq!(error.to_string(), "1 problem found");
    }

    #[test]
    fn config_validate_accepts_a_valid_override_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[appearance]\npack = \"catppuccin\"\n").expect("fixture");

        config_validate(Some(path), false).expect("a valid file validates");
    }

    #[test]
    fn config_validate_reports_a_misspelled_table() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[panle]\nsize = 36\n").expect("fixture");

        let error = config_validate(Some(path), false).expect_err("an unknown table is refused");
        assert_eq!(error.to_string(), "1 problem found");
    }

    #[test]
    fn config_path_resolves_to_exactly_the_override_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[appearance]\npack = \"catppuccin\"\n").expect("fixture");

        config_path(Some(path), false).expect("a single override file resolves");
    }

    #[test]
    fn the_status_column_aligns_past_the_longest_path() {
        let rows = vec![
            (PathBuf::from("/etc/glimpse/config.toml"), true),
            (
                PathBuf::from("/home/user/.config/glimpse/config.d/10-overrides.toml"),
                false,
            ),
        ];

        let rendered = render_stack(&rows);
        let mut lines = rendered.lines();

        assert!(lines.next().unwrap().contains("found"));
        assert!(lines.next().unwrap().contains("missing"));
    }

    #[test]
    fn stack_json_carries_path_and_existence() {
        let rows = vec![(PathBuf::from("/a/config.toml"), true)];

        let rendered = render_stack_json(&rows);

        assert!(rendered.contains("/a/config.toml"));
        assert!(rendered.contains("true"));
    }

    #[test]
    fn problems_render_one_per_line() {
        let problems = vec![
            ConfigError::Schema {
                message: "bad key".into(),
            },
            ConfigError::Schema {
                message: "another bad key".into(),
            },
        ];

        assert_eq!(render_problems(&problems), "bad key\nanother bad key");
    }

    #[test]
    fn problems_json_marks_the_document_invalid() {
        let problems = vec![ConfigError::Schema {
            message: "bad key".into(),
        }];

        let rendered = render_problems_json(&problems);

        assert!(rendered.contains("\"valid\": false"));
        assert!(rendered.contains("bad key"));
    }

    #[test]
    fn the_summary_error_counts_problems() {
        let problems = vec![
            ConfigError::Schema {
                message: "one".into(),
            },
            ConfigError::Schema {
                message: "two".into(),
            },
        ];

        assert_eq!(
            report_problems(&problems, false).to_string(),
            "2 problems found"
        );
    }
}
