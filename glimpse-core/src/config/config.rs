use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use tokio::sync::mpsc;

use crate::{
    AppletConfig, BackdropConfig, CalendarConfig, ConfigFileDiscovery, IdleConfig, KeyboardConfig,
    LocationConfig, LockConfig, MonitorsConfig, NightLightConfig, PanelConfig,
    ResolvedWallpaperSpec, ThemeMode, ThemePack, WallpaperConfig, resolve_wallpaper_spec,
    services::theme::EffectiveThemeMode, watch_config_file,
};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub location: LocationConfig,
    pub theme: String,
    pub theme_mode: ThemeMode,
    pub panels: Vec<PanelConfig>,
    pub applets: HashMap<String, AppletConfig>,
    #[serde(default)]
    pub night_light: NightLightConfig,
    #[serde(default)]
    pub idle: IdleConfig,
    #[serde(default)]
    pub keyboard: KeyboardConfig,
    #[serde(default)]
    pub wallpaper: WallpaperConfig,
    #[serde(default)]
    pub backdrop: BackdropConfig,
    #[serde(default)]
    pub lock: LockConfig,
    #[serde(default)]
    pub monitors: MonitorsConfig,
    #[serde(default)]
    pub calendar: CalendarConfig,
}

impl Config {
    pub fn autodetect() -> Self {
        Self::load()
    }

    pub fn load() -> Self {
        let path = Self::detect_config_file();
        if path.exists() && path.is_file() {
            return Self::load_from_file(&path);
        }
        Self::default()
    }

    pub fn from_toml_str(content: &str) -> Result<Self, toml::de::Error> {
        let mut config = toml::from_str::<Self>(content)?;
        config.expand_panel_placeholders();
        Ok(config)
    }

    pub fn detect_config_file() -> PathBuf {
        ConfigDiscovery::from_process().detect_config_file()
    }

    pub fn config_dir() -> PathBuf {
        ConfigDiscovery::from_process().config_dir()
    }

    pub fn config_file() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn themes_dir() -> PathBuf {
        Self::config_dir().join("themes")
    }

    pub fn theme_pack(&self) -> ThemePack {
        ThemePack::resolve(&self.theme)
    }

    /// User-only override CSS layered on top of the active pack's `panel.css`.
    /// Returns `None` when the file is absent. The lock screen's equivalent
    /// override path is configured via `LockConfig::css_path` (default
    /// `themes/lock.css`), kept separate so it can be overridden explicitly.
    pub fn override_panel_css() -> Option<PathBuf> {
        existing_file(Self::themes_dir().join("panel.css"))
    }

    pub fn resolve_wallpaper(&self, mode: EffectiveThemeMode) -> ResolvedWallpaperSpec {
        resolve_wallpaper_spec(&self.wallpaper, &self.backdrop, &self.theme_pack(), mode)
    }

    pub fn load_from_file(path: &Path) -> Self {
        tracing::info!("loading configuration from {}", path.display());
        match Self::try_load_from_file(path) {
            Ok(config) => config,
            Err(err) => {
                tracing::error!("failed to load configuration: {}", err);
                Self::default()
            }
        }
    }

    pub fn try_load_from_file(path: &Path) -> Result<Self, String> {
        let (value, _) = load_toml_with_includes(path)?;
        let mut config = value
            .try_into::<Self>()
            .map_err(|err| format!("failed to parse config: {err}"))?;
        config.expand_panel_placeholders();
        Ok(config)
    }

    pub fn watch_files_for(path: &Path) -> Vec<PathBuf> {
        load_toml_with_includes(path)
            .map(|(_, files)| files)
            .unwrap_or_else(|err| {
                tracing::debug!(
                    config_file = %path.display(),
                    "failed to resolve config include watch files: {err}"
                );
                vec![path.canonicalize().unwrap_or_else(|_| path.to_path_buf())]
            })
    }

    fn expand_panel_placeholders(&mut self) {
        let defaults = PanelConfig::default();
        for panel in &mut self.panels {
            expand_panel_section("left", &mut panel.left, &defaults.left);
            expand_panel_section("center", &mut panel.center, &defaults.center);
            expand_panel_section("right", &mut panel.right, &defaults.right);
        }
    }
}

fn existing_file(path: PathBuf) -> Option<PathBuf> {
    fs::metadata(&path)
        .ok()
        .filter(|m| m.is_file())
        .map(|_| path)
}

