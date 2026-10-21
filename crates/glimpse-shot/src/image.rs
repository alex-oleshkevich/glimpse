use std::io;
use std::path::Path;

use anyhow::bail;
use glimpse_shot::doc::document::{Document, Source};
use gtk4::gdk_pixbuf::Pixbuf;

use crate::adapters::windows::decode;
use crate::errors::NotFound;

pub(crate) const LARGEST_SIDE: u32 = 16384;

pub fn load(path: &Path) -> anyhow::Result<Document> {
    std::fs::metadata(path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => anyhow::Error::new(NotFound {
            what: format!("{} does not exist", path.display()),
            hint: None,
        }),
        _ => anyhow::Error::new(error),
    })?;
    let Some((_, width, height)) = Pixbuf::file_info(path) else {
        bail!("{} is not a supported image", path.display());
    };
    if width.max(height) > LARGEST_SIDE as i32 {
        bail!(
            "{} is {width} × {height}; the largest side opened is {LARGEST_SIDE} pixels",
            path.display()
        );
    }
    let frame = decode(path).map_err(anyhow::Error::new)?;
    Ok(Document {
        source: Source::File {
            path: path.canonicalize()?,
            frame,
        },
        crop: None,
        marks: Vec::new(),
        selected: None,
    })
}

#[cfg(test)]
mod tests;
