use std::io;
use std::path::{Path, PathBuf};

pub(crate) const MAX_FILE_BYTES: u64 = 1 << 20;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{}: {source}", located(path, resolved))]
    Unreadable {
        path: PathBuf,
        resolved: Option<PathBuf>,
        #[source]
        source: io::Error,
    },

    #[error("{}: not a regular file", located(path, resolved))]
    NotRegularFile {
        path: PathBuf,
        resolved: Option<PathBuf>,
    },

    #[error("{}: larger than {MAX_FILE_BYTES} bytes", located(path, resolved))]
    TooLarge {
        path: PathBuf,
        resolved: Option<PathBuf>,
    },

    #[error("{}:{line}:{column}: {message}", path.display())]
    Parse {
        path: PathBuf,
        line: usize,
        column: usize,
        message: String,
    },

    #[error("{message}")]
    Schema { message: String },
}

impl ConfigError {
    pub(crate) fn unreadable(path: &Path, source: io::Error) -> Self {
        Self::Unreadable {
            path: path.to_path_buf(),
            resolved: link_target(path),
            source,
        }
    }

    pub(crate) fn not_regular_file(path: &Path) -> Self {
        Self::NotRegularFile {
            path: path.to_path_buf(),
            resolved: link_target(path),
        }
    }

    pub(crate) fn too_large(path: &Path) -> Self {
        Self::TooLarge {
            path: path.to_path_buf(),
            resolved: link_target(path),
        }
    }

    pub(crate) fn parse(path: &Path, text: &str, error: &toml::de::Error) -> Self {
        let (line, column) = error
            .span()
            .map_or((1, 1), |span| line_column(text, span.start));
        Self::Parse {
            path: path.to_path_buf(),
            line,
            column,
            message: error.message().to_owned(),
        }
    }

    pub(crate) fn schema(error: config::ConfigError) -> Self {
        Self::Schema {
            message: schema_message(&error),
        }
    }
}

fn schema_message(error: &config::ConfigError) -> String {
    match error {
        config::ConfigError::Type {
            origin,
            expected,
            key,
            ..
        } => describe_type_mismatch(expected, key.as_deref(), origin.as_deref()),
        config::ConfigError::At { error, origin, key } => {
            let mut message = schema_message(error);
            if let Some(key) = key {
                message.push_str(&format!(" for key `{key}`"));
            }
            if let Some(origin) = origin {
                message.push_str(&format!(" in {origin}"));
            }
            message
        }
        other => other.to_string(),
    }
}

fn describe_type_mismatch(expected: &str, key: Option<&str>, origin: Option<&str>) -> String {
    let mut message = format!("expected {expected}");
    if let Some(key) = key {
        message.push_str(&format!(" for key `{key}`"));
    }
    if let Some(origin) = origin {
        message.push_str(&format!(" in {origin}"));
    }
    message
}

fn located(path: &Path, resolved: &Option<PathBuf>) -> String {
    match resolved {
        Some(target) => format!("{} → {}", path.display(), target.display()),
        None => path.display().to_string(),
    }
}

fn link_target(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path)
        .ok()
        .filter(|target| target != path)
}

fn line_column(text: &str, offset: usize) -> (usize, usize) {
    let mut end = offset.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }

    let head = &text[..end];
    let line = head.matches('\n').count() + 1;
    let column = head.rsplit('\n').next().unwrap_or(head).chars().count() + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_are_one_based_and_counted_in_characters() {
        let text = "a = 1\nb = \"ключ\"\nc = 3\n";

        assert_eq!(line_column(text, 0), (1, 1));
        assert_eq!(line_column(text, 6), (2, 1));
        assert_eq!(line_column(text, 17), (2, 9));
    }

    #[test]
    fn an_offset_off_the_end_or_mid_character_does_not_panic() {
        let text = "x = \"é\"\n";

        assert_eq!(line_column(text, 999), (2, 1));
        assert_eq!(line_column(text, 6), (1, 6));
    }

    #[test]
    fn a_parse_error_names_the_position_without_quoting_the_line() {
        let text = "token = ghp_thisisasecret\n";
        let Err(error) = text.parse::<toml::Table>() else {
            panic!("expected a parse failure");
        };

        let rendered = ConfigError::parse(Path::new("config.toml"), text, &error).to_string();

        assert!(rendered.starts_with("config.toml:1:9:"), "{rendered}");
        assert!(!rendered.contains("ghp_thisisasecret"), "{rendered}");
    }

    #[test]
    fn a_type_mismatch_names_the_key_without_quoting_the_value() {
        // Only ever exercises the Err path, so `count` is never read back.
        #[derive(serde::Deserialize)]
        struct Fixture {
            #[allow(dead_code)]
            count: u64,
        }

        let raw = config::Config::builder()
            .add_source(config::File::from_str(
                "count = \"ghp_thisisasecret\"",
                config::FileFormat::Toml,
            ))
            .build()
            .expect("valid TOML, just the wrong type for `count`");

        let Err(error) = raw.try_deserialize::<Fixture>() else {
            panic!("expected a type mismatch");
        };

        let rendered = ConfigError::schema(error).to_string();

        assert!(rendered.contains("count"), "{rendered}");
        assert!(!rendered.contains("ghp_thisisasecret"), "{rendered}");
    }
}
