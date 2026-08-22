use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::error::{ConfigError, MAX_DROPINS, MAX_FILE_BYTES};
use crate::schema::Config;

const SYSTEM_DIR: &str = "/etc/glimpse";
const FILE_NAME: &str = "config.toml";
const DROPIN_DIR: &str = "config.d";

pub fn load(config_path: Option<&Path>) -> Result<Config, Vec<ConfigError>> {
    let user_dir = dirs::config_dir().map(|dir| dir.join("glimpse"));
    load_from(Path::new(SYSTEM_DIR), user_dir.as_deref(), config_path)
}

pub fn resolved_files(config_path: Option<&Path>) -> Result<Vec<PathBuf>, ConfigError> {
    let user_dir = dirs::config_dir().map(|dir| dir.join("glimpse"));
    stack(Path::new(SYSTEM_DIR), user_dir.as_deref(), config_path)
}

fn load_from(
    system_dir: &Path,
    user_dir: Option<&Path>,
    config_path: Option<&Path>,
) -> Result<Config, Vec<ConfigError>> {
    let files = stack(system_dir, user_dir, config_path).map_err(|error| vec![error])?;

    let mut merged = toml::Table::new();
    let mut errors = Vec::new();

    for path in files {
        let text = match read(&path) {
            Ok(text) => text,
            Err(error) => {
                if !is_missing(&error) {
                    errors.push(error);
                }
                continue;
            }
        };

        match text.parse::<toml::Table>() {
            Ok(table) => merge(&mut merged, table),
            Err(error) => errors.push(ConfigError::parse(&path, &text, &error)),
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    toml::Value::Table(merged)
        .try_into()
        .map_err(|error| vec![ConfigError::schema(error)])
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

    if paths.len() > MAX_DROPINS {
        return Err(ConfigError::TooManyDropins {
            dir: dir.to_path_buf(),
        });
    }

    paths.sort();
    Ok(paths)
}

fn read(path: &Path) -> Result<String, ConfigError> {
    let file = File::open(path).map_err(|error| ConfigError::unreadable(path, error))?;
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
    file.take(MAX_FILE_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|error| ConfigError::unreadable(path, error))?;
    if text.len() as u64 > MAX_FILE_BYTES {
        return Err(ConfigError::too_large(path));
    }

    Ok(text)
}

fn merge(base: &mut toml::Table, overlay: toml::Table) {
    for (key, value) in overlay {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(existing)), toml::Value::Table(incoming)) => {
                merge(existing, incoming);
            }
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

fn is_missing(error: &ConfigError) -> bool {
    matches!(
        error,
        ConfigError::Unreadable { source, .. } if source.kind() == io::ErrorKind::NotFound
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::schema::Scheme;

    fn table(text: &str) -> toml::Table {
        text.parse().expect("valid TOML fixture")
    }

    #[test]
    fn tables_merge_to_depth_and_arrays_replace() {
        let mut base = table(
            r#"
            [appearance]
            pack = "catppuccin"
            scheme = "auto"

            [[idle.profiles.ac.listeners]]
            timeout = 600
            [[idle.profiles.ac.listeners]]
            timeout = 900
            "#,
        );

        merge(
            &mut base,
            table(
                r#"
                [appearance]
                scheme = "dark"

                [[idle.profiles.ac.listeners]]
                timeout = 300
                "#,
            ),
        );

        assert_eq!(base["appearance"]["pack"].as_str(), Some("catppuccin"));
        assert_eq!(base["appearance"]["scheme"].as_str(), Some("dark"));
        let listeners = base["idle"]["profiles"]["ac"]["listeners"]
            .as_array()
            .expect("an array");
        assert_eq!(listeners.len(), 1);
        assert_eq!(listeners[0]["timeout"].as_integer(), Some(300));
    }

    #[test]
    fn dropins_are_toml_only_lexical_and_one_level_deep() {
        let dir = tempfile::tempdir().expect("tempdir");
        for name in ["20-b.toml", "10-a.toml", "notes.md", "30-c.toml.disabled"] {
            fs::write(dir.path().join(name), "").expect("fixture");
        }
        fs::create_dir(dir.path().join("nested")).expect("fixture");
        fs::write(dir.path().join("nested/99-deep.toml"), "").expect("fixture");

        let found = dropins(dir.path()).expect("listed");

        let names: Vec<_> = found
            .iter()
            .filter_map(|path| path.file_name()?.to_str())
            .collect();
        assert_eq!(names, ["10-a.toml", "20-b.toml"]);
    }

    #[test]
    fn a_missing_dropin_directory_is_normal() {
        let dir = tempfile::tempdir().expect("tempdir");

        assert!(
            dropins(&dir.path().join("config.d"))
                .expect("listed")
                .is_empty()
        );
    }

    #[test]
    fn past_the_cap_the_load_fails_rather_than_applying_a_prefix() {
        let dir = tempfile::tempdir().expect("tempdir");
        for index in 0..=MAX_DROPINS {
            fs::write(dir.path().join(format!("{index:03}.toml")), "").expect("fixture");
        }

        assert!(matches!(
            dropins(dir.path()),
            Err(ConfigError::TooManyDropins { .. })
        ));
    }

    #[test]
    fn later_layers_win_per_key() {
        let root = tempfile::tempdir().expect("tempdir");
        let system = root.path().join("etc");
        let user = root.path().join("user");
        fs::create_dir_all(system.join("config.d")).expect("fixture");
        fs::create_dir_all(user.join("config.d")).expect("fixture");

        fs::write(
            system.join("config.toml"),
            "[appearance]\npack = \"system\"\nscheme = \"light\"\n",
        )
        .expect("fixture");
        fs::write(user.join("config.toml"), "[appearance]\npack = \"user\"\n").expect("fixture");
        fs::write(
            user.join("config.d/10-scheme.toml"),
            "[appearance]\nscheme = \"dark\"\n",
        )
        .expect("fixture");

        let config = load_from(&system, Some(&user), None).expect("loaded");

        assert_eq!(config.appearance.pack, "user");
        assert_eq!(config.appearance.scheme, Scheme::Dark);
    }

    #[test]
    fn a_named_config_replaces_the_middle_layers_and_reads_no_dropins() {
        let root = tempfile::tempdir().expect("tempdir");
        let user = root.path().join("user");
        fs::create_dir_all(user.join("config.d")).expect("fixture");
        fs::write(user.join("config.toml"), "[appearance]\npack = \"user\"\n").expect("fixture");
        fs::write(
            user.join("config.d/10-scheme.toml"),
            "[appearance]\nscheme = \"dark\"\n",
        )
        .expect("fixture");

        let named = root.path().join("named.toml");
        fs::write(&named, "[appearance]\npack = \"named\"\n").expect("fixture");

        let config =
            load_from(&root.path().join("etc"), Some(&user), Some(&named)).expect("loaded");

        assert_eq!(config.appearance.pack, "named");
        assert_eq!(config.appearance.scheme, Scheme::Auto);
    }

    #[test]
    fn a_missing_file_is_an_absent_layer_everywhere_in_the_stack() {
        let root = tempfile::tempdir().expect("tempdir");

        let config = load_from(&root.path().join("etc"), None, None).expect("loaded");
        assert_eq!(config, Config::default());

        let named = load_from(
            &root.path().join("etc"),
            None,
            Some(&root.path().join("nowhere.toml")),
        )
        .expect("a missing --config path is an absent layer too");
        assert_eq!(named, Config::default());
    }

    #[test]
    fn a_dangling_dropin_is_an_absent_layer() {
        let root = tempfile::tempdir().expect("tempdir");
        let user = root.path().join("user");
        fs::create_dir_all(user.join("config.d")).expect("fixture");
        fs::write(user.join("config.toml"), "[appearance]\npack = \"user\"\n").expect("fixture");
        std::os::unix::fs::symlink(
            root.path().join("uninstalled.toml"),
            user.join("config.d/10-stale.toml"),
        )
        .expect("fixture");

        let config = load_from(&root.path().join("etc"), Some(&user), None)
            .expect("a stale link left by an uninstalled package must not cost the session");

        assert_eq!(config.appearance.pack, "user");
    }

    #[test]
    fn a_dropin_directory_that_is_not_a_directory_fails_the_whole_load() {
        let root = tempfile::tempdir().expect("tempdir");
        let user = root.path().join("user");
        fs::create_dir_all(&user).expect("fixture");
        fs::write(user.join("config.toml"), "[appearance]\npack = \"user\"\n").expect("fixture");
        fs::write(user.join("config.d"), "not a directory").expect("fixture");

        let problems = load_from(&root.path().join("etc"), Some(&user), None)
            .expect_err("a broken config.d/ is not skipped");

        assert!(matches!(problems[..], [ConfigError::Unreadable { .. }]));
    }

    #[test]
    fn a_failed_load_reports_every_problem_not_just_the_first() {
        let root = tempfile::tempdir().expect("tempdir");
        let user = root.path().join("user");
        fs::create_dir_all(user.join("config.d")).expect("fixture");
        fs::write(user.join("config.toml"), "[appearance]\npack = \"user\"\n").expect("fixture");
        fs::write(user.join("config.d/10-broken.toml"), "[appearance\n").expect("fixture");
        let padding = "#".repeat(MAX_FILE_BYTES as usize + 1);
        fs::write(user.join("config.d/20-large.toml"), padding).expect("fixture");

        let problems =
            load_from(&root.path().join("etc"), Some(&user), None).expect_err("two problems");

        assert_eq!(problems.len(), 2);
        assert!(
            problems[0].to_string().contains("10-broken.toml:1:12"),
            "{}",
            problems[0]
        );
        assert!(matches!(problems[1], ConfigError::TooLarge { .. }));
    }

    #[test]
    fn a_misspelled_top_level_table_is_refused() {
        let root = tempfile::tempdir().expect("tempdir");
        let user = root.path().join("user");
        fs::create_dir_all(&user).expect("fixture");
        fs::write(user.join("config.toml"), "[panle]\nsize = 36\n").expect("fixture");

        let problems =
            load_from(&root.path().join("etc"), Some(&user), None).expect_err("an unknown table");

        assert!(problems[0].to_string().contains("panle"), "{}", problems[0]);
    }

    #[test]
    fn a_file_past_the_size_cap_is_refused() {
        let root = tempfile::tempdir().expect("tempdir");
        let user = root.path().join("user");
        fs::create_dir_all(&user).expect("fixture");
        let padding = "#".repeat(MAX_FILE_BYTES as usize + 1);
        fs::write(user.join("config.toml"), padding).expect("fixture");

        let problems =
            load_from(&root.path().join("etc"), Some(&user), None).expect_err("past the cap");

        assert!(matches!(problems[..], [ConfigError::TooLarge { .. }]));
    }

    #[test]
    fn resolved_files_lists_the_same_stack_load_reads() {
        let root = tempfile::tempdir().expect("tempdir");
        let user = root.path().join("user");
        fs::create_dir_all(user.join("config.d")).expect("fixture");
        fs::write(user.join("config.toml"), "[appearance]\npack = \"user\"\n").expect("fixture");
        fs::write(user.join("config.d/10-scheme.toml"), "").expect("fixture");

        let files = stack(&root.path().join("etc"), Some(&user), None).expect("resolved");

        assert_eq!(
            files,
            [
                root.path().join("etc/config.toml"),
                user.join("config.toml"),
                user.join("config.d/10-scheme.toml"),
            ]
        );
    }

    #[test]
    fn a_directory_where_a_file_belongs_is_refused() {
        let root = tempfile::tempdir().expect("tempdir");
        let user = root.path().join("user");
        fs::create_dir_all(user.join("config.toml")).expect("fixture");

        let problems =
            load_from(&root.path().join("etc"), Some(&user), None).expect_err("not a regular file");

        assert!(matches!(problems[..], [ConfigError::NotRegularFile { .. }]));
    }
}
