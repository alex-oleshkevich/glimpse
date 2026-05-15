use std::{
    collections::HashMap,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::config::{discovery::ConfigFileDiscovery, panels::AppletConfig, panels::AppletType};

pub const SYSTEM_APPLETS_DIR: &str = "/usr/share/glimpse/applets";

#[derive(Debug, Clone)]
pub struct AppletDirectoryScanner {
    pub system_dir: PathBuf,
    pub user_dir: PathBuf,
}

#[derive(Debug, Default, Clone)]
pub struct DiscoveredApplets {
    pub normal: HashMap<String, AppletConfig>,
    pub dev: HashMap<String, AppletConfig>,
}

/// Where a discovered applet package came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppletSource {
    System,
    User,
    Dev,
}

impl std::fmt::Display for AppletSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::System => "system",
            Self::User => "user",
            Self::Dev => "dev",
        })
    }
}

/// One row of provenance-aware applet discovery (for `applets ls`).
#[derive(Debug, Clone)]
pub struct DiscoveredAppletInfo {
    pub id: String,
    pub kind: String,
    pub source: AppletSource,
}

fn applet_kind(config: &AppletConfig) -> String {
    match config.extends {
        Some(AppletType::Exec) => "exec",
        Some(AppletType::Command) => "command",
        _ => "other",
    }
    .to_owned()
}

impl AppletDirectoryScanner {
    pub fn new(system_dir: PathBuf, user_dir: PathBuf) -> Self {
        Self {
            system_dir,
            user_dir,
        }
    }

    pub fn from_process() -> Self {
        let discovery = ConfigFileDiscovery::from_process("GLIMPSE_CONFIG", "config.toml");
        let user_dir = discovery.config_dir().join("applets");
        Self::new(PathBuf::from(SYSTEM_APPLETS_DIR), user_dir)
    }

    pub fn scan(&self) -> DiscoveredApplets {
        let mut result = DiscoveredApplets::default();
        scan_dir(&self.system_dir, &mut result.normal, &mut result.dev);
        scan_dir(&self.user_dir, &mut result.normal, &mut result.dev);
        result
    }

    /// Provenance-aware listing. A user package shadows a system package of
    /// the same id (same precedence as [`scan`]); a user `*.dev.toml` shadows
    /// a system one. Dev packages share the id namespace with normal ones, so
    /// the same base id can appear once as normal and once as `dev`. Sorted by
    /// id, then source.
    pub fn scan_sources(&self) -> Vec<DiscoveredAppletInfo> {
        let mut sys_normal = HashMap::new();
        let mut sys_dev = HashMap::new();
        scan_dir(&self.system_dir, &mut sys_normal, &mut sys_dev);
        let mut user_normal = HashMap::new();
        let mut user_dev = HashMap::new();
        scan_dir(&self.user_dir, &mut user_normal, &mut user_dev);

        let mut out: Vec<DiscoveredAppletInfo> = Vec::new();
        for (id, cfg) in &user_normal {
            out.push(DiscoveredAppletInfo {
                id: id.clone(),
                kind: applet_kind(cfg),
                source: AppletSource::User,
            });
        }
        for (id, cfg) in &sys_normal {
            if !user_normal.contains_key(id) {
                out.push(DiscoveredAppletInfo {
                    id: id.clone(),
                    kind: applet_kind(cfg),
                    source: AppletSource::System,
                });
            }
        }
        for (id, cfg) in user_dev.iter().chain(sys_dev.iter()) {
            if out
                .iter()
                .any(|a| a.id == *id && a.source == AppletSource::Dev)
            {
                continue;
            }
            out.push(DiscoveredAppletInfo {
                id: id.clone(),
                kind: applet_kind(cfg),
                source: AppletSource::Dev,
            });
        }

        out.sort_by(|a, b| {
            a.id.cmp(&b.id)
                .then_with(|| a.source.to_string().cmp(&b.source.to_string()))
        });
        out
    }
}

