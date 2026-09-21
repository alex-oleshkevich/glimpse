use std::path::Path;

use gtk4::{gdk, gdk_pixbuf::Pixbuf, prelude::*};

/// The largest source this will decode, per side — twenty times the slot, which no real cover
/// approaches. `mpris:artUrl` is chosen by another application, and the decode runs on the GTK main
/// loop, so the bound on how much work one player can ask for is the header check rather than
/// anything downstream of it.
const LARGEST: i32 = 4096;

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

/// A square texture for an image on disk, cropped from the middle so it fills its slot whatever
/// shape it arrived in.
///
/// `Gtk.Image` centres a paintable at the paintable's own aspect ratio, so a 16:9 thumbnail handed
/// over as-is leaves the rounded corners of the art slot showing the surface behind it. Handing it
/// something already square is what keeps that widget's measured sizing behaviour intact.
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

/// A bounded thumbnail for image bytes another application put on the clipboard.
///
/// `gdk::Texture::from_bytes` decodes at full size, and a small compressed image can decode to an
/// enormous bitmap — the byte cap the clipboard applies says nothing about the pixel count. The
/// loader is asked for its dimensions first and told to scale during decode, so the work and the
/// memory are bounded on **both** axes rather than by whatever the sender chose.
pub fn thumbnail(bytes: &[u8], side: i32) -> Option<gdk::Texture> {
    use gtk4::gdk_pixbuf::PixbufLoader;
    use gtk4::prelude::PixbufLoaderExt;

    let loader = PixbufLoader::new();
    loader.connect_size_prepared(move |loader, width, height| {
        // `cover` scales by the SHORTER side, so a 65535x48 image passes through it untouched and
        // decodes to megabytes. Both axes are clamped here, which is the bound the caller is told
        // it has — the sender chose these dimensions and is not to be trusted with them.
        if width > LARGEST || height > LARGEST {
            loader.set_size(side.min(width), side.min(height));
            return;
        }
        let Some(cover) = cover(width, height, side) else {
            return;
        };
        loader.set_size(cover.width.min(LARGEST), cover.height.min(LARGEST));
    });
    if loader.write(bytes).is_err() {
        let _ = loader.close();
        return None;
    }
    loader.close().ok()?;

    let scaled = loader.pixbuf()?;
    Some(
        gdk::MemoryTexture::new(
            scaled.width(),
            scaled.height(),
            match scaled.has_alpha() {
                true => gdk::MemoryFormat::R8g8b8a8,
                false => gdk::MemoryFormat::R8g8b8,
            },
            &scaled.read_pixel_bytes(),
            scaled.rowstride().max(0) as usize,
        )
        .upcast(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: i32, height: i32) -> Vec<u8> {
        let source = Pixbuf::new(gtk4::gdk_pixbuf::Colorspace::Rgb, false, 8, width, height)
            .expect("a pixbuf");
        source.fill(0x00ff00ff);
        source
            .save_to_bufferv("png", &[])
            .expect("a png buffer")
            .to_vec()
    }

    /// The clipboard caps an entry's *bytes*, which says nothing about its pixel count: a small
    /// compressed image can decode to an enormous bitmap. The decode has to be bounded too.
    #[test]
    fn a_large_image_is_scaled_during_decode_rather_than_after() {
        let texture = thumbnail(&png(4000, 3000), 48).expect("a thumbnail");

        assert!(texture.width() <= 64, "got {}", texture.width());
        assert!(texture.height() <= 64, "got {}", texture.height());
    }

    #[test]
    fn a_small_image_is_not_enlarged() {
        let texture = thumbnail(&png(16, 16), 48).expect("a thumbnail");

        assert_eq!((texture.width(), texture.height()), (16, 16));
    }

    /// Another application chose these bytes. A picture that will not decode is not an error path:
    /// the entry stays restorable and the row falls back to its icon.
    #[test]
    fn bytes_that_are_not_an_image_yield_nothing_rather_than_panicking() {
        assert!(thumbnail(b"definitely not a png", 48).is_none());
        assert!(thumbnail(&[], 48).is_none());
    }

    #[test]
    fn a_square_source_larger_than_the_slot_is_scaled_and_not_cropped() {
        let cover = cover(1000, 1000, 100).expect("a cover");

        assert_eq!((cover.width, cover.height), (100, 100));
        assert_eq!((cover.x, cover.y, cover.side), (0, 0, 100));
    }

    /// A video thumbnail is the common case, and the whole reason this exists: letterboxed inside
    /// a square slot it leaves the corners showing the popover behind it.
    #[test]
    fn a_wide_source_is_scaled_by_its_shorter_side_and_cropped_in_the_middle() {
        let cover = cover(1920, 1080, 100).expect("a cover");

        assert_eq!(
            cover.height, 100,
            "the shorter side is what reaches the slot"
        );
        assert_eq!(cover.width, 178);
        assert_eq!(cover.side, 100);
        assert_eq!(cover.x, 39, "the crop is centred, not taken from the left");
        assert_eq!(cover.y, 0);
        assert_eq!(
            cover.width - (cover.x + cover.side),
            cover.x,
            "the margin dropped on the left and on the right must be the same"
        );
    }

    #[test]
    fn a_tall_source_crops_vertically_instead() {
        let cover = cover(1080, 1920, 100).expect("a cover");

        assert_eq!(cover.width, 100);
        assert_eq!(cover.side, 100);
        assert_eq!(cover.x, 0);
        assert_eq!(cover.y, 39);
    }

    /// Enlarging costs memory and buys nothing: `Gtk.Image` scales whatever it is given up to the
    /// slot anyway, and it looks no better for having been enlarged twice.
    #[test]
    fn a_source_smaller_than_the_slot_is_left_at_its_own_size() {
        let cover = cover(64, 64, 192).expect("a cover");

        assert_eq!((cover.width, cover.height), (64, 64));
        assert_eq!(cover.side, 64);
    }

    #[test]
    fn a_small_wide_source_is_still_cropped_square_without_being_enlarged() {
        let cover = cover(160, 90, 192).expect("a cover");

        assert_eq!((cover.width, cover.height), (160, 90));
        assert_eq!(cover.side, 90);
        assert_eq!(cover.x, 35);
    }

    #[test]
    fn a_source_with_no_area_yields_nothing_rather_than_a_division() {
        assert_eq!(cover(0, 100, 64), None);
        assert_eq!(cover(100, 0, 64), None);
        assert_eq!(cover(-1, 100, 64), None);
        assert_eq!(cover(100, 100, 0), None);
    }

    #[test]
    fn the_crop_never_reaches_outside_the_scaled_image() {
        for (width, height) in [(1920, 1080), (1080, 1920), (3000, 2999), (7, 4000), (1, 1)] {
            let cover = cover(width, height, 100).expect("a cover");
            assert!(cover.x >= 0 && cover.y >= 0);
            assert!(cover.x + cover.side <= cover.width, "{width}x{height}");
            assert!(cover.y + cover.side <= cover.height, "{width}x{height}");
        }
    }
}
