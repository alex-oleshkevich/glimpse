use std::fs;
use std::path::{Path, PathBuf};

use glimpsectl::cli::{AppletLanguage, NewAppletArgs};
use glimpsectl::commands::applets_author::applets_new_with_catalog;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn block(page: &str, fence: &str) -> String {
    let path = repo().join("website/applets/exec").join(page);
    let text = fs::read_to_string(&path).unwrap();
    let mut lines = text.lines().skip_while(|line| *line != fence);
    assert!(lines.next().is_some(), "{page} has no {fence} block");
    lines
        .take_while(|line| *line != "```")
        .map(|line| format!("{line}\n"))
        .collect()
}

fn render(args: NewAppletArgs, file: &str) -> String {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("glimpse-todo");
    let template = repo().join("sdk/templates").join(args.language.dir());
    let args = NewAppletArgs {
        dir: Some(dir.clone()),
        ..args
    };
    applets_new_with_catalog(args, &template, |_| false).unwrap();
    fs::read_to_string(dir.join(file)).unwrap()
}

#[test]
fn tutorial_shows_the_generated_index_tsx() {
    let args = NewAppletArgs {
        id: Some("me.aresa.GlimpseTodo".to_owned()),
        language: AppletLanguage::Ts,
        name: Some("Todo".to_owned()),
        description: Some("A list of things to do".to_owned()),
        icon: Some("view-list-symbolic".to_owned()),
        dir: None,
        yes: true,
    };
    assert_eq!(block("index.md", "```tsx"), render(args, "index.tsx"));
}

#[test]
fn start_page_shows_each_generated_entry_file() {
    for (language, fence, file) in [
        (AppletLanguage::Py, "```python [Python]", "applet.py"),
        (AppletLanguage::Go, "```go [Go]", "main.go"),
        (AppletLanguage::Rs, "```rust [Rust]", "src/main.rs"),
    ] {
        let args = NewAppletArgs {
            id: Some("me.aresa.GlimpseTodo".to_owned()),
            language,
            name: None,
            description: None,
            icon: None,
            dir: None,
            yes: true,
        };
        assert_eq!(block("start.md", fence), render(args, file), "{file}");
    }
}