fn scan_dir(
    dir: &Path,
    normal: &mut HashMap<String, AppletConfig>,
    dev: &mut HashMap<String, AppletConfig>,
) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        // Use fs::metadata (follows symlinks) so that symlinked .toml files
        // installed by `glimpse-applet link` are not skipped.
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };

        if meta.is_dir() {
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            scan_normal_package(&path.join("applet.toml"), name, normal);
            continue;
        }

        if !meta.is_file() || path.extension() != Some(OsStr::new("toml")) {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };

        if let Some(base) = stem.strip_suffix(".dev") {
            if !base.is_empty() {
                if let Some(config) = parse_dev_package(&path) {
                    dev.insert(base.to_string(), config);
                }
            }
            continue;
        }

        scan_normal_package(&path, stem, normal);
    }
}

fn scan_normal_package(path: &Path, expected_id: &str, normal: &mut HashMap<String, AppletConfig>) {
    if let Some((id, config)) = parse_package(path) {
        if id != expected_id {
            tracing::warn!(
                path = %path.display(),
                id,
                expected_id,
                "applet id does not match package path"
            );
        }
        if normal.contains_key(&id) {
            tracing::warn!(
                id,
                path = %path.display(),
                "duplicate applet id; overwriting previous entry"
            );
        }
        normal.insert(id, config);
    }
}

#[derive(Deserialize)]
struct AppletDescriptor {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    command: Option<toml::Value>,
    exec: Option<toml::Value>,
}

fn parse_package(path: &Path) -> Option<(String, AppletConfig)> {
    let content = fs::read_to_string(path)
        .map_err(|e| tracing::warn!(path = %path.display(), %e, "could not read applet package"))
        .ok()?;

    let desc: AppletDescriptor = toml::from_str(&content)
        .map_err(|e| tracing::warn!(path = %path.display(), %e, "could not parse applet package"))
        .ok()?;

    if desc.id.is_empty() {
        tracing::warn!(path = %path.display(), "applet package has empty id, skipping");
        return None;
    }

    let (extends, settings) = match desc.kind.as_str() {
        "exec" => match desc.exec {
            Some(s) => (AppletType::Exec, s),
            None => {
                tracing::warn!(path = %path.display(), "exec applet package missing [exec] section, skipping");
                return None;
            }
        },
        "command" => match desc.command {
            Some(s) => (AppletType::Command, s),
            None => {
                tracing::warn!(path = %path.display(), "command applet package missing [command] section, skipping");
                return None;
            }
        },
        other => {
            tracing::warn!(path = %path.display(), kind = other, "unknown applet type, skipping");
            return None;
        }
    };

    Some((
        desc.id,
        AppletConfig {
            extends: Some(extends),
            settings,
        },
    ))
}

fn parse_dev_package(path: &Path) -> Option<AppletConfig> {
    let content = fs::read_to_string(path)
        .map_err(
            |e| tracing::warn!(path = %path.display(), %e, "could not read dev applet package"),
        )
        .ok()?;
    let desc: AppletDescriptor = toml::from_str(&content)
        .map_err(
            |e| tracing::warn!(path = %path.display(), %e, "could not parse dev applet package"),
        )
        .ok()?;
    if desc.id.is_empty() {
        tracing::warn!(path = %path.display(), "dev applet package has empty id, skipping");
        return None;
    }
    match desc.kind.as_str() {
        "exec" => match desc.exec {
            Some(settings) => Some(AppletConfig {
                extends: Some(AppletType::Exec),
                settings,
            }),
            None => {
                tracing::warn!(path = %path.display(), "dev exec applet package missing [exec] section, skipping");
                None
            }
        },
        other => {
            tracing::warn!(path = %path.display(), kind = other, "dev applet has unknown type, skipping");
            None
        }
    }
}

