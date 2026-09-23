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
    Some(dirs::runtime_dir()?.join("glimpse/art").join(digest(url)))
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

    let body = response.bytes().await.map_err(transport)?;
    if body.len() as u64 > cap {
        return over();
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

fn keep(path: PathBuf) -> Result<String, String> {
    path.into_os_string()
        .into_string()
        .map_err(|_| "a cache path that is not UTF-8".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_url_becomes_the_path_it_names() {
        assert_eq!(
            classify(Some("file:///music/cover.png")),
            Some(Art::Local("/music/cover.png".to_owned()))
        );
    }

    /// A track whose folder has a space in it is ordinary, and the URL that names it is
    /// percent-encoded. Opening the undecoded form fails and the artwork silently disappears.
    #[test]
    fn a_file_url_is_percent_decoded() {
        assert_eq!(
            classify(Some("file:///music/My%20Album/cover.png")),
            Some(Art::Local("/music/My Album/cover.png".to_owned()))
        );
    }

    #[test]
    fn a_bare_absolute_path_is_taken_as_one() {
        assert_eq!(
            classify(Some("/music/cover.png")),
            Some(Art::Local("/music/cover.png".to_owned()))
        );
    }

    #[test]
    fn http_and_https_are_the_only_schemes_that_are_fetched() {
        assert_eq!(
            classify(Some("https://example.test/a.png")),
            Some(Art::Remote("https://example.test/a.png".to_owned()))
        );
        assert!(matches!(
            classify(Some("http://example.test/a.png")),
            Some(Art::Remote(_))
        ));
    }

    /// The URL is chosen by another application, so a scheme this daemon would not go and read has
    /// to come out as nothing rather than be handed to a fetcher.
    #[test]
    fn any_other_scheme_is_refused() {
        for refused in [
            "javascript:alert(1)",
            "data:image/png;base64,AAAA",
            "ftp://example.test/a.png",
            "mailto:someone@example.test",
        ] {
            assert_eq!(classify(Some(refused)), None, "{refused} was accepted");
        }
    }

    /// A `file://` URL naming anything at all still resolves to that path, traversal included —
    /// which is correct rather than a hole. A player runs as the same user and can name
    /// `/etc/shadow` outright without a `..` in sight, so refusing traversal would buy nothing.
    /// What bounds this is that the path is only ever decoded as an image, under a size cap.
    #[test]
    fn a_file_url_is_taken_at_its_word() {
        assert_eq!(
            classify(Some("file:///../../etc/shadow")),
            Some(Art::Local("/etc/shadow".to_owned()))
        );
    }

    #[test]
    fn nothing_at_all_is_nothing() {
        assert_eq!(classify(None), None);
        assert_eq!(classify(Some("")), None);
        assert_eq!(classify(Some("   ")), None);
        assert_eq!(classify(Some("not a url")), None);
    }

    /// Asserted on the digest rather than on `cached`, which answers `None` where
    /// `XDG_RUNTIME_DIR` is unset — a test written against it passes having checked nothing in
    /// exactly the environment least like the one it is meant to cover.
    #[test]
    fn one_url_always_hashes_to_one_name() {
        assert_eq!(
            digest("https://example.test/a.png"),
            digest("https://example.test/a.png")
        );
        assert_ne!(
            digest("https://example.test/a.png"),
            digest("https://example.test/b.png")
        );
        assert_eq!(digest("https://example.test/a.png").len(), 16);
    }
}
