use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::SystemTime;

use gtk4::{gdk, gdk_pixbuf::Pixbuf, prelude::*};

/// The largest source this will decode, per side — twenty times the slot, which no real cover
/// approaches. `mpris:artUrl` is chosen by another application, so the bound on how much work one
/// player can ask for is the header check rather than anything downstream of it.
const LARGEST: i32 = 4096;

const STRICT_LARGEST: i32 = 2048;

const STRICT_MAX_BYTES: u64 = 8 << 20;

const ANY_MAX_BYTES: u64 = 64 << 20;

const CHUNK: usize = 16 << 10;

/// How a source of `(width, height)` is made to fill a `side × side` slot: scale so the shorter
/// side reaches `side`, then take the middle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cover {
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    side: i32,
}

fn cover(width: i32, height: i32, side: i32) -> Option<Cover> {
    if width <= 0 || height <= 0 || side <= 0 {
        return None;
    }

    let shorter = width.min(height);
    let (scaled_width, scaled_height) = match shorter > side {
        true => {
            let factor = f64::from(side) / f64::from(shorter);
            (
                ((f64::from(width) * factor).round() as i32).max(1),
                ((f64::from(height) * factor).round() as i32).max(1),
            )
        }
        false => (width, height),
    };

    let square = scaled_width.min(scaled_height);
    Some(Cover {
        width: scaled_width,
        height: scaled_height,
        x: (scaled_width - square) / 2,
        y: (scaled_height - square) / 2,
        side: square,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Formats {
    Strict,
    Any,
}

impl Formats {
    fn max_bytes(self) -> u64 {
        match self {
            Self::Strict => STRICT_MAX_BYTES,
            Self::Any => ANY_MAX_BYTES,
        }
    }

    fn largest(self) -> i32 {
        match self {
            Self::Strict => STRICT_LARGEST,
            Self::Any => LARGEST,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Artwork {
    Unchanged,
    Decoded(gdk::Texture, Stamp),
    Rejected(Option<Stamp>),
}

enum Format {
    Png,
    Jpeg,
    Webp,
}

impl Format {
    fn of(bytes: &[u8]) -> Option<Self> {
        match bytes {
            [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, ..] => Some(Self::Png),
            [0xff, 0xd8, 0xff, ..] => Some(Self::Jpeg),
            [
                b'R',
                b'I',
                b'F',
                b'F',
                _,
                _,
                _,
                _,
                b'W',
                b'E',
                b'B',
                b'P',
                ..,
            ] => Some(Self::Webp),
            _ => None,
        }
    }

    fn loader(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Webp => "webp",
        }
    }
}

enum Opened {
    Unchanged,
    Ready(std::fs::File, Stamp),
}

fn open(path: &Path, known: Option<&Stamp>, max_bytes: u64) -> Result<Opened, Option<Stamp>> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(path)
        .map_err(|_| None)?;
    let metadata = file.metadata().map_err(|_| None)?;
    if !metadata.is_file() {
        return Err(None);
    }
    let stamp = Stamp {
        modified: metadata.modified().ok(),
        len: metadata.len(),
    };
    if known == Some(&stamp) {
        return Ok(Opened::Unchanged);
    }
    if stamp.len > max_bytes {
        return Err(Some(stamp));
    }
    Ok(Opened::Ready(file, stamp))
}

/// A square texture for an image on disk, cropped from the middle so it fills its slot whatever
/// shape it arrived in.
///
/// `Gtk.Image` centres a paintable at the paintable's own aspect ratio, so a 16:9 thumbnail handed
/// over as-is leaves the rounded corners of the art slot showing the surface behind it. Handing it
/// something already square is what keeps that widget's measured sizing behaviour intact.
pub fn artwork(path: &Path, side: i32, formats: Formats) -> Option<gdk::Texture> {
    match cached_artwork(path, side, formats, None) {
        Artwork::Decoded(texture, _) => Some(texture),
        Artwork::Unchanged | Artwork::Rejected(_) => None,
    }
}

pub fn cached_artwork(path: &Path, side: i32, formats: Formats, known: Option<&Stamp>) -> Artwork {
    let (file, stamp) = match open(path, known, formats.max_bytes()) {
        Ok(Opened::Ready(file, stamp)) => (file, stamp),
        Ok(Opened::Unchanged) => return Artwork::Unchanged,
        Err(stamp) => return unchanged_or_rejected(stamp, known),
    };
    match decode(file, side, formats) {
        Some(texture) => Artwork::Decoded(texture, stamp),
        None => Artwork::Rejected(Some(stamp)),
    }
}

fn unchanged_or_rejected(stamp: Option<Stamp>, known: Option<&Stamp>) -> Artwork {
    match (stamp, known) {
        (Some(stamp), Some(known)) if stamp == *known => Artwork::Unchanged,
        (stamp, _) => Artwork::Rejected(stamp),
    }
}

fn decode(mut file: std::fs::File, side: i32, formats: Formats) -> Option<gdk::Texture> {
    let max_bytes = formats.max_bytes();
    let mut chunk = vec![0u8; CHUNK];
    let first = read_chunk(&mut file, &mut chunk)?;
    let loader = match formats {
        Formats::Strict => Some(Format::of(&chunk[..first])?.loader()),
        Formats::Any => None,
    };
    let decoder = Decoder::new(side, loader, Some(formats.largest()))?;
    let mut total = first as u64;
    let mut read = first;
    while read > 0 {
        if total > max_bytes || !decoder.write(&chunk[..read]) {
            decoder.abandon();
            return None;
        }
        read = read_chunk(&mut file, &mut chunk)?;
        total += read as u64;
    }
    let (loaded, source) = decoder.finish()?;
    let cover = cover(source.0, source.1, side)?;
    if loaded.width() < cover.x + cover.side || loaded.height() < cover.y + cover.side {
        return None;
    }
    let square = loaded.new_subpixbuf(cover.x, cover.y, cover.side, cover.side);
    Some(texture_of(&square))
}

fn read_chunk(file: &mut std::fs::File, chunk: &mut [u8]) -> Option<usize> {
    loop {
        match file.read(chunk) {
            Ok(read) => return Some(read),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
}

fn texture_of(pixbuf: &Pixbuf) -> gdk::Texture {
    gdk::MemoryTexture::new(
        pixbuf.width(),
        pixbuf.height(),
        match pixbuf.has_alpha() {
            true => gdk::MemoryFormat::R8g8b8a8,
            false => gdk::MemoryFormat::R8g8b8,
        },
        &pixbuf.read_pixel_bytes(),
        pixbuf.rowstride().max(0) as usize,
    )
    .upcast()
}

struct Decoder {
    closed: std::cell::Cell<bool>,
    loader: gtk4::gdk_pixbuf::PixbufLoader,
    source: std::rc::Rc<std::cell::Cell<(i32, i32)>>,
    reject_over: Option<i32>,
}

impl Decoder {
    fn new(side: i32, format: Option<&str>, reject_over: Option<i32>) -> Option<Self> {
        use gtk4::gdk_pixbuf::PixbufLoader;
        use gtk4::prelude::PixbufLoaderExt;

        let limit = reject_over.unwrap_or(LARGEST);
        let source = std::rc::Rc::new(std::cell::Cell::new((0, 0)));
        let loader = match format {
            Some(format) => PixbufLoader::with_type(format).ok()?,
            None => PixbufLoader::new(),
        };
        loader.connect_size_prepared({
            let source = source.clone();
            move |loader, width, height| {
                source.set((width, height));
                let oversize = width > limit || height > limit;
                let target = match (oversize, reject_over) {
                    (true, Some(_)) => Some((1, 1)),
                    (true, None) => {
                        let factor = f64::from(side) / f64::from(width.max(height));
                        Some((
                            ((f64::from(width) * factor).round() as i32).max(1),
                            ((f64::from(height) * factor).round() as i32).max(1),
                        ))
                    }
                    (false, _) => cover(width, height, side)
                        .map(|c| (c.width.min(limit), c.height.min(limit))),
                };
                if let Some((width, height)) = target {
                    loader.set_size(width, height);
                }
            }
        });
        Some(Self {
            closed: std::cell::Cell::new(false),
            loader,
            source,
            reject_over,
        })
    }

    fn oversize(&self) -> bool {
        let (width, height) = self.source.get();
        self.reject_over
            .is_some_and(|limit| width > limit || height > limit)
    }

    fn write(&self, bytes: &[u8]) -> bool {
        use gtk4::prelude::PixbufLoaderExt;

        self.loader.write(bytes).is_ok() && !self.oversize()
    }

    fn abandon(&self) {
        use gtk4::prelude::PixbufLoaderExt;

        if !self.closed.replace(true) {
            let _ = self.loader.close();
        }
    }

    fn finish(self) -> Option<(Pixbuf, (i32, i32))> {
        use gtk4::prelude::PixbufLoaderExt;

        self.closed.set(true);
        self.loader.close().ok()?;
        if self.oversize() {
            return None;
        }
        Some((self.loader.pixbuf()?, self.source.get()))
    }
}

/// A bounded thumbnail for image bytes another application put on the clipboard.
///
/// `gdk::Texture::from_bytes` decodes at full size, and a small compressed image can decode to an
/// enormous bitmap — the byte cap the clipboard applies says nothing about the pixel count. The
/// loader is asked for its dimensions first and told to scale during decode, so the work and the
/// memory are bounded on **both** axes rather than by whatever the sender chose.
pub type Thumbnail = (gdk::Texture, (i32, i32));

pub fn thumbnail(bytes: &[u8], side: i32) -> Option<Thumbnail> {
    let decoder = Decoder::new(side, None, None)?;
    if !decoder.write(bytes) {
        decoder.abandon();
        return None;
    }
    let (scaled, source) = decoder.finish()?;
    Some((texture_of(&scaled), source))
}

impl Drop for Decoder {
    fn drop(&mut self) {
        self.abandon();
    }
}

#[cfg(test)]
mod tests;