fn load_toml_with_includes(path: &Path) -> Result<(toml::Value, Vec<PathBuf>), String> {
    let mut files = Vec::new();
    let value = load_toml_with_includes_inner(path, &mut Vec::new(), &mut files)?;
    Ok((value, files))
}

fn load_toml_with_includes_inner(
    path: &Path,
    include_stack: &mut Vec<PathBuf>,
    files: &mut Vec<PathBuf>,
) -> Result<toml::Value, String> {
    let path = fs::canonicalize(path).map_err(|err| {
        format!(
            "failed to resolve configuration file {}: {err}",
            path.display()
        )
    })?;
    if include_stack.iter().any(|visited| visited == &path) {
        return Err(format!(
            "include cycle detected: {} -> {}",
            include_stack
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(" -> "),
            path.display()
        ));
    }

    include_stack.push(path.clone());
    if !files.iter().any(|file| file == &path) {
        files.push(path.clone());
    }
    let result = load_toml_file_with_includes(&path, include_stack, files);
    include_stack.pop();
    result
}

fn load_toml_file_with_includes(
    path: &Path,
    include_stack: &mut Vec<PathBuf>,
    files: &mut Vec<PathBuf>,
) -> Result<toml::Value, String> {
    tracing::debug!(
        config_file = %path.display(),
        "loading Glimpse config TOML"
    );
    let content = fs::read_to_string(path).map_err(|err| {
        format!(
            "failed to read configuration file {}: {err}",
            path.display()
        )
    })?;
    let mut value = content
        .parse::<toml::Value>()
        .map_err(|err| format!("failed to parse config {}: {err}", path.display()))?;

    let include = match value.as_table_mut() {
        Some(table) => table.remove("include"),
        None => None,
    };

    let mut merged = toml::Value::Table(toml::map::Map::new());
    for include_path in parse_include_paths(include.as_ref(), path)? {
        tracing::debug!(
            config_file = %path.display(),
            include_file = %include_path.display(),
            "loading included Glimpse config"
        );
        let include_value = load_toml_with_includes_inner(&include_path, include_stack, files)?;
        tracing::debug!(
            config_file = %path.display(),
            include_file = %include_path.display(),
            "merging included Glimpse config"
        );
        merge_toml_values(&mut merged, include_value);
    }
    tracing::debug!(
        config_file = %path.display(),
        "merging Glimpse config overrides"
    );
    merge_toml_values(&mut merged, value);
    Ok(merged)
}

fn parse_include_paths(
    include: Option<&toml::Value>,
    source: &Path,
) -> Result<Vec<PathBuf>, String> {
    let Some(include) = include else {
        return Ok(Vec::new());
    };
    let Some(paths) = include.as_array() else {
        return Err(format!(
            "invalid include in {}: expected an array of strings",
            source.display()
        ));
    };

    let base_dir = source.parent().unwrap_or_else(|| Path::new("."));
    paths
        .iter()
        .map(|value| {
            let Some(path) = value.as_str() else {
                return Err(format!(
                    "invalid include in {}: expected an array of strings",
                    source.display()
                ));
            };
            let path = PathBuf::from(path);
            if path.is_absolute() {
                Ok(path)
            } else {
                Ok(base_dir.join(path))
            }
        })
        .collect()
}

