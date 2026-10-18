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
            ".gitignore",
            "README.md",
            "biome.json",
            "devconfig.toml",
            "index.tsx",
            "justfile",
            "me.example.Hello.desktop",
            "package.json",
            "tsconfig.json"
        ]
    );
    for name in &files {
        let content = fs::read_to_string(dir.join(name)).unwrap();
        assert!(!content.replace("{{ zone }}", "").contains("{{"), "{name}");
    }
    let justfile = fs::read_to_string(dir.join("justfile")).unwrap();
    assert!(justfile.lines().any(|line| line == "dev zone=\"left\":"));
    assert!(justfile.contains("\n    glimpsectl applet run --dev --zone {{ zone }} .\n"));
    let desktop = fs::read_to_string(dir.join("me.example.Hello.desktop")).unwrap();
    assert!(desktop.contains("\nExec=node applet.mjs\n"));
    assert!(!desktop.contains("\nPath="));
    assert!(
        package_scripts(&dir)["start"]
            .as_str()
            .unwrap()
            .ends_with("node dist/applet.mjs")
    );
    assert!(desktop.contains("\nName=Hello \"Applet\"\n"));
    assert!(desktop.ends_with("Implements=me.aresa.Glimpse.Applet1\n"));
    assert!(
        fs::read_to_string(dir.join("index.tsx"))
            .unwrap()
            .contains(r#"const NAME = "Hello \"Applet\"";"#)
    );
    let package: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("package.json")).unwrap()).unwrap();
    assert_eq!(package["name"], "me.example.Hello");
    assert_eq!(package["dependencies"]["glimpse-sdk"], "^0.13.0");
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
        (AppletLanguage::Rs, "src/main.rs", "./hello", "Cargo.toml"),
        (
            AppletLanguage::Py,
            "applet.py",
            "uv run applet.py",
            "applet.py",
        ),
        (AppletLanguage::Go, "main.go", "./hello", "go.mod"),
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
        assert!(
            !files.iter().any(|path| path.starts_with("glimpse_sdk")),
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
            assert!(
                !content.replace("{{ zone }}", "").contains("{{"),
                "{}",
                relative.display()
            );
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
            .contains("glimpse-sdk = \"0.13\"")
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
    assert!(
        fs::read_to_string(dir.join("go.mod"))
            .unwrap()
            .contains("require github.com/alex-oleshkevich/glimpse/sdk/sdk-go v0.13.0")
    );
    let dir = root.path().join("py");
    applets_new_with_catalog(
        args_for(AppletLanguage::Py, &dir, "Hello"),
        &template_for(AppletLanguage::Py),
        |_| false,
    )
    .unwrap();
    assert!(
        fs::read_to_string(dir.join("applet.py"))
            .unwrap()
            .contains("dependencies = [\"glimpse-applet-sdk>=0.13\"]")
    );
}

#[test]
fn every_template_builds_into_dist_and_dev_has_no_build_dependency() {
    for language in [
        AppletLanguage::Ts,
        AppletLanguage::Py,
        AppletLanguage::Go,
        AppletLanguage::Rs,
    ] {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(language.dir());
        applets_new_with_catalog(
            args_for(language, &dir, "Hello"),
            &template_for(language),
            |_| false,
        )
        .unwrap();
        let justfile = fs::read_to_string(dir.join("justfile")).unwrap();
        let build = justfile
            .split("\nbuild:")
            .nth(1)
            .and_then(|rest| rest.split_once('\n'))
            .unwrap()
            .1
            .split("\n\n")
            .next()
            .unwrap();
        assert!(
            justfile.lines().any(|line| line == "dev zone=\"left\":"),
            "{}",
            language.dir()
        );
        assert!(
            justfile.contains("\n    glimpsectl applet run --dev --zone {{ zone }} .\n"),
            "{}",
            language.dir()
        );
        assert!(build.contains("me.example.Hello.desktop dist/"), "{build}");
        assert!(!build.contains("cp hello "), "{build}");
        assert!(!build.contains("cp applet.mjs "), "{build}");
        assert!(!build.contains("./hello"), "{build}");
        let ignore = fs::read_to_string(dir.join(".gitignore")).unwrap();
        assert!(
            ignore
                .lines()
                .any(|line| line.trim_start_matches('/') == "dist/")
        );
        assert!(
            !ignore
                .lines()
                .any(|line| matches!(line.trim_start_matches('/'), "applet.mjs" | "hello"))
        );
        match language {
            AppletLanguage::Ts => {
                let scripts = package_scripts(&dir);
                assert!(
                    scripts["build"]
                        .as_str()
                        .unwrap()
                        .ends_with("--outfile=dist/applet.mjs")
                );
                assert_eq!(scripts["start"], "node --run build && node dist/applet.mjs");
            }
            AppletLanguage::Go => assert!(build.contains("go build -o dist/hello .")),
            AppletLanguage::Rs => {
                assert!(
                    build.contains(
                        "install -m755 \"${CARGO_TARGET_DIR:-target}/release/hello\" dist/"
                    )
                )
            }
            AppletLanguage::Py => {
                assert!(build.contains("cp applet.py me.example.Hello.desktop dist/"))
            }
        }
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
