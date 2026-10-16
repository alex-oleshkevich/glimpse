use std::fs;
use std::path::{Path, PathBuf};

use glimpsectl::cli::{AppletLanguage, NewAppletArgs};
use glimpsectl::commands::applets_author::applets_new_with_catalog;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn template() -> PathBuf {
    template_for(AppletLanguage::Ts)
}

fn template_for(language: AppletLanguage) -> PathBuf {
    repo().join("sdk/templates").join(language.dir())
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut pending = vec![dir.to_owned()];
    let mut files = Vec::new();
    while let Some(current) = pending.pop() {
        for item in fs::read_dir(current).unwrap() {
            let path = item.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path.strip_prefix(dir).unwrap().to_owned());
            }
        }
    }
    files.sort();
    files
}

fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|item| item.unwrap().file_name().into_string().unwrap())
        .filter(|name| name != "__pycache__")
        .collect();
    names.sort();
    names
}

fn assert_same(canonical: &Path, vendored: &Path) {
    assert_eq!(
        fs::read(canonical).unwrap(),
        fs::read(vendored).unwrap(),
        "{}",
        canonical.display()
    );
}

fn package_scripts(dir: &Path) -> serde_json::Value {
    let package: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("package.json")).unwrap()).unwrap();
    package["scripts"].clone()
}

fn args(dir: &Path, name: &str) -> NewAppletArgs {
    args_for(AppletLanguage::Ts, dir, name)
}

fn args_for(language: AppletLanguage, dir: &Path, name: &str) -> NewAppletArgs {
    NewAppletArgs {
        id: Some("me.example.Hello".to_owned()),
        language,
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
    assert_eq!(package["dependencies"]["glimpse-sdk"], "^0.12.0");
    assert!(
        fs::read_to_string(dir.join("index.tsx"))
            .unwrap()
            .contains("<Popover icon=")
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

#[test]
fn new_renders_every_language_without_placeholders() {
    let cases = [
        (
            AppletLanguage::Rs,
            "src/main.rs",
            "env CARGO_TARGET_DIR=/tmp/glimpse-hello cargo run --release --quiet",
            "glimpse_sdk/src/session.rs",
        ),
        (
            AppletLanguage::Py,
            "applet.py",
            "uv run applet.py",
            "glimpse_sdk/session.py",
        ),
        (
            AppletLanguage::Go,
            "main.go",
            "go run .",
            "glimpse_sdk/sdk/session.go",
        ),
    ];
    for (language, entry, exec, vendored) in cases {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(language.dir());
        applets_new_with_catalog(
            args_for(language, &dir, "Hello \"Applet\""),
            &template_for(language),
            |_| false,
        )
        .unwrap();
        let files = files_under(&dir);
        assert!(files.contains(&PathBuf::from(entry)), "{}", language.dir());
        assert!(
            files.contains(&PathBuf::from(vendored)),
            "{}",
            language.dir()
        );
        for relative in &files {
            assert!(
                !relative.to_string_lossy().contains(".jinja"),
                "{}",
                relative.display()
            );
            let content = fs::read_to_string(dir.join(relative)).unwrap();
            assert!(!content.contains("{{"), "{}", relative.display());
        }
        let desktop = fs::read_to_string(dir.join("me.example.Hello.desktop")).unwrap();
        assert!(desktop.contains(&format!("\nExec={exec}\n")));
        assert!(desktop.contains("\nName=Hello \"Applet\"\n"));
    }
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("rs");
    applets_new_with_catalog(
        args_for(AppletLanguage::Rs, &dir, "Hello"),
        &template_for(AppletLanguage::Rs),
        |_| false,
    )
    .unwrap();
    assert!(
        fs::read_to_string(dir.join("Cargo.toml"))
            .unwrap()
            .contains("name = \"hello\"")
    );
    assert!(
        fs::read_to_string(dir.join("Cargo.toml"))
            .unwrap()
            .contains("exclude = [\"glimpse_sdk\"]")
    );
    let dir = root.path().join("go");
    applets_new_with_catalog(
        args_for(AppletLanguage::Go, &dir, "Hello"),
        &template_for(AppletLanguage::Go),
        |_| false,
    )
    .unwrap();
    assert!(
        fs::read_to_string(dir.join("go.mod"))
            .unwrap()
            .contains("module glimpse.local/hello")
    );
}

#[test]
fn vendored_template_sdks_match_canonical_sources() {
    let root = repo();
    let py = root.join("sdk/sdk-py/glimpse_sdk");
    let vendored_py = root.join("sdk/templates/py/glimpse_sdk");
    assert_eq!(file_names(&py), file_names(&vendored_py));
    for name in file_names(&py) {
        assert_same(&py.join(&name), &vendored_py.join(&name));
    }

    let go = root.join("sdk/sdk-go");
    let vendored_go = root.join("sdk/templates/go/glimpse_sdk");
    for name in ["go.mod", "LICENSE"] {
        assert_same(&go.join(name), &vendored_go.join(name));
    }
    let go_sources: Vec<_> = file_names(&go.join("sdk"))
        .into_iter()
        .filter(|name| !name.ends_with("_test.go"))
        .collect();
    assert_eq!(go_sources, file_names(&vendored_go.join("sdk")));
    for name in go_sources {
        assert_same(
            &go.join("sdk").join(&name),
            &vendored_go.join("sdk").join(&name),
        );
    }

    let rs = root.join("sdk/sdk-rs");
    let vendored_rs = root.join("sdk/templates/rs/glimpse_sdk");
    for name in ["Cargo.toml", "LICENSE", "README.md"] {
        assert_same(&rs.join(name), &vendored_rs.join(name));
    }
    assert_eq!(
        file_names(&rs.join("src")),
        file_names(&vendored_rs.join("src"))
    );
    for name in file_names(&rs.join("src")) {
        assert_same(
            &rs.join("src").join(&name),
            &vendored_rs.join("src").join(&name),
        );
    }
}

#[test]
fn new_refuses_a_missing_template_before_creating_the_destination() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("hello");
    assert!(
        applets_new_with_catalog(args(&dir, "Hello"), &root.path().join("missing"), |_| false)
            .is_err()
    );
    assert!(!dir.exists());
}
