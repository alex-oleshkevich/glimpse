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

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut pending = vec![dir.to_owned()];
    let mut files = Vec::new();
    while let Some(current) = pending.pop() {
        for item in fs::read_dir(current).expect("a readable directory") {
            let path = item.expect("a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path.strip_prefix(dir).expect("a relative path").to_owned());
            }
        }
    }
    files.sort();
    files
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
fn idle_portal_routing_assets_reach_every_package() {
    let root = workspace_root();
    assert!(root.join("data/portals/glimpse.portal").is_file());
    let manifest = manifest(&root);
    for conf in [
        "glimpse-portals.conf",
        "niri-portals.conf",
        "hyprland-portals.conf",
    ] {
        let text = fs::read_to_string(root.join("data/portals").join(conf)).expect(conf);
        assert!(
            text.contains("org.freedesktop.impl.portal.Inhibit=glimpse"),
            "{conf}"
        );
        assert!(
            !text
                .lines()
                .any(|line| line.trim_start().starts_with("default")),
            "{conf} must not set a default: it would swallow every interface niri routes"
        );

        for kind in ["deb", "generate-rpm"] {
            let assets = assets(&manifest, kind);
            assert!(
                assets
                    .iter()
                    .any(|(from, to)| from.ends_with(conf)
                        && to.starts_with("etc/xdg-desktop-portal/")),
                "[package.metadata.{kind}] does not install {conf} into /etc/xdg-desktop-portal"
            );
            assert!(
                !assets
                    .iter()
                    .any(|(from, to)| from.ends_with("-portals.conf")
                        && to.starts_with("usr/share/xdg-desktop-portal")),
                "[package.metadata.{kind}] installs a portals.conf where it would replace niri's"
            );
            assert!(
                assets
                    .iter()
                    .any(|(from, to)| from.ends_with("-portals.conf")
                        && to.starts_with("usr/share/glimpse/portals")),
                "[package.metadata.{kind}] does not ship the reference copy of {conf}"
            );
        }
    }
}

#[test]
fn wallpaper_portal_assets_agree_with_the_binary() {
    let root = workspace_root();
    let portal =
        fs::read_to_string(root.join("data/portals/glimpse-wallpaper.portal")).expect(".portal");
    assert!(portal.contains("DBusName=me.aresa.Glimpse.Wallpaper.Portal"));
    assert!(portal.contains("Interfaces=org.freedesktop.impl.portal.Wallpaper"));
    assert!(portal.contains("UseIn=glimpse"));

    let activation = fs::read_to_string(
        root.join("data/dbus-1/services/me.aresa.Glimpse.Wallpaper.Portal.service"),
    )
    .expect("activation file");
    assert!(activation.contains("Name=me.aresa.Glimpse.Wallpaper.Portal"));
    assert!(activation.contains("Exec=/usr/bin/glimpse-wallpaper"));
    assert!(activation.contains("SystemdService=glimpse-wallpaper.service"));

    let source = fs::read_to_string(root.join("crates/glimpse-wallpaper/src/portal.rs"))
        .expect("portal source");
    assert!(source.contains("\"me.aresa.Glimpse.Wallpaper.Portal\""));

    for conf in [
        "glimpse-portals.conf",
        "niri-portals.conf",
        "hyprland-portals.conf",
    ] {
        let text = fs::read_to_string(root.join("data/portals").join(conf)).expect(conf);
        assert!(
            text.contains("org.freedesktop.impl.portal.Wallpaper=glimpse-wallpaper"),
            "{conf}"
        );
        assert!(
            text.contains("org.freedesktop.impl.portal.Screenshot=glimpse-shot"),
            "{conf}"
        );
        assert!(
            !text
                .lines()
                .any(|line| line.trim_start().starts_with("default")),
            "{conf} must not set a default"
        );
    }

    let uninstall = fs::read_to_string(root.join("scripts/uninstall.sh")).expect("uninstall");
    assert!(uninstall.contains("glimpse-wallpaper.portal"));
    assert!(uninstall.contains("me.aresa.Glimpse.Wallpaper.Portal.service"));
}

