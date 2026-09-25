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
    fs::read_to_string(root.join("crates/glimpse-package/Cargo.toml"))
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
fn the_lock_pam_stack_reaches_both_package_manifests_as_configuration() {
    let root = workspace_root();
    assert!(root.join("data/pam.d/glimpse-lock").is_file());
    let manifest = manifest(&root);

    for kind in ["deb", "generate-rpm"] {
        assert!(
            assets(&manifest, kind).iter().any(|(from, to)| {
                from.contains("data/pam.d/")
                    && from.ends_with("/glimpse-lock")
                    && to == "etc/pam.d/glimpse-lock"
            }),
            "[package.metadata.{kind}] does not install a data/pam.d stack at /etc/pam.d/glimpse-lock; \
             PAM would fall through to `other`, which denies every password"
        );
    }

    let conf_files = manifest["package"]["metadata"]["deb"]["conf-files"]
        .as_array()
        .expect("the deb manifest declares conf-files");
    assert!(
        conf_files
            .iter()
            .any(|file| file.as_str() == Some("/etc/pam.d/glimpse-lock"))
    );
    let rpm = manifest["package"]["metadata"]["generate-rpm"]["assets"]
        .as_array()
        .expect("the rpm manifest declares assets");
    assert!(rpm.iter().any(|asset| {
        asset.get("dest").and_then(Value::as_str) == Some("/etc/pam.d/glimpse-lock")
            && asset.get("config").and_then(Value::as_bool) == Some(true)
    }));
}

#[test]
fn each_package_ships_the_pam_stack_its_distribution_has() {
    let root = workspace_root();
    let manifest = manifest(&root);
    let source = |kind: &str| {
        let (from, _) = assets(&manifest, kind)
            .into_iter()
            .find(|(_, to)| to == "etc/pam.d/glimpse-lock")
            .unwrap_or_else(|| panic!("[package.metadata.{kind}] ships no PAM stack"));
        let base = if kind == "deb" {
            root.join("crates/glimpse-package")
        } else {
            root.clone()
        };
        fs::read_to_string(base.join(&from)).expect("the PAM stack source is readable")
    };
    let deb = source("deb");
    assert!(
        !deb.contains("system-auth"),
        "Debian has no system-auth; including it fails every attempt"
    );
    assert!(deb.contains("common-auth") && deb.contains("common-account"));
    let fedora = source("generate-rpm");
    assert!(
        fedora.contains("password-auth") && !fedora.contains("system-auth"),
        "Fedora's system-auth can hold pam_fprintd, which waits for a finger the prompt never asks for"
    );
}

#[test]
fn the_opensuse_rpm_differs_from_the_fedora_rpm_only_in_its_pam_stack() {
    let root = workspace_root();
    let manifest = manifest(&root);
    let rpm = &manifest["package"]["metadata"]["generate-rpm"];
    let fedora = rpm["assets"]
        .as_array()
        .expect("the rpm manifest declares assets");
    let opensuse = rpm["variants"]["opensuse"]["assets"]
        .as_array()
        .expect("the opensuse variant declares its own assets, since a variant replaces the list");
    let pam = |asset: &&Value| asset["dest"].as_str() == Some("/etc/pam.d/glimpse-lock");
    let rest = |assets: &[Value]| -> Vec<Value> {
        assets.iter().filter(|asset| !pam(asset)).cloned().collect()
    };
    assert_eq!(
        rest(fedora),
        rest(opensuse),
        "the opensuse variant has drifted from the base rpm asset list"
    );
    assert!(
        rpm["variants"]["opensuse"].get("requires").is_some(),
        "without its own requires the opensuse rpm inherits Fedora package names and will not install"
    );

    let stack = opensuse
        .iter()
        .find(pam)
        .expect("the opensuse rpm ships a PAM stack");
    assert_eq!(stack["config"].as_bool(), Some(true));
    let text = fs::read_to_string(root.join(stack["source"].as_str().expect("a source path")))
        .expect("the PAM stack source is readable");
    assert!(
        text.contains("common-auth") && text.contains("common-account"),
        "openSUSE has no system-auth or password-auth"
    );
}

