use std::io;
use std::path::{Path, PathBuf};

use glimpse_utils::clean;

const CAP: usize = 128;

pub(crate) fn sanitize(name: &str) -> Option<String> {
    let last = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned = clean(last, usize::MAX);
    let cleaned = cleaned.trim_start_matches(|c: char| c == '.' || c.is_whitespace());
    if cleaned.is_empty() {
        return None;
    }
    let (stem, extension) = split(cleaned);
    let budget = CAP.saturating_sub(extension.chars().count());
    let stem: String = stem.chars().take(budget).collect();
    let stem = stem.trim_end();
    if stem.is_empty() && extension.is_empty() {
        return None;
    }
    Some(format!("{stem}{extension}"))
}

fn split(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(0) | None => (name, ""),
        Some(index) => (&name[..index], &name[index..]),
    }
}

pub(crate) async fn relocate(source: &Path, downloads: &Path, name: &str) -> io::Result<PathBuf> {
    let base = sanitize(name).ok_or_else(|| io::Error::other("no usable file name"))?;
    let (stem, extension) = split(&base);
    let mut index = 0;
    let target = loop {
        let candidate = match index {
            0 => downloads.join(&base),
            _ => downloads.join(format!("{stem} ({index}){extension}")),
        };
        index += 1;
        match place(source, &candidate).await {
            Ok(()) => break candidate,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    };
    tokio::fs::remove_file(source).await?;
    tracing::info!(file = %name, target = %target.display(), "obex: moved the received file to the downloads");
    Ok(target)
}

async fn place(source: &Path, target: &Path) -> io::Result<()> {
    match tokio::fs::hard_link(source, target).await {
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::CrossesDevices
                    | io::ErrorKind::Unsupported
                    | io::ErrorKind::PermissionDenied
            ) =>
        {
            copy_new(source, target).await
        }
        other => other,
    }
}

async fn copy_new(source: &Path, target: &Path) -> io::Result<()> {
    let mut from = tokio::fs::File::open(source).await?;
    let mut to = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .await?;
    if let Err(error) = tokio::io::copy(&mut from, &mut to).await {
        drop(to);
        discard(target).await;
        return Err(error);
    }
    Ok(())
}

pub(crate) async fn discard(path: &Path) {
    match tokio::fs::remove_file(path).await {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::debug!(%error, path = %path.display(), "obex: cannot remove the file");
        }
    }
}
