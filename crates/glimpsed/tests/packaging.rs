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

#[test]
fn session_target_is_the_only_graphical_session_entrypoint() {
    let root = workspace_root();
    let directory = root.join("data/systemd");
    let members = [
        "glimpsed",
        "glimpse-panel",
        "glimpse-wallpaper",
        "glimpse-sunset",
        "glimpse-notifications",
    ];
    let target = fs::read_to_string(directory.join("glimpse-session.target")).expect("target");
    assert!(target.contains("WantedBy=graphical-session.target"));
    assert!(target.contains("PartOf=graphical-session.target"));

    for member in members {
        let name = format!("{member}.service");
        let unit = fs::read_to_string(directory.join(&name)).expect("member unit");
        assert!(unit.contains("PartOf=glimpse-session.target"), "{name}");
        assert!(
            !unit.contains("WantedBy=graphical-session.target"),
            "{name}"
        );
        assert!(
            unit.contains("ExecReload=/bin/kill -HUP $MAINPID"),
            "{name}"
        );
        assert!(
            target
                .lines()
                .any(|line| line.starts_with("Wants=") && line.contains(&name))
        );
        assert!(
            target
                .lines()
                .any(|line| { line.starts_with("PropagatesReloadTo=") && line.contains(&name) })
        );
        if !matches!(member, "glimpsed" | "glimpse-notifications") {
            assert!(unit.contains("After=glimpsed.service"), "{name}");
            assert!(unit.contains("Wants=glimpsed.service"), "{name}");
        }
    }

    let lock = fs::read_to_string(directory.join("glimpse-lock.service")).expect("lock unit");
    assert!(lock.contains("PartOf=graphical-session.target"));
    assert!(!target.contains("glimpse-lock.service"));
}

#[test]
fn notification_provider_binary_is_packaged_with_its_local_services() {
    let root = workspace_root();
    let manifest = fs::read_to_string(root.join("crates/glimpse-notifications/Cargo.toml"))
        .expect("notifications manifest");
    for forbidden in ["glimpse-panel", "glimpsed", "glimpse-ipc"] {
        assert!(!manifest.contains(forbidden), "depends on {forbidden}");
    }
    assert!(manifest.contains("glimpse-services.workspace = true"));
    assert!(manifest.contains("glimpse-dbus.workspace = true"));

    let package =
        fs::read_to_string(root.join("crates/glimpsed/Cargo.toml")).expect("package manifest");
    assert_eq!(
        package
            .matches("target/release/glimpse-notifications")
            .count(),
        2
    );
    let binaries = fs::read_to_string(root.join("justfile")).expect("justfile");
    assert!(binaries.contains(
        "binaries := \"glimpsectl glimpsed glimpse-panel glimpse-lock glimpse-wallpaper glimpse-sunset glimpse-notifications glimpse-weather\""
    ));
}

#[test]
fn weather_provider_is_packaged_and_dbus_activated_without_eager_session_start() {
    let root = workspace_root();
    let manifest = fs::read_to_string(root.join("crates/glimpse-weather/Cargo.toml"))
        .expect("weather manifest");
    for forbidden in ["glimpse-panel", "glimpsed", "glimpse-ipc"] {
        assert!(!manifest.contains(forbidden), "depends on {forbidden}");
    }
    assert!(manifest.contains("glimpse-services.workspace = true"));
    assert!(manifest.contains("glimpse-dbus.workspace = true"));

    let package =
        fs::read_to_string(root.join("crates/glimpsed/Cargo.toml")).expect("package manifest");
    assert_eq!(package.matches("target/release/glimpse-weather").count(), 2);

    let unit = fs::read_to_string(root.join("data/systemd/glimpse-weather.service"))
        .expect("weather unit");
    assert!(unit.contains("Type=dbus"));
    assert!(unit.contains("BusName=me.aresa.Glimpse.Weather"));
    assert!(!unit.contains("glimpsed.service"));

    let activation =
        fs::read_to_string(root.join("data/dbus-1/services/me.aresa.Glimpse.Weather.service"))
            .expect("weather activation");
    assert!(activation.contains("SystemdService=glimpse-weather.service"));

    let uninstall =
        fs::read_to_string(root.join("scripts/uninstall.sh")).expect("uninstall script");
    assert!(uninstall.contains("me.aresa.Glimpse.Weather.service"));

    let target = fs::read_to_string(root.join("data/systemd/glimpse-session.target"))
        .expect("session target");
    assert!(!target.contains("glimpse-weather.service"));
}
