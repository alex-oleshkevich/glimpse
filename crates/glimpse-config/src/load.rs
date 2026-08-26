use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::error::{ConfigError, MAX_FILE_BYTES};
use crate::schema::Config;

const SYSTEM_DIR: &str = "/etc/glimpse";
const FILE_NAME: &str = "config.toml";
const DROPIN_DIR: &str = "config.d";
const FOLDER_NAME: &str = "glimpse";

pub fn load(config_path: Option<&Path>) -> Result<Config, ConfigError> {
    let user_dir = dirs::config_dir().map(|dir| dir.join(FOLDER_NAME));
    load_from(Path::new(SYSTEM_DIR), user_dir.as_deref(), config_path)
}

pub fn resolved_files(config_path: Option<&Path>) -> Result<Vec<PathBuf>, ConfigError> {
    let user_dir = dirs::config_dir().map(|dir| dir.join(FOLDER_NAME));
    stack(Path::new(SYSTEM_DIR), user_dir.as_deref(), config_path)
}

pub fn watch_dirs(config_path: Option<&Path>) -> Vec<PathBuf> {
    let user_dir = dirs::config_dir().map(|dir| dir.join(FOLDER_NAME));
    watch_dirs_from(Path::new(SYSTEM_DIR), user_dir.as_deref(), config_path)
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

fn push(dirs: &mut Vec<PathBuf>, dir: PathBuf) {
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
            Ok(text) => text,
            Err(error) if is_missing(&error) => continue,
            Err(error) => return Err(error),
        };

        if let Err(error) = text.parse::<toml::Table>() {
            return Err(ConfigError::parse(&path, &text, &error));
        }
        builder = builder.add_source(config::File::from_str(&text, config::FileFormat::Toml));
    }

    builder
        .build()
        .map_err(ConfigError::schema)?
        .try_deserialize()
        .map_err(ConfigError::schema)
}

fn stack(
    system_dir: &Path,
    user_dir: Option<&Path>,
    config_path: Option<&Path>,
) -> Result<Vec<PathBuf>, ConfigError> {
    if let Some(path) = config_path {
        return Ok(vec![path.to_path_buf()]);
    }

    let mut files = Vec::new();
    for dir in [Some(system_dir), user_dir].into_iter().flatten() {
        files.push(dir.join(FILE_NAME));
        files.extend(dropins(&dir.join(DROPIN_DIR))?);
    }
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
mod tests {
    use super::*;

    #[test]
    fn the_stack_is_each_directory_and_its_dropins_in_merge_order() {
        assert_eq!(
            watch_dirs_from(
                Path::new("/etc/glimpse"),
                Some(Path::new("/home/u/.config/glimpse")),
                None,
            ),
            [
                PathBuf::from("/etc/glimpse"),
                PathBuf::from("/etc/glimpse/config.d"),
                PathBuf::from("/home/u/.config/glimpse"),
                PathBuf::from("/home/u/.config/glimpse/config.d"),
            ]
        );
    }

    /// `--config` replaces the stack, drop-ins included, so there is nothing else to watch.
    #[test]
    fn an_explicit_file_is_watched_through_its_own_directory_alone() {
        assert_eq!(
            watch_dirs_from(
                Path::new("/etc/glimpse"),
                Some(Path::new("/home/u/.config/glimpse")),
                Some(Path::new("/tmp/nowhere/config.toml")),
            ),
            [PathBuf::from("/tmp/nowhere")]
        );
    }

    /// The dotfile case: the file lives in a repository and is linked into place. Both ends need a
    /// watch, because editing the file and replacing the link produce events in different
    /// directories.
    #[test]
    fn a_symlinked_base_file_adds_the_directory_holding_its_target() {
        let root = tempfile::tempdir().expect("a temporary directory");
        let store = root.path().join("dotfiles");
        let config = root.path().join("glimpse");
        std::fs::create_dir_all(&store).expect("creates");
        std::fs::create_dir_all(&config).expect("creates");
        std::fs::write(store.join(FILE_NAME), "").expect("writes");
        std::os::unix::fs::symlink(store.join(FILE_NAME), config.join(FILE_NAME)).expect("links");

        let dirs = watch_dirs_from(&config, None, None);

        assert_eq!(
            dirs,
            [
                config.clone(),
                config.join(DROPIN_DIR),
                store.canonicalize().expect("the target resolves"),
            ]
        );
    }
}
