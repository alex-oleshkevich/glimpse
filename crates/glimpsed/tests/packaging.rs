use std::fs;
use std::path::{Path, PathBuf};

use toml::{Table, Value};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("the workspace root sits two levels above this crate")
}

fn languages(root: &Path) -> Vec<String> {
    let linguas = fs::read_to_string(root.join("po/LINGUAS")).expect("po/LINGUAS is readable");
    linguas
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

fn manifest(root: &Path) -> Table {
    fs::read_to_string(root.join("crates/glimpsed/Cargo.toml"))
        .expect("the manifest is readable")
        .parse()
        .expect("the manifest is valid TOML")
}

fn assets(manifest: &Table, kind: &str) -> Vec<(String, String)> {
    manifest["package"]["metadata"][kind]["assets"]
        .as_array()
        .unwrap_or_else(|| panic!("[package.metadata.{kind}] declares an assets array"))
        .iter()
        .map(|asset| match asset {
            Value::Array(row) => (text(&row[0]), text(&row[1])),
            table => (text(&table["source"]), text(&table["dest"])),
        })
        .collect()
}

fn text(value: &Value) -> String {
    value
        .as_str()
        .expect("an asset source and destination are strings")
        .trim_start_matches('/')
        .to_owned()
}

#[test]
fn every_language_in_linguas_reaches_both_package_manifests() {
    let root = workspace_root();
    let manifest = manifest(&root);
    let languages = languages(&root);
    assert!(!languages.is_empty(), "po/LINGUAS names no language");

    for kind in ["deb", "generate-rpm"] {
        let assets = assets(&manifest, kind);
        for language in &languages {
            let source = format!("target/locale/{language}/LC_MESSAGES/glimpse.mo");
            let destination = format!("usr/share/locale/{language}/LC_MESSAGES");
            assert!(
                assets
                    .iter()
                    .any(|(from, to)| from.ends_with(&source) && to.starts_with(&destination)),
                "po/LINGUAS names {language} but [package.metadata.{kind}] has no asset \
                 putting {source} at /{destination}; the package would be built without it"
            );
        }
    }
}

#[test]
fn no_catalog_asset_reaches_a_language_through_a_glob() {
    let manifest = manifest(&workspace_root());

    for kind in ["deb", "generate-rpm"] {
        for (source, _) in assets(&manifest, kind) {
            assert!(
                !(source.contains("locale") && source.contains('*')),
                "[package.metadata.{kind}] reaches the catalogs through a glob ({source}); \
                 cargo-deb flattens an asset glob onto the destination directory, so it would \
                 ship one arbitrary language at the wrong path and still report success"
            );
        }
    }
}
