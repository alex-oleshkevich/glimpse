use std::path::{Path, PathBuf};

pub(crate) struct Roots {
    pub(crate) home: PathBuf,
    pub(crate) config: PathBuf,
    pub(crate) gvfs: PathBuf,
    pub(crate) trash_files: PathBuf,
}

impl Roots {
    pub(crate) fn resolve() -> Self {
        Self {
            home: dirs::home_dir().unwrap_or_default(),
            config: dirs::config_dir().unwrap_or_default(),
            gvfs: dirs::runtime_dir().unwrap_or_default().join("gvfs"),
            trash_files: dirs::data_dir()
                .unwrap_or_default()
                .join("Trash")
                .join("files"),
        }
    }
}

pub(crate) fn parse_user_dirs(text: &str, home: &Path) -> Vec<(String, PathBuf)> {
    text.lines()
        .take(super::ENTRIES)
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, raw_value) = line.split_once('=')?;
            let name = key.trim().strip_prefix("XDG_")?.strip_suffix("_DIR")?;
            let value = unquote(raw_value.trim())?;
            Some((name.to_ascii_lowercase(), resolve_dir(&value, home)))
        })
        .collect()
}

pub(crate) fn parse_bookmarks(text: &str) -> Vec<(String, Option<String>, PathBuf)> {
    text.lines()
        .take(super::ENTRIES)
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let mut parts = line.splitn(2, ' ');
            let uri = parts.next()?;
            let label = parts
                .next()
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .map(str::to_owned);
            let url = url::Url::parse(uri).ok()?;
            if url.scheme() != "file" {
                return None;
            }
            let target = url.to_file_path().ok()?;
            Some((uri.to_owned(), label, target))
        })
        .collect()
}

fn unquote(value: &str) -> Option<String> {
    let inner = value.strip_prefix('"')?.strip_suffix('"')?;
    let mut unescaped = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(character) = chars.next() {
        if character == '\\' {
            if let Some(next) = chars.next() {
                unescaped.push(next);
            }
        } else {
            unescaped.push(character);
        }
    }
    Some(unescaped)
}

fn resolve_dir(value: &str, home: &Path) -> PathBuf {
    match value.strip_prefix("$HOME") {
        Some(rest) => {
            let rest = rest.trim_start_matches('/');
            if rest.is_empty() {
                home.to_path_buf()
            } else {
                home.join(rest)
            }
        }
        None => PathBuf::from(value),
    }
}

#[cfg(test)]
mod tests;
