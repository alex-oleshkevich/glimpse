use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use tempfile::NamedTempFile;

use crate::error::{ConfigError, MAX_FILE_BYTES};
use crate::schema::{Applet, Config};

const SYSTEM_DIR: &str = "/etc/glimpse";
const FILE_NAME: &str = "config.toml";
pub const SCREENSHOT_FILE_NAME: &str = "shot.toml";
const SCREENSHOT_TABLE: &str = "screenshot";
const COMMENTED_FILE_NAME: &str = "config.commented.toml";
const DROPIN_DIR: &str = "config.d";
const FOLDER_NAME: &str = "glimpse";
const WRITE_ATTEMPTS: usize = 3;

/// `$PREFIX/share/glimpse`, `/usr/share/glimpse` by default — where the package installs files a
/// user reads but does not edit. Beside [`user_dir`] because the two are asked in the same breath:
/// the user's copy, then the shipped one.
pub const DATA_DIR: &str = env!("GLIMPSE_DATA_DIR_DEFAULT");

/// `~/.config/glimpse`, or `None` on a platform that names no config directory. Everything a user
/// may override lives here, not only `config.toml`, so other crates locate their own files through
/// this rather than rebuilding the path.
pub fn user_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join(FOLDER_NAME))
}

pub fn state_dir() -> Option<PathBuf> {
    dirs::state_dir().map(|dir| dir.join(FOLDER_NAME))
}

pub fn applets_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join(FOLDER_NAME).join("applets"))
}

