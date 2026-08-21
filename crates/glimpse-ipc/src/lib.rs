use std::path::PathBuf;

pub const PROTOCOL_VERSION: u32 = 1;
pub const SOCKET_ENV: &str = "GLIMPSE_SOCKET";
pub const SOCKET_RELATIVE_PATH: &str = "glimpse/glimpsed.sock";

pub fn socket_path() -> Option<PathBuf> {
    let mut possible_paths: Vec<PathBuf> = vec![];
    if let Some(socket) = std::env::var(SOCKET_ENV).ok() {
        possible_paths.push(PathBuf::from(socket));
    }

    if let Some(runtime_dir) = dirs::runtime_dir() {
        let socket_path = runtime_dir.join(SOCKET_RELATIVE_PATH);
        possible_paths.push(socket_path);
    }

    for socket_path in possible_paths {
        if socket_path.exists() {
            return Some(socket_path);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
}
