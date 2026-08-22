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
        .map_err(|error| ConfigError::schema(error))?
        .try_deserialize()
        .map_err(|error| ConfigError::schema(error))
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
