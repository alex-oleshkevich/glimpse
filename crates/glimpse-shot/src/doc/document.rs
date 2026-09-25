use std::path::PathBuf;

use glimpse_config::{PaletteColor, Stroke};
use glimpse_wayland::capture::Frame;

use super::units::{BufferPx, Point, Rect, Scale};

#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    Output {
        name: String,
        scale: Scale,
        frame: Frame,
    },
    Window {
        id: u64,
        app_id: Option<String>,
        title: Option<String>,
        scale: Scale,
        frame: Frame,
    },
    File {
        path: PathBuf,
        frame: Frame,
    },
}

impl Source {
    fn frame(&self) -> &Frame {
        match self {
            Source::Output { frame, .. }
            | Source::Window { frame, .. }
            | Source::File { frame, .. } => frame,
        }
    }

    fn frame_bounds(&self) -> Rect<BufferPx> {
        let frame = self.frame();
        Rect {
            x: BufferPx(0),
            y: BufferPx(0),
            width: BufferPx(frame.width as i32),
            height: BufferPx(frame.height as i32),
        }
    }
}

pub type MarkId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Tool {
    Crop,
    Arrow,
    Line,
    Freehand,
    Highlighter,
    Rect,
    FilledRect,
    Oval,
    Number,
    Text,
    Blur,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    pub color: PaletteColor,
    pub stroke: Stroke,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mark {
    Arrow {
        from: Point<BufferPx>,
        to: Point<BufferPx>,
        style: Style,
    },
    Line {
        from: Point<BufferPx>,
        to: Point<BufferPx>,
        style: Style,
    },
    Path {
        points: Vec<Point<BufferPx>>,
        style: Style,
        highlighter: bool,
    },
    Rect {
        rect: Rect<BufferPx>,
        style: Style,
        filled: bool,
    },
    Oval {
        rect: Rect<BufferPx>,
        style: Style,
    },
    Number {
        at: Point<BufferPx>,
        style: Style,
    },
    Text {
        at: Point<BufferPx>,
        text: String,
        style: Style,
    },
    Blur {
        rect: Rect<BufferPx>,
    },
    Pixelate {
        points: Vec<Point<BufferPx>>,
        width: u32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub source: Source,
    pub crop: Option<Rect<BufferPx>>,
    pub marks: Vec<(MarkId, Mark)>,
    pub selected: Option<MarkId>,
}

impl Document {
    pub fn hit(
        &self,
        at: Point<BufferPx>,
        label_bounds: impl Fn(&Mark) -> Rect<BufferPx>,
    ) -> Option<MarkId> {
        let band = self.band();
        self.marks
            .iter()
            .rev()
            .find(|(_, mark)| match mark {
                Mark::Rect {
                    rect, filled: true, ..
                } => contains(*rect, at),
                Mark::Rect { rect, style, .. } => near_rect(*rect, at, band(*style)),
                Mark::Oval { rect, style } => near_oval(*rect, at, band(*style)),
                Mark::Number { .. } | Mark::Text { .. } => contains(label_bounds(mark), at),
                Mark::Arrow { from, to, style } => near_segment(
                    *from,
                    *to,
                    at,
                    band(*style).max(5.0 * f64::from(style.stroke as u8) * self.scale()),
                ),
                Mark::Line { from, to, style } => near_segment(*from, *to, at, band(*style)),
                Mark::Path {
                    points,
                    style,
                    highlighter,
                } => {
                    let reach = if *highlighter {
                        (6.0 * f64::from(style.stroke as u8) * self.scale()).max(MIN_BAND)
                    } else {
                        band(*style)
                    };
                    points
                        .windows(2)
                        .any(|pair| near_segment(pair[0], pair[1], at, reach))
                }
                Mark::Blur { rect } => contains(*rect, at),
                Mark::Pixelate { points, width } => {
                    points
                        .windows(2)
                        .any(|pair| near_segment(pair[0], pair[1], at, f64::from(*width) / 2.0))
                        || points.len() == 1
                            && near_segment(points[0], points[0], at, f64::from(*width) / 2.0)
                }
            })
            .map(|(id, _)| *id)
    }

    pub fn scale(&self) -> f64 {
        match &self.source {
            Source::Output { scale, .. } | Source::Window { scale, .. } => scale.0,
            Source::File { .. } => 1.0,
        }
    }

    fn band(&self) -> impl Fn(Style) -> f64 {
        let scale = self.scale();
        move |style| (2.0 * f64::from(style.stroke as u8) * scale).max(MIN_BAND)
    }

    pub fn number_of(&self, id: MarkId) -> Option<u32> {
        let mut n = 0u32;
        for (mark_id, mark) in &self.marks {
            if matches!(mark, Mark::Number { .. }) {
                n += 1;
                if *mark_id == id {
                    return Some(n);
                }
            }
        }
        None
    }

    pub fn exported_rect(&self) -> Rect<BufferPx> {
        self.crop.unwrap_or_else(|| self.source.frame_bounds())
    }

    pub fn dropped(&self) -> u32 {
        let Some(crop) = self.crop else {
            return 0;
        };
        self.marks
            .iter()
            .filter(|(_, mark)| wholly_outside(mark, crop, self.scale()))
            .count() as u32
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Add(Mark),
    Remove(MarkId),
    Move(MarkId, Point<BufferPx>),
    Replace(MarkId, Mark),
    SetCrop(Option<Rect<BufferPx>>),
}

pub(crate) fn footprint(mark: &Mark) -> Rect<BufferPx> {
    match mark {
        Mark::Arrow { from, to, .. } | Mark::Line { from, to, .. } => {
            let x0 = from.x.0.min(to.x.0);
            let y0 = from.y.0.min(to.y.0);
            let x1 = from.x.0.max(to.x.0);
            let y1 = from.y.0.max(to.y.0);
            Rect {
                x: BufferPx(x0),
                y: BufferPx(y0),
                width: BufferPx(saturating_span(x0, x1)),
                height: BufferPx(saturating_span(y0, y1)),
            }
        }
        Mark::Path { points, .. } | Mark::Pixelate { points, .. } => {
            let first = points.first().copied().unwrap_or(Point {
                x: BufferPx(0),
                y: BufferPx(0),
            });
            let (mut x0, mut y0, mut x1, mut y1) = (first.x.0, first.y.0, first.x.0, first.y.0);
            for point in points {
                x0 = x0.min(point.x.0);
                y0 = y0.min(point.y.0);
                x1 = x1.max(point.x.0);
                y1 = y1.max(point.y.0);
            }
            Rect {
                x: BufferPx(x0),
                y: BufferPx(y0),
                width: BufferPx(saturating_span(x0, x1)),
                height: BufferPx(saturating_span(y0, y1)),
            }
        }
        Mark::Rect { rect, .. } | Mark::Oval { rect, .. } | Mark::Blur { rect } => *rect,
        Mark::Number { at, .. } | Mark::Text { at, .. } => Rect {
            x: at.x,
            y: at.y,
            width: BufferPx(0),
            height: BufferPx(0),
        },
    }
}

fn saturating_span(low: i32, high: i32) -> i32 {
    i32::try_from(i64::from(high) - i64::from(low)).unwrap_or(i32::MAX)
}

const MIN_BAND: f64 = 6.0;

fn near_rect(rect: Rect<BufferPx>, at: Point<BufferPx>, band: f64) -> bool {
    let (x0, y0) = (f64::from(rect.x.0), f64::from(rect.y.0));
    let (x1, y1) = (x0 + f64::from(rect.width.0), y0 + f64::from(rect.height.0));
    let (x, y) = (f64::from(at.x.0), f64::from(at.y.0));
    let outside = (x0 - x)
        .max(x - x1)
        .max(0.0)
        .hypot((y0 - y).max(y - y1).max(0.0));
    let inside = (x - x0).min(x1 - x).min(y - y0).min(y1 - y);
    match outside > 0.0 {
        true => outside <= band,
        false => inside <= band,
    }
}

fn near_oval(rect: Rect<BufferPx>, at: Point<BufferPx>, band: f64) -> bool {
    let (a, b) = (
        f64::from(rect.width.0) / 2.0,
        f64::from(rect.height.0) / 2.0,
    );
    if a < 1.0 || b < 1.0 {
        return near_rect(rect, at, band);
    }
    let dx = f64::from(at.x.0) - (f64::from(rect.x.0) + a);
    let dy = f64::from(at.y.0) - (f64::from(rect.y.0) + b);
    let radius = (dx / a).hypot(dy / b);
    if radius == 0.0 {
        return a.min(b) <= band;
    }
    let gradient = (dx / (a * a)).hypot(dy / (b * b)) / radius;
    (radius - 1.0).abs() / gradient <= band
}

pub(crate) fn near_segment(
    from: Point<BufferPx>,
    to: Point<BufferPx>,
    at: Point<BufferPx>,
    band: f64,
) -> bool {
    let (x0, y0) = (f64::from(from.x.0), f64::from(from.y.0));
    let (dx, dy) = (f64::from(to.x.0) - x0, f64::from(to.y.0) - y0);
    let (px, py) = (f64::from(at.x.0) - x0, f64::from(at.y.0) - y0);
    let length_squared = dx * dx + dy * dy;
    let position = if length_squared == 0.0 {
        0.0
    } else {
        ((px * dx + py * dy) / length_squared).clamp(0.0, 1.0)
    };
    (px - position * dx).hypot(py - position * dy) <= band
}

fn painted_footprint(mark: &Mark, scale: f64) -> Rect<BufferPx> {
    let pad = match mark {
        Mark::Arrow { style, .. } => 5.0 * f64::from(style.stroke as u8) * scale,
        Mark::Path {
            style,
            highlighter: true,
            ..
        } => 6.0 * f64::from(style.stroke as u8) * scale,
        Mark::Line { style, .. }
        | Mark::Path { style, .. }
        | Mark::Rect {
            style,
            filled: false,
            ..
        }
        | Mark::Oval { style, .. } => f64::from(style.stroke as u8) * scale,
        Mark::Pixelate { width, .. } => f64::from(*width) / 2.0,
        _ => 0.0,
    };
    inflate(footprint(mark), pad.ceil() as i32)
}

fn inflate(rect: Rect<BufferPx>, by: i32) -> Rect<BufferPx> {
    Rect {
        x: BufferPx(rect.x.0.saturating_sub(by)),
        y: BufferPx(rect.y.0.saturating_sub(by)),
        width: BufferPx(rect.width.0.saturating_add(by.saturating_mul(2))),
        height: BufferPx(rect.height.0.saturating_add(by.saturating_mul(2))),
    }
}

fn contains(rect: Rect<BufferPx>, at: Point<BufferPx>) -> bool {
    let (rx0, ry0) = (i64::from(rect.x.0), i64::from(rect.y.0));
    let (rx1, ry1) = (
        rx0 + i64::from(rect.width.0),
        ry0 + i64::from(rect.height.0),
    );
    let (ax, ay) = (i64::from(at.x.0), i64::from(at.y.0));
    ax >= rx0 && ax < rx1 && ay >= ry0 && ay < ry1
}

fn axis_overlaps(a0: i64, a1: i64, b0: i64, b1: i64) -> bool {
    if a0 == a1 {
        a0 >= b0 && a0 <= b1
    } else {
        a0 < b1 && b0 < a1
    }
}

pub(crate) fn overlaps(a: Rect<BufferPx>, b: Rect<BufferPx>) -> bool {
    let (ax0, ay0) = (i64::from(a.x.0), i64::from(a.y.0));
    let (ax1, ay1) = (ax0 + i64::from(a.width.0), ay0 + i64::from(a.height.0));
    let (bx0, by0) = (i64::from(b.x.0), i64::from(b.y.0));
    let (bx1, by1) = (bx0 + i64::from(b.width.0), by0 + i64::from(b.height.0));
    axis_overlaps(ax0, ax1, bx0, bx1) && axis_overlaps(ay0, ay1, by0, by1)
}

pub(crate) fn wholly_outside(mark: &Mark, bounds: Rect<BufferPx>, scale: f64) -> bool {
    !overlaps(painted_footprint(mark, scale), bounds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Frame {
        Frame {
            connector: "DP-2".to_owned(),
            width: 400,
            height: 300,
            pixels: glib::Bytes::from_owned(vec![0u8; 400 * 300 * 4]),
            alpha: false,
        }
    }

    fn number_at(id: MarkId, x: i32, y: i32) -> (MarkId, Mark) {
        (
            id,
            Mark::Number {
                at: Point {
                    x: BufferPx(x),
                    y: BufferPx(y),
                },
                style: Style {
                    color: PaletteColor::Red,
                    stroke: Stroke::Medium,
                },
            },
        )
    }

    fn label(mark: &Mark) -> Rect<BufferPx> {
        let (Mark::Number { at, .. } | Mark::Text { at, .. }) = mark else {
            panic!("only a label is measured");
        };
        Rect {
            x: BufferPx(at.x.0 - 10),
            y: BufferPx(at.y.0 - 10),
            width: BufferPx(20),
            height: BufferPx(20),
        }
    }

    fn outlined(marks: Vec<(MarkId, Mark)>) -> Document {
        let area = Rect {
            x: BufferPx(100),
            y: BufferPx(100),
            width: BufferPx(100),
            height: BufferPx(60),
        };
        let style = Style {
            color: PaletteColor::Red,
            stroke: Stroke::Thin,
        };
        let mut all = vec![
            (
                1,
                Mark::Rect {
                    rect: area,
                    style,
                    filled: false,
                },
            ),
            (
                2,
                Mark::Oval {
                    rect: Rect {
                        x: BufferPx(250),
                        ..area
                    },
                    style,
                },
            ),
        ];
        all.extend(marks);
        Document {
            source: Source::File {
                path: PathBuf::from("/tmp/x.png"),
                frame: frame(),
            },
            crop: None,
            marks: all,
            selected: None,
        }
    }

    fn point(x: i32, y: i32) -> Point<BufferPx> {
        Point {
            x: BufferPx(x),
            y: BufferPx(y),
        }
    }

    #[test]
    fn a_click_inside_a_shape_hits_nothing_and_so_a_number_there_stamps() {
        let doc = outlined(Vec::new());

        assert_eq!(doc.hit(point(150, 130), label), None);
        assert_eq!(doc.hit(point(300, 130), label), None);
    }

    #[test]
    fn a_filled_rect_is_hit_anywhere_inside_and_nowhere_outside() {
        let mut doc = outlined(Vec::new());
        if let Some((_, Mark::Rect { filled, .. })) = doc.marks.first_mut() {
            *filled = true;
        }

        assert_eq!(doc.hit(point(150, 130), label), Some(1));
        assert_eq!(doc.hit(point(100, 100), label), Some(1));
        assert_eq!(
            doc.hit(point(94, 130), label),
            None,
            "no stroke band outside"
        );
    }

    #[test]
    fn a_click_on_a_shape_stroke_hits_it_within_six_pixels() {
        let doc = outlined(Vec::new());

        assert_eq!(doc.hit(point(100, 130), label), Some(1));
        assert_eq!(doc.hit(point(105, 130), label), Some(1));
        assert_eq!(doc.hit(point(94, 130), label), Some(1));
        assert_eq!(doc.hit(point(92, 130), label), None);
        assert_eq!(doc.hit(point(107, 130), label), None);
        assert_eq!(doc.hit(point(250, 130), label), Some(2));
        assert_eq!(doc.hit(point(300, 104), label), Some(2));
        assert_eq!(doc.hit(point(300, 90), label), None);
    }

    #[test]
    fn a_number_over_a_shape_stroke_wins() {
        let doc = outlined(vec![number_at(3, 100, 130)]);

        assert_eq!(doc.hit(point(100, 130), label), Some(3));
        assert_eq!(doc.hit(point(108, 136), label), Some(3));
    }

    #[test]
    fn hit_finds_the_topmost_mark_at_a_point() {
        let doc = Document {
            source: Source::File {
                path: PathBuf::from("/tmp/x.png"),
                frame: frame(),
            },
            crop: None,
            marks: vec![number_at(1, 10, 0), number_at(2, 10, 0)],
            selected: None,
        };

        assert_eq!(
            doc.hit(
                Point {
                    x: BufferPx(10),
                    y: BufferPx(0)
                },
                label
            ),
            Some(2)
        );
    }

    #[test]
    fn exported_rect_is_the_crop_when_set_and_the_frame_bounds_otherwise() {
        let crop = Rect {
            x: BufferPx(0),
            y: BufferPx(0),
            width: BufferPx(10),
            height: BufferPx(10),
        };
        let mut doc = Document {
            source: Source::File {
                path: PathBuf::from("/tmp/x.png"),
                frame: frame(),
            },
            crop: Some(crop),
            marks: Vec::new(),
            selected: None,
        };
        assert_eq!(doc.exported_rect(), crop);

        doc.crop = None;
        assert_eq!(
            doc.exported_rect(),
            Rect {
                x: BufferPx(0),
                y: BufferPx(0),
                width: BufferPx(400),
                height: BufferPx(300),
            }
        );
    }

    #[test]
    fn a_zero_extent_mark_exactly_on_the_crop_edge_is_not_wholly_outside() {
        let crop = Rect {
            x: BufferPx(0),
            y: BufferPx(0),
            width: BufferPx(100),
            height: BufferPx(100),
        };
        let number_on_the_corner = Mark::Number {
            at: Point {
                x: BufferPx(0),
                y: BufferPx(0),
            },
            style: Style {
                color: PaletteColor::Red,
                stroke: Stroke::Medium,
            },
        };
        assert!(!wholly_outside(&number_on_the_corner, crop, 1.0));

        let horizontal_arrow_on_the_top_edge = Mark::Arrow {
            from: Point {
                x: BufferPx(10),
                y: BufferPx(0),
            },
            to: Point {
                x: BufferPx(50),
                y: BufferPx(0),
            },
            style: Style {
                color: PaletteColor::Red,
                stroke: Stroke::Medium,
            },
        };
        assert!(!wholly_outside(
            &horizontal_arrow_on_the_top_edge,
            crop,
            1.0
        ));
    }

    #[test]
    fn line_and_path_hit_only_near_painted_segments() {
        let style = Style {
            color: PaletteColor::Red,
            stroke: Stroke::Thin,
        };
        let mut doc = outlined(Vec::new());
        doc.marks = vec![(
            3,
            Mark::Line {
                from: point(20, 20),
                to: point(80, 80),
                style,
            },
        )];
        assert_eq!(doc.hit(point(50, 50), label), Some(3));
        assert_eq!(doc.hit(point(20, 80), label), None);

        doc.marks[0].1 = Mark::Path {
            points: vec![point(20, 20), point(80, 20), point(80, 80)],
            style,
            highlighter: false,
        };
        assert_eq!(doc.hit(point(50, 22), label), Some(3));
        assert_eq!(doc.hit(point(50, 50), label), None);
    }

    #[test]
    fn hit_does_not_overflow_near_i32_max() {
        let doc = Document {
            source: Source::File {
                path: PathBuf::from("/tmp/x.png"),
                frame: frame(),
            },
            crop: None,
            marks: vec![(
                0,
                Mark::Arrow {
                    from: Point {
                        x: BufferPx(i32::MAX - 5),
                        y: BufferPx(i32::MAX - 5),
                    },
                    to: Point {
                        x: BufferPx(i32::MAX),
                        y: BufferPx(i32::MAX),
                    },
                    style: Style {
                        color: PaletteColor::Red,
                        stroke: Stroke::Thick,
                    },
                },
            )],
            selected: None,
        };

        assert_eq!(
            doc.hit(
                Point {
                    x: BufferPx(i32::MAX - 2),
                    y: BufferPx(i32::MAX - 2),
                },
                label
            ),
            Some(0)
        );
    }
}
