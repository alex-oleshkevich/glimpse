use std::path::PathBuf;

use glimpse_config::Stroke;
use glimpse_shot::doc::units::Scale;

use super::*;

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/every_mark.png");
const BLESS: &str = "GLIMPSE_SHOT_BLESS";

fn style(color: PaletteColor) -> Style {
    Style {
        color,
        stroke: Stroke::Medium,
    }
}

fn at(x: i32, y: i32) -> Point<BufferPx> {
    Point {
        x: BufferPx(x),
        y: BufferPx(y),
    }
}

fn area(x: i32, y: i32, width: i32, height: i32) -> Rect<BufferPx> {
    Rect {
        x: BufferPx(x),
        y: BufferPx(y),
        width: BufferPx(width),
        height: BufferPx(height),
    }
}

fn document() -> Document {
    let (width, height) = (400u32, 300u32);
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let checker = (200..320).contains(&x) && (160..240).contains(&y);
            let value = match checker {
                true if (x + y) % 2 == 0 => 255,
                true => 0,
                false => 128,
            };
            let at = ((y * width + x) * 4) as usize;
            pixels[at..at + 4].copy_from_slice(&[value, value, value, 255]);
        }
    }
    let marks = vec![
        Mark::Arrow {
            from: at(40, 40),
            to: at(200, 40),
            style: style(PaletteColor::Red),
        },
        Mark::Rect {
            rect: area(220, 60, 100, 80),
            style: style(PaletteColor::Blue),
            filled: false,
        },
        Mark::Oval {
            rect: area(60, 120, 120, 80),
            style: style(PaletteColor::Green),
        },
        Mark::Number {
            at: at(340, 220),
            style: style(PaletteColor::Yellow),
        },
        Mark::Text {
            at: at(40, 220),
            text: "Hi".to_owned(),
            style: style(PaletteColor::White),
        },
        Mark::Blur {
            rect: area(240, 180, 60, 40),
        },
    ];
    Document {
        source: Source::Output {
            name: "eDP-1".to_owned(),
            scale: Scale(1.25),
            frame: Frame {
                connector: "eDP-1".to_owned(),
                width,
                height,
                pixels: glib::Bytes::from_owned(pixels),
                alpha: false,
                layout: glimpse_wayland::capture::Layout::Rgbx,
            },
        },
        crop: Some(area(20, 10, 360, 280)),
        marks: marks
            .into_iter()
            .enumerate()
            .map(|(id, mark)| (id as u32, mark))
            .collect(),
        selected: None,
    }
}

struct Decoded {
    width: u32,
    height: u32,
    stride: usize,
    rgba: glib::Bytes,
}

impl Decoded {
    fn new(png: &[u8]) -> Self {
        let texture = gdk::Texture::from_bytes(&glib::Bytes::from(png)).expect("a decodable PNG");
        let mut downloader = gdk::TextureDownloader::new(&texture);
        downloader.set_format(gdk::MemoryFormat::R8g8b8a8);
        let (rgba, stride) = downloader.download_bytes();
        Self {
            width: texture.width() as u32,
            height: texture.height() as u32,
            stride,
            rgba,
        }
    }

    fn at(&self, x: u32, y: u32) -> [u8; 3] {
        let at = y as usize * self.stride + x as usize * 4;
        [self.rgba[at], self.rgba[at + 1], self.rgba[at + 2]]
    }
}

fn near(actual: [u8; 3], expected: [u8; 3]) -> bool {
    actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 2)
}

fn corner(alpha: bool) -> Frame {
    Frame {
        connector: "eDP-1".to_owned(),
        width: 2,
        height: 1,
        pixels: glib::Bytes::from_owned(vec![0, 0, 0, 0, 200, 100, 50, 255]),
        alpha,
        layout: glimpse_wayland::capture::Layout::Rgbx,
    }
}

fn alphas(texture: &impl IsA<gdk::Texture>) -> Vec<u8> {
    let mut downloader = gdk::TextureDownloader::new(texture);
    downloader.set_format(gdk::MemoryFormat::R8g8b8a8);
    let (rgba, _) = downloader.download_bytes();
    rgba.chunks(4).map(|pixel| pixel[3]).collect()
}

#[test]
fn a_frame_with_alpha_uploads_its_transparent_pixels_as_transparent() {
    assert_eq!(alphas(&frame_texture(&corner(true))), [0, 255]);
    assert_eq!(alphas(&frame_texture(&corner(false))), [255, 255]);
}

#[test]
fn a_bgr_frame_preserves_color_and_alpha_through_png_encoding() {
    for alpha in [false, true] {
        let frame = Frame {
            layout: Layout::Bgrx,
            ..corner(alpha)
        };
        let png = frame_texture(&frame).save_to_png_bytes();
        let image = Decoded::new(&png);
        assert_eq!(image.at(1, 0), [50, 100, 200]);
        assert_eq!(image.rgba[3], if alpha { 0 } else { 255 });
    }
}

#[test]
#[ignore = "needs a display"]
fn a_transparent_window_pixel_stays_transparent_through_export() {
    gtk4::init().expect("display");
    let doc = Document {
        source: Source::Window {
            id: 42,
            app_id: None,
            title: None,
            scale: Scale(1.25),
            frame: corner(true),
        },
        crop: None,
        marks: Default::default(),
        selected: None,
    };

    let png = export(&doc, 1.0).expect("renders");

    assert_eq!(
        alphas(&gdk::Texture::from_bytes(&glib::Bytes::from(&png.bytes)).expect("a PNG")),
        [0, 255]
    );
}

