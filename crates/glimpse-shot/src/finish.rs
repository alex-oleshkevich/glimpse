use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use chrono::{DateTime, Local};
use glimpse_config::ScreenshotConfig;
use glimpse_shot::doc::naming::file_name;
use glimpse_shot::ports::Files;
use gtk4::glib;

pub const STDOUT: &str = "-";

pub enum Destination {
    Stdout,
    Nowhere,
    File(PathBuf),
    Directory(PathBuf),
}

impl Destination {
    pub fn of(output: Option<&Path>, no_save: bool, config: &ScreenshotConfig) -> Result<Self> {
        Ok(match output {
            Some(path) if path == Path::new("-") => Self::Stdout,
            Some(path) => Self::File(path.to_path_buf()),
            None if no_save => Self::Nowhere,
            None => Self::Directory(match &config.directory {
                Some(directory) => directory.clone(),
                None => pictures()?,
            }),
        })
    }

    pub fn requested(&self, config: &ScreenshotConfig, taken: DateTime<Local>) -> Option<PathBuf> {
        match self {
            Self::Stdout => Some(PathBuf::from(STDOUT)),
            Self::Nowhere => None,
            Self::File(path) => Some(path.clone()),
            Self::Directory(dir) => Some(dir.join(file_name(&config.filename, taken))),
        }
    }
}

fn pictures() -> Result<PathBuf> {
    match glib::user_special_dir(glib::UserDirectory::Pictures) {
        Some(pictures) => Ok(pictures.join("Screenshots")),
        None => bail!(
            "there is no Pictures directory to save into; set [screenshot] directory or pass -o FILE"
        ),
    }
}

pub fn write(
    destination: &Destination,
    requested: &Path,
    files: &dyn Files,
    bytes: &[u8],
    stdout: &mut dyn Write,
) -> Result<PathBuf> {
    match destination {
        Destination::Nowhere => bail!("there is nowhere to write the screenshot"),
        Destination::Stdout => {
            ignoring_broken_pipe(stdout.write_all(bytes).and_then(|()| stdout.flush()))?;
            Ok(PathBuf::from(STDOUT))
        }
        Destination::File(path) => {
            let dir = match path.parent() {
                Some(parent) if !parent.as_os_str().is_empty() => parent,
                _ => Path::new("."),
            };
            files
                .write_new(dir, &name_of(path), Some(path), bytes)
                .with_context(|| format!("cannot write {}", path.display()))
        }
        Destination::Directory(dir) => {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("cannot create {}", dir.display()))?;
            files
                .write_new(dir, &name_of(requested), None, bytes)
                .with_context(|| format!("cannot save into {}", dir.display()))
        }
    }
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn line(stdout: &mut dyn Write, text: &str) -> Result<()> {
    ignoring_broken_pipe(writeln!(stdout, "{text}").and_then(|()| stdout.flush()))
}

fn ignoring_broken_pipe(result: io::Result<()>) -> Result<()> {
    match result {
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => other.context("cannot write to stdout"),
    }
}

#[cfg(test)]
mod tests;
