use std::io::{self, Write};

use anyhow::{Context, Result};
use glimpse_dbus::clipboard::{Clipboard1Proxy, ClipboardItem};
use zbus::Connection;

use super::{emit, proxy, within};
use crate::render::{Section, Table};

const PREVIEW: usize = 60;

pub async fn clipboard_list(connection: &Connection, json: bool) -> Result<()> {
    let items = within(proxy::<Clipboard1Proxy>(connection).await?.list())
        .await
        .context("cannot read the clipboard history")?;

    if json {
        return emit(&items);
    }

    Section::new("Clipboard")
        .with(
            Table::new()
                .with_headers(["ID", "KIND", "PIN", "PREVIEW"])
                .with_empty("the history is empty")
                .with_rows(items.iter().map(row))
                .render(),
        )
        .print()
}

pub async fn clipboard_get(connection: &Connection, id: u64) -> Result<()> {
    let (_, bytes) = within(proxy::<Clipboard1Proxy>(connection).await?.get(id))
        .await
        .with_context(|| format!("cannot read clipboard entry {id}"))?;
    let mut stdout = io::stdout().lock();
    match stdout.write_all(&bytes).and_then(|()| stdout.flush()) {
        Err(error) if error.kind() != io::ErrorKind::BrokenPipe => Err(error.into()),
        _ => Ok(()),
    }
}

pub async fn clipboard_copy(connection: &Connection, id: u64) -> Result<()> {
    within(proxy::<Clipboard1Proxy>(connection).await?.copy(id))
        .await
        .with_context(|| format!("cannot copy clipboard entry {id}"))
}

pub async fn clipboard_pin(connection: &Connection, id: u64, pinned: bool) -> Result<()> {
    within(proxy::<Clipboard1Proxy>(connection).await?.pin(id, pinned))
        .await
        .with_context(|| match pinned {
            true => format!("cannot pin clipboard entry {id}"),
            false => format!("cannot unpin clipboard entry {id}"),
        })
}

pub async fn clipboard_forget(connection: &Connection, id: u64) -> Result<()> {
    within(proxy::<Clipboard1Proxy>(connection).await?.forget(id))
        .await
        .with_context(|| format!("cannot forget clipboard entry {id}"))
}

pub async fn clipboard_clear(connection: &Connection) -> Result<()> {
    within(proxy::<Clipboard1Proxy>(connection).await?.clear())
        .await
        .context("cannot clear the clipboard history")
}

fn row(item: &ClipboardItem) -> [String; 4] {
    [
        item.id.to_string(),
        glimpse_utils::clean(&item.kind, PREVIEW),
        match item.pinned {
            true => "★".to_owned(),
            false => String::new(),
        },
        preview(item),
    ]
}

fn preview(item: &ClipboardItem) -> String {
    match item.kind.as_str() {
        "text" => glimpse_utils::clean(&item.preview, PREVIEW),
        _ => format!(
            "{} · {}",
            glimpse_utils::clean(&item.mime, PREVIEW),
            size(item.size)
        ),
    }
}

fn size(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{} KiB", bytes / 1024),
        _ => format!("{:.1} MiB", bytes as f64 / 1_048_576.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: &str, preview: &str, size: u64) -> ClipboardItem {
        ClipboardItem {
            id: 7,
            kind: kind.to_owned(),
            mime: "image/png".to_owned(),
            pinned: true,
            copied_at_secs: 0,
            size,
            preview: preview.to_owned(),
        }
    }

    #[test]
    fn an_image_reads_as_its_mime_and_size_rather_than_its_preview() {
        assert_eq!(
            preview(&item("image", "ignored", 188_416)),
            "image/png · 184 KiB"
        );
    }

    #[test]
    fn a_text_preview_is_cleaned_and_capped() {
        let long = "a\u{1b}[31m".to_owned() + &"x".repeat(200);
        let shown = preview(&item("text", &long, 0));
        assert!(!shown.contains('\u{1b}'));
        assert!(shown.chars().count() <= PREVIEW + 1);
    }

    #[test]
    fn sizes_step_through_their_units() {
        assert_eq!(size(512), "512 B");
        assert_eq!(size(2048), "2 KiB");
        assert_eq!(size(3 * 1_048_576), "3.0 MiB");
    }
}
