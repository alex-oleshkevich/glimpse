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
mod tests {
    use super::*;

    #[test]
    fn a_taken_name_gets_the_next_suffix_and_the_sessions_own_file_is_overwritten() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let name = "Screenshot from 2026-09-24 14-03-11.png";

        let first = Disk
            .write_new(dir.path(), name, None, b"one")
            .expect("first");
        let second = Disk
            .write_new(dir.path(), name, None, b"two")
            .expect("second");
        let again = Disk
            .write_new(dir.path(), name, Some(&second), b"three")
            .expect("own");

        assert_eq!(first, dir.path().join(name));
        assert_eq!(
            second,
            dir.path()
                .join("Screenshot from 2026-09-24 14-03-11 (2).png")
        );
        assert_eq!(again, second);
        assert_eq!(std::fs::read(&first).expect("reads"), b"one");
        assert_eq!(std::fs::read(&second).expect("reads"), b"three");
    }
}
