use std::io::{Read as _, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use chrono::{DateTime, Local};
use glimpse_compositors::model::{Output, Workspace};
use glimpse_config::ScreenshotConfig;
use glimpse_shot::doc::document::{Document, Mark, Source};
use glimpse_shot::doc::geometry::within_one_output;
use glimpse_shot::doc::units::{BufferPx, Logical, Rect, Scale};
use glimpse_shot::doc::wire::{self, AnnotationV1, RectV1, ResultV1, TargetV1};
use glimpse_shot::ports::{Clipboard, Clock, Files, Screens, Window, Windows};
use glimpse_shot::session::{self, Effect, Event, Face, FinishKey, Outcome, Phase, Target};
use glimpse_wayland::capture::CaptureError;
use glimpse_wayland::selection::Entry;
use serde_json::json;

use crate::cli::{CaptureArgs, ScaleMode, Terminal, missing_directory, validate_capture};
use crate::errors::{Cancelled, NotFound, TimedOut, Usage};
use crate::finish::{self, Destination, STDOUT};
use crate::render::{Png, within_frame};

#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub name: String,
    pub rect: Rect<Logical>,
    pub scale: Scale,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub outputs: Vec<Output>,
    pub workspaces: Vec<Workspace>,
    pub windows: Vec<Window>,
    pub focused_output: Option<String>,
}

pub fn placed(layout: &Layout) -> Vec<Placed> {
    layout
        .outputs
        .iter()
        .filter_map(|output| {
            let logical = output.logical.as_ref()?;
            Some(Placed {
                name: output.connector.clone(),
                rect: Rect {
                    x: Logical(f64::from(logical.x)),
                    y: Logical(f64::from(logical.y)),
                    width: Logical(f64::from(logical.width)),
                    height: Logical(f64::from(logical.height)),
                },
                scale: Scale(logical.scale),
            })
        })
        .collect()
}

pub struct Deps<'a> {
    pub screens: &'a dyn Screens,
    pub windows: Option<&'a dyn Windows>,
    pub clipboard: &'a dyn Clipboard,
    pub files: &'a dyn Files,
    pub clock: &'a dyn Clock,
    pub outputs: Vec<Placed>,
    pub encode: &'a dyn Fn(&Document, f64) -> Result<Png>,
    pub step: fn(Phase, Event) -> (Phase, Vec<Effect>),
}

const FACE: Face = Face {
    headless: true,
    notify: false,
};

pub(crate) const NO_LAYOUT: &str = "no compositor provider is running";
pub(crate) const NO_WINDOWS: &str = "no compositor provider is running";
const LIST_HINT: &str = "run `glimpse-shot list` to see the windows";

pub struct Shot {
    pub doc: Document,
    pub target: TargetV1,
    pub taken: DateTime<Local>,
}

