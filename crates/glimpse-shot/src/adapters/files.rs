use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use glimpse_shot::doc::naming::candidates;
use glimpse_shot::ports::Files;

pub struct Disk;

impl Files for Disk {
    fn write_new(
        &self,
        dir: &Path,
        name: &str,
        own: Option<&Path>,
        bytes: &[u8],
    ) -> io::Result<PathBuf> {
        if let Some(own) = own {
            std::fs::write(own, bytes)?;
            return Ok(own.to_path_buf());
        }
        for candidate in candidates(name) {
            let path = dir.join(candidate);
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    file.write_all(bytes)?;
                    return Ok(path);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::other("no free file name"))
    }
}

#[cfg(test)]
mod tests;
