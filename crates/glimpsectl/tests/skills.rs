use std::fs;
use std::path::{Path, PathBuf};

use glimpsectl::cli::Cli;

fn skills() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/skills");
    let mut files: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path().join("SKILL.md"))
        .collect();
    files.sort();
    assert!(!files.is_empty());
    files
}

fn blocks(text: &str, language: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let mut current: Option<(usize, String)> = None;
    for (index, line) in text.lines().enumerate() {
        let fence = line.trim_start();
        match current.take() {
            Some(block) if fence.starts_with("```") => found.push(block),
            Some((start, mut body)) => {
                body.push_str(line);
                body.push('\n');
                current = Some((start, body));
            }
            None => {
                assert!(
                    !fence.starts_with("~~~"),
                    "line {}: use ``` fences",
                    index + 1
                );
                if let Some(info) = fence.strip_prefix("```") {
                    assert!(
                        info == language || !info.starts_with(language),
                        "line {}: write ```{language} with nothing after it",
                        index + 1
                    );
                    if info == language {
                        current = Some((index + 1, String::new()));
                    }
                }
            }
        }
    }
    assert!(current.is_none(), "unterminated ```{language} fence");
    found
}

#[test]
fn every_toml_block_in_a_skill_is_a_panel_toml_that_loads() {
    for skill in skills() {
        let text = fs::read_to_string(&skill).unwrap();
        for (line, body) in blocks(&text, "toml") {
            let dir = tempfile::tempdir().unwrap();
            let panel = dir.path().join("panel.toml");
            fs::write(&panel, &body).unwrap();
            let config = glimpse_config::load_files(&[panel])
                .unwrap_or_else(|error| panic!("{}:{line}: {error}", skill.display()));
            let unknown = glimpse_config::unknown_applets(&config);
            assert!(
                unknown.is_empty(),
                "{}:{line}: {unknown:?}",
                skill.display()
            );
        }
    }
}

#[test]
fn every_glimpsectl_line_in_a_skill_parses() {
    for skill in skills() {
        let text = fs::read_to_string(&skill).unwrap();
        for (line, body) in blocks(&text, "sh") {
            for command in body.lines().map(str::trim) {
                if !command.starts_with("glimpsectl ") {
                    continue;
                }
                assert!(
                    !command.contains(['"', '\'', '\\', '$', '|']),
                    "{}:{line}: keep glimpsectl lines free of shell syntax: {command}",
                    skill.display()
                );
                if let Err(error) = Cli::try_parse_from(command.split_whitespace()) {
                    panic!("{}:{line}: {command}: {error}", skill.display());
                }
            }
        }
    }
}