pub fn applet_dir(id: &str) -> Option<PathBuf> {
    applets_dir().map(|dir| dir.join(id))
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

pub fn shadowing_dropin(
    user_dir: Option<&Path>,
    table: &str,
    key: &str,
) -> Result<Option<PathBuf>, ConfigError> {
    let Some(dir) = user_dir else {
        return Ok(None);
    };
    let mut shadowing = None;
    for path in dropins(&dir.join(DROPIN_DIR))? {
        let text = match read(&path) {
            Ok(text) => text,
            Err(error) if is_missing(&error) => continue,
            Err(error) => return Err(error),
        };
        let parsed = text
            .parse::<toml::Table>()
            .map_err(|error| ConfigError::parse(&path, &text, &error))?;
        if parsed
            .get(table)
            .and_then(toml::Value::as_table)
            .is_some_and(|values| values.contains_key(key))
        {
            shadowing = Some(path);
        }
    }
    Ok(shadowing)
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
        if let Err(error) = text.parse::<toml::Table>() {
            return Err(ConfigError::parse(&target, &text, &error));
        }
        let mut document =
            text.parse::<toml_edit::DocumentMut>()
                .map_err(|error| ConfigError::Parse {
                    path: target.clone(),
                    line: 1,
                    column: 1,
                    message: error.message().to_owned(),
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

pub fn load(config_path: Option<&Path>) -> Result<Config, ConfigError> {
    load_from(Path::new(SYSTEM_DIR), user_dir().as_deref(), config_path)
}

pub fn seed_user_config(config_path: Option<&Path>) {
    let source = Path::new(DATA_DIR).join(COMMENTED_FILE_NAME);
    match seed_from(&source, user_dir().as_deref(), config_path) {
        Ok(Some(path)) => tracing::info!(path = %path.display(), "seeded a starting configuration"),
        Ok(None) => {}
        Err(error) => tracing::warn!(%error, "cannot seed a starting configuration"),
    }
}

fn seed_from(
    source: &Path,
    user_dir: Option<&Path>,
    config_path: Option<&Path>,
) -> io::Result<Option<PathBuf>> {
    if config_path.is_some() {
        return Ok(None);
    }
    let Some(dir) = user_dir else {
        return Ok(None);
    };
    let destination = dir.join(FILE_NAME);
    if destination.try_exists()? {
        return Ok(None);
    }
    let text = match fs::read(source) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            tracing::debug!(path = %source.display(), "no shipped configuration to seed from");
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    fs::create_dir_all(dir)?;
    let mut file = match File::create_new(&destination) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(None),
        Err(error) => return Err(error),
    };
    if let Err(error) = file.write_all(&text).and_then(|()| file.flush()) {
        let _ = fs::remove_file(&destination);
        return Err(error);
    }
    Ok(Some(destination))
}

pub fn resolved_files(config_path: Option<&Path>) -> Result<Vec<PathBuf>, ConfigError> {
    stack(Path::new(SYSTEM_DIR), user_dir().as_deref(), config_path)
}

pub fn screenshot_file(config_path: Option<&Path>) -> Option<PathBuf> {
    screenshot_file_in(user_dir().as_deref(), config_path)
}

fn screenshot_file_in(user_dir: Option<&Path>, config_path: Option<&Path>) -> Option<PathBuf> {
    match config_path {
        Some(path) => path.parent().map(|dir| dir.join(SCREENSHOT_FILE_NAME)),
        None => user_dir.map(|dir| dir.join(SCREENSHOT_FILE_NAME)),
    }
}

pub fn watch_dirs(config_path: Option<&Path>) -> Vec<PathBuf> {
    watch_dirs_from(Path::new(SYSTEM_DIR), user_dir().as_deref(), config_path)
}

pub fn resolve_image(path: &Path) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    resolve_in(
        path,
        home.as_deref(),
        user_dir().as_deref(),
        Path::new(DATA_DIR),
    )
}

fn resolve_in(
    path: &Path,
    home: Option<&Path>,
    user: Option<&Path>,
    data: &Path,
) -> Option<PathBuf> {
    let expanded = expand_tilde(path, home);

    if expanded.is_absolute() {
        return expanded.is_file().then_some(expanded);
    }

    if let Some(user) = user {
        let candidate = user.join(&expanded);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    let candidate = data.join("wallpapers").join(&expanded);
    candidate.is_file().then_some(candidate)
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

fn watch_dirs_from(
    system_dir: &Path,
    user_dir: Option<&Path>,
    config_path: Option<&Path>,
) -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(path) = config_path {
        push_parent(&mut dirs, path);
        push_target_parent(&mut dirs, path);
        return dirs;
    }

    for dir in [Some(system_dir), user_dir].into_iter().flatten() {
        push(&mut dirs, dir.to_path_buf());
        push(&mut dirs, dir.join(DROPIN_DIR));
        push_target_parent(&mut dirs, &dir.join(FILE_NAME));
    }
    dirs
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
fn push_target_parent(dirs: &mut Vec<PathBuf>, file: &Path) {
    if let Ok(target) = std::fs::canonicalize(file) {
        push_parent(dirs, &target);
    }
}

fn load_from(
    system_dir: &Path,
    user_dir: Option<&Path>,
    config_path: Option<&Path>,
) -> Result<Config, ConfigError> {
    let files = stack(system_dir, user_dir, config_path)?;

    let mut builder = config::Config::builder();
    for path in files {
        let text = match read(&path) {
            Ok(text) => {
                tracing::info!(path = ?path, "load config");
                text
            }
            Err(error) if is_missing(&error) => continue,
            Err(error) => return Err(error),
        };

        let mut table = match text.parse::<toml::Table>() {
            Ok(table) => table,
            Err(error) => return Err(ConfigError::parse(&path, &text, &error)),
        };
        let text = if Some(&path) == screenshot_file_in(user_dir, config_path).as_ref() {
            table.retain(|key, _| key == SCREENSHOT_TABLE);
            table.to_string()
        } else {
            text
        };
        builder = builder.add_source(config::File::from_str(&text, config::FileFormat::Toml));
    }

    builder
        .build()
        .map_err(ConfigError::schema)?
        .try_deserialize()
        .map_err(ConfigError::schema)
}

pub fn load_or_default(config_path: Option<&Path>) -> (Config, Option<ConfigError>) {
    match load(config_path) {
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

fn stack(
    system_dir: &Path,
    user_dir: Option<&Path>,
    config_path: Option<&Path>,
) -> Result<Vec<PathBuf>, ConfigError> {
    let mut files = Vec::new();
    match config_path {
        Some(path) => files.push(path.to_path_buf()),
        None => {
            for dir in [Some(system_dir), user_dir].into_iter().flatten() {
                files.push(dir.join(FILE_NAME));
                files.extend(dropins(&dir.join(DROPIN_DIR))?);
            }
        }
    }
    files.extend(screenshot_file_in(user_dir, config_path));
    Ok(files)
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

fn read(path: &Path) -> Result<String, ConfigError> {
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

fn is_missing(error: &ConfigError) -> bool {
    matches!(
        error,
        ConfigError::Unreadable { source, .. } if source.kind() == io::ErrorKind::NotFound
    )
}

#[cfg(test)]
mod tests;
