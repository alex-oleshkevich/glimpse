use std::path::Path;

use gtk4::{gdk, gdk_pixbuf::Pixbuf, prelude::*};

const LARGEST: i32 = 4096;

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

pub fn artwork(path: &Path, side: i32) -> Option<gdk::Texture> {
    let (_, width, height) = Pixbuf::file_info(path)?;
    if width > LARGEST || height > LARGEST {
        tracing::debug!(
            ?path,
            width,
            height,
            "artwork is larger than this will decode"
        );
        return None;
    }

    let cover = cover(width, height, side)?;
    let loaded = Pixbuf::from_file_at_scale(path, cover.width, cover.height, true).ok()?;
    let square = loaded.new_subpixbuf(cover.x, cover.y, cover.side, cover.side);

    Some(
        gdk::MemoryTexture::new(
            square.width(),
            square.height(),
            match square.has_alpha() {
                true => gdk::MemoryFormat::R8g8b8a8,
                false => gdk::MemoryFormat::R8g8b8,
            },
            &square.read_pixel_bytes(),
            square.rowstride().max(0) as usize,
        )
        .upcast(),
    )
}

pub type Thumbnail = (gdk::Texture, (i32, i32));

pub fn thumbnail(bytes: &[u8], side: i32) -> Option<Thumbnail> {
    use gtk4::gdk_pixbuf::PixbufLoader;
    use gtk4::prelude::PixbufLoaderExt;

    let source = std::rc::Rc::new(std::cell::Cell::new((0, 0)));
    let loader = PixbufLoader::new();
    loader.connect_size_prepared({
        let source = source.clone();
        move |loader, width, height| {
            source.set((width, height));
            if width > LARGEST || height > LARGEST {
                let factor = f64::from(side) / f64::from(width.max(height));
                loader.set_size(
                    ((f64::from(width) * factor).round() as i32).max(1),
                    ((f64::from(height) * factor).round() as i32).max(1),
                );
                return;
            }
            let Some(cover) = cover(width, height, side) else {
                return;
            };
            loader.set_size(cover.width.min(LARGEST), cover.height.min(LARGEST));
        }
    });
    if loader.write(bytes).is_err() {
        let _ = loader.close();
        return None;
    }
    loader.close().ok()?;

    let scaled = loader.pixbuf()?;
    let texture = gdk::MemoryTexture::new(
        scaled.width(),
        scaled.height(),
        match scaled.has_alpha() {
            true => gdk::MemoryFormat::R8g8b8a8,
            false => gdk::MemoryFormat::R8g8b8,
        },
        &scaled.read_pixel_bytes(),
        scaled.rowstride().max(0) as usize,
    )
    .upcast();
    Some((texture, source.get()))
}

#[cfg(test)]
mod tests;
