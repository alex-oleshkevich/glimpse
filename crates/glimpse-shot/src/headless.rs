use std::io::{Read as _, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use chrono::{DateTime, Local};
use glimpse_compositors::Snapshot;
use glimpse_config::ScreenshotConfig;
use glimpse_shot::doc::document::{Document, Mark, Source};
use glimpse_shot::doc::geometry::within_one_output;
use glimpse_shot::doc::units::{BufferPx, Logical, Rect, Scale};
use glimpse_shot::doc::wire::{self, AnnotationV1, RectV1, ResultV1, TargetV1};
use glimpse_shot::ports::{Clipboard, Clock, Files, Screens, Windows};
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

pub fn placed(snapshot: &Snapshot) -> Vec<Placed> {
    snapshot
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

const NO_LAYOUT: &str = "there is no output layout: neither NIRI_SOCKET nor HYPRLAND_INSTANCE_SIGNATURE names a reachable compositor";
const NO_WINDOWS: &str =
    "neither NIRI_SOCKET nor HYPRLAND_INSTANCE_SIGNATURE names a reachable compositor";
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

    let mut shot: Option<Shot> = None;
    let mut png: Option<Png> = None;
    let (mut phase, mut effects) = session::start(target, until, FACE);
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
            output: Some(output_of(&shot.doc)),
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
        Target::Window(id) => listed.into_iter().find(|window| window.id.0 == id),
        _ => listed.into_iter().find(|window| window.is_focused),
    };
    let window = window.ok_or_else(|| NotFound {
        what: match target {
            Target::Window(id) => format!("there is no window with id {id}"),
            _ => "no window is focused".to_owned(),
        },
        hint: Some(LIST_HINT),
    })?;
    let frame = windows
        .capture(window.id.0)
        .context("cannot capture the window")?;
    let taken = Local::now();
    let scale = scale_on(&frame.connector, mode, deps)?;
    let target = TargetV1::Window {
        id: window.id.0,
        app_id: window.app_id.clone(),
        title: window.title.clone(),
    };
    let source = Source::Window {
        id: window.id.0,
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

pub fn list(snapshot: &Snapshot, json: bool, terminal: bool, stdout: &mut dyn Write) -> Result<()> {
    let text = match json {
        true => list_json(snapshot).to_string(),
        false => list_text(snapshot, terminal),
    };
    finish::line(stdout, text.trim_end())
}

fn global_rect(
    snapshot: &Snapshot,
    window: &glimpse_compositors::Window,
) -> Option<glimpse_compositors::Rect> {
    let rect = window.rect?;
    let output = snapshot
        .outputs
        .iter()
        .find(|output| Some(&output.connector) == window.output.as_ref())?;
    let logical = output.logical.as_ref()?;
    Some(glimpse_compositors::Rect {
        x: rect.x + f64::from(logical.x),
        y: rect.y + f64::from(logical.y),
        ..rect
    })
}

pub fn list_json(snapshot: &Snapshot) -> serde_json::Value {
    let outputs: Vec<_> = snapshot
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
                "focused": snapshot.focused_output.as_ref() == Some(&output.connector),
            }))
        })
        .collect();
    let windows: Vec<_> = snapshot
        .windows
        .iter()
        .map(|window| {
            json!({
                "id": window.id.0,
                "app_id": window.app_id,
                "title": window.title,
                "workspace": window.workspace_id.map(|id| id.0),
                "output": window.output,
                "focused": window.is_focused,
                "floating": window.is_floating,
                "width": window.size.map(|(width, _)| width),
                "height": window.size.map(|(_, height)| height),
                "rect": global_rect(snapshot, window),
            })
        })
        .collect();
    json!({ "version": 1, "outputs": outputs, "windows": windows })
}

