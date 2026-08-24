use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error("No parent directory for {0}")]
    NoParent(PathBuf),
}

pub enum Change {
    FileChanged(PathBuf),
    DirChanged(PathBuf),
}

pub async fn watch_file(path: PathBuf) -> Result<(), WatchError> {
    let watch_file = path.canonicalize().unwrap_or_else(|_| path.clone());
    let Some(parent_dir) = path.parent().map(PathBuf::from) else {
        return Err(WatchError::NoParent(path));
    };

    return watch_dir(parent_dir).await;
}

pub async fn watch_dir(dir: PathBuf) -> Result<(), WatchError> {
    tracing::debug!(dir = %dir.display(), "watching directory for changes");

    Ok(())
}