pub fn capture(
    args: &CaptureArgs,
    config: &ScreenshotConfig,
    terminal: Terminal,
    deps: &Deps,
    stdout: &mut dyn Write,
) -> Result<()> {
    validate_capture(args, terminal).map_err(Usage)?;
    if let Some(dir) = missing_directory(args.output.as_deref()) {
        bail!("the directory {} does not exist", dir.display());
    }
    let target = args.target.resolve(true).map_err(Usage)?;
    if deps.windows.is_none() && matches!(target, Target::FocusedWindow | Target::Window(_)) {
        bail!("--window needs the compositor's windows; {NO_WINDOWS}");
    }
    if deps.outputs.is_empty() {
        if matches!(target, Target::Region(_)) {
            bail!("--region needs the compositor's output positions; {NO_LAYOUT}");
        }
        if args.scale == ScaleMode::Logical {
            bail!("--scale logical needs the compositor's output scale; {NO_LAYOUT}");
        }
    }
    let destination = Destination::of(args.output.as_deref(), args.no_save, config)?;
    let marks = match &args.annotate {
        Some(path) => read_marks(path)?,
        None => Vec::new(),
    };
    let key = match args.copy {
        true => FinishKey::CopyClose,
        false => FinishKey::SaveAsClose,
    };
    let until = args
        .delay
        .filter(|delay| *delay > 0)
        .map(|delay| deps.clock.now() + Duration::from_secs(delay.into()));

    let mut shot = match args.image.as_deref() {
        Some(path) => {
            let doc = crate::image::load(path)?;
            let target = TargetV1::File {
                path: path.canonicalize()?,
            };
            Some(self::shot(
                doc.source,
                doc.crop,
                &marks,
                target,
                Local::now(),
            ))
        }
        None => None,
    };
    let mut png: Option<Png> = None;
    let (mut phase, mut effects) = if shot.is_some() {
        (
            Phase::Editing {
                saved: None,
                copied: false,
                face: FACE,
            },
            Vec::new(),
        )
    } else {
        session::start(target, until, FACE)
    };
    let outcome = loop {
        let event = match ((!effects.is_empty()).then(|| effects.remove(0)), &phase) {
            (Some(Effect::Capture(target)), _) => {
                let taken = take(target, args.cursor(config.cursor), args.scale, &marks, deps)?;
                let source = taken.doc.source.clone();
                shot = Some(taken);
                Event::Captured(Ok(source))
            }
            (Some(Effect::Encode), _) => {
                let doc = &shot.as_ref().context("nothing was captured")?.doc;
                match (deps.encode)(doc, factor(args.scale, doc)) {
                    Ok(encoded) => {
                        png = Some(encoded);
                        Event::Effected(Effect::Encode, Ok(()))
                    }
                    Err(error) => Event::Effected(Effect::Encode, Err(format!("{error:#}"))),
                }
            }
            (Some(Effect::Write(requested)), _) => {
                let bytes = png
                    .as_ref()
                    .map(|png| png.bytes.as_slice())
                    .context("nothing was encoded")?;
                match finish::write(&destination, &requested, deps.files, bytes, stdout) {
                    Ok(actual) => Event::Effected(Effect::Write(actual), Ok(())),
                    Err(error) => {
                        Event::Effected(Effect::Write(requested), Err(format!("{error:#}")))
                    }
                }
            }
            (Some(Effect::Copy), _) => {
                let bytes = &png.as_ref().context("nothing was encoded")?.bytes;
                let entry = Entry {
                    mime: "image/png".to_owned(),
                    data: Arc::from(bytes.as_slice()),
                };
                Event::Effected(Effect::Copy, deps.clipboard.copy(vec![entry]))
            }
            (Some(other), _) => {
                Event::Effected(other, Err("not available without the editor".to_owned()))
            }
            (None, Phase::Countdown { until, .. }) => {
                std::thread::sleep(until.saturating_duration_since(deps.clock.now()));
                Event::Tick(deps.clock.now())
            }
            (None, Phase::Editing { .. }) => {
                let taken = shot.as_ref().context("nothing was captured")?.taken;
                Event::Finish(key, destination.requested(config, taken))
            }
            (None, Phase::Done(outcome)) => break outcome.clone(),
            (None, Phase::Capturing { .. } | Phase::Finishing { .. }) => {
                bail!("the capture session stalled")
            }
        };
        let (next, more) = (deps.step)(phase, event);
        phase = next;
        effects.extend(more);
    };

    let (path, copied) = match outcome {
        Outcome::Saved { path, copied } => (Some(path), copied),
        Outcome::Shared(path) => (Some(path), false),
        Outcome::Copied => (None, true),
        Outcome::Cancelled => return Err(Cancelled.into()),
        Outcome::TimedOut => return Err(TimedOut.into()),
        Outcome::Failed(reason) => bail!(reason),
    };
    let path = path.filter(|path| path != Path::new(STDOUT));
    let (Some(shot), Some(png)) = (shot, png) else {
        bail!("the capture session ended with no image");
    };
    if args.json {
        let (annotations, dropped) = wire::annotations(&shot.doc);
        let result = ResultV1 {
            version: 1,
            path,
            copied,
            width: png.width,
            height: png.height,
            scale: scale_of(&shot.doc),
            output: (!matches!(shot.doc.source, Source::File { .. })).then(|| output_of(&shot.doc)),
            target: shot.target,
            crop: shot.doc.crop.map(RectV1::from),
            dropped,
            annotations,
        };
        finish::line(stdout, &serde_json::to_string(&result)?)?;
    } else if let Some(path) = path {
        finish::line(stdout, &path.display().to_string())?;
    }
    Ok(())
}

pub fn scale_of(doc: &Document) -> f64 {
    match &doc.source {
        Source::Output { scale, .. } | Source::Window { scale, .. } => scale.0,
        Source::File { .. } => 1.0,
    }
}

pub fn output_of(doc: &Document) -> String {
    match &doc.source {
        Source::Output { name, .. } => name.clone(),
        Source::Window { frame, .. } | Source::File { frame, .. } => frame.connector.clone(),
    }
}

pub fn factor(mode: ScaleMode, doc: &Document) -> f64 {
    match mode {
        ScaleMode::Buffer => 1.0,
        ScaleMode::Logical => 1.0 / scale_of(doc),
    }
}