#[test]
#[ignore = "needs a display"]
fn a_filled_rect_is_the_exact_palette_color_inside_and_nothing_outside() {
    gtk4::init().expect("display");
    let mut doc = document();
    doc.marks = vec![(
        0,
        Mark::Rect {
            rect: area(60, 150, 80, 60),
            style: style(PaletteColor::Blue),
            filled: true,
        },
    )];
    let image = Decoded::new(&export(&doc, 1.0).expect("renders").bytes);
    let crop = |x: u32, y: u32| (x - 20, y - 10);
    let (x, y) = crop(100, 180);
    assert_eq!(image.at(x, y), [53, 132, 228], "center");
    let (x, y) = crop(62, 152);
    assert_eq!(image.at(x, y), [53, 132, 228], "inside a corner");
    let (x, y) = crop(100, 230);
    assert_eq!(image.at(x, y), [128, 128, 128], "below it");
}

#[test]
#[ignore = "needs a display"]
fn every_mark_renders_at_the_cropped_size_and_matches_the_golden_image() {
    gtk4::init().expect("display");
    let doc = document();

    let png = export(&doc, 1.0).expect("renders");
    let image = Decoded::new(&png.bytes);
    assert_eq!((png.width, png.height), (360, 280));
    assert_eq!((image.width, image.height), (360, 280));

    let crop = |x: u32, y: u32| (x - 20, y - 10);
    let sample = |(x, y): (u32, u32)| image.at(x, y);
    assert!(
        near(sample(crop(120, 40)), [0xe0, 0x1b, 0x24]),
        "arrow shaft"
    );
    assert!(
        near(sample(crop(370, 20)), [128, 128, 128]),
        "untouched frame"
    );
    for point in [
        (245, 185),
        (246, 185),
        (279, 186),
        (245, 219),
        (278, 218),
        (260, 201),
    ] {
        assert!(
            near(sample(crop(point.0, point.1)), [127, 127, 127]),
            "the blur leaves no checker at {point:?}"
        );
    }

    let logical = export(&doc, 1.0 / 1.25).expect("renders");
    let small = Decoded::new(&logical.bytes);
    assert_eq!((logical.width, logical.height), (288, 224));
    assert_eq!((small.width, small.height), (288, 224));
    assert!(
        near(small.at(80, 24), [0xe0, 0x1b, 0x24]),
        "arrow shaft, logical"
    );

    if std::env::var_os(BLESS).is_some() {
        std::fs::create_dir_all(PathBuf::from(GOLDEN).parent().expect("a parent"))
            .expect("creates the golden directory");
        std::fs::write(GOLDEN, &png.bytes).expect("writes the golden image");
    }
    let golden = Decoded::new(
        &std::fs::read(GOLDEN)
            .unwrap_or_else(|_| panic!("{GOLDEN} is missing; rerun with {BLESS}=1")),
    );
    assert_eq!((golden.width, golden.height), (image.width, image.height));
    for (x, y) in [
        crop(120, 40),
        crop(370, 20),
        crop(245, 185),
        crop(270, 60),
        crop(60, 160),
    ] {
        assert_eq!(
            image.at(x, y),
            golden.at(x, y),
            "golden pixel at ({x}, {y})"
        );
    }
}

#[test]
#[ignore = "needs a display"]
fn pixelation_export_paints_only_the_brush_path() {
    gtk4::init().expect("display");
    let mut doc = document();
    doc.crop = None;
    doc.marks = vec![(
        0,
        Mark::Pixelate {
            points: vec![at(100, 100), at(140, 100)],
            width: 20,
        },
    )];
    let image = Decoded::new(&export(&doc, 1.0).expect("renders").bytes);
    assert!(near(image.at(100, 100), [127, 127, 127]));
    assert_eq!(image.at(100, 120), [128, 128, 128]);
    let scaled = Decoded::new(&export(&doc, 0.5).expect("renders").bytes);
    assert!(near(scaled.at(50, 50), [127, 127, 127]));
    assert_eq!(scaled.at(50, 60), [128, 128, 128]);
}

#[test]
#[ignore = "needs a display"]
fn number_digits_are_centered_by_their_ink_rect() {
    gtk4::init().expect("display");
    let label = gtk4::Label::new(None);
    for n in [1, 7, 10, 88] {
        for pixels in [12.0_f64, 19.2, 28.8, 40.0] {
            let layout = label.create_pango_layout(Some(&n.to_string()));
            let mut font = pango::FontDescription::from_string("Sans Bold");
            font.set_absolute_size(pixels * f64::from(pango::SCALE));
            layout.set_font_description(Some(&font));
            let (left, top) = ink_origin(&layout, (100.0, 100.0));
            let (ink, _) = layout.extents();
            let scale = pango::SCALE as f32;
            let cx = left + (ink.x() as f32 + ink.width() as f32 / 2.0) / scale;
            let cy = top + (ink.y() as f32 + ink.height() as f32 / 2.0) / scale;
            assert!((cx - 100.0).abs() < 0.5 && (cy - 100.0).abs() < 0.5);
        }
    }
}
