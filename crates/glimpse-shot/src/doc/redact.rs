use glimpse_wayland::capture::Frame;

use super::document::near_segment;
use super::units::{BufferPx, Point, Rect};

#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn redact(frame: &Frame, rect: Rect<BufferPx>) -> Tile {
    let x0 = rect.x.0.max(0);
    let y0 = rect.y.0.max(0);
    let x1 = ((i64::from(rect.x.0) + i64::from(rect.width.0))
        .min(i64::from(frame.width))
        .max(i64::from(x0))) as i32;
    let y1 = ((i64::from(rect.y.0) + i64::from(rect.height.0))
        .min(i64::from(frame.height))
        .max(i64::from(y0))) as i32;
    let width = (x1 - x0) as u32;
    let height = (y1 - y0) as u32;
    let block = block_size(rect.height.0.max(1) as u32);

    let mut rgb = vec![0f32; (width * height * 3) as usize];
    let mut by = 0u32;
    while by < height {
        let bh = block.min(height - by);
        let mut bx = 0u32;
        while bx < width {
            let bw = block.min(width - bx);
            let mut sum = [0u32; 3];
            for dy in 0..bh {
                for dx in 0..bw {
                    let pixel = frame.pixel(x0 as u32 + bx + dx, y0 as u32 + by + dy);
                    for (total, channel) in sum.iter_mut().zip(pixel) {
                        *total += u32::from(channel);
                    }
                }
            }
            let count = (bw * bh).max(1) as f32;
            for dy in 0..bh {
                for dx in 0..bw {
                    let at = (((by + dy) * width + (bx + dx)) * 3) as usize;
                    for (channel, total) in sum.iter().enumerate() {
                        rgb[at + channel] = *total as f32 / count;
                    }
                }
            }
            bx += block;
        }
        by += block;
    }

    for _ in 0..PASSES {
        box_blur(
            &mut rgb,
            width as usize,
            height as usize,
            block as usize * 2,
            true,
        );
        box_blur(
            &mut rgb,
            width as usize,
            height as usize,
            block as usize * 2,
            false,
        );
    }

    let rgba = rgb
        .chunks_exact(3)
        .flat_map(|pixel| {
            [
                pixel[0].round() as u8,
                pixel[1].round() as u8,
                pixel[2].round() as u8,
                255,
            ]
        })
        .collect();
    Tile {
        width,
        height,
        rgba,
    }
}

