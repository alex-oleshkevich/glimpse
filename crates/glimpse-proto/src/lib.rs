use std::path::{Path, PathBuf};

pub const PROTOCOL_VERSION: u32 = 1;

pub const SOCKET_RELATIVE_PATH: &str = "glimpse/glimpsed.sock";

pub fn socket_path(explicit: Option<&Path>, runtime_dir: &Path) -> PathBuf {
    match explicit {
        Some(path) => path.to_path_buf(),
        None => runtime_dir.join(SOCKET_RELATIVE_PATH),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_socket_defaults_under_the_runtime_directory() {
        assert_eq!(
            socket_path(None, Path::new("/run/user/1000")),
            Path::new("/run/user/1000/glimpse/glimpsed.sock")
        );
    }

    #[test]
    fn an_explicit_socket_wins() {
        let explicit = Path::new("/run/user/1000/other.sock");
        assert_eq!(
            socket_path(Some(explicit), Path::new("/run/user/1000")),
            explicit
        );
    }
}
