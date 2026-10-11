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

    #[error("{}: {source}", located(path, resolved))]
    Unwritable {
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

    pub(crate) fn unwritable(path: &Path, source: io::Error) -> Self {
        Self::Unwritable {
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
        } => describe_location(
            format!("expected {expected}"),
            key.as_deref(),
            origin.as_deref(),
        ),
        config::ConfigError::At { error, origin, key } => {
            describe_location(schema_message(error), key.as_deref(), origin.as_deref())
        }
        other => other.to_string(),
    }
}

fn describe_location(mut message: String, key: Option<&str>, origin: Option<&str>) -> String {
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
mod tests;