pub fn take(
    target: Target,
    cursor: bool,
    mode: ScaleMode,
    marks: &[Mark],
    deps: &Deps,
) -> Result<Shot> {
    let (name, crop, target) = match target {
        Target::CurrentScreen => {
            let name = deps.screens.current().map_err(|error| match error {
                CaptureError::NoSuchOutput(_) | CaptureError::NoOutputs => {
                    anyhow::Error::new(NotFound {
                        what: "cannot tell which output is current".to_owned(),
                        hint: Some("pass --screen NAME"),
                    })
                }
                other => anyhow::Error::new(other),
            })?;
            (name.clone(), None, TargetV1::Screen { name })
        }
        Target::Screen(name) => (name.clone(), None, TargetV1::Screen { name }),
        Target::Region(region) => {
            let (name, crop) = region_crop(region, &deps.outputs).map_err(|what| NotFound {
                what,
                hint: Some("run `glimpse-shot list` to see the outputs"),
            })?;
            (name.clone(), Some(crop), TargetV1::Region { name })
        }
        Target::FocusedWindow | Target::Window(_) => return take_window(target, mode, marks, deps),
        Target::App | Target::PickWindow | Target::PickRegion => {
            bail!("this target needs the editor, which capture never shows")
        }
    };
    let frame = deps
        .screens
        .capture(&name, cursor)
        .map_err(|error| match error {
            CaptureError::NoSuchOutput(name) => anyhow::Error::new(NotFound {
                what: format!("there is no output named {name}"),
                hint: Some("run `glimpse-shot list` to see the outputs"),
            }),
            other => anyhow::Error::new(other).context("cannot capture the screen"),
        })?;
    let taken = Local::now();
    let scale = scale_on(&name, mode, deps)?;
    let crop = crop.map(|crop| within_frame(crop, &frame));
    let source = Source::Output {
        name,
        scale: Scale(scale),
        frame,
    };
    Ok(shot(source, crop, marks, target, taken))
}

fn scale_on(name: &str, mode: ScaleMode, deps: &Deps) -> Result<f64> {
    match deps.outputs.iter().find(|output| output.name == name) {
        Some(output) => Ok(output.scale.0),
        None if mode == ScaleMode::Logical => {
            bail!("--scale logical needs the scale of {name}, which the compositor did not report")
        }
        None => {
            tracing::warn!("no output layout names {name}; strokes and the result assume scale 1");
            Ok(1.0)
        }
    }
}

pub fn shot(
    source: Source,
    crop: Option<Rect<BufferPx>>,
    marks: &[Mark],
    target: TargetV1,
    taken: DateTime<Local>,
) -> Shot {
    Shot {
        doc: Document {
            source,
            crop,
            marks: marks
                .iter()
                .cloned()
                .enumerate()
                .map(|(id, mark)| (id as u32, mark))
                .collect(),
            selected: None,
        },
        target,
        taken,
    }
}

fn take_window(target: Target, mode: ScaleMode, marks: &[Mark], deps: &Deps) -> Result<Shot> {
    let windows = deps.windows.context(NO_WINDOWS)?;
    let listed = windows.list().context("cannot list the windows")?;
    let window = match target {
        Target::Window(id) => listed.into_iter().find(|window| window.id == id),
        _ => listed.into_iter().find(|window| window.focused),
    };
    let window = window.ok_or_else(|| NotFound {
        what: match target {
            Target::Window(id) => format!("there is no window with id {id}"),
            _ => "no window is focused".to_owned(),
        },
        hint: Some(LIST_HINT),
    })?;
    let frame = windows
        .capture(window.id)
        .context("cannot capture the window")?;
    let taken = Local::now();
    let scale = scale_on(&frame.connector, mode, deps)?;
    let target = TargetV1::Window {
        id: window.id,
        app_id: window.app_id.clone(),
        title: window.title.clone(),
    };
    let source = Source::Window {
        id: window.id,
        app_id: window.app_id,
        title: window.title,
        scale: Scale(scale),
        frame,
    };
    Ok(shot(source, None, marks, target, taken))
}

pub const ANNOTATIONS_CAP: usize = 2 * 1024 * 1024;

pub fn read_marks(path: &Path) -> Result<Vec<Mark>> {
    let mut bytes = Vec::new();
    let limit = ANNOTATIONS_CAP as u64 + 1;
    if path == Path::new("-") {
        std::io::stdin()
            .take(limit)
            .read_to_end(&mut bytes)
            .context("cannot read annotations from stdin")?;
    } else {
        std::fs::File::open(path)
            .and_then(|file| file.take(limit).read_to_end(&mut bytes))
            .with_context(|| format!("cannot read annotations from {}", path.display()))?;
    }
    parse_marks(&capped_text(bytes)?)
}

pub fn capped_text(bytes: Vec<u8>) -> Result<String> {
    if bytes.len() > ANNOTATIONS_CAP {
        bail!("annotations are larger than {ANNOTATIONS_CAP} bytes");
    }
    String::from_utf8(bytes).context("annotations must be UTF-8 text")
}

pub fn parse_marks(text: &str) -> Result<Vec<Mark>> {
    let values: Vec<serde_json::Value> =
        serde_json::from_str(text).context("annotations must be a JSON array")?;
    let annotations = values
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            serde_json::from_value::<AnnotationV1>(value)
                .with_context(|| format!("annotation {index} is invalid"))
        })
        .collect::<Result<Vec<_>>>()?;
    wire::marks(&annotations).map_err(anyhow::Error::msg)
}