fn merge_toml_values(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base), toml::Value::Table(overlay)) => {
            for (key, value) in overlay {
                match base.get_mut(&key) {
                    Some(existing) => merge_toml_values(existing, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

fn expand_panel_section(section: &'static str, applets: &mut Vec<String>, defaults: &[String]) {
    let mut expanded = Vec::with_capacity(applets.len() + defaults.len());
    let mut inserted_defaults = false;
    for applet in applets.drain(..) {
        if applet == crate::DEFAULT_PANEL_APPLETS_PLACEHOLDER {
            if inserted_defaults {
                tracing::warn!(
                    section,
                    placeholder = crate::DEFAULT_PANEL_APPLETS_PLACEHOLDER,
                    "extra panel applet placeholder ignored"
                );
                continue;
            }
            expanded.extend(defaults.iter().cloned());
            inserted_defaults = true;
            continue;
        }
        expanded.push(applet);
    }
    *applets = expanded;
}

impl Default for Config {
    fn default() -> Self {
        Self {
            location: LocationConfig::default(),
            theme: "adwaita".into(),
            theme_mode: ThemeMode::Auto,
            panels: vec![PanelConfig::default()],
            applets: HashMap::new(),
            night_light: NightLightConfig::default(),
            idle: IdleConfig::default(),
            keyboard: KeyboardConfig::default(),
            wallpaper: WallpaperConfig::default(),
            backdrop: BackdropConfig::default(),
            lock: LockConfig::default(),
            monitors: MonitorsConfig::default(),
            calendar: CalendarConfig::default(),
        }
    }
}

#[cfg(test)]
mod night_light_config_tests {
    use crate::{Config, NightLightSchedule};

    #[test]
    fn default_config_includes_automatic_night_light() {
        let config = Config::default();

        assert_eq!(config.night_light.temperature, 4200);
        assert_eq!(config.night_light.schedule, NightLightSchedule::Automatic);
        assert_eq!(config.night_light.transition_minutes, 15);
    }

    #[test]
    fn config_parses_night_light_block() {
        let config = Config::from_toml_str(
            r#"
[night_light]
temperature = 4200
schedule = "schedule"
start_time = "18:00"
end_time = "06:30"
transition_minutes = 75
"#,
        )
        .expect("config should parse");

        assert_eq!(config.night_light.temperature, 4200);
        assert_eq!(config.night_light.schedule, NightLightSchedule::Schedule);
        assert_eq!(config.night_light.start_time.as_deref(), Some("18:00"));
        assert_eq!(config.night_light.end_time.as_deref(), Some("06:30"));
        assert_eq!(config.night_light.transition_minutes, 75);
    }
}

#[cfg(test)]
mod idle_config_tests {
    use crate::Config;

    #[test]
    fn default_config_includes_idle_lock_listeners() {
        let config = Config::default();

        assert!(config.idle.enabled);
        assert!(config.idle.respect_inhibitors);
        let ac_steps: Vec<_> = config
            .idle
            .profiles
            .ac
            .listeners
            .iter()
            .map(|l| (l.timeout, l.on_idle.as_str(), l.on_resume.as_str()))
            .collect();
        assert_eq!(
            ac_steps,
            vec![
                (
                    600,
                    "/usr/share/glimpse/scripts/monitors off",
                    "/usr/share/glimpse/scripts/monitors on",
                ),
                (900, "loginctl lock-session", ""),
                (3600, "systemctl suspend", ""),
            ]
        );

        let battery_steps: Vec<_> = config
            .idle
            .profiles
            .battery
            .listeners
            .iter()
            .map(|l| (l.timeout, l.on_idle.as_str(), l.on_resume.as_str()))
            .collect();
        assert_eq!(
            battery_steps,
            vec![
                (
                    300,
                    "/usr/share/glimpse/scripts/monitors off",
                    "/usr/share/glimpse/scripts/monitors on",
                ),
                (900, "loginctl lock-session", ""),
                (1800, "systemctl suspend", ""),
            ]
        );
    }

    #[test]
    fn config_parses_idle_block() {
        let config = Config::from_toml_str(
            r#"
[idle]
enabled = true
respect_inhibitors = false

[idle.profiles.ac]
listeners = [
  { timeout = 60, on_idle = "one", on_resume = "two", respect_inhibitors = true },
]

[idle.profiles.battery]
listeners = [
  { timeout = 30, on_idle = "three" },
]
"#,
        )
        .expect("config should parse");

        assert!(!config.idle.respect_inhibitors);
        assert_eq!(config.idle.profiles.ac.listeners[0].timeout, 60);
        assert_eq!(
            config.idle.profiles.ac.listeners[0].respect_inhibitors,
            Some(true)
        );
    }
}

#[cfg(test)]
mod calendar_config_tests {
    use crate::{CalendarSourceType, Config};

    #[test]
    fn default_config_includes_calendar_defaults_without_sources() {
        let config = Config::default();

        assert_eq!(config.calendar.poll_interval, 600);
        assert!(config.calendar.sources.is_empty());
    }

    #[test]
    fn calendar_config_parses_sources_and_numeric_intervals() {
        let config = Config::from_toml_str(
            r##"
[calendar]
poll_interval = 900

[[calendar.sources]]
id = "google-personal"
type = "ical"
name = "Google Personal"
uri = "https://calendar.google.com/calendar/ical/example/basic.ics"
poll_interval = 300
color = "#4285f4"

[[calendar.sources]]
id = "local-calendars"
type = "directory"
name = "Local Calendars"
uri = "file:///home/alex/.config/glimpse/calendars"
"##,
        )
        .expect("calendar config should parse");

        assert_eq!(config.calendar.poll_interval, 900);
        assert_eq!(config.calendar.sources.len(), 2);
        assert_eq!(config.calendar.sources[0].id, "google-personal");
        assert_eq!(
            config.calendar.sources[0].source_type,
            CalendarSourceType::Ical
        );
        assert_eq!(
            config.calendar.sources[0].name.as_deref(),
            Some("Google Personal")
        );
        assert_eq!(
            config.calendar.sources[0].uri,
            "https://calendar.google.com/calendar/ical/example/basic.ics"
        );
        assert_eq!(config.calendar.sources[0].poll_interval, Some(300));
        assert_eq!(config.calendar.sources[0].color.as_deref(), Some("#4285f4"));
        assert_eq!(
            config.calendar.sources[1].source_type,
            CalendarSourceType::Directory
        );
        assert_eq!(config.calendar.sources[1].poll_interval, None);
    }
}

#[cfg(test)]
mod panel_config_tests {
    use crate::{Config, PanelConfig};

    #[test]
    fn config_expands_panel_default_placeholder() {
        let config = Config::from_toml_str(
            r#"
[[panels]]
left = ["custom", "..."]
center = ["..."]
right = ["...", "custom"]
"#,
        )
        .unwrap();

        assert_eq!(
            config.panels[0].left,
            vec!["custom", "pager", "mpris", "__dev__"]
        );
        assert_eq!(
            config.panels[0].center,
            vec!["clock", "weather", "notifications", "privacy"]
        );
        assert_eq!(
            config.panels[0].right,
            vec![
                "__dynamic__",
                "next_event",
                "tray",
                "removable",
                "clipboard",
                "keyboard",
                "printing",
                "bluetooth",
                "network",
                "display",
                "audio",
                "idle",
                "battery",
                "session",
                "custom"
            ]
        );
    }

    #[test]
    fn config_keeps_panel_section_without_placeholder_as_full_override() {
        let config = Config::from_toml_str(
            r#"
[[panels]]
right = ["custom"]
"#,
        )
        .unwrap();

        assert_eq!(config.panels[0].right, vec!["custom"]);
    }

    #[test]
    fn config_expands_only_first_panel_default_placeholder() {
        let config = Config::from_toml_str(
            r#"
[[panels]]
center = ["before", "...", "middle", "...", "after"]
"#,
        )
        .unwrap();

        assert_eq!(
            config.panels[0].center,
            vec![
                "before",
                "clock",
                "weather",
                "notifications",
                "privacy",
                "middle",
                "after"
            ]
        );
    }

    #[test]
    fn panel_config_deserialize_keeps_placeholder_until_config_normalization() {
        let panel = toml::from_str::<PanelConfig>(
            r#"
left = ["custom", "..."]
"#,
        )
        .unwrap();

        assert_eq!(panel.left, vec!["custom", "..."]);
    }
}

#[cfg(test)]
mod include_config_tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::Config;

    #[test]
    fn config_include_loads_base_file_and_main_config_wins() {
        let dir = TestConfigDir::new("config-include-base");
        dir.write(
            "base.toml",
            r#"
theme = "adwaita"

[applets.clock]
format = "%H:%M"
"#,
        );
        dir.write(
            "config.toml",
            r#"
include = ["base.toml"]
theme = "rosepine"

[applets.clock]
format = "%a %H:%M"
"#,
        );

        let config = Config::try_load_from_file(&dir.file("config.toml")).unwrap();

        assert_eq!(config.theme, "rosepine");
        assert_eq!(
            config.applets["clock"].settings["format"].as_str(),
            Some("%a %H:%M")
        );
    }

    #[test]
    fn config_include_recursively_merges_nested_tables() {
        let dir = TestConfigDir::new("config-include-tables");
        dir.write(
            "base.toml",
            r#"
[applets.clock]
format = "%H:%M"
tooltip = "Local time"
"#,
        );
        dir.write(
            "config.toml",
            r#"
include = ["base.toml"]

[applets.clock]
format = "%a %H:%M"
"#,
        );

        let config = Config::try_load_from_file(&dir.file("config.toml")).unwrap();

        assert_eq!(
            config.applets["clock"].settings["format"].as_str(),
            Some("%a %H:%M")
        );
        assert_eq!(
            config.applets["clock"].settings["tooltip"].as_str(),
            Some("Local time")
        );
    }

    #[test]
    fn config_include_replaces_arrays_instead_of_appending() {
        let dir = TestConfigDir::new("config-include-arrays");
        dir.write(
            "base.toml",
            r#"
[[panels]]
left = ["workspace"]
right = ["clock"]
"#,
        );
        dir.write(
            "config.toml",
            r#"
include = ["base.toml"]

[[panels]]
left = ["workspace", "..."]
right = ["tray"]
"#,
        );

        let config = Config::try_load_from_file(&dir.file("config.toml")).unwrap();

        assert_eq!(
            config.panels[0].left,
            vec!["workspace", "pager", "mpris", "__dev__"]
        );
        assert_eq!(config.panels[0].right, vec!["tray"]);
        assert_eq!(config.panels.len(), 1);
    }

    #[test]
    fn config_include_paths_are_relative_to_declaring_file() {
        let dir = TestConfigDir::new("config-include-relative");
        dir.write(
            "profiles/base.toml",
            r#"
theme = "rosepine"
include = ["applets.toml"]
"#,
        );
        dir.write(
            "profiles/applets.toml",
            r#"
[applets.clock]
format = "%H:%M"
"#,
        );
        dir.write(
            "config.toml",
            r#"
include = ["profiles/base.toml"]
"#,
        );

        let config = Config::try_load_from_file(&dir.file("config.toml")).unwrap();

        assert_eq!(config.theme, "rosepine");
        assert_eq!(
            config.applets["clock"].settings["format"].as_str(),
            Some("%H:%M")
        );
    }

    #[test]
    fn config_include_cycles_return_error() {
        let dir = TestConfigDir::new("config-include-cycle");
        dir.write(
            "config.toml",
            r#"
include = ["base.toml"]
"#,
        );
        dir.write(
            "base.toml",
            r#"
include = ["config.toml"]
"#,
        );

        let err = Config::try_load_from_file(&dir.file("config.toml")).unwrap_err();

        assert!(err.contains("include cycle"), "{err}");
    }

    struct TestConfigDir {
        root: PathBuf,
    }

    impl TestConfigDir {
        fn new(name: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("{name}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn file(&self, relative: &str) -> PathBuf {
            self.root.join(relative)
        }

        fn write(&self, relative: &str, content: &str) {
            let path = self.file(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, content).unwrap();
        }
    }

    impl Drop for TestConfigDir {
        fn drop(&mut self) {
            let _ = remove_dir_all_if_exists(&self.root);
        }
    }

    fn remove_dir_all_if_exists(path: &Path) -> std::io::Result<()> {
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ConfigDiscovery {
    inner: ConfigFileDiscovery,
}

impl ConfigDiscovery {
    pub fn new(
        env: HashMap<String, String>,
        cwd: PathBuf,
        xdg_config_home: Option<PathBuf>,
        home: Option<PathBuf>,
    ) -> Self {
        Self {
            inner: ConfigFileDiscovery::new(
                env,
                cwd,
                xdg_config_home,
                home,
                "GLIMPSE_CONFIG",
                "config.toml",
            ),
        }
    }

    pub fn from_process() -> Self {
        Self {
            inner: ConfigFileDiscovery::from_process("GLIMPSE_CONFIG", "config.toml"),
        }
    }

    pub fn detect_config_file(&self) -> PathBuf {
        self.inner.detect_config_file()
    }

    pub fn config_dir(&self) -> PathBuf {
        self.inner.config_dir()
    }

    pub fn config_file(&self) -> PathBuf {
        self.inner.config_file()
    }
}

pub enum ConfigEvent {
    Changed(Config),
}

pub async fn watch_for_config_changes(sender: mpsc::Sender<ConfigEvent>) {
    watch_config_file(
        Config::detect_config_file(),
        sender,
        "shared",
        Config::watch_files_for,
        |path| ConfigEvent::Changed(Config::load_from_file(path)),
    )
    .await;
}
