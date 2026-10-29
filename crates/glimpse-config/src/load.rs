use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::SystemTime;

use tempfile::NamedTempFile;

use crate::error::{ConfigError, MAX_FILE_BYTES};
use crate::schema::{Applet, Config, PANEL_TABLES};

pub(crate) const SYSTEM_DIR: &str = "/etc/glimpse";
pub(crate) const CONFIG_FILE: &str = "config.toml";
pub(crate) const PANEL_FILE: &str = "panel.toml";
pub(crate) const PANEL_DROPINS: &str = "panel.d";
const FOLDER_NAME: &str = "glimpse";
const WRITE_ATTEMPTS: usize = 3;
const LOCALE: &str = "language, times and units follow LANG and LC_*";
const SHOWN: &str = "the lock screen always shows it";
const RETIRED: [(&[&str], &str); 8] = [
    (&["regional"], LOCALE),
    (&["lock", "clock", "time-format"], LOCALE),
    (
        &["lock", "status"],
        "the lock screen always shows the battery and keyboard layout",
    ),
    (&["lock", "clock", "enabled"], SHOWN),
    (&["lock", "notifications"], SHOWN),
    (
        &["lock", "pam-service"],
        "the lock screen always checks passwords through /etc/pam.d/glimpse-lock",
    ),
    (&["lock", "media"], SHOWN),
    (
        &["lock", "session"],
        "the lock screen always offers suspend, restart and power off",
    ),
];
const CONFIG_STUB: &str = concat!(
    "#:schema ",
    env!("GLIMPSE_DATA_DIR_DEFAULT"),
    "/config.schema.json\n# https://alex-oleshkevich.github.io/glimpse/config/\n"
);
const PANEL_STUB: &str = concat!(
    "#:schema ",
    env!("GLIMPSE_DATA_DIR_DEFAULT"),
    "/panel.schema.json\n# https://alex-oleshkevich.github.io/glimpse/config/panels\n"
);

static CONFIG_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Documents {
    Config,
    ConfigAndPanel,
}

impl Documents {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Config => "config",
            Self::ConfigAndPanel => "panel",
        }
    }

    pub(crate) fn renders(self, table: &str) -> bool {
        PANEL_TABLES.contains(&table) == (self == Self::ConfigAndPanel)
    }
}

/// `$PREFIX/share/glimpse`, `/usr/share/glimpse` by default — where the package installs files a
/// user reads but does not edit.
pub const DATA_DIR: &str = env!("GLIMPSE_DATA_DIR_DEFAULT");

/// `~/.config/glimpse`, or `None` on a platform that names no config directory.
fn user_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join(FOLDER_NAME))
}

pub fn init(config_dir: Option<PathBuf>) {
    let _ = CONFIG_DIR.set(config_dir.filter(|dir| !dir.as_os_str().is_empty()));
}

pub fn config_dir() -> Option<&'static Path> {
    stored(&CONFIG_DIR)
}

fn stored(cell: &OnceLock<Option<PathBuf>>) -> Option<&Path> {
    cell.get().and_then(Option::as_deref)
}

pub(crate) fn roots() -> Vec<PathBuf> {
    roots_in(config_dir(), user_dir().as_deref(), Path::new(SYSTEM_DIR))
}

fn roots_in(config_dir: Option<&Path>, user: Option<&Path>, system: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for dir in [config_dir, user, Some(system)].into_iter().flatten() {
        push(&mut roots, dir.to_path_buf());
    }
    roots
}

pub fn config_files(documents: Documents) -> Result<Vec<PathBuf>, ConfigError> {
    files_in(&roots(), documents)
}

fn files_in(roots: &[PathBuf], documents: Documents) -> Result<Vec<PathBuf>, ConfigError> {
    let mut files: Vec<PathBuf> = roots
        .iter()
        .rev()
        .map(|root| root.join(CONFIG_FILE))
        .collect();
    if documents == Documents::ConfigAndPanel {
        for root in roots.iter().rev() {
            files.push(root.join(PANEL_FILE));
            files.extend(dropins(&root.join(PANEL_DROPINS))?);
        }
    }
    Ok(files)
}

pub fn find(relative: &Path) -> Option<PathBuf> {
    find_in(&roots(), Path::new(DATA_DIR), relative)
}

fn find_in(roots: &[PathBuf], data: &Path, relative: &Path) -> Option<PathBuf> {
    find_in_roots(roots, relative)
        .or_else(|| Some(data.join(relative)).filter(|path| path.exists()))
}

