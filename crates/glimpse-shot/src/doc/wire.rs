use std::path::PathBuf;

use glimpse_config::{PaletteColor, Stroke};
use serde::{Deserialize, Serialize};

use super::document::{Document, Mark, Style};
use super::units::{BufferPx, Point, Rect};

pub const MAX_MARKS: usize = 256;
pub const MAX_TEXT_CHARS: usize = 1024;
pub const MAX_PATH_POINTS: usize = 4096;

fn bounded_points<'de, D>(deserializer: D) -> Result<Vec<[i32; 2]>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Points;

    impl<'de> serde::de::Visitor<'de> for Points {
        type Value = Vec<[i32; 2]>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("at most 4096 coordinate pairs")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            if seq.size_hint().is_some_and(|size| size > MAX_PATH_POINTS) {
                return Err(serde::de::Error::custom("path exceeds 4096 points"));
            }
            let mut points = Vec::new();
            while let Some(point) = seq.next_element::<[i32; 2]>()? {
                if points.len() == MAX_PATH_POINTS {
                    return Err(serde::de::Error::custom("path exceeds 4096 points"));
                }
                points.push(point);
            }
            Ok(points)
        }
    }

    deserializer.deserialize_seq(Points)
}

fn default_number_stroke() -> u8 {
    2
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResultV1 {
    pub version: u32,
    pub path: Option<PathBuf>,
    pub copied: bool,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
    pub output: Option<String>,
    pub target: TargetV1,
    pub crop: Option<RectV1>,
    pub dropped: u32,
    pub annotations: Vec<AnnotationV1>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TargetV1 {
    Screen {
        name: String,
    },
    Window {
        id: u64,
        app_id: Option<String>,
        title: Option<String>,
    },
    Region {
        name: String,
    },
    File {
        path: PathBuf,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnnotationV1 {
    Arrow {
        from: [i32; 2],
        to: [i32; 2],
        color: PaletteColor,
        stroke: u8,
    },
    Line {
        from: [i32; 2],
        to: [i32; 2],
        color: PaletteColor,
        stroke: u8,
    },
    Freehand {
        #[serde(deserialize_with = "bounded_points")]
        points: Vec<[i32; 2]>,
        color: PaletteColor,
        stroke: u8,
    },
    Highlighter {
        #[serde(deserialize_with = "bounded_points")]
        points: Vec<[i32; 2]>,
        color: PaletteColor,
        stroke: u8,
    },
    Rect {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        color: PaletteColor,
        stroke: u8,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        filled: bool,
    },
    Oval {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        color: PaletteColor,
        stroke: u8,
    },
    Number {
        x: i32,
        y: i32,
        color: PaletteColor,
        #[serde(default = "default_number_stroke")]
        stroke: u8,
        #[serde(default, skip_deserializing)]
        n: u32,
    },
    Text {
        x: i32,
        y: i32,
        text: String,
        color: PaletteColor,
        stroke: u8,
    },
    Blur {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
    Pixelate {
        #[serde(deserialize_with = "bounded_points")]
        points: Vec<[i32; 2]>,
        width: u32,
    },
}

#[derive(Serialize, Deserialize)]
pub struct RectV1 {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Serialize, Deserialize)]
pub struct ErrorV1 {
    pub code: u8,
    pub kind: ErrorKind,
    pub message: String,
    pub hint: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Failed,
    Usage,
    Config,
    Cancelled,
    TargetNotFound,
    TimedOut,
    OutputGone,
    Busy,
}

impl From<Rect<BufferPx>> for RectV1 {
    fn from(rect: Rect<BufferPx>) -> Self {
        Self {
            x: rect.x.0,
            y: rect.y.0,
            width: rect.width.0,
            height: rect.height.0,
        }
    }
}

impl From<RectV1> for Rect<BufferPx> {
    fn from(rect: RectV1) -> Self {
        Self {
            x: BufferPx(rect.x),
            y: BufferPx(rect.y),
            width: BufferPx(rect.width),
            height: BufferPx(rect.height),
        }
    }
}

pub fn annotations(doc: &Document) -> (Vec<AnnotationV1>, u32) {
    let out = doc
        .marks
        .iter()
        .filter(|(_, mark)| {
            doc.crop
                .is_none_or(|crop| !super::document::wholly_outside(mark, crop, doc.scale()))
        })
        .map(|(id, mark)| to_annotation(mark, doc.number_of(*id)))
        .collect();
    (out, doc.dropped())
}

pub fn marks(annotations: &[AnnotationV1]) -> Result<Vec<Mark>, String> {
    if annotations.len() > MAX_MARKS {
        return Err(format!(
            "at most {MAX_MARKS} annotations are allowed, first offending annotation at index {MAX_MARKS}"
        ));
    }
    let mut out = Vec::with_capacity(annotations.len());
    for (index, annotation) in annotations.iter().enumerate() {
        if let AnnotationV1::Text { text, .. } = annotation
            && text.chars().count() > MAX_TEXT_CHARS
        {
            return Err(format!(
                "text exceeds {MAX_TEXT_CHARS} characters at annotation {index}"
            ));
        }
        if let AnnotationV1::Freehand { points, .. } | AnnotationV1::Highlighter { points, .. } =
            annotation
            && !(2..=MAX_PATH_POINTS).contains(&points.len())
        {
            return Err(format!(
                "path needs 2 to {MAX_PATH_POINTS} points at annotation {index}"
            ));
        }
        if let AnnotationV1::Pixelate { points, width } = annotation
            && (points.is_empty() || points.len() > MAX_PATH_POINTS || !(1..=128).contains(width))
        {
            return Err(format!(
                "pixelate needs 1 to {MAX_PATH_POINTS} points and width 1 to 128 at annotation {index}"
            ));
        }
        out.push(to_mark(annotation));
    }
    Ok(out)
}

fn to_annotation(mark: &Mark, n: Option<u32>) -> AnnotationV1 {
    match mark {
        Mark::Arrow { from, to, style } => AnnotationV1::Arrow {
            from: [from.x.0, from.y.0],
            to: [to.x.0, to.y.0],
            color: style.color,
            stroke: style.stroke as u8,
        },
        Mark::Line { from, to, style } => AnnotationV1::Line {
            from: [from.x.0, from.y.0],
            to: [to.x.0, to.y.0],
            color: style.color,
            stroke: style.stroke as u8,
        },
        Mark::Path {
            points,
            style,
            highlighter,
        } => {
            let points = points.iter().map(|point| [point.x.0, point.y.0]).collect();
            match highlighter {
                true => AnnotationV1::Highlighter {
                    points,
                    color: style.color,
                    stroke: style.stroke as u8,
                },
                false => AnnotationV1::Freehand {
                    points,
                    color: style.color,
                    stroke: style.stroke as u8,
                },
            }
        }
        Mark::Rect {
            rect,
            style,
            filled,
        } => AnnotationV1::Rect {
            x: rect.x.0,
            y: rect.y.0,
            width: rect.width.0,
            height: rect.height.0,
            color: style.color,
            stroke: style.stroke as u8,
            filled: *filled,
        },
        Mark::Oval { rect, style } => AnnotationV1::Oval {
            x: rect.x.0,
            y: rect.y.0,
            width: rect.width.0,
            height: rect.height.0,
            color: style.color,
            stroke: style.stroke as u8,
        },
        Mark::Number { at, style } => AnnotationV1::Number {
            x: at.x.0,
            y: at.y.0,
            color: style.color,
            stroke: style.stroke as u8,
            n: n.unwrap_or(0),
        },
        Mark::Text { at, text, style } => AnnotationV1::Text {
            x: at.x.0,
            y: at.y.0,
            text: text.clone(),
            color: style.color,
            stroke: style.stroke as u8,
        },
        Mark::Blur { rect } => AnnotationV1::Blur {
            x: rect.x.0,
            y: rect.y.0,
            width: rect.width.0,
            height: rect.height.0,
        },
        Mark::Pixelate { points, width } => AnnotationV1::Pixelate {
            points: points.iter().map(|point| [point.x.0, point.y.0]).collect(),
            width: *width,
        },
    }
}

fn to_mark(annotation: &AnnotationV1) -> Mark {
    match annotation {
        AnnotationV1::Arrow {
            from,
            to,
            color,
            stroke,
        } => Mark::Arrow {
            from: Point {
                x: BufferPx(from[0]),
                y: BufferPx(from[1]),
            },
            to: Point {
                x: BufferPx(to[0]),
                y: BufferPx(to[1]),
            },
            style: Style {
                color: *color,
                stroke: stroke_from_u8(*stroke),
            },
        },
        AnnotationV1::Line {
            from,
            to,
            color,
            stroke,
        } => Mark::Line {
            from: Point {
                x: BufferPx(from[0]),
                y: BufferPx(from[1]),
            },
            to: Point {
                x: BufferPx(to[0]),
                y: BufferPx(to[1]),
            },
            style: Style {
                color: *color,
                stroke: stroke_from_u8(*stroke),
            },
        },
        AnnotationV1::Freehand {
            points,
            color,
            stroke,
        }
        | AnnotationV1::Highlighter {
            points,
            color,
            stroke,
        } => Mark::Path {
            points: points
                .iter()
                .map(|[x, y]| Point {
                    x: BufferPx(*x),
                    y: BufferPx(*y),
                })
                .collect(),
            style: Style {
                color: *color,
                stroke: stroke_from_u8(*stroke),
            },
            highlighter: matches!(annotation, AnnotationV1::Highlighter { .. }),
        },
        AnnotationV1::Rect {
            x,
            y,
            width,
            height,
            color,
            stroke,
            filled,
        } => Mark::Rect {
            rect: Rect {
                x: BufferPx(*x),
                y: BufferPx(*y),
                width: BufferPx(*width),
                height: BufferPx(*height),
            },
            style: Style {
                color: *color,
                stroke: stroke_from_u8(*stroke),
            },
            filled: *filled,
        },
        AnnotationV1::Oval {
            x,
            y,
            width,
            height,
            color,
            stroke,
        } => Mark::Oval {
            rect: Rect {
                x: BufferPx(*x),
                y: BufferPx(*y),
                width: BufferPx(*width),
                height: BufferPx(*height),
            },
            style: Style {
                color: *color,
                stroke: stroke_from_u8(*stroke),
            },
        },
        AnnotationV1::Number {
            x,
            y,
            color,
            stroke,
            ..
        } => Mark::Number {
            at: Point {
                x: BufferPx(*x),
                y: BufferPx(*y),
            },
            style: Style {
                color: *color,
                stroke: stroke_from_u8(*stroke),
            },
        },
        AnnotationV1::Text {
            x,
            y,
            text,
            color,
            stroke,
        } => Mark::Text {
            at: Point {
                x: BufferPx(*x),
                y: BufferPx(*y),
            },
            text: text.clone(),
            style: Style {
                color: *color,
                stroke: stroke_from_u8(*stroke),
            },
        },
        AnnotationV1::Blur {
            x,
            y,
            width,
            height,
        } => Mark::Blur {
            rect: Rect {
                x: BufferPx(*x),
                y: BufferPx(*y),
                width: BufferPx(*width),
                height: BufferPx(*height),
            },
        },
        AnnotationV1::Pixelate { points, width } => Mark::Pixelate {
            points: points
                .iter()
                .map(|point| Point {
                    x: BufferPx(point[0]),
                    y: BufferPx(point[1]),
                })
                .collect(),
            width: *width,
        },
    }
}

fn stroke_from_u8(value: u8) -> Stroke {
    match value {
        1 => Stroke::Thin,
        3 => Stroke::Thick,
        _ => Stroke::Medium,
    }
}

#[cfg(test)]
mod tests;
