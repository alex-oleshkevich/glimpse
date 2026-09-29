use std::fs;
use std::path::Path;

use glimpsectl::cli::NewAppletArgs;
use glimpsectl::commands::applets_author::{
    applets_new_with_catalog, exec_quote, rewrite_desktop, run_argv,
};

fn args(dir: &Path, name: &str) -> NewAppletArgs {
    NewAppletArgs {
        id: Some("me.example.Hello".to_owned()),
        name: Some(name.to_owned()),
        description: Some("A test applet".to_owned()),
        icon: None,
        dir: Some(dir.to_owned()),
        yes: true,
    }
}

#[test]
fn bundled_exec_has_only_the_installed_executable() {
    assert_eq!(
        run_argv(Path::new(
            "/usr/share/glimpse/applets/me.example.A/applet.mjs"
        )),
        "glimpsectl applets run /usr/share/glimpse/applets/me.example.A/applet.mjs"
    );
}

#[test]
fn exec_arguments_quote_reserved_characters() {
    assert_eq!(exec_quote("plain"), "plain");
    assert_eq!(exec_quote("a b"), "\"a b\"");
    assert_eq!(exec_quote("a'b"), "\"a'b\"");
    assert_eq!(exec_quote("a\"`$\\b"), "\"a\\\"\\`\\$\\\\b\"");
    assert_eq!(exec_quote("a%b"), "a%%b");
}

#[test]
fn desktop_rewrite_preserves_actions() {
    let source = "[Desktop Entry]\nName=Applet\nExec=old\nTryExec=old\n[Desktop Action a]\nExec=action\nTryExec=action\n";
    let rewritten = rewrite_desktop(source, "glimpsectl applets run /usr/share/applet.mjs");
    assert!(rewritten.contains("Name=Applet\nTryExec=glimpsectl\nExec=glimpsectl applets run /usr/share/applet.mjs\n[Desktop Action a]"));
    assert!(rewritten.ends_with("[Desktop Action a]\nExec=action\nTryExec=action\n"));
}

#[test]
fn new_renders_a_valid_scaffold_without_placeholders() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("hello");
    applets_new_with_catalog(args(&dir, "Hello \"Applet\""), |_| false).unwrap();
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|item| item.unwrap().file_name().into_string().unwrap())
        .collect();
    files.sort();
    assert_eq!(
        files,
        [
            "README.md",
            "index.tsx",
            "me.example.Hello.desktop",
            "package.json",
            "tsconfig.json"
        ]
    );
    for name in &files {
        assert!(!fs::read_to_string(dir.join(name)).unwrap().contains("{{"));
    }
    let desktop = fs::read_to_string(dir.join("me.example.Hello.desktop")).unwrap();
    assert!(desktop.contains(&format!(
        "\nExec=glimpsectl applets run \"{}/build/applet.mjs\"\n",
        dir.canonicalize().unwrap().display()
    )));
    assert!(desktop.contains("\nName=Hello \"Applet\"\n"));
    assert!(desktop.ends_with("Implements=me.aresa.Glimpse.Applet1\n"));
    assert!(
        fs::read_to_string(dir.join("index.tsx"))
            .unwrap()
            .contains(r#"title={ "Hello \"Applet\"" }"#)
    );
    let package: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("package.json")).unwrap()).unwrap();
    assert_eq!(package["name"], "me.example.Hello");
    assert_eq!(package["dependencies"]["@glimpse/applet"], "^0.1.0");
    assert!(
        fs::read_to_string(dir.join("index.tsx"))
            .unwrap()
            .contains("<Popover>")
    );
}

#[test]
fn new_refuses_values_a_desktop_entry_cannot_carry_unescaped() {
    let root = tempfile::tempdir().unwrap();
    assert!(applets_new_with_catalog(args(&root.path().join("a$b"), "Hello"), |_| false).is_err());
    assert!(applets_new_with_catalog(args(&root.path().join("ok"), "a\\b"), |_| false).is_err());
    assert!(!root.path().join("ok").exists());
}