pub fn merge_applet_configs(
    discovered: &HashMap<String, AppletConfig>,
    explicit: &HashMap<String, AppletConfig>,
) -> HashMap<String, AppletConfig> {
    let mut merged = discovered.clone();
    merged.extend(explicit.iter().map(|(k, v)| (k.clone(), v.clone())));
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(name: &str) -> Self {
            let suffix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("glimpse-applet-discovery-{name}-{suffix}"));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn write(&self, relative: &str, content: &str) -> PathBuf {
            let path = self.path.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, content).unwrap();
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    const EXEC_PACKAGE: &str = r#"
id   = "my-applet"
type = "exec"

[exec]
command = ["/usr/bin/my-applet"]
"#;

    const COMMAND_PACKAGE: &str = r#"
id   = "my-applet"
type = "command"

[command]
icon       = "camera-photo"
left_click = ["gnome-screenshot"]
"#;

    #[test]
    fn exec_package_discovered_as_normal() {
        let dir = TempDir::new("exec-pkg");
        dir.write("my-applet.toml", EXEC_PACKAGE);
        let scanner = AppletDirectoryScanner::new(dir.path.clone(), PathBuf::new());
        let found = scanner.scan();
        assert!(found.normal.contains_key("my-applet"));
        assert_eq!(found.normal["my-applet"].extends, Some(AppletType::Exec));
        assert!(found.dev.is_empty());
    }

    #[test]
    fn command_package_discovered_as_normal() {
        let dir = TempDir::new("cmd-pkg");
        dir.write("my-applet.toml", COMMAND_PACKAGE);
        let scanner = AppletDirectoryScanner::new(dir.path.clone(), PathBuf::new());
        let found = scanner.scan();
        assert!(found.normal.contains_key("my-applet"));
        assert_eq!(found.normal["my-applet"].extends, Some(AppletType::Command));
        assert!(found.dev.is_empty());
    }

    #[test]
    fn directory_package_applet_toml_is_discovered_as_normal() {
        let dir = TempDir::new("dir-pkg");
        dir.write("my-applet/applet.toml", COMMAND_PACKAGE);
        let scanner = AppletDirectoryScanner::new(dir.path.clone(), PathBuf::new());

        let found = scanner.scan();

        assert!(found.normal.contains_key("my-applet"));
        assert_eq!(found.normal["my-applet"].extends, Some(AppletType::Command));
        assert!(found.dev.is_empty());
    }

    #[test]
    fn packaged_terminal_applet_is_discovered_as_command() {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let scanner =
            AppletDirectoryScanner::new(repo_root.join("packaged-applets"), PathBuf::new());

        let found = scanner.scan();
        let applet = found.normal.get("me.aresa.glimpse.terminal").unwrap();

        assert_eq!(applet.extends, Some(AppletType::Command));
        assert_eq!(
            applet.settings["on_click"][0].as_str(),
            Some("/usr/share/glimpse/applets/me.aresa.glimpse.terminal/open-terminal")
        );
    }

    #[test]
    fn id_is_used_as_map_key_not_filename() {
        let dir = TempDir::new("id-key");
        dir.write(
            "filename.toml",
            r#"id = "my-id"
type = "exec"

[exec]
command = ["/bin/true"]
"#,
        );
        let scanner = AppletDirectoryScanner::new(dir.path.clone(), PathBuf::new());
        let found = scanner.scan();
        assert!(
            found.normal.contains_key("my-id"),
            "id field should be the map key"
        );
        assert!(!found.normal.contains_key("filename"));
    }

    #[test]
    fn missing_exec_section_is_skipped() {
        let dir = TempDir::new("missing-exec");
        dir.write(
            "my-applet.toml",
            r#"id = "my-applet"
type = "exec"
"#,
        );
        let scanner = AppletDirectoryScanner::new(dir.path.clone(), PathBuf::new());
        let found = scanner.scan();
        assert!(found.normal.is_empty());
    }

    #[test]
    fn unknown_type_is_skipped() {
        let dir = TempDir::new("unknown-type");
        dir.write(
            "my-applet.toml",
            r#"id = "my-applet"
type = "widget"

[widget]
foo = "bar"
"#,
        );
        let scanner = AppletDirectoryScanner::new(dir.path.clone(), PathBuf::new());
        let found = scanner.scan();
        assert!(found.normal.is_empty());
    }

    #[test]
    fn dev_toml_goes_to_dev_map() {
        let dir = TempDir::new("dev");
        dir.write("my-applet.dev.toml", EXEC_PACKAGE);
        let scanner = AppletDirectoryScanner::new(dir.path.clone(), PathBuf::new());
        let found = scanner.scan();
        assert!(found.normal.is_empty());
        assert!(found.dev.contains_key("my-applet"));
    }

    #[test]
    fn user_dir_overrides_system_dir_by_id() {
        let system = TempDir::new("system");
        let user = TempDir::new("user");
        system.write(
            "shared.toml",
            r#"id = "shared"
type = "exec"

[exec]
command = ["/system/binary"]
"#,
        );
        user.write(
            "shared.toml",
            r#"id = "shared"
type = "exec"

[exec]
command = ["/user/binary"]
"#,
        );
        let scanner = AppletDirectoryScanner::new(system.path.clone(), user.path.clone());
        let found = scanner.scan();
        let settings = &found.normal["shared"].settings;
        let cmd = settings.get("command").unwrap();
        assert_eq!(cmd.as_array().unwrap()[0].as_str().unwrap(), "/user/binary");
    }

    #[test]
    fn missing_dirs_produce_empty_results() {
        let scanner = AppletDirectoryScanner::new(
            PathBuf::from("/nonexistent/system"),
            PathBuf::from("/nonexistent/user"),
        );
        let found = scanner.scan();
        assert!(found.normal.is_empty());
        assert!(found.dev.is_empty());
    }

    #[test]
    fn merge_applet_configs_explicit_wins() {
        let mut discovered = HashMap::new();
        discovered.insert(
            "shared".to_string(),
            AppletConfig {
                extends: Some(AppletType::Exec),
                settings: toml::Value::Table(toml::map::Map::new()),
            },
        );
        discovered.insert(
            "only-discovered".to_string(),
            AppletConfig {
                extends: Some(AppletType::Exec),
                settings: toml::Value::Table(toml::map::Map::new()),
            },
        );

        let mut explicit = HashMap::new();
        explicit.insert(
            "shared".to_string(),
            AppletConfig {
                extends: Some(AppletType::Battery),
                settings: toml::Value::Table(toml::map::Map::new()),
            },
        );

        let merged = merge_applet_configs(&discovered, &explicit);
        assert_eq!(merged["shared"].extends, Some(AppletType::Battery));
        assert!(merged.contains_key("only-discovered"));
    }

    #[test]
    fn scan_sources_tags_provenance_and_user_shadows_system() {
        let system = TempDir::new("src-sys");
        system.write(
            "sys-applet.toml",
            "id = \"sys-applet\"\ntype = \"command\"\n[command]\nleft_click = [\"x\"]\n",
        );
        system.write(
            "shared.toml",
            "id = \"shared\"\ntype = \"command\"\n[command]\nleft_click = [\"x\"]\n",
        );
        let user = TempDir::new("src-usr");
        user.write(
            "usr-applet.toml",
            "id = \"usr-applet\"\ntype = \"exec\"\n[exec]\ncommand = [\"/bin/x\"]\n",
        );
        user.write(
            "shared.toml",
            "id = \"shared\"\ntype = \"exec\"\n[exec]\ncommand = [\"/bin/x\"]\n",
        );
        user.write(
            "beta.dev.toml",
            "id = \"beta\"\ntype = \"command\"\n[command]\nleft_click = [\"x\"]\n",
        );

        let scanner = AppletDirectoryScanner::new(system.path.clone(), user.path.clone());
        let listed = scanner.scan_sources();
        let by_id = |id: &str| listed.iter().find(|a| a.id == id).cloned();

        assert_eq!(by_id("sys-applet").unwrap().source, AppletSource::System);
        assert_eq!(by_id("usr-applet").unwrap().source, AppletSource::User);
        let shared = by_id("shared").unwrap();
        assert_eq!(shared.source, AppletSource::User, "user must shadow system");
        assert_eq!(shared.kind, "exec");
        assert_eq!(listed.iter().filter(|a| a.id == "shared").count(), 1);
        assert_eq!(by_id("beta").unwrap().source, AppletSource::Dev);
        // Sorted by id.
        let ids: Vec<&str> = listed.iter().map(|a| a.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }
}