pub(crate) fn find_in_roots(roots: &[PathBuf], relative: &Path) -> Option<PathBuf> {
    roots
        .iter()
        .map(|root| root.join(relative))
        .find(|path| path.exists())
}

pub fn writable_file(name: &str) -> Option<PathBuf> {
    writable_in(config_dir(), user_dir().as_deref(), name)
}

fn writable_in(config_dir: Option<&Path>, user: Option<&Path>, name: &str) -> Option<PathBuf> {
    config_dir
        .or(user)
        .map(|dir| dir.join(format!("{name}.toml")))
}

pub fn state_dir() -> Option<PathBuf> {
    dirs::state_dir().map(|dir| dir.join(FOLDER_NAME))
}

pub fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|dir| dir.join(FOLDER_NAME))
}

pub fn applets_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join(FOLDER_NAME).join("applets"))
}

pub fn runtime_dir() -> Option<PathBuf> {
    runtime_dir_in(std::env::var_os("XDG_RUNTIME_DIR"))
}

fn runtime_dir_in(runtime: Option<OsString>) -> Option<PathBuf> {
    runtime
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(FOLDER_NAME))
}

pub fn write_table(
    path: &Path,
    table: &str,
    values: &toml_edit::Table,
    remove: &[&str],
) -> Result<(), ConfigError> {
    write_table_with(path, table, values, remove, &mut || {})
}

fn write_table_with(
    path: &Path,
    table: &str,
    values: &toml_edit::Table,
    remove: &[&str],
    before_rename: &mut dyn FnMut(),
) -> Result<(), ConfigError> {
    let target = match fs::canonicalize(path) {
        Ok(target) => target,
        Err(error) if error.kind() == io::ErrorKind::NotFound && !path.is_symlink() => {
            path.to_path_buf()
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(ConfigError::unwritable(
                path,
                io::Error::other("the symlink points at nothing"),
            ));
        }
        Err(error) => return Err(ConfigError::unwritable(path, error)),
    };
    let dir = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(|error| ConfigError::unwritable(dir, error))?;

    for _ in 0..WRITE_ATTEMPTS {
        let read_at = stamp(&target)?;
        let text = match read_at {
            Some(_) => read(&target)?,
            None => String::new(),
        };
        let mut document = text.parse::<toml_edit::DocumentMut>().map_err(|error| {
            ConfigError::parse_at(&target, &text, error.message(), error.span())
        })?;
        set_table(&mut document, table, values, remove)?;
        let mut output = document.to_string();
        if text.contains("\r\n") {
            output = output.replace("\r\n", "\n").replace('\n', "\r\n");
        }

        let temporary = temporary_beside(&target, dir, &output)
            .map_err(|error| ConfigError::unwritable(&target, error))?;
        before_rename();
        if stamp(&target)? != read_at {
            continue;
        }
        return temporary
            .persist(&target)
            .map(drop)
            .map_err(|error| ConfigError::unwritable(&target, error.error));
    }
    Err(ConfigError::unwritable(
        &target,
        io::Error::other("the file kept changing while it was being written"),
    ))
}