#[test]
fn the_etc_portal_routing_is_configuration_in_every_package() {
    let root = workspace_root();
    let manifest = manifest(&root);
    let metadata = &manifest["package"]["metadata"];
    let pkgbuild = fs::read_to_string(root.join("PKGBUILD")).expect("PKGBUILD");
    let install = fs::read_to_string(root.join("scripts/install.sh")).expect("install script");
    assert!(install.contains("$portaletcdir"));

    let conf_files = metadata["deb"]["conf-files"]
        .as_array()
        .expect("the deb manifest declares conf-files");
    let rpm_tables = [
        &metadata["generate-rpm"]["assets"],
        &metadata["generate-rpm"]["variants"]["opensuse"]["assets"],
    ];

    for path in [
        "/etc/geoclue/conf.d/glimpse.conf",
        "/etc/pam.d/glimpse-lock",
        "/etc/xdg-desktop-portal/glimpse-portals.conf",
        "/etc/xdg-desktop-portal/niri-portals.conf",
        "/etc/xdg-desktop-portal/hyprland-portals.conf",
    ] {
        assert!(
            pkgbuild.contains(&format!("'{}'", path.trim_start_matches('/'))),
            "PKGBUILD backup=() lacks {path}, so pacman overwrites an edit on upgrade"
        );
        if !path.starts_with("/etc/xdg-desktop-portal/") {
            continue;
        }
        assert!(
            conf_files.iter().any(|file| file.as_str() == Some(path)),
            "deb conf-files lacks {path}"
        );
        for table in rpm_tables {
            let asset = table
                .as_array()
                .expect("an rpm assets array")
                .iter()
                .find(|asset| asset["dest"].as_str() == Some(path))
                .unwrap_or_else(|| panic!("an rpm table does not install {path}"));
            assert!(asset.get("config").is_some(), "{path} is not rpm %config");
        }
    }
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
        .filter(|entry| entry.path().join("Cargo.toml").exists())
        .flat_map(|entry| {
            let manifest: Table = fs::read_to_string(entry.path().join("Cargo.toml"))
                .expect("crate manifest")
                .parse()
                .expect("valid TOML");
            let mut binaries = Vec::new();
            if entry.path().join("src/main.rs").exists() {
                binaries.push(text(&manifest["package"]["name"]));
            }
            if let Some(targets) = manifest.get("bin").and_then(Value::as_array) {
                binaries.extend(targets.iter().map(|target| text(&target["name"])));
            }
            binaries
        })
        .collect();
    binaries.sort();
    binaries.dedup();
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
fn weather_and_calendar_providers_are_not_started_by_the_session_target() {
    let root = workspace_root();
    let target = fs::read_to_string(root.join("data/systemd/glimpse-session.target"))
        .expect("session target");
    let wants = target
        .lines()
        .find_map(|line| line.strip_prefix("Wants="))
        .expect("Wants=");
    for unit in ["glimpse-weather.service", "glimpse-calendar.service"] {
        assert!(
            !wants.split_whitespace().any(|wanted| wanted == unit),
            "{unit} starts on its first D-Bus call, not with the session"
        );
    }
}

#[test]
fn session_target_wants_both_compositor_providers() {
    let root = workspace_root();
    let target = fs::read_to_string(root.join("data/systemd/glimpse-session.target"))
        .expect("session target");
    let wants = target
        .lines()
        .find_map(|line| line.strip_prefix("Wants="))
        .expect("Wants=");
    for unit in [
        "glimpse-compositor-niri.service",
        "glimpse-compositor-hyprland.service",
    ] {
        assert!(root.join("data/systemd").join(unit).is_file(), "{unit}");
        assert!(wants.split_whitespace().any(|want| want == unit), "{unit}");
    }
}

#[test]
fn notification_and_weather_providers_are_standalone() {
    let root = workspace_root();
    for provider in [
        "glimpse-notifications",
        "glimpse-weather",
        "glimpse-calendar",
    ] {
        let manifest = fs::read_to_string(root.join("crates").join(provider).join("Cargo.toml"))
            .expect(provider);
        assert!(
            !manifest.contains("glimpse-panel"),
            "{provider} depends on the panel, so it is not standalone"
        );
        assert!(manifest.contains("glimpse-services.workspace = true"));
        assert!(manifest.contains("glimpse-dbus.workspace = true"));
    }
}

#[test]
fn the_lens_ships_as_a_plain_binary() {
    let root = workspace_root();
    let package = fs::read_to_string(root.join("crates/glimpse-package/Cargo.toml"))
        .expect("package manifest");
    assert_eq!(package.matches("target/release/glimpse-lens").count(), 3);
    assert!(!package.contains("target/release/glimpse-picker"));
    assert!(!package.contains("target/release/glimpse-ruler"));
    assert!(!root.join("data/systemd/glimpse-lens.service").exists());

    for entry in fs::read_dir(root.join("data/dbus-1/services")).expect("dbus services") {
        let service =
            fs::read_to_string(entry.expect("dbus service").path()).expect("dbus service");
        assert!(
            !service.contains("glimpse-lens"),
            "the lens is a command, not a D-Bus provider"
        );
    }
}

#[test]
fn screenshot_binary_desktop_entry_and_service_reach_every_package() {
    let root = workspace_root();
    let manifest = manifest(&root);

    for kind in ["deb", "generate-rpm"] {
        let assets = assets(&manifest, kind);
        assert!(
            assets.iter().any(|(from, to)| {
                from.ends_with("target/release/glimpse-shot") && to.starts_with("usr/bin/")
            }),
            "{kind} omits glimpse-shot"
        );
        assert!(
            assets.iter().any(|(from, to)| {
                from.ends_with("target/locale/me.aresa.Glimpse.Shot.desktop")
                    && to.starts_with("usr/share/applications/")
            }),
            "{kind} omits the translated desktop entry"
        );
        assert!(
            assets.iter().any(|(from, to)| {
                from.ends_with("data/dbus-1/services/*.service")
                    && to.starts_with("usr/share/dbus-1/services/")
            }),
            "{kind} omits D-Bus services"
        );
    }

    let package = fs::read_to_string(root.join("crates/glimpse-package/Cargo.toml"))
        .expect("package manifest");
    assert_eq!(package.matches("target/release/glimpse-shot").count(), 3);
    assert_eq!(
        package
            .matches("target/locale/me.aresa.Glimpse.Shot.desktop")
            .count(),
        3
    );
    assert!(
        root.join("data/dbus-1/services/me.aresa.Glimpse.Shot.service")
            .is_file()
    );

    let justfile = fs::read_to_string(root.join("justfile")).expect("justfile");
    assert!(justfile.contains("glimpse-lens glimpse-shot"));
    assert!(justfile.contains("GLIMPSE_BINARIES=\"{{ binaries }}\" scripts/package-binary.sh"));
    for script in ["scripts/glimpse-paths.sh", "scripts/package-binary.sh"] {
        let text = fs::read_to_string(root.join(script)).expect(script);
        assert!(text.contains("${GLIMPSE_BINARIES:?"), "{script}");
    }
    for script in ["scripts/install.sh", "scripts/package-binary.sh"] {
        let text = fs::read_to_string(root.join(script)).expect(script);
        assert!(
            text.contains("$applicationsdir/me.aresa.Glimpse.Shot.desktop"),
            "{script}"
        );
    }
    let pkgbuild = fs::read_to_string(root.join("PKGBUILD")).expect("PKGBUILD");
    assert!(pkgbuild.contains("glib2>=2.80"));
}

#[test]
fn every_install_route_ships_the_six_config_references() {
    let root = workspace_root();
    let files: Vec<String> = ["config", "panel"]
        .iter()
        .flat_map(|doc| {
            ["default.toml", "commented.toml", "schema.json"].map(|kind| format!("{doc}.{kind}"))
        })
        .collect();

    for file in &files {
        assert!(root.join("data").join(file).is_file(), "{file}");
        for script in ["scripts/install.sh", "scripts/package-binary.sh"] {
            let text = fs::read_to_string(root.join(script)).expect(script);
            assert!(text.contains(&format!("data/{file}")), "{script} {file}");
            assert!(
                text.contains(&format!("usr/share/glimpse/{file}"))
                    || text.contains(&format!("$sharedir/{file}")),
                "{script} {file}"
            );
        }
        let manifest = manifest(&root);
        for kind in ["deb", "generate-rpm"] {
            assert!(
                assets(&manifest, kind)
                    .iter()
                    .any(
                        |(source, destination)| source.ends_with(&format!("data/{file}"))
                            && destination.contains("usr/share/glimpse")
                    ),
                "{kind} {file}"
            );
        }
    }
}

#[test]
fn every_install_route_ships_the_applet_templates_and_nothing_else_of_the_sdk() {
    let root = workspace_root();
    let templates = root.join("sdk/templates");
    let mut template: Vec<String> = files_under(&templates)
        .into_iter()
        .map(|path| format!("sdk/templates/{}", path.display()))
        .collect();
    template.sort();
    assert!(
        !template.iter().any(|path| path.contains("glimpse_sdk")),
        "templates carry no SDK sources"
    );
    for nested in [
        "sdk/templates/ts/{{id}}.desktop.jinja",
        "sdk/templates/rs/src/main.rs.jinja",
        "sdk/templates/py/applet.py.jinja",
        "sdk/templates/go/main.go.jinja",
    ] {
        assert!(template.contains(&nested.to_owned()), "{nested}");
    }
    for script in ["scripts/install.sh", "scripts/package-binary.sh"] {
        let body = fs::read_to_string(root.join(script)).expect(script);
        let selection = body
            .lines()
            .find_map(|line| {
                line.strip_prefix("done < <(")
                    .and_then(|line| line.strip_suffix(')'))
            })
            .expect("the template copy selects its files");
        let output = std::process::Command::new("bash")
            .args(["-c", &selection.replace("-print0", "-print")])
            .current_dir(&root)
            .output()
            .expect("the template selection runs");
        assert!(output.status.success(), "{script}");
        let mut selected: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_owned)
            .collect();
        selected.sort();
        assert_eq!(selected, template, "{script}");
    }
    let manifest = manifest(&root);
    for kind in ["deb", "generate-rpm"] {
        let sdk: Vec<_> = assets(&manifest, kind)
            .into_iter()
            .filter(|(path, _)| path.contains("sdk/"))
            .collect();
        assert_eq!(sdk.len(), 1, "{kind}");
        assert!(sdk[0].0.ends_with("sdk/templates/**/*"), "{kind}");
        assert!(sdk[0].1.contains("share/glimpse/sdk/templates"), "{kind}");
    }
}

