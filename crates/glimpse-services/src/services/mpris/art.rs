use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash as _, Hasher as _};
use std::path::PathBuf;

use url::Url;

use super::{Event, say, transport};

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Art {
    Local(String),
    Remote(String),
}

pub fn classify(raw: Option<&str>) -> Option<Art> {
    let raw = raw.map(str::trim).filter(|text| !text.is_empty())?;

    let Ok(parsed) = Url::parse(raw) else {
        return raw.starts_with('/').then(|| Art::Local(raw.to_owned()));
    };

    match parsed.scheme() {
        "file" => Some(Art::Local(parsed.to_file_path().ok()?.to_str()?.to_owned())),
        "http" | "https" => Some(Art::Remote(parsed.into())),
        _ => None,
    }
}

fn digest(url: &str) -> String {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub fn cached(url: &str) -> Option<PathBuf> {
    Some(glimpse_config::runtime_dir()?.join("art").join(digest(url)))
}

pub async fn fetch(client: reqwest::Client, url: String, max_kib: u32) -> Event {
    let path = match download(&client, &url, max_kib).await {
        Ok(path) => Some(path),
        Err(reason) => {
            tracing::debug!(%url, reason, "artwork was not fetched");
            None
        }
    };
    Event::Art { url, path }
}

async fn download(client: &reqwest::Client, url: &str, max_kib: u32) -> Result<String, String> {
    let target = cached(url).ok_or("XDG_RUNTIME_DIR is not set")?;
    let cap = u64::from(max_kib) * 1024;
    let over = || Err(format!("larger than the {max_kib} KiB cap"));

    if let Ok(held) = tokio::fs::metadata(&target).await {
        return match held.len() > cap {
            true => over(),
            false => keep(target),
        };
    }

    let response = client
        .get(url)
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(transport)?
        .error_for_status()
        .map_err(transport)?;

    if response.content_length().is_some_and(|length| length > cap) {
        return over();
    }

    let mut response = response;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport)? {
        if !append_capped(&mut body, &chunk, cap) {
            return over();
        }
    }

    let parent = target.parent().ok_or("no cache directory")?;
    tokio::fs::create_dir_all(parent).await.map_err(say)?;

    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("a cache path with no file name")?;
    let tmp = parent.join(format!("{name}.tmp-{}", std::process::id()));
    tokio::fs::write(&tmp, &body).await.map_err(say)?;
    if let Err(error) = tokio::fs::rename(&tmp, &target).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(say(error));
    }

    keep(target)
}

fn append_capped(body: &mut Vec<u8>, chunk: &[u8], cap: u64) -> bool {
    let Some(next) = (body.len() as u64).checked_add(chunk.len() as u64) else {
        return false;
    };
    if next > cap {
        return false;
    }
    body.extend_from_slice(chunk);
    true
}

fn keep(path: PathBuf) -> Result<String, String> {
    path.into_os_string()
        .into_string()
        .map_err(|_| "a cache path that is not UTF-8".to_owned())
}

#[cfg(test)]
mod tests;