fn temporary_beside(target: &Path, dir: &Path, text: &str) -> io::Result<NamedTempFile> {
    let mut file = NamedTempFile::new_in(dir)?;
    let permissions = match fs::metadata(target) {
        Ok(metadata) if metadata.permissions().readonly() => {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok(metadata) => metadata.permissions(),
        Err(_) => fs::Permissions::from_mode(0o644),
    };
    file.as_file().set_permissions(permissions)?;
    file.write_all(text.as_bytes())?;
    file.as_file().sync_all()?;
    Ok(file)
}

fn stamp(path: &Path) -> Result<Option<(SystemTime, u64, u64, u64)>, ConfigError> {
    match fs::metadata(path) {
        Ok(metadata) => metadata
            .modified()
            .map(|modified| Some((modified, metadata.len(), metadata.dev(), metadata.ino())))
            .map_err(|error| ConfigError::unreadable(path, error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ConfigError::unreadable(path, error)),
    }
}

fn set_table(
    document: &mut toml_edit::DocumentMut,
    table: &str,
    values: &toml_edit::Table,
    remove: &[&str],
) -> Result<(), ConfigError> {
    let Some(existing) = document
        .entry(table)
        .or_insert(toml_edit::table())
        .as_table_like_mut()
    else {
        return Err(ConfigError::Schema {
            message: format!("`{table}` is not a table"),
        });
    };
    if let Some((key, _)) = values.iter().find(|(_, item)| !item.is_value()) {
        return Err(ConfigError::Schema {
            message: format!("`{table}.{key}` is not a value"),
        });
    }
    for (key, item) in values.iter() {
        let mut item = item.clone();
        if let (Some(new), Some(old)) = (
            item.as_value_mut(),
            existing.get(key).and_then(toml_edit::Item::as_value),
        ) {
            *new.decor_mut() = old.decor().clone();
        }
        match existing.get_mut(key) {
            Some(slot) => *slot = item,
            None => {
                existing.insert(key, item);
            }
        }
    }
    for key in remove {
        existing.remove(key);
    }
    Ok(())
}

pub fn resolve_image(path: &Path) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    resolve_in(path, home.as_deref(), &roots(), Path::new(DATA_DIR))
}

fn resolve_in(path: &Path, home: Option<&Path>, roots: &[PathBuf], data: &Path) -> Option<PathBuf> {
    let expanded = expand_tilde(path, home);

    if expanded.is_absolute() {
        return expanded.is_file().then_some(expanded);
    }

    [expanded.clone(), Path::new("wallpapers").join(&expanded)]
        .iter()
        .find_map(|relative| {
            roots
                .iter()
                .map(|root| root.join(relative))
                .chain([data.join(relative)])
                .find(|candidate| candidate.is_file())
        })
}

fn expand_tilde(path: &Path, home: Option<&Path>) -> PathBuf {
    let Some(home) = home else {
        return path.to_path_buf();
    };
    let text = path.to_string_lossy();
    match text.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if text == "~" => home.to_path_buf(),
        None => path.to_path_buf(),
    }
}

pub(crate) fn push(dirs: &mut Vec<PathBuf>, dir: PathBuf) {
    if !dirs.contains(&dir) {
        dirs.push(dir);
    }
}

fn push_parent(dirs: &mut Vec<PathBuf>, file: &Path) {
    let parent = file
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    push(dirs, parent.to_path_buf());
}

/// A base file symlinked into a dotfile repository is edited on the other end of the link, and
/// editors write a new file and rename it over the old one. Watching only the link's own directory
/// therefore goes quiet after the first save.
pub(crate) fn push_target_parent(dirs: &mut Vec<PathBuf>, file: &Path) {
    if let Ok(target) = std::fs::canonicalize(file) {
        push_parent(dirs, &target);
    }
}

pub fn load(documents: Documents) -> Result<Config, ConfigError> {
    load_files(&config_files(documents)?)
}

pub fn load_files(files: &[PathBuf]) -> Result<Config, ConfigError> {
    let mut builder = config::Config::builder();
    for path in files {
        let text = match read(path) {
            Ok(text) => {
                tracing::info!(path = ?path, "load config");
                text
            }
            Err(error) if is_missing(&error) => continue,
            Err(error) => return Err(error),
        };

        let mut table = match text.parse::<toml::Table>() {
            Ok(table) => table,
            Err(error) => return Err(ConfigError::parse(path, &text, &error)),
        };
        let text = match retire(&mut table) {
            true => toml::to_string(&table).map_err(|error| ConfigError::Schema {
                message: format!("{}: {error}", path.display()),
            })?,
            false => text,
        };
        let panel_file = path.file_name().is_some_and(|name| name == PANEL_FILE)
            || path
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|name| name == PANEL_DROPINS);
        if let Some((key, value)) = table.iter().find(|(key, _)| {
            PANEL_TABLES.contains(&key.as_str()) != panel_file
                && (!panel_file || is_config_table(key))
        }) {
            let header = if value.is_array() {
                format!("[[{key}]]")
            } else {
                format!("[{key}]")
            };
            return Err(ConfigError::Schema {
                message: format!(
                    "{}: {header} belongs in {}",
                    path.display(),
                    if panel_file { CONFIG_FILE } else { PANEL_FILE }
                ),
            });
        }
        let source = config::File::from_str(&text, config::FileFormat::Toml);
        config::Config::builder()
            .add_source(source.clone())
            .build()
            .and_then(config::Config::try_deserialize::<Config>)
            .map_err(|error| ConfigError::schema(error, Some(path)))?;
        builder = builder.add_source(source);
    }

    builder
        .build()
        .map_err(|error| ConfigError::schema(error, None))?
        .try_deserialize()
        .map_err(|error| ConfigError::schema(error, None))
}