fn list_text(snapshot: &Snapshot, terminal: bool) -> String {
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
    for output in &snapshot.outputs {
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
    for window in &snapshot.windows {
        let rect = global_rect(snapshot, window);
        let size = window.size;
        rows.push(vec![
            "window".to_owned(),
            window.id.0.to_string(),
            rect.map(|rect| rect.x.to_string())
                .unwrap_or_else(|| "-".to_owned()),
            rect.map(|rect| rect.y.to_string())
                .unwrap_or_else(|| "-".to_owned()),
            size.map(|(width, _)| width.to_string())
                .unwrap_or_else(|| "-".to_owned()),
            size.map(|(_, height)| height.to_string())
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::time::Instant;

    use clap::Parser as _;
    use glimpse_compositors::{Logical as OutputLogical, Output, Window, WindowId, WorkspaceId};
    use glimpse_wayland::capture::Frame;

    use super::*;
    use crate::cli::{Cli, Command};
    use crate::errors::{Exit, report};

    #[test]
    fn annotations_above_the_byte_cap_are_refused_rather_than_truncated() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let oversized = dir.path().join("big.json");
        let mut text = b"[]".to_vec();
        text.resize(ANNOTATIONS_CAP + 1, b' ');
        std::fs::write(&oversized, &text).expect("writes");
        let refused = format!("{:#}", read_marks(&oversized).expect_err("over the cap"));
        assert!(refused.contains("larger than"), "{refused}");

        text.truncate(ANNOTATIONS_CAP);
        let fits = dir.path().join("fits.json");
        std::fs::write(&fits, &text).expect("writes");
        assert!(read_marks(&fits).expect("at the cap").is_empty());
        assert!(capped_text(vec![0xff]).is_err());
    }

    fn outputs() -> Vec<Placed> {
        vec![
            Placed {
                name: "DP-2".to_owned(),
                rect: Rect {
                    x: Logical(1920.0),
                    y: Logical(0.0),
                    width: Logical(3072.0),
                    height: Logical(1728.0),
                },
                scale: Scale(1.25),
            },
            Placed {
                name: "eDP-1".to_owned(),
                rect: Rect {
                    x: Logical(-1440.0),
                    y: Logical(0.0),
                    width: Logical(2304.0),
                    height: Logical(1440.0),
                },
                scale: Scale(1.25),
            },
            Placed {
                name: "HDMI-1".to_owned(),
                rect: Rect {
                    x: Logical(0.0),
                    y: Logical(2000.0),
                    width: Logical(1707.0),
                    height: Logical(960.0),
                },
                scale: Scale(1.5),
            },
        ]
    }

    fn region(x: f64, y: f64, width: f64, height: f64) -> Rect<Logical> {
        Rect {
            x: Logical(x),
            y: Logical(y),
            width: Logical(width),
            height: Logical(height),
        }
    }

    fn buffer(x: i32, y: i32, width: i32, height: i32) -> Rect<BufferPx> {
        Rect {
            x: BufferPx(x),
            y: BufferPx(y),
            width: BufferPx(width),
            height: BufferPx(height),
        }
    }

    #[test]
    fn a_region_becomes_a_buffer_crop_of_the_one_output_holding_it() {
        assert_eq!(
            region_crop(region(100.0, 50.0, 400.0, 300.0), &outputs()),
            Ok(("eDP-1".to_owned(), buffer(1925, 63, 500, 375)))
        );
        assert_eq!(
            region_crop(region(-1000.0, 40.0, 400.0, 300.0), &outputs()),
            Ok(("eDP-1".to_owned(), buffer(550, 50, 500, 375)))
        );
        assert_eq!(
            region_crop(region(2000.0, 100.0, 400.0, 300.0), &outputs()),
            Ok(("DP-2".to_owned(), buffer(100, 125, 500, 375)))
        );
        let partly_outside = region_crop(region(800.0, 0.0, 400.0, 300.0), &outputs());
        assert!(partly_outside.is_err());
        let nowhere = region_crop(region(-5000.0, 0.0, 10.0, 10.0), &outputs());
        assert!(nowhere.is_err());
    }

    struct FakeScreens;

    impl Screens for FakeScreens {
        fn current(&self) -> Result<String, CaptureError> {
            Ok("eDP-1".to_owned())
        }

        fn capture(&self, name: &str, _cursor: bool) -> Result<Frame, CaptureError> {
            let (width, height) = match name {
                "eDP-1" => (2880, 1800),
                "HDMI-1" => (2560, 1440),
                other => return Err(CaptureError::NoSuchOutput(other.to_owned())),
            };
            Ok(Frame {
                connector: name.to_owned(),
                width,
                height,
                pixels: glib::Bytes::from_owned(vec![0u8; 16]),
                alpha: false,
            })
        }
    }

    struct FakeWindows;

    impl Windows for FakeWindows {
        fn list(&self) -> Result<Vec<Window>, CaptureError> {
            Ok(snapshot().windows)
        }

        fn capture(&self, id: u64) -> Result<Frame, CaptureError> {
            Ok(Frame {
                connector: "eDP-1".to_owned(),
                width: 500 + id as u32,
                height: 375,
                pixels: glib::Bytes::from_owned(vec![0u8; 16]),
                alpha: false,
            })
        }
    }

    #[derive(Default)]
    struct FakeClipboard(RefCell<Vec<Vec<Entry>>>);

    impl Clipboard for FakeClipboard {
        fn copy(&self, entries: Vec<Entry>) -> Result<(), String> {
            self.0.borrow_mut().push(entries);
            Ok(())
        }

        fn snapshot(&self) -> std::io::Result<glimpse_wayland::selection::Snapshot> {
            Ok(Default::default())
        }
    }

    #[derive(Default)]
    struct FakeFiles(RefCell<Vec<PathBuf>>);

    impl Files for FakeFiles {
        fn write_new(
            &self,
            dir: &Path,
            name: &str,
            own: Option<&Path>,
            _bytes: &[u8],
        ) -> std::io::Result<PathBuf> {
            let path = own.map_or_else(|| dir.join(name), Path::to_path_buf);
            self.0.borrow_mut().push(path.clone());
            Ok(path)
        }
    }

    struct FakeClock {
        start: Instant,
        calls: std::cell::Cell<u32>,
    }

    impl Default for FakeClock {
        fn default() -> Self {
            Self {
                start: Instant::now(),
                calls: std::cell::Cell::new(0),
            }
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> Instant {
            let calls = self.calls.get();
            self.calls.set(calls + 1);
            self.start + Duration::from_secs(u64::from(calls) * 10)
        }
    }

    fn encode(doc: &Document, factor: f64) -> Result<Png> {
        let rect = doc.exported_rect();
        Ok(Png {
            bytes: b"\x89PNG".to_vec(),
            width: (f64::from(rect.width.0) * factor).round() as u32,
            height: (f64::from(rect.height.0) * factor).round() as u32,
        })
    }

    const PIPES: Terminal = Terminal {
        stdin: false,
        stdout: false,
    };

    fn run(args: &[&str], files: &FakeFiles) -> (Exit, Vec<u8>, String) {
        run_with(args, files, outputs(), PIPES)
    }

    fn run_copying(args: &[&str], clipboard: &FakeClipboard) -> (Exit, Vec<u8>, String) {
        run_all(args, &FakeFiles::default(), clipboard, outputs(), PIPES)
    }

    fn run_driven(
        args: &[&str],
        files: &FakeFiles,
        clock: &FakeClock,
        step: fn(Phase, Event) -> (Phase, Vec<Effect>),
    ) -> (Exit, Vec<u8>, String) {
        let cli = crate::cli::parse(
            std::iter::once("glimpse-shot")
                .chain(args.iter().copied())
                .map(std::ffi::OsString::from),
        )
        .expect("parses");
        let json = cli.json();
        let Some(Command::Capture(capture_args)) = cli.command else {
            panic!("expected capture");
        };
        let windows = FakeWindows;
        let clipboard = FakeClipboard::default();
        let deps = Deps {
            screens: &FakeScreens,
            windows: Some(&windows),
            clipboard: &clipboard,
            files,
            clock,
            outputs: outputs(),
            encode: &encode,
            step,
        };
        let config = ScreenshotConfig::default();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit = match capture(&capture_args, &config, PIPES, &deps, &mut stdout) {
            Ok(()) => Exit::Ok,
            Err(error) => report(&error, json, &mut stderr),
        };
        (exit, stdout, String::from_utf8(stderr).expect("utf-8"))
    }

    fn run_with(
        args: &[&str],
        files: &FakeFiles,
        outputs: Vec<Placed>,
        terminal: Terminal,
    ) -> (Exit, Vec<u8>, String) {
        run_all(args, files, &FakeClipboard::default(), outputs, terminal)
    }

    fn run_all(
        args: &[&str],
        files: &FakeFiles,
        clipboard: &FakeClipboard,
        outputs: Vec<Placed>,
        terminal: Terminal,
    ) -> (Exit, Vec<u8>, String) {
        let cli = Cli::try_parse_from(crate::cli::attach_negative_geometry(
            std::iter::once("glimpse-shot")
                .chain(args.iter().copied())
                .map(std::ffi::OsString::from),
        ))
        .expect("parses");
        let json = cli.json();
        let Some(Command::Capture(capture_args)) = cli.command else {
            panic!("expected capture");
        };
        let clock = FakeClock::default();
        let windows = FakeWindows;
        let deps = Deps {
            screens: &FakeScreens,
            windows: (!outputs.is_empty()).then_some(&windows as &dyn Windows),
            clipboard,
            files,
            clock: &clock,
            outputs,
            encode: &encode,
            step: session::step,
        };
        let config = ScreenshotConfig::default();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit = match capture(&capture_args, &config, terminal, &deps, &mut stdout) {
            Ok(()) => Exit::Ok,
            Err(error) => report(&error, json, &mut stderr),
        };
        (exit, stdout, String::from_utf8(stderr).expect("utf-8"))
    }

    #[test]
    fn json_prints_exactly_one_result_line() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let out = dir.path().join("shot.png");
        let files = FakeFiles::default();
        let (exit, stdout, stderr) = run(
            &[
                "capture",
                "--region",
                "100,50 400x300",
                "--scale",
                "logical",
                "--json",
                "-o",
                out.to_str().expect("utf-8 path"),
            ],
            &files,
        );

        assert_eq!(exit, Exit::Ok, "{stderr}");
        let stdout = String::from_utf8(stdout).expect("utf-8");
        assert_eq!(stdout.lines().count(), 1);
        let result: serde_json::Value = serde_json::from_str(&stdout).expect("json");
        assert_eq!(result["version"], 1);
        assert_eq!(result["path"], out.to_str().expect("utf-8 path"));
        assert_eq!(
            result["target"],
            serde_json::json!({ "kind": "region", "name": "eDP-1" })
        );
        assert_eq!(
            result["crop"],
            serde_json::json!({ "x": 1925, "y": 63, "width": 500, "height": 375 })
        );
        assert_eq!(
            (result["width"].clone(), result["height"].clone()),
            (400.into(), 300.into())
        );
        assert_eq!(result["scale"], 1.25);
        assert_eq!(files.0.borrow().as_slice(), [out]);
    }

    #[test]
    fn a_failure_with_json_leaves_stdout_empty_and_ends_stderr_with_the_error() {
        let files = FakeFiles::default();
        let (exit, stdout, stderr) = run(&["capture", "--screen", "DP-9", "--json"], &files);
        assert_eq!(exit, Exit::NotFound, "{stderr}");
        assert!(stdout.is_empty());
        let last: serde_json::Value =
            serde_json::from_str(stderr.lines().last().expect("a line")).expect("json");
        assert_eq!(last["error"]["code"], 5);
        assert_eq!(last["error"]["kind"], "target_not_found");
        assert!(files.0.borrow().is_empty());
    }

    #[test]
    fn a_window_copied_with_no_file_is_captured_encoded_and_copied() {
        let clipboard = FakeClipboard::default();
        let (exit, stdout, stderr) = run_copying(
            &["capture", "--window", "42", "--copy", "--no-save", "--json"],
            &clipboard,
        );

        assert_eq!(exit, Exit::Ok, "{stderr}");
        let result: serde_json::Value = serde_json::from_slice(&stdout).expect("json");
        assert_eq!(result["path"], serde_json::Value::Null);
        assert_eq!(result["copied"], true);
        assert_eq!(result["width"], 542);
        assert_eq!(result["output"], "eDP-1");
        assert_eq!(result["target"]["kind"], "window");
        assert_eq!(result["target"]["id"], 42);
        let copies = clipboard.0.borrow();
        assert_eq!(copies.len(), 1);
        assert_eq!(copies[0].len(), 1);
        assert_eq!(copies[0][0].mime, "image/png");
        assert_eq!(&*copies[0][0].data, b"\x89PNG");
    }

    #[test]
    fn a_window_at_logical_scale_is_downscaled_by_its_outputs_scale() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let out = dir.path().join("window.png");
        let files = FakeFiles::default();
        let (exit, stdout, stderr) = run(
            &[
                "capture",
                "--window",
                "42",
                "--scale",
                "logical",
                "--json",
                "-o",
                out.to_str().expect("utf-8 path"),
            ],
            &files,
        );

        assert_eq!(exit, Exit::Ok, "{stderr}");
        let result: serde_json::Value = serde_json::from_slice(&stdout).expect("json");
        assert_eq!(result["scale"], 1.25);
        assert_eq!(
            (result["width"].clone(), result["height"].clone()),
            (434.into(), 300.into())
        );
    }

    #[test]
    fn copy_with_a_file_saves_and_copies() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let out = dir.path().join("shot.png");
        let files = FakeFiles::default();
        let clipboard = FakeClipboard::default();
        let (exit, stdout, stderr) = run_all(
            &[
                "capture",
                "--copy",
                "--json",
                "-o",
                out.to_str().expect("utf-8 path"),
            ],
            &files,
            &clipboard,
            outputs(),
            PIPES,
        );

        assert_eq!(exit, Exit::Ok, "{stderr}");
        let result: serde_json::Value = serde_json::from_slice(&stdout).expect("json");
        assert_eq!(result["copied"], true);
        assert_eq!(result["path"], out.to_str().expect("utf-8 path"));
        assert_eq!(files.0.borrow().as_slice(), [out]);
        assert_eq!(clipboard.0.borrow().len(), 1);
    }

    #[test]
    fn a_window_target_resolves_or_exits_five() {
        let clipboard = FakeClipboard::default();
        let (exit, stdout, stderr) = run_copying(
            &["capture", "--window", "--copy", "--no-save", "--json"],
            &clipboard,
        );
        assert_eq!(exit, Exit::Ok, "{stderr}");
        let result: serde_json::Value = serde_json::from_slice(&stdout).expect("json");
        assert_eq!(result["target"]["id"], 42);

        let (exit, stdout, stderr) = run_copying(
            &["capture", "--window", "7", "--copy", "--no-save", "--json"],
            &clipboard,
        );
        assert_eq!(exit, Exit::NotFound, "{stderr}");
        assert!(stdout.is_empty());
        assert!(stderr.contains("no window with id 7"), "{stderr}");
        assert_eq!(clipboard.0.borrow().len(), 1);
    }

    #[test]
    fn a_window_with_no_compositor_fails_naming_the_variables() {
        let files = FakeFiles::default();
        let (exit, _, stderr) = run_with(
            &["capture", "--window", "42", "-o", "/run/user/1000/w.png"],
            &files,
            Vec::new(),
            PIPES,
        );

        assert_eq!(exit, Exit::Failed);
        assert!(stderr.contains("HYPRLAND_INSTANCE_SIGNATURE"), "{stderr}");
        assert!(files.0.borrow().is_empty());
    }

    #[test]
    fn a_delay_waits_on_the_clock_and_then_captures() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let out = dir.path().join("shot.png");
        let files = FakeFiles::default();
        let clock = FakeClock::default();
        let started = Instant::now();

        let (exit, _, stderr) = run_driven(
            &[
                "capture",
                "--delay",
                "3",
                "-o",
                out.to_str().expect("utf-8 path"),
            ],
            &files,
            &clock,
            session::step,
        );

        assert_eq!(exit, Exit::Ok, "{stderr}");
        assert!(clock.calls.get() >= 3, "the countdown never read the clock");
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(files.0.borrow().as_slice(), [out]);
    }

    fn write_before_encode(phase: Phase, event: Event) -> (Phase, Vec<Effect>) {
        match event {
            Event::Captured(_) => (phase, vec![Effect::Write(PathBuf::from("/p/x.png"))]),
            _ => (phase, Vec::new()),
        }
    }

    #[test]
    fn a_write_with_nothing_encoded_fails_instead_of_writing_nothing() {
        let files = FakeFiles::default();
        let (exit, _, stderr) = run_driven(
            &["capture", "-o", "/run/user/1000/never.png"],
            &files,
            &FakeClock::default(),
            write_before_encode,
        );

        assert_eq!(exit, Exit::Failed);
        assert!(stderr.contains("nothing was encoded"), "{stderr}");
        assert!(files.0.borrow().is_empty());
    }

    fn two_at_once(phase: Phase, event: Event) -> (Phase, Vec<Effect>) {
        match event {
            Event::Captured(_) => (
                phase,
                vec![Effect::Encode, Effect::Write(PathBuf::from("/p/x.png"))],
            ),
            Event::Effected(Effect::Write(path), Ok(())) => (
                Phase::Done(Outcome::Saved {
                    path,
                    copied: false,
                }),
                Vec::new(),
            ),
            _ => (phase, Vec::new()),
        }
    }

    #[test]
    fn effects_emitted_together_run_first_to_last() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let out = dir.path().join("shot.png");
        let files = FakeFiles::default();
        let (exit, _, stderr) = run_driven(
            &["capture", "-o", out.to_str().expect("utf-8 path")],
            &files,
            &FakeClock::default(),
            two_at_once,
        );

        assert_eq!(exit, Exit::Ok, "{stderr}");
        assert_eq!(files.0.borrow().as_slice(), [out]);
    }

    #[test]
    fn plain_prints_the_saved_path_and_a_dash_prints_the_png() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let out = dir.path().join("shot.png");
        let files = FakeFiles::default();

        let (exit, stdout, _) = run(
            &["capture", "-o", out.to_str().expect("utf-8 path")],
            &files,
        );
        assert_eq!(exit, Exit::Ok);
        assert_eq!(stdout, format!("{}\n", out.display()).into_bytes());

        let (exit, stdout, _) = run(&["capture", "-o", "-"], &files);
        assert_eq!(exit, Exit::Ok);
        assert_eq!(stdout, b"\x89PNG");
    }

    #[test]
    fn every_capture_usage_row_exits_two() {
        let files = FakeFiles::default();
        let tty = Terminal {
            stdin: true,
            stdout: true,
        };
        let code = |args: &[&str], terminal| run_with(args, &files, outputs(), terminal).0;

        assert_eq!(code(&["capture", "--region"], PIPES), Exit::Usage);
        assert_eq!(
            code(&["capture", "--region", "garbage"], PIPES),
            Exit::Usage
        );
        assert_eq!(code(&["capture", "-o", "-", "--json"], PIPES), Exit::Usage);
        assert_eq!(code(&["capture", "-o", "-"], tty), Exit::Usage);
        assert_eq!(code(&["capture", "--annotate", "-"], tty), Exit::Usage);
        assert_eq!(code(&["capture", "--screen", "all"], PIPES), Exit::Usage);
        assert!(files.0.borrow().is_empty());
    }

    #[test]
    fn a_crop_rounded_past_the_frame_is_clamped_to_it() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let out = dir.path().join("shot.png");
        let files = FakeFiles::default();
        let (exit, stdout, stderr) = run(
            &[
                "capture",
                "--region",
                "1700,2950 7x10",
                "--json",
                "-o",
                out.to_str().expect("utf-8 path"),
            ],
            &files,
        );

        assert_eq!(exit, Exit::Ok, "{stderr}");
        let result: serde_json::Value = serde_json::from_slice(&stdout).expect("json");
        assert_eq!(result["output"], "HDMI-1");
        assert_eq!(
            result["crop"],
            serde_json::json!({ "x": 2550, "y": 1425, "width": 10, "height": 15 })
        );
        assert_eq!(result["scale"], 1.5);
    }

    #[test]
    fn with_no_layout_a_region_or_logical_scale_fails_naming_the_variables() {
        let files = FakeFiles::default();
        for args in [
            &["capture", "--region", "0,0 10x10"][..],
            &["capture", "--scale", "logical", "--screen", "eDP-1"][..],
        ] {
            let (exit, _, stderr) = run_with(args, &files, Vec::new(), PIPES);
            assert_eq!(exit, Exit::Failed, "{args:?}");
            assert!(stderr.contains("NIRI_SOCKET"), "{stderr}");
        }
        let (exit, _, stderr) = run_with(
            &["capture", "--scale", "logical", "--screen", "DP-3"],
            &files,
            vec![outputs().remove(0)],
            PIPES,
        );
        assert_ne!(exit, Exit::Ok, "{stderr}");
        assert!(files.0.borrow().is_empty());

        let dir = tempfile::tempdir().expect("a temporary directory");
        let out = dir.path().join("shot.png");
        let (exit, stdout, _) = run_with(
            &[
                "capture",
                "--screen",
                "eDP-1",
                "--json",
                "-o",
                out.to_str().expect("utf-8 path"),
            ],
            &files,
            Vec::new(),
            PIPES,
        );
        assert_eq!(exit, Exit::Ok);
        let result: serde_json::Value = serde_json::from_slice(&stdout).expect("json");
        assert_eq!(result["width"], 2880);
    }

    #[test]
    fn an_output_directory_that_does_not_exist_fails_before_capturing() {
        let files = FakeFiles::default();
        let (exit, _, stderr) = run(&["capture", "-o", "/nonexistent/glimpse/shot.png"], &files);

        assert_eq!(exit, Exit::Failed);
        assert!(
            stderr.contains("/nonexistent/glimpse does not exist"),
            "{stderr}"
        );
        assert!(files.0.borrow().is_empty());
    }

    #[test]
    fn annotations_name_the_first_bad_element_by_index() {
        let good = r#"[{"kind":"blur","x":1,"y":2,"width":3,"height":4}]"#;
        assert_eq!(parse_marks(good).expect("valid").len(), 1);

        let unknown = r#"[{"kind":"blur","x":1,"y":2,"width":3,"height":4},{"kind":"smudge"}]"#;
        let error = format!("{:#}", parse_marks(unknown).expect_err("unknown kind"));
        assert!(error.contains("annotation 1"), "{error}");
        assert!(error.contains("smudge"), "{error}");

        assert!(parse_marks("{").is_err());
    }

    fn snapshot() -> Snapshot {
        let output = |connector: &str, x: i32| Output {
            connector: connector.to_owned(),
            make: None,
            model: None,
            serial: None,
            description: None,
            logical: Some(OutputLogical {
                x,
                y: 0,
                width: 2304,
                height: 1440,
                scale: 1.25,
            }),
            current_mode: None,
            enabled: true,
            built_in: false,
        };
        let window = |id: u64, rect: Option<glimpse_compositors::Rect>| Window {
            id: WindowId(id),
            title: Some("<b>&title</b>".to_owned()),
            app_id: Some("firefox".to_owned()),
            pid: None,
            workspace_id: Some(WorkspaceId(8)),
            is_focused: id == 42,
            is_floating: rect.is_some(),
            is_urgent: false,
            layout_order: None,
            size: Some((400, 300)),
            rect,
            output: Some("eDP-1".to_owned()),
        };
        Snapshot {
            outputs: vec![output("DP-2", 1920), output("eDP-1", -1440)],
            windows: vec![
                window(42, None),
                window(
                    371,
                    Some(glimpse_compositors::Rect {
                        x: 952.0,
                        y: 568.0,
                        width: 400.0,
                        height: 300.0,
                    }),
                ),
            ],
            focused_output: Some("DP-2".to_owned()),
            ..Snapshot::default()
        }
    }

    #[test]
    fn list_json_has_the_documented_shape() {
        let value = list_json(&snapshot());

        assert_eq!(value["version"], 1);
        assert_eq!(
            value["outputs"][1],
            json!({ "name": "eDP-1", "x": -1440, "y": 0, "width": 2304, "height": 1440, "scale": 1.25, "focused": false })
        );
        assert_eq!(value["outputs"][0]["focused"], true);
        assert_eq!(
            value["windows"][0],
            json!({ "id": 42, "app_id": "firefox", "title": "<b>&title</b>", "workspace": 8, "output": "eDP-1",
                    "focused": true, "floating": false, "width": 400, "height": 300, "rect": null })
        );
        assert_eq!(
            value["windows"][1]["rect"],
            json!({ "x": -488.0, "y": 568.0, "width": 400.0, "height": 300.0 })
        );
    }

    #[test]
    fn list_text_is_tab_separated_off_a_terminal_and_aligned_on_one() {
        let snapshot = snapshot();
        let piped = list_text(&snapshot, false);
        assert_eq!(piped.lines().count(), 4);
        assert_eq!(
            piped.lines().next(),
            Some("output\tDP-2\t1920\t0\t2304\t1440\t1.25\t\t")
        );

        let aligned = list_text(&snapshot, true);
        let lines: Vec<&str> = aligned.lines().collect();
        assert_eq!(lines.len(), 5);
        assert!(lines[0].starts_with("KIND"));
        let column = lines[0].find("NAME/ID").expect("a header");
        assert!(
            lines
                .iter()
                .skip(1)
                .all(|line| !line[column..].starts_with(' '))
        );
    }
}