pub fn pixelate_stroke(
    frame: &Frame,
    points: &[Point<BufferPx>],
    width: u32,
) -> (Rect<BufferPx>, Tile) {
    const BLOCK: i32 = 12;
    let Some(first) = points.first() else {
        return (
            Rect {
                x: BufferPx(0),
                y: BufferPx(0),
                width: BufferPx(0),
                height: BufferPx(0),
            },
            Tile {
                width: 0,
                height: 0,
                rgba: Vec::new(),
            },
        );
    };
    let (mut left, mut top, mut right, mut bottom) = (first.x.0, first.y.0, first.x.0, first.y.0);
    for point in points {
        left = left.min(point.x.0);
        top = top.min(point.y.0);
        right = right.max(point.x.0);
        bottom = bottom.max(point.y.0);
    }
    let radius = (width.max(1) / 2) as i64 + 1;
    let x0 = (i64::from(left) - radius).clamp(0, i64::from(frame.width)) as i32;
    let y0 = (i64::from(top) - radius).clamp(0, i64::from(frame.height)) as i32;
    let x1 = (i64::from(right) + radius + 1).clamp(0, i64::from(frame.width)) as i32;
    let y1 = (i64::from(bottom) + radius + 1).clamp(0, i64::from(frame.height)) as i32;
    let rect = Rect {
        x: BufferPx(x0),
        y: BufferPx(y0),
        width: BufferPx(x1 - x0),
        height: BufferPx(y1 - y0),
    };
    let mut tile = Tile {
        width: (x1 - x0) as u32,
        height: (y1 - y0) as u32,
        rgba: vec![0; ((x1 - x0) as usize) * ((y1 - y0) as usize) * 4],
    };
    for by in (y0.div_euclid(BLOCK)..(y1 + BLOCK - 1).div_euclid(BLOCK)).map(|v| v * BLOCK) {
        for bx in (x0.div_euclid(BLOCK)..(x1 + BLOCK - 1).div_euclid(BLOCK)).map(|v| v * BLOCK) {
            let mut sum = [0u32; 3];
            let mut count = 0u32;
            for y in by.max(0)..(by + BLOCK).min(frame.height as i32) {
                for x in bx.max(0)..(bx + BLOCK).min(frame.width as i32) {
                    for (total, channel) in sum.iter_mut().zip(frame.pixel(x as u32, y as u32)) {
                        *total += u32::from(channel);
                    }
                    count += 1;
                }
            }
            if count == 0 {
                continue;
            }
            let color = sum.map(|total| (total / count) as u8);
            for y in by.max(y0)..(by + BLOCK).min(y1) {
                for x in bx.max(x0)..(bx + BLOCK).min(x1) {
                    let at = (((y - y0) as u32 * tile.width + (x - x0) as u32) * 4) as usize;
                    tile.rgba[at..at + 3].copy_from_slice(&color);
                }
            }
        }
    }
    let mut paint = |from: Point<BufferPx>, to: Point<BufferPx>| {
        let min_x = (i64::from(from.x.0.min(to.x.0)) - radius).max(i64::from(x0)) as i32;
        let max_x = (i64::from(from.x.0.max(to.x.0)) + radius + 1).min(i64::from(x1)) as i32;
        let min_y = (i64::from(from.y.0.min(to.y.0)) - radius).max(i64::from(y0)) as i32;
        let max_y = (i64::from(from.y.0.max(to.y.0)) + radius + 1).min(i64::from(y1)) as i32;
        for y in min_y..max_y {
            for x in min_x..max_x {
                if near_segment(
                    from,
                    to,
                    Point {
                        x: BufferPx(x),
                        y: BufferPx(y),
                    },
                    f64::from(width) / 2.0,
                ) {
                    let at = (((y - y0) as u32 * tile.width + (x - x0) as u32) * 4) as usize;
                    let source = ((y as u32 * frame.width + x as u32) * 4 + 3) as usize;
                    tile.rgba[at + 3] = if frame.alpha {
                        frame.pixels.as_ref()[source]
                    } else {
                        255
                    };
                }
            }
        }
    };
    if points.len() == 1 {
        paint(*first, *first);
    }
    for pair in points.windows(2) {
        paint(pair[0], pair[1]);
    }
    (rect, tile)
}

const PASSES: usize = 4;
const MIN_BLOCK: u32 = 8;

fn block_size(height: u32) -> u32 {
    (height / 3).max(MIN_BLOCK)
}

