use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use anyhow::{Context, Result};
use glimpse_config::ExecAppletConfig;

pub(super) const GROUP: &str = "Desktop Entry";
pub(super) const MARKER: &str = "X-Glimpse-Installed-By";

pub(super) fn settings(dir: &Path, id: &str) -> Result<ExecAppletConfig> {
    let path = dir.join("devconfig.toml");
    match fs::symlink_metadata(&path) {
        Ok(_) => super::run::settings(&path, id),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ExecAppletConfig {
            applet: id.to_owned(),
            options: serde_json::Map::new(),
            env: std::collections::BTreeMap::new(),
        }),
        Err(error) => Err(error).with_context(|| format!("cannot read {}", path.display())),
    }
}

pub(super) fn marked(path: &Path) -> Result<(String, fs::Metadata)> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| {
            if error.raw_os_error() == Some(libc::ELOOP) {
                anyhow::Error::new(crate::errors::Refused(
                    "refusing a symlink desktop entry".to_owned(),
                ))
            } else {
                error.into()
            }
        })?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(
            crate::errors::Refused("desktop entry is not a regular file".to_owned()).into(),
        );
    }
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let desktop = glib::KeyFile::new();
    desktop.load_from_data(&contents, glib::KeyFileFlags::NONE)?;
    if !desktop
        .string(GROUP, MARKER)
        .is_ok_and(|value| value == "glimpsectl")
    {
        return Err(crate::errors::Refused(
            "desktop entry was not installed by glimpsectl".to_owned(),
        )
        .into());
    }
    Ok((contents, metadata))
}

#[cfg(test)]
mod tests;