#[test]
fn the_install_scripts_copy_pam_files_but_not_the_debian_directory() {
    let root = workspace_root();
    assert!(root.join("data/pam.d/debian").is_dir());
    for script in ["scripts/install.sh", "scripts/package-binary.sh"] {
        let text = fs::read_to_string(root.join(script)).expect("script");
        let guard = text
            .lines()
            .skip_while(|line| !line.contains("data/pam.d/*"))
            .nth(1)
            .expect("the pam.d loop has a guard");
        assert!(
            guard.contains("[[ -f \"$f\""),
            "{script} must copy regular files only, or data/pam.d/debian lands in /etc/pam.d"
        );
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
        "glimpse-panel",
        "glimpse-wallpaper",
        "glimpse-sunset",
        "glimpse-notifications",
        "glimpse-idle",
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
    }

    let lock = fs::read_to_string(directory.join("glimpse-lock.service")).expect("lock unit");
    assert!(lock.contains("PartOf=graphical-session.target"));
    assert!(!lock.contains("glimpse-session.target"));
    let mentions: Vec<&str> = target
        .lines()
        .filter(|line| line.contains("glimpse-lock.service"))
        .collect();
    assert_eq!(mentions.len(), 1);
    assert!(mentions[0].starts_with("Wants="));
}

#[test]
fn binary_tarball_includes_idle_portal_assets() {
    let root = workspace_root();
    let script = fs::read_to_string(root.join("scripts/package-binary.sh"))
        .expect("binary packaging script");

    for (source, destination) in [
        (
            "data/portals/*.portal",
            "usr/share/xdg-desktop-portal/portals",
        ),
        (
            "data/portals/*-portals.conf",
            "usr/share/xdg-desktop-portal",
        ),
    ] {
        assert!(script.contains(source), "{source}");
        assert!(script.contains(destination), "{destination}");
    }

    assert!(root.join("data/portals/glimpse.portal").is_file());
    assert!(root.join("data/portals/glimpse-portals.conf").is_file());
}

#[test]
fn the_default_idle_handler_script_ships_everywhere() {
    let root = workspace_root();
    let read = |path: &str| fs::read_to_string(root.join(path)).expect(path);

    assert_eq!(
        read("crates/glimpse-package/Cargo.toml")
            .matches("data/bin/glimpse-dpms\"")
            .count(),
        3,
        "the .deb, the Fedora .rpm and the openSUSE .rpm"
    );
    for script in ["scripts/install.sh", "scripts/package-binary.sh"] {
        assert!(
            read(script).contains("install -Dm755 data/bin/glimpse-dpms"),
            "{script}"
        );
    }
    assert!(read("data/config.default.toml").contains("\"glimpse-dpms off\""));
}

#[test]
fn every_workspace_binary_is_built_and_reaches_every_package() {
    let root = workspace_root();
    let justfile = fs::read_to_string(root.join("justfile")).expect("justfile");
    let listed: Vec<&str> = justfile
        .lines()
        .find_map(|line| line.strip_prefix("binaries :="))
        .expect("justfile has a binaries list")
        .trim()
        .trim_matches('"')
        .split_whitespace()
        .collect();
    let package = fs::read_to_string(root.join("crates/glimpse-package/Cargo.toml"))
        .expect("package manifest");

    let mut binaries: Vec<String> = fs::read_dir(root.join("crates"))
        .expect("crates directory")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("src/main.rs").exists())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    binaries.sort();
    assert!(!binaries.is_empty());

    for binary in &binaries {
        assert!(
            listed.contains(&binary.as_str()),
            "{binary} is missing from the justfile's binaries list"
        );
        assert_eq!(
            package
                .matches(&format!("target/release/{binary}\""))
                .count(),
            3,
            "{binary} must ship in the .deb, the Fedora .rpm and the openSUSE .rpm"
        );
    }
    assert_eq!(
        listed.len(),
        binaries.len(),
        "the justfile lists a binary no crate builds"
    );
}

