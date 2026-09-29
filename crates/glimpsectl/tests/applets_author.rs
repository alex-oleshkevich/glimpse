use std::fs;
use std::path::Path;

use glimpsectl::cli::NewAppletArgs;
use glimpsectl::commands::applets_author::{
    applets_new_with_catalog, desktop_escape, exec_quote, rewrite_desktop, run_argv,
};

#[test]
fn desktop_values_escape_control_characters() {
    assert_eq!(desktop_escape("a\\b\nc\rd\te"), "a\\\\b\\nc\\rd\\te");
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
    applets_new_with_catalog(
        NewAppletArgs {
            id: Some("me.example.Hello".to_owned()),
            name: Some("Hello Applet".to_owned()),
            description: Some("A test applet".to_owned()),
            icon: None,
            dir: Some(dir.clone()),
            yes: true,
        },
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/applet/template"),
        |_| false,
    )
    .unwrap();
    for name in [
        "index.tsx",
        "package.json",
        "tsconfig.json",
        "me.example.Hello.desktop",
        "README.md",
    ] {
        assert!(!fs::read_to_string(dir.join(name)).unwrap().contains("{{"));
    }
    let desktop = fs::read_to_string(dir.join("me.example.Hello.desktop")).unwrap();
    assert!(desktop.contains(&run_argv(&dir.join("build/applet.mjs"))));
    assert!(desktop.contains("Implements=me.aresa.Glimpse.Applet1\n"));
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
