use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gettextrs::gettext;
use gtk4::gdk_pixbuf::Pixbuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const LIMIT: u64 = 64 * 1024 * 1024;
const CEILING: i32 = 8192;
const URI_CAP: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reject {
    NotFileUri,
    TooLarge,
    NotImage,
    Unreadable,
}

pub fn reason_text(reject: Reject) -> String {
    match reject {
        Reject::NotFileUri => gettext("Only a file on this computer can be set as the wallpaper."),
        Reject::TooLarge => {
            gettext("The image is larger than {limit} MiB.").replace("{limit}", "64")
        }
        Reject::NotImage => gettext("The file is not an image."),
        Reject::Unreadable => gettext("The file cannot be read."),
    }
}

pub fn file_path_from_uri(uri: &str) -> Result<PathBuf, Reject> {
    if uri.len() > URI_CAP {
        return Err(Reject::NotFileUri);
    }
    let parsed = url::Url::parse(uri).map_err(|_| Reject::NotFileUri)?;
    if parsed.scheme() != "file" {
        return Err(Reject::NotFileUri);
    }
    let path = parsed.to_file_path().map_err(|_| Reject::NotFileUri)?;
    if !path.is_absolute() {
        return Err(Reject::NotFileUri);
    }
    Ok(path)
}

fn copy_name(source: &Path) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let extension = source
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| {
            !extension.is_empty()
                && extension.len() <= 8
                && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
        .map(|extension| format!(".{}", extension.to_ascii_lowercase()))
        .unwrap_or_default();
    format!(
        "wallpaper-{}-{}-{}{}",
        std::process::id(),
        now.as_secs(),
        now.subsec_nanos(),
        extension
    )
}

fn decodes(path: &Path) -> Result<(), Reject> {
    let (_, width, height) = Pixbuf::file_info(path).ok_or(Reject::NotImage)?;
    if width > CEILING || height > CEILING {
        return Err(Reject::NotImage);
    }
    Pixbuf::from_file_at_scale(path, 512, 512, true).map_err(|_| Reject::NotImage)?;
    Ok(())
}

const COPY_TIMEOUT: Duration = Duration::from_secs(30);

pub async fn copy_image(source: &Path, dir: &Path) -> Result<PathBuf, Reject> {
    bounded(COPY_TIMEOUT, copy_file(source, dir)).await
}

pub(super) async fn bounded(
    limit: Duration,
    copied: impl std::future::Future<Output = Result<PathBuf, Reject>>,
) -> Result<PathBuf, Reject> {
    tokio::time::timeout(limit, copied)
        .await
        .unwrap_or(Err(Reject::Unreadable))
}

pub(super) async fn open_regular(source: &Path) -> Result<tokio::fs::File, Reject> {
    let input = tokio::fs::OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
        .open(source)
        .await
        .map_err(|_| Reject::Unreadable)?;
    let metadata = input.metadata().await.map_err(|_| Reject::Unreadable)?;
    if !metadata.is_file() {
        return Err(Reject::Unreadable);
    }
    Ok(input)
}

async fn copy_file(source: &Path, dir: &Path) -> Result<PathBuf, Reject> {
    let input = open_regular(source).await?;
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|_| Reject::Unreadable)?;
    let temporary = tempfile::NamedTempFile::new_in(dir).map_err(|_| Reject::Unreadable)?;
    let reopened = temporary.reopen().map_err(|_| Reject::Unreadable)?;
    let mut output = tokio::fs::File::from_std(reopened);
    let copied = tokio::io::copy(&mut input.take(LIMIT + 1), &mut output)
        .await
        .map_err(|_| Reject::Unreadable)?;
    if copied > LIMIT {
        return Err(Reject::TooLarge);
    }
    output.flush().await.map_err(|_| Reject::Unreadable)?;
    drop(output);
    let checked = temporary.path().to_path_buf();
    tokio::task::spawn_blocking(move || decodes(&checked))
        .await
        .map_err(|_| Reject::Unreadable)??;
    let destination = dir.join(copy_name(source));
    temporary
        .persist(&destination)
        .map_err(|_| Reject::Unreadable)?;
    Ok(destination)
}

pub fn collect_copies(dir: &Path, keep: &[PathBuf]) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && !keep.contains(&path) {
            let _ = std::fs::remove_file(path);
        }
    }
}