#[test]
fn the_install_script_seeds_no_user_config() {
    let install = fs::read_to_string(workspace_root().join("scripts/install.sh"))
        .expect("the install script");

    for needle in ["seed_user_config", "XDG_CONFIG_HOME", "SUDO_USER", "$HOME"] {
        assert!(!install.contains(needle), "{needle}");
    }
}

#[test]
fn binary_portal_routing_preserves_the_compositors_share_directory() {
    let root = workspace_root();
    let dir = tempfile::tempdir().unwrap();
    let scratch = dir.path();
    let script = fs::read_to_string(root.join("scripts/package-binary.sh")).unwrap();
    let routing = script
        .lines()
        .skip_while(|line| !line.starts_with("for f in data/portals/"))
        .take_while(|line| !line.starts_with("for f in data/pam.d/"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!routing.is_empty());
    let output = std::process::Command::new("bash")
        .args(["-euc", &routing])
        .env("pkgroot", scratch)
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let share = scratch.join("usr/share/xdg-desktop-portal");
    for entry in fs::read_dir(root.join("data/portals")).unwrap() {
        let name = entry.unwrap().file_name();
        if name.to_string_lossy().ends_with("-portals.conf") {
            assert!(scratch.join("etc/xdg-desktop-portal").join(&name).is_file());
            assert!(!share.join(&name).exists());
            assert!(
                scratch
                    .join("usr/share/glimpse/portals")
                    .join(&name)
                    .is_file()
            );
        }
    }
    assert!(share.join("portals/glimpse.portal").is_file());
}

#[test]
fn weather_and_calendar_activation_names_agree_and_uninstall_removes_the_chain() {
    for provider in ["glimpse-weather", "glimpse-calendar"] {
        let root = workspace_root();
        let unit_name = format!("{provider}.service");
        let unit = fs::read_to_string(root.join("data/systemd").join(&unit_name)).unwrap();
        let value = |body: &str, key: &str| {
            body.lines()
                .find_map(|line| line.strip_prefix(key))
                .unwrap()
                .to_owned()
        };
        assert_eq!(value(&unit, "Type="), "dbus");
        let bus_name = value(&unit, "BusName=");
        let activation_name = format!("{bus_name}.service");
        let activation =
            fs::read_to_string(root.join("data/dbus-1/services").join(&activation_name)).unwrap();
        assert_eq!(value(&activation, "Name="), bus_name);
        assert_eq!(value(&activation, "SystemdService="), unit_name);
        assert_eq!(value(&activation, "Exec="), value(&unit, "ExecStart="));
        let dir = tempfile::tempdir().unwrap();
        let scratch = dir.path();
        let unit_path = scratch.join("usr/lib/systemd/user").join(unit_name);
        let activation_path = scratch
            .join("usr/share/dbus-1/services")
            .join(activation_name);
        for path in [&unit_path, &activation_path] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "fixture").unwrap();
        }
        let output = std::process::Command::new("bash")
            .arg(root.join("scripts/uninstall.sh"))
            .env("DESTDIR", scratch)
            .env("PREFIX", "/usr")
            .env("GLIMPSE_BINARIES", provider)
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(!unit_path.exists());
        assert!(!activation_path.exists());
    }
}