#[test]
fn notification_provider_binary_is_packaged_with_its_local_services() {
    let root = workspace_root();
    let manifest = fs::read_to_string(root.join("crates/glimpse-notifications/Cargo.toml"))
        .expect("notifications manifest");
    assert!(
        !manifest.contains("glimpse-panel"),
        "a provider that depends on the panel is not standalone"
    );
    assert!(manifest.contains("glimpse-services.workspace = true"));
    assert!(manifest.contains("glimpse-dbus.workspace = true"));

    let package = fs::read_to_string(root.join("crates/glimpse-package/Cargo.toml"))
        .expect("package manifest");
    assert_eq!(
        package
            .matches("target/release/glimpse-notifications")
            .count(),
        3
    );
    let binaries = fs::read_to_string(root.join("justfile")).expect("justfile");
    let binaries_line = binaries
        .lines()
        .find(|line| line.starts_with("binaries :="))
        .expect("justfile has a binaries list");
    assert!(binaries_line.contains("glimpse-notifications"));
}

#[test]
fn weather_provider_is_packaged_and_dbus_activated_without_eager_session_start() {
    let root = workspace_root();
    let manifest = fs::read_to_string(root.join("crates/glimpse-weather/Cargo.toml"))
        .expect("weather manifest");
    assert!(
        !manifest.contains("glimpse-panel"),
        "a provider that depends on the panel is not standalone"
    );
    assert!(manifest.contains("glimpse-services.workspace = true"));
    assert!(manifest.contains("glimpse-dbus.workspace = true"));

    let package = fs::read_to_string(root.join("crates/glimpse-package/Cargo.toml"))
        .expect("package manifest");
    assert_eq!(package.matches("target/release/glimpse-weather").count(), 3);

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

#[test]
fn the_color_picker_ships_as_a_plain_binary() {
    let root = workspace_root();
    let package = fs::read_to_string(root.join("crates/glimpse-package/Cargo.toml"))
        .expect("package manifest");
    assert_eq!(package.matches("target/release/glimpse-picker").count(), 3);
    assert!(!root.join("data/systemd/glimpse-picker.service").exists());

    let manifest =
        fs::read_to_string(root.join("crates/glimpse-picker/Cargo.toml")).expect("picker manifest");
    assert!(
        !manifest.contains("zbus"),
        "the picker is a command, not a D-Bus provider"
    );
}

#[test]
fn every_install_route_ships_the_commented_reference() {
    let root = workspace_root();
    assert!(root.join("data/config.commented.toml").is_file());

    for script in ["scripts/install.sh", "scripts/package-binary.sh"] {
        let text = fs::read_to_string(root.join(script)).expect(script);
        assert!(text.contains("data/config.commented.toml"), "{script}");
        assert!(
            text.contains("usr/share/glimpse/config.commented.toml")
                || text.contains("$sharedir/config.commented.toml"),
            "{script}"
        );
    }

    let manifest = manifest(&root);
    for kind in ["deb", "generate-rpm"] {
        assert!(
            assets(&manifest, kind)
                .iter()
                .any(
                    |(source, destination)| source.ends_with("data/config.commented.toml")
                        && destination.contains("usr/share/glimpse")
                ),
            "{kind}"
        );
    }
}

#[test]
fn every_install_route_ships_the_applet_sdk() {
    let root = workspace_root();
    for script in ["scripts/install.sh", "scripts/package-binary.sh"] {
        let body = fs::read_to_string(root.join(script)).expect(script);
        assert!(body.contains("find sdk/applet -type f -print0"), "{script}");
    }
    let manifest = manifest(&root);
    for kind in ["deb", "generate-rpm"] {
        let listed = assets(&manifest, kind);
        for source in [
            "sdk/applet/*.ts",
            "sdk/applet/template/*",
            "sdk/applet/examples/todo/*",
        ] {
            assert!(
                listed.iter().any(|(path, dest)| path.ends_with(source)
                    && dest.contains("share/glimpse/sdk/applet")),
                "{kind}: {source}"
            );
        }
    }
}

#[test]
fn the_seeded_user_config_is_the_commented_copy_and_not_the_defaults() {
    let install = fs::read_to_string(workspace_root().join("scripts/install.sh"))
        .expect("the install script");

    assert!(install.contains("install -m644 data/config.commented.toml \"$config\""));
    assert!(!install.contains("install -m644 data/config.default.toml \"$config\""));
}
