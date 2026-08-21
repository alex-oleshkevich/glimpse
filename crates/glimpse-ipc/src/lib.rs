mod client;
pub use client::IPCClient;
use std::path::PathBuf;

pub const PROTOCOL_VERSION: u32 = 1;
pub const SOCKET_ENV: &str = "GLIMPSE_SOCKET";
pub const SOCKET_RELATIVE_PATH: &str = "glimpse/glimpsed.sock";

pub fn socket_path() -> Option<PathBuf> {
    discover(
        std::env::var(SOCKET_ENV).ok().map(PathBuf::from),
        dirs::runtime_dir(),
    )
}

// Split from `socket_path` so the search order is testable without mutating the environment,
// which edition 2024 makes `unsafe` — correctly, since the test harness is threaded.
fn discover(from_env: Option<PathBuf>, runtime_dir: Option<PathBuf>) -> Option<PathBuf> {
    let mut possible_paths: Vec<PathBuf> = vec![];
    if let Some(socket) = from_env {
        possible_paths.push(socket);
    }

    if let Some(runtime_dir) = runtime_dir {
        let socket_path = runtime_dir.join(SOCKET_RELATIVE_PATH);
        possible_paths.push(socket_path);
    }

    possible_paths
        .into_iter()
        .find(|socket_path| socket_path.exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Discovery answers by what is on disk, so the fixtures are real files. One directory per test
    // keeps a threaded run from colliding, and `tempfile` is not a dependency.
    fn runtime_dir_holding_a_socket(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("glimpse-ipc-{}-{test}", std::process::id()));
        let socket = dir.join(SOCKET_RELATIVE_PATH);
        let parent = socket.parent().expect("the socket path has a parent");
        std::fs::create_dir_all(parent).expect("the temporary directory should be creatable");
        std::fs::File::create(&socket).expect("the socket stand-in should be creatable");
        dir
    }

    fn nowhere() -> PathBuf {
        std::env::temp_dir().join("glimpse-ipc-no-such-socket")
    }

    #[test]
    fn the_default_sits_under_the_runtime_directory() {
        let runtime_dir = runtime_dir_holding_a_socket("default");
        assert_eq!(
            discover(None, Some(runtime_dir.clone())),
            Some(runtime_dir.join("glimpse/glimpsed.sock"))
        );
        std::fs::remove_dir_all(&runtime_dir).expect("the fixture should be removable");
    }

    #[test]
    fn the_environment_wins_over_the_default() {
        let runtime_dir = runtime_dir_holding_a_socket("environment");
        let from_env = runtime_dir.join("other.sock");
        std::fs::File::create(&from_env).expect("the socket stand-in should be creatable");
        assert_eq!(
            discover(Some(from_env.clone()), Some(runtime_dir.clone())),
            Some(from_env)
        );
        std::fs::remove_dir_all(&runtime_dir).expect("the fixture should be removable");
    }

    #[test]
    fn an_environment_socket_that_is_not_there_falls_through_to_the_default() {
        let runtime_dir = runtime_dir_holding_a_socket("fallthrough");
        assert_eq!(
            discover(Some(nowhere()), Some(runtime_dir.clone())),
            Some(runtime_dir.join(SOCKET_RELATIVE_PATH))
        );
        std::fs::remove_dir_all(&runtime_dir).expect("the fixture should be removable");
    }

    #[test]
    fn nothing_on_disk_is_nothing_to_discover() {
        assert_eq!(discover(Some(nowhere()), Some(nowhere())), None);
        assert_eq!(discover(None, None), None);
    }
}
