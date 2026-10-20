use std::io::{self, Read as _};
use std::path::Path;

use anyhow::{Context as _, bail};
use glimpse_shot::doc::document::{Document, Source};

use crate::adapters::windows::decode;
use crate::errors::NotFound;

pub(crate) const LARGEST_SIDE: u32 = 16384;
pub(crate) const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

pub fn load(path: &Path) -> anyhow::Result<Document> {
    let mut header = [0u8; 24];
    std::fs::File::open(path)
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => anyhow::Error::new(NotFound {
                what: format!("{} does not exist", path.display()),
                hint: None,
            }),
            _ => anyhow::Error::new(error),
        })?
        .read_exact(&mut header)
        .with_context(|| format!("cannot read {}", path.display()))?;
    let Some((width, height)) = png_size(&header) else {
        bail!("{} is not a PNG image", path.display());
    };
    if width > LARGEST_SIDE || height > LARGEST_SIDE {
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

pub(crate) fn png_size(header: &[u8]) -> Option<(u32, u32)> {
    if header.get(..8)? != PNG_SIGNATURE || header.get(12..16)? != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(header.get(16..20)?.try_into().ok()?);
    let height = u32::from_be_bytes(header.get(20..24)?.try_into().ok()?);
    Some((width, height))
}

#[cfg(test)]
mod tests;