fn box_blur(rgb: &mut [f32], width: usize, height: usize, radius: usize, horizontal: bool) {
    let (lines, length) = match horizontal {
        true => (height, width),
        false => (width, height),
    };
    if length == 0 {
        return;
    }
    let index = |line: usize, at: usize| match horizontal {
        true => (line * width + at) * 3,
        false => (at * width + line) * 3,
    };
    let span = (2 * radius + 1) as f32;
    let mut line_values = vec![[0f32; 3]; length];
    for line in 0..lines {
        for (at, value) in line_values.iter_mut().enumerate() {
            let i = index(line, at);
            *value = [rgb[i], rgb[i + 1], rgb[i + 2]];
        }
        let clamped = |at: isize| line_values[at.clamp(0, length as isize - 1) as usize];
        let mut sum = [0f32; 3];
        for offset in -(radius as isize)..=radius as isize {
            let value = clamped(offset);
            for channel in 0..3 {
                sum[channel] += value[channel];
            }
        }
        for at in 0..length {
            let i = index(line, at);
            for channel in 0..3 {
                rgb[i + channel] = sum[channel] / span;
            }
            let leaving = clamped(at as isize - radius as isize);
            let entering = clamped(at as isize + radius as isize + 1);
            for channel in 0..3 {
                sum[channel] += entering[channel] - leaving[channel];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkerboard_frame(width: u32, height: u32) -> Frame {
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        for y in 0..height {
            for x in 0..width {
                let at = ((y * width + x) * 4) as usize;
                let value = if (x + y) % 2 == 0 { 255 } else { 0 };
                pixels[at..at + 3].copy_from_slice(&[value, value, value]);
                pixels[at + 3] = 255;
            }
        }
        Frame {
            connector: "DP-2".to_owned(),
            width,
            height,
            pixels: glib::Bytes::from_owned(pixels),
            alpha: false,
        }
    }

    #[test]
    fn a_pixelation_stroke_has_flat_blocks_and_a_round_brush_mask() {
        let frame = checkerboard_frame(64, 64);
        let points = [
            Point {
                x: BufferPx(20),
                y: BufferPx(20),
            },
            Point {
                x: BufferPx(44),
                y: BufferPx(20),
            },
        ];
        let (bounds, tile) = pixelate_stroke(&frame, &points, 12);
        let sample =
            |x: i32, y: i32| pixel_at(&tile, (x - bounds.x.0) as usize, (y - bounds.y.0) as usize);
        assert_eq!(sample(20, 20), sample(21, 20));
        assert!((120..=135).contains(&sample(20, 20)[0]));
        assert_eq!(sample(20, 20)[3], 255);
        assert_eq!(sample(20, 27)[3], 0);
        assert_eq!(sample(44, 20)[3], 255);
    }

    fn pixel_at(tile: &Tile, x: usize, y: usize) -> [u8; 4] {
        let at = (y * tile.width as usize + x) * 4;
        [
            tile.rgba[at],
            tile.rgba[at + 1],
            tile.rgba[at + 2],
            tile.rgba[at + 3],
        ]
    }

    #[test]
    fn a_blur_leaves_no_trace_of_detail_finer_than_its_block() {
        let frame = checkerboard_frame(100, 100);
        let rect = Rect {
            x: BufferPx(10),
            y: BufferPx(10),
            width: BufferPx(64),
            height: BufferPx(24),
        };
        let tile = redact(&frame, rect);

        assert_eq!((tile.width, tile.height), (64, 24));
        for y in 0..tile.height as usize {
            for x in 0..tile.width as usize {
                let [r, g, b, a] = pixel_at(&tile, x, y);
                assert!((120..=135).contains(&r), "({x},{y}) kept detail: {r}");
                assert_eq!((r, g, a), (b, b, 255));
            }
        }
    }

    #[test]
    fn a_blur_is_a_smooth_gradient_not_flat_blocks() {
        let mut frame = checkerboard_frame(64, 16);
        let mut pixels = frame.pixels.to_vec();
        for y in 0..16 {
            for x in 0..64 {
                let at = ((y * 64 + x) * 4) as usize;
                let value = if x < 32 { 0 } else { 255 };
                pixels[at..at + 3].copy_from_slice(&[value, value, value]);
            }
        }
        frame.pixels = glib::Bytes::from_owned(pixels);
        let rect = Rect {
            x: BufferPx(0),
            y: BufferPx(0),
            width: BufferPx(64),
            height: BufferPx(16),
        };
        let tile = redact(&frame, rect);

        let row: Vec<u8> = (0..64).map(|x| pixel_at(&tile, x, 8)[0]).collect();
        assert!(row.windows(2).all(|pair| pair[0] <= pair[1]), "{row:?}");
        let steps = row.windows(2).filter(|pair| pair[0] != pair[1]).count();
        assert!(steps > 16, "the edge is spread over many pixels: {row:?}");
    }

    #[test]
    fn the_tile_is_clamped_to_the_frame() {
        let frame = checkerboard_frame(20, 20);
        let rect = Rect {
            x: BufferPx(15),
            y: BufferPx(15),
            width: BufferPx(30),
            height: BufferPx(30),
        };
        let tile = redact(&frame, rect);

        assert_eq!(tile.width, 5);
        assert_eq!(tile.height, 5);
    }

    #[test]
    fn a_rect_near_i32_max_does_not_overflow() {
        let frame = checkerboard_frame(10, 10);
        let rect = Rect {
            x: BufferPx(i32::MAX - 5),
            y: BufferPx(0),
            width: BufferPx(20),
            height: BufferPx(5),
        };

        let tile = redact(&frame, rect);

        assert_eq!(tile.width, 0);
        assert_eq!(tile.height, 5);
    }
}
