use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::error::{ConfigError, MAX_FILE_BYTES};
use crate::schema::{Applet, Config};

const SYSTEM_DIR: &str = "/etc/glimpse";
const FILE_NAME: &str = "config.toml";
const COMMENTED_FILE_NAME: &str = "config.commented.toml";
const DROPIN_DIR: &str = "config.d";
const FOLDER_NAME: &str = "glimpse";

/// `/usr/share/glimpse` — where the package installs files a user reads but does not edit. Beside
/// [`user_dir`] because the two are asked in the same breath: the user's copy, then the shipped one.
pub const DATA_DIR: &str = "/usr/share/glimpse";

/// `~/.config/glimpse`, or `None` on a platform that names no config directory. Everything a user
/// may override lives here, not only `config.toml`, so other crates locate their own files through
/// this rather than rebuilding the path.
pub fn user_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join(FOLDER_NAME))
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

pub fn named_applets_exist(config: &Config) -> Result<(), ConfigError> {
    for (index, panel) in config.panels.iter().enumerate() {
        for (zone, names) in [
            ("left", &panel.left),
            ("center", &panel.center),
            ("right", &panel.right),
        ] {
            for name in names {
                if config.applets.contains_key(name) || Applet::from_name(name).is_some() {
                    continue;
                }
                return Err(ConfigError::Schema {
                    message: format!(
                        "[[panels]] #{index} {zone}: unknown applet `{name}`{}",
                        suggestion(name)
                    ),
                });
            }
        }
    }
    Ok(())
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

    fn shipped(dir: &Path) -> PathBuf {
        let source = dir.join(COMMENTED_FILE_NAME);
        std::fs::write(&source, crate::commented_document()).expect("writes");
        source
    }

    #[test]
    fn a_first_run_seeds_the_user_config_from_the_shipped_file() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let source = shipped(dir.path());
        let user = dir.path().join("config/glimpse");

        let seeded = seed_from(&source, Some(&user), None)
            .expect("seeding succeeds")
            .expect("a first run writes the file");

        assert_eq!(seeded, user.join(FILE_NAME));
        assert_eq!(
            std::fs::read_to_string(&seeded).expect("reads"),
            crate::commented_document()
        );
    }

    #[test]
    fn an_existing_user_config_is_never_replaced() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let source = shipped(dir.path());
        let user = dir.path().join("glimpse");
        std::fs::create_dir_all(&user).expect("creates");
        std::fs::write(user.join(FILE_NAME), "[appearance]\ntheme = \"mine\"\n").expect("writes");

        assert_eq!(
            seed_from(&source, Some(&user), None).expect("succeeds"),
            None
        );
        assert_eq!(
            std::fs::read_to_string(user.join(FILE_NAME)).expect("reads"),
            "[appearance]\ntheme = \"mine\"\n"
        );
    }

    #[test]
    fn an_explicit_config_path_seeds_nothing() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let source = shipped(dir.path());
        let user = dir.path().join("glimpse");
        let explicit = dir.path().join("elsewhere.toml");

        assert_eq!(
            seed_from(&source, Some(&user), Some(&explicit)).expect("succeeds"),
            None
        );
        assert!(!user.exists());
    }

    #[test]
    fn a_missing_shipped_file_is_not_a_failure() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let user = dir.path().join("glimpse");

        let source = dir.path().join(COMMENTED_FILE_NAME);
        assert_eq!(
            seed_from(&source, Some(&user), None).expect("succeeds"),
            None
        );
        assert!(!user.exists());
    }

    #[test]
    fn a_seeded_document_loads_to_the_shipped_defaults() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let source = shipped(dir.path());
        let user = dir.path().join("glimpse");
        let seeded = seed_from(&source, Some(&user), None)
            .expect("succeeds")
            .expect("writes");

        let empty = dir.path().join("empty");
        std::fs::create_dir_all(&empty).expect("creates");
        assert_eq!(
            load_from(&empty, Some(&user), None).expect("the seed loads"),
            Config::default()
        );
        assert!(seeded.is_file());
    }

    #[test]
    fn two_binaries_starting_at_once_seed_one_intact_file() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let source = shipped(dir.path());
        let user = dir.path().join("glimpse");

        let written = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| scope.spawn(|| seed_from(&source, Some(&user), None).expect("succeeds")))
                .collect();
            handles
                .into_iter()
                .filter_map(|handle| handle.join().expect("the thread finishes"))
                .count()
        });

        assert_eq!(written, 1);
        assert_eq!(
            std::fs::read_to_string(user.join(FILE_NAME)).expect("reads"),
            crate::commented_document()
        );
    }

    fn panel_naming(name: &str) -> Config {
        Config {
            panels: vec![crate::schema::Panel {
                right: vec![name.to_owned()],
                ..crate::schema::Panel::default()
            }],
            ..Config::default()
        }
    }

    #[test]
    fn an_applet_name_the_document_cannot_resolve_is_a_load_error() {
        let error = named_applets_exist(&panel_naming("not-an-applet"))
            .expect_err("a name nothing resolves is not a panel glimpse can build");
        assert!(
            error.to_string().contains("not-an-applet"),
            "the message has to name the applet: {error}"
        );
        assert!(error.to_string().contains("right"));
    }

    #[test]
    fn an_underscore_spelling_names_the_kebab_case_one_it_meant() {
        let error = named_applets_exist(&panel_naming("next_event"))
            .expect_err("every applet kind is kebab-case");
        assert!(
            error.to_string().contains("did you mean `next-event`?"),
            "an underscore is the typo worth spelling out: {error}"
        );
    }

    #[test]
    fn a_kind_name_and_a_configured_instance_name_both_resolve() {
        named_applets_exist(&panel_naming("next-event")).expect("a kind name resolves");

        let mut named = panel_naming("my-clock");
        named.applets.insert(
            "my-clock".to_owned(),
            Applet::from_name("clock").expect("clock"),
        );
        named_applets_exist(&named).expect("a name declared under [applets] resolves");
    }

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

    /// `[geolocation]` is one internally tagged enum, so it is the table most likely to break under
    /// layering: the layers have to merge as plain maps and be typed once at the end, or a drop-in
    /// completing a base table would be read as a table missing a key.
    #[test]
    fn a_dropin_completes_a_base_table_rather_than_replacing_it() {
        let root = tempfile::tempdir().expect("a temporary directory");
        let system = root.path().join("etc");
        let user = root.path().join("config");
        std::fs::create_dir_all(system.join(DROPIN_DIR)).expect("creates");
        std::fs::create_dir_all(&user).expect("creates");
        std::fs::write(
            system.join(FILE_NAME),
            "[geolocation]\nprovider = \"manual\"\nlatitude = 51.5074\nlongitude = -0.1278\n",
        )
        .expect("writes");
        std::fs::write(
            system.join(DROPIN_DIR).join("10-move.toml"),
            "[geolocation]\nlatitude = 52.2297\n",
        )
        .expect("writes");

        let loaded = load_from(&system, Some(&user), None).expect("the layers merge");

        assert_eq!(
            loaded.geolocation,
            crate::Geolocation::Manual {
                latitude: 52.2297,
                longitude: -0.1278
            },
            "the drop-in overrides one key and leaves the other standing"
        );
    }

    #[test]
    fn a_tilde_prefixed_path_expands_against_home() {
        let home = Path::new("/home/u");
        let resolved = expand_tilde(Path::new("~/wallpapers/city.jpg"), Some(home));
        assert_eq!(resolved, PathBuf::from("/home/u/wallpapers/city.jpg"));
    }

    #[test]
    fn an_absolute_path_that_exists_is_used_as_is() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let file = dir.path().join("city.jpg");
        std::fs::write(&file, b"").expect("writes");

        let resolved = resolve_in(&file, None, None, Path::new("/nonexistent"));
        assert_eq!(resolved.as_deref(), Some(file.as_path()));
    }

    #[test]
    fn an_absolute_path_that_is_missing_resolves_to_nothing() {
        let resolved = resolve_in(
            Path::new("/nonexistent/city.jpg"),
            None,
            None,
            Path::new("/nonexistent"),
        );
        assert_eq!(resolved, None);
    }

    #[test]
    fn a_relative_path_prefers_the_user_root() {
        let root = tempfile::tempdir().expect("a temp dir");
        let user = root.path().join("user");
        let data = root.path().join("data");
        std::fs::create_dir_all(&user).expect("creates");
        std::fs::create_dir_all(data.join("wallpapers")).expect("creates");
        std::fs::write(user.join("city.jpg"), b"user").expect("writes");
        std::fs::write(data.join("wallpapers").join("city.jpg"), b"data").expect("writes");

        let resolved = resolve_in(Path::new("city.jpg"), None, Some(&user), &data)
            .expect("the user copy resolves");
        assert_eq!(resolved, user.join("city.jpg"));
    }

    #[test]
    fn a_relative_path_falls_through_to_the_data_root() {
        let root = tempfile::tempdir().expect("a temp dir");
        let user = root.path().join("user");
        let data = root.path().join("data");
        std::fs::create_dir_all(&user).expect("creates");
        std::fs::create_dir_all(data.join("wallpapers")).expect("creates");
        std::fs::write(data.join("wallpapers").join("city.jpg"), b"data").expect("writes");

        let resolved = resolve_in(Path::new("city.jpg"), None, Some(&user), &data)
            .expect("the data copy resolves");
        assert_eq!(resolved, data.join("wallpapers").join("city.jpg"));
    }

    #[test]
    fn a_relative_path_resolving_nowhere_is_a_miss() {
        let root = tempfile::tempdir().expect("a temp dir");
        let user = root.path().join("user");
        let data = root.path().join("data");
        std::fs::create_dir_all(&user).expect("creates");

        let resolved = resolve_in(Path::new("city.jpg"), None, Some(&user), &data);
        assert_eq!(resolved, None);
    }
}