pub fn region_crop(
    region: Rect<Logical>,
    outputs: &[Placed],
) -> Result<(String, Rect<BufferPx>), String> {
    let named: Vec<(String, Rect<Logical>)> = outputs
        .iter()
        .map(|output| (output.name.clone(), output.rect))
        .collect();
    let name = within_one_output(region, &named)?;
    let output = outputs
        .iter()
        .find(|output| output.name == name)
        .ok_or_else(|| format!("there is no output named {name}"))?;
    let to_buffer =
        |logical: f64, origin: f64| ((logical - origin) * output.scale.0).round() as i32;
    let x0 = to_buffer(region.x.0, output.rect.x.0);
    let y0 = to_buffer(region.y.0, output.rect.y.0);
    let x1 = to_buffer(region.x.0 + region.width.0, output.rect.x.0);
    let y1 = to_buffer(region.y.0 + region.height.0, output.rect.y.0);
    Ok((
        name,
        Rect {
            x: BufferPx(x0),
            y: BufferPx(y0),
            width: BufferPx(x1 - x0),
            height: BufferPx(y1 - y0),
        },
    ))
}

pub fn list(layout: &Layout, json: bool, terminal: bool, stdout: &mut dyn Write) -> Result<()> {
    let text = match json {
        true => list_json(layout).to_string(),
        false => list_text(layout, terminal),
    };
    finish::line(stdout, text.trim_end())
}

pub fn list_json(layout: &Layout) -> serde_json::Value {
    let outputs: Vec<_> = layout
        .outputs
        .iter()
        .filter_map(|output| {
            let logical = output.logical.as_ref()?;
            Some(json!({
                "name": output.connector,
                "x": logical.x,
                "y": logical.y,
                "width": logical.width,
                "height": logical.height,
                "scale": logical.scale,
                "focused": layout.focused_output.as_ref() == Some(&output.connector),
            }))
        })
        .collect();
    let windows: Vec<_> = layout
        .windows
        .iter()
        .map(|window| {
            json!({
                "id": window.id,
                "app_id": window.app_id,
                "title": window.title,
                "workspace": window.workspace,
                "output": window.output,
                "focused": window.focused,
                "floating": window.floating,
                "width": window.rect.map(|rect| whole(rect.width)),
                "height": window.rect.map(|rect| whole(rect.height)),
                "rect": window.rect,
            })
        })
        .collect();
    json!({ "version": 1, "outputs": outputs, "windows": windows })
}

fn list_text(layout: &Layout, terminal: bool) -> String {
    let mut rows = vec![vec![
        "KIND".to_owned(),
        "NAME/ID".to_owned(),
        "X".to_owned(),
        "Y".to_owned(),
        "WIDTH".to_owned(),
        "HEIGHT".to_owned(),
        "SCALE/OUTPUT".to_owned(),
        "APP ID".to_owned(),
        "TITLE".to_owned(),
    ]];
    for output in &layout.outputs {
        let Some(logical) = &output.logical else {
            continue;
        };
        rows.push(vec![
            "output".to_owned(),
            output.connector.clone(),
            logical.x.to_string(),
            logical.y.to_string(),
            logical.width.to_string(),
            logical.height.to_string(),
            logical.scale.to_string(),
            String::new(),
            String::new(),
        ]);
    }
    for window in &layout.windows {
        let rect = window.rect;
        rows.push(vec![
            "window".to_owned(),
            window.id.to_string(),
            rect.map(|rect| rect.x.to_string())
                .unwrap_or_else(|| "-".to_owned()),
            rect.map(|rect| rect.y.to_string())
                .unwrap_or_else(|| "-".to_owned()),
            rect.map(|rect| whole(rect.width).to_string())
                .unwrap_or_else(|| "-".to_owned()),
            rect.map(|rect| whole(rect.height).to_string())
                .unwrap_or_else(|| "-".to_owned()),
            window.output.clone().unwrap_or_else(|| "-".to_owned()),
            window.app_id.clone().unwrap_or_default(),
            window.title.clone().unwrap_or_default(),
        ]);
    }
    if !terminal {
        return rows
            .iter()
            .skip(1)
            .map(|row| row.join("\t"))
            .collect::<Vec<_>>()
            .join("\n");
    }
    let widths: Vec<usize> = (0..rows[0].len())
        .map(|column| {
            rows.iter()
                .map(|row| row[column].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    rows.iter()
        .map(|row| {
            row.iter()
                .zip(&widths)
                .map(|(cell, width)| format!("{cell:<width$}"))
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn whole(px: f64) -> i64 {
    px.round() as i64
}

#[cfg(test)]
mod tests;
