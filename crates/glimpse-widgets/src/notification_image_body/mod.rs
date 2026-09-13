mod imp;

use std::path::Path;

use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

const IMAGE_SIDE: i32 = 64;
const IMAGE_LARGEST: i32 = 4096;

glib::wrapper! {
    pub struct NotificationImageBody(ObjectSubclass<imp::NotificationImageBody>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for NotificationImageBody {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationImageBody {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_image(&self, image: Option<&gdk::Texture>) -> bool {
        let imp = self.imp();
        if imp.image.borrow().as_ref() == image {
            return imp.picture.paintable().is_some();
        }
        imp.image.replace(image.cloned());
        let bounded = image.and_then(|image| bound(image, IMAGE_SIDE, IMAGE_SIDE));
        let paintable = bounded
            .as_ref()
            .map(|texture| texture.upcast_ref::<gdk::Paintable>());
        imp.picture.set_paintable(paintable);
        self.set_visible(paintable.is_some());
        paintable.is_some()
    }
}

pub fn notification_image(path: &Path) -> Option<gdk::Texture> {
    crate::artwork(path, IMAGE_SIDE)
}

pub(crate) fn bound(
    texture: &gdk::Texture,
    max_width: i32,
    max_height: i32,
) -> Option<gdk::Texture> {
    let (width, height) = (texture.width(), texture.height());
    if width <= 0 || height <= 0 || width > IMAGE_LARGEST || height > IMAGE_LARGEST {
        return None;
    }
    if width <= max_width && height <= max_height {
        return Some(texture.clone());
    }

    let factor =
        (f64::from(max_width) / f64::from(width)).min(f64::from(max_height) / f64::from(height));
    let target_width = (f64::from(width) * factor).round().max(1.0) as i32;
    let target_height = (f64::from(height) * factor).round().max(1.0) as i32;
    let (source_stride, target_stride) = (width as usize * 4, target_width as usize * 4);
    let mut source = vec![0u8; source_stride * height as usize];
    texture.download(&mut source, source_stride);
    let mut target = vec![0u8; target_stride * target_height as usize];
    for row in 0..target_height as usize {
        let (from_y, to_y) = span(row, target_height as usize, height as usize);
        for column in 0..target_width as usize {
            let (from_x, to_x) = span(column, target_width as usize, width as usize);
            let mut totals = [0u32; 4];
            let mut counted = 0u32;
            for y in from_y..to_y {
                for x in from_x..to_x {
                    let at = y * source_stride + x * 4;
                    for (channel, total) in totals.iter_mut().enumerate() {
                        *total += u32::from(source[at + channel]);
                    }
                    counted += 1;
                }
            }
            let at = row * target_stride + column * 4;
            for (channel, total) in totals.iter().enumerate() {
                target[at + channel] = (total / counted.max(1)) as u8;
            }
        }
    }
    Some(
        gdk::MemoryTexture::new(
            target_width,
            target_height,
            gdk::MemoryFormat::B8g8r8a8Premultiplied,
            &glib::Bytes::from_owned(target),
            target_stride,
        )
        .upcast(),
    )
}

fn span(at: usize, out_of: usize, source: usize) -> (usize, usize) {
    let from = at * source / out_of;
    let to = ((at + 1) * source / out_of).max(from + 1).min(source);
    (from, to)
}