fn is_config_table(key: &str) -> bool {
    toml::Table::try_from(Config::default()).is_ok_and(|table| table.contains_key(key))
}

fn retire(table: &mut toml::Table) -> bool {
    let mut retired = false;
    for (path, reason) in RETIRED {
        let Some((last, parents)) = path.split_last() else {
            continue;
        };
        let mut current = Some(&mut *table);
        for parent in parents {
            current = current
                .and_then(|table| table.get_mut(*parent))
                .and_then(toml::Value::as_table_mut);
        }
        if current.and_then(|table| table.remove(*last)).is_some() {
            tracing::warn!(
                key = path.join("."),
                "this key is no longer read: {reason}; remove it"
            );
            retired = true;
        }
    }
    retired
}

pub fn seed_user_config() {
    match seed_from(config_dir(), user_dir().as_deref()) {
        Ok(created) => {
            for path in created {
                tracing::info!(path = %path.display(), "seeded a starting configuration");
            }
        }
        Err(error) => tracing::warn!(%error, "cannot seed a starting configuration"),
    }
}

fn seed_from(config_dir: Option<&Path>, user: Option<&Path>) -> io::Result<Vec<PathBuf>> {
    let (None, Some(dir)) = (config_dir, user) else {
        return Ok(Vec::new());
    };
    fs::create_dir_all(dir)?;
    let mut created = Vec::new();
    for (name, stub) in [(CONFIG_FILE, CONFIG_STUB), (PANEL_FILE, PANEL_STUB)] {
        let destination = dir.join(name);
        let mut file = match File::create_new(&destination) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        if let Err(error) = file.write_all(stub.as_bytes()).and_then(|()| file.flush()) {
            let _ = fs::remove_file(&destination);
            return Err(error);
        }
        created.push(destination);
    }
    Ok(created)
}

pub fn load_or_default(documents: Documents) -> (Config, Option<ConfigError>) {
    or_default(load(documents))
}

fn or_default(loaded: Result<Config, ConfigError>) -> (Config, Option<ConfigError>) {
    match loaded {
        Ok(config) => {
            for problem in config.validate() {
                tracing::warn!(%problem, "configuration problem");
            }
            (config, None)
        }
        Err(error) => {
            tracing::error!(%error, "starting with the default configuration");
            (Config::default(), Some(error))
        }
    }
}

pub fn unknown_applets(config: &Config) -> Vec<String> {
    let mut unknown = Vec::new();
    for (index, panel) in config.panels.iter().enumerate() {
        for (zone, names) in [
            ("left", &panel.left),
            ("center", &panel.center),
            ("right", &panel.right),
        ] {
            for name in names {
                if config.applets.contains_key(name) {
                    continue;
                }
                if Applet::from_name(name).is_some() {
                    continue;
                }
                unknown.push(format!(
                    "[[panels]] #{index} {zone}: unknown applet `{name}`{}",
                    suggestion(name)
                ));
            }
        }
    }
    unknown
}

fn suggestion(name: &str) -> String {
    let kebab = name.replace('_', "-");
    match Applet::from_name(&kebab).is_some() {
        true => format!(", did you mean `{kebab}`?"),
        false => String::new(),
    }
}

fn dropins(dir: &Path) -> Result<Vec<PathBuf>, ConfigError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(ConfigError::unreadable(dir, error)),
    };

    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| ConfigError::unreadable(dir, error))?;
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "toml")
        {
            paths.push(path);
        }
    }

    paths.sort();
    Ok(paths)
}

pub(crate) fn read(path: &Path) -> Result<String, ConfigError> {
    let mut file = File::open(path).map_err(|error| ConfigError::unreadable(path, error))?;
    let metadata = file
        .metadata()
        .map_err(|error| ConfigError::unreadable(path, error))?;

    if !metadata.is_file() {
        return Err(ConfigError::not_regular_file(path));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(ConfigError::too_large(path));
    }

    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|error| ConfigError::unreadable(path, error))?;

    Ok(text)
}

pub(crate) fn is_missing(error: &ConfigError) -> bool {
    matches!(
        error,
        ConfigError::Unreadable { source, .. } if source.kind() == io::ErrorKind::NotFound
    )
}

#[cfg(test)]
mod tests;
