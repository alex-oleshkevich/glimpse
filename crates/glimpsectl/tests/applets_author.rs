use std::fs;
use std::path::{Path, PathBuf};

use glimpsectl::cli::NewAppletArgs;
use glimpsectl::commands::applets_author::applets_new_with_catalog;

fn template() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/template")
}

fn package_scripts(dir: &Path) -> serde_json::Value {
    let package: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("package.json")).unwrap()).unwrap();
    package["scripts"].clone()
}

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
fn new_renders_a_valid_scaffold_without_placeholders() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("hello");
    applets_new_with_catalog(args(&dir, "Hello \"Applet\""), &template(), |_| false).unwrap();
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|item| item.unwrap().file_name().into_string().unwrap())
        .collect();
    files.sort();
    assert_eq!(
        files,
        [
            "README.md",
            "config.toml",
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
    assert!(desktop.contains("\nExec=node --run start\n"));
    assert!(!desktop.contains("\nPath="));
    assert!(
        package_scripts(&dir)["start"]
            .as_str()
            .unwrap()
            .ends_with("node build/applet.mjs")
    );
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
    assert_eq!(package["dependencies"]["glimpse-sdk"], "^0.9.0");
    assert!(
        fs::read_to_string(dir.join("index.tsx"))
            .unwrap()
            .contains("<Popover>")
    );
}

#[test]
fn new_refuses_values_a_desktop_entry_cannot_carry_unescaped() {
    let root = tempfile::tempdir().unwrap();
    assert!(
        applets_new_with_catalog(args(&root.path().join("ok"), "a\\b"), &template(), |_| {
            false
        })
        .is_err()
    );
    assert!(!root.path().join("ok").exists());
}
