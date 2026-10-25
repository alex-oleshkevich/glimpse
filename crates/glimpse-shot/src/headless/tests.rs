use std::cell::RefCell;
use std::path::PathBuf;
use std::time::Instant;

use clap::Parser as _;
use glimpse_compositors::model::Logical as OutputLogical;
use glimpse_shot::ports::Rect as WindowRect;
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
            layout: glimpse_wayland::capture::Layout::Rgbx,
        })
    }
}

struct FakeWindows;

impl Windows for FakeWindows {
    fn list(&self) -> Result<Vec<Window>, CaptureError> {
        Ok(layout().windows)
    }

    fn capture(&self, id: u64) -> Result<Frame, CaptureError> {
        Ok(Frame {
            connector: "eDP-1".to_owned(),
            width: 500 + id as u32,
            height: 375,
            pixels: glib::Bytes::from_owned(vec![0u8; 16]),
            alpha: false,
            layout: glimpse_wayland::capture::Layout::Rgbx,
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

#[test]
#[ignore = "needs a display"]
fn file_annotation_reuses_destinations_and_exports_without_capture() {
    gtk4::init().expect("display");
    let dir = tempfile::tempdir().expect("scratch");
    let image = dir.path().join("input.data");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/every_mark.png"),
        &image,
    )
    .expect("PNG fixture");
    let marks_path = dir.path().join("marks.json");
    std::fs::write(
        &marks_path,
        r#"[{"kind":"number","x":20,"y":20,"color":"red","n":77}]"#,
    )
    .expect("marks");
    let files = FakeFiles::default();
    let clipboard = FakeClipboard::default();
    let clock = FakeClock::default();
    let encode = |doc: &Document, factor| {
        assert!(
            matches!(&doc.source, Source::File { path, .. } if *path == image.canonicalize().expect("canonical"))
        );
        assert_eq!(doc.marks[0].0, 0);
        assert_eq!(factor, 1.0);
        encode(doc, factor)
    };
    let deps = Deps {
        screens: &FakeScreens,
        windows: None,
        clipboard: &clipboard,
        files: &files,
        clock: &clock,
        outputs: Vec::new(),
        encode: &encode,
        step: session::step,
    };
    let config = ScreenshotConfig {
        directory: Some(dir.path().into()),
        ..Default::default()
    };
    let mut args = crate::cli::CaptureArgs {
        image: Some(image.clone()),
        annotate: Some(marks_path),
        json: true,
        ..Default::default()
    };
    let mut stdout = Vec::new();
    capture(&args, &config, PIPES, &deps, &mut stdout).expect("default output");
    let value: serde_json::Value = serde_json::from_slice(&stdout).expect("JSON");
    assert_eq!(value["target"]["kind"], "file");
    assert_eq!(
        value["target"]["path"],
        image
            .canonicalize()
            .expect("canonical")
            .to_str()
            .expect("UTF-8")
    );
    assert!(value["output"].is_null());
    assert_eq!(value["scale"], 1.0);
    assert_eq!(value["annotations"][0]["n"], 1);
    assert!(Path::new(value["path"].as_str().expect("path")).starts_with(dir.path()));
    assert_eq!(files.0.borrow().len(), 1);
    args.copy = true;
    args.no_save = true;
    stdout.clear();
    capture(&args, &config, PIPES, &deps, &mut stdout).expect("copy only");
    let value: serde_json::Value = serde_json::from_slice(&stdout).expect("JSON");
    assert!(value["path"].is_null());
    assert_eq!(value["copied"], true);
    assert_eq!(clipboard.0.borrow().len(), 1);
    assert_eq!(files.0.borrow().len(), 1);
    args.copy = false;
    args.no_save = false;
    args.json = false;
    args.output = Some("-".into());
    stdout.clear();
    capture(&args, &config, PIPES, &deps, &mut stdout).expect("PNG stdout");
    assert_eq!(stdout, b"\x89PNG");
    struct BrokenPipe;
    impl Write for BrokenPipe {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    capture(&args, &config, PIPES, &deps, &mut BrokenPipe).expect("closed pipe");
    assert_eq!(files.0.borrow().len(), 1);
}

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
fn a_window_without_a_provider_fails_saying_no_compositor_provider_is_running() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let out = dir.path().join("w.png");
    let files = FakeFiles::default();
    let (exit, _, stderr) = run_with(
        &[
            "capture",
            "--window",
            "42",
            "-o",
            out.to_str().expect("utf-8 path"),
        ],
        &files,
        Vec::new(),
        PIPES,
    );

    assert_eq!(exit, Exit::Failed);
    assert!(
        stderr.contains("no compositor provider is running"),
        "{stderr}"
    );
    assert!(!stderr.contains("NIRI_SOCKET"), "{stderr}");
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
    let dir = tempfile::tempdir().expect("a temporary directory");
    let out = dir.path().join("never.png");
    let files = FakeFiles::default();
    let (exit, _, stderr) = run_driven(
        &["capture", "-o", out.to_str().expect("utf-8 path")],
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
fn without_a_provider_a_region_or_logical_scale_fails_saying_no_compositor_provider_is_running() {
    let files = FakeFiles::default();
    for args in [
        &["capture", "--region", "0,0 10x10"][..],
        &["capture", "--scale", "logical", "--screen", "eDP-1"][..],
    ] {
        let (exit, _, stderr) = run_with(args, &files, Vec::new(), PIPES);
        assert_eq!(exit, Exit::Failed, "{args:?}");
        assert!(
            stderr.contains("no compositor provider is running"),
            "{stderr}"
        );
        assert!(!stderr.contains("NIRI_SOCKET"), "{stderr}");
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

fn layout() -> Layout {
    let output = |connector: &str, x: i32| Output {
        connector: connector.to_owned(),
        enabled: true,
        description: None,
        make: None,
        model: None,
        mode: None,
        logical: Some(OutputLogical {
            x,
            y: 0,
            width: 2304,
            height: 1440,
            scale: 1.25,
        }),
    };
    let window = |id: u64, rect: Option<WindowRect>| Window {
        id,
        title: Some("<b>&title</b>".to_owned()),
        app_id: Some("firefox".to_owned()),
        workspace: Some(8),
        output: Some("eDP-1".to_owned()),
        focused: id == 42,
        floating: rect.is_some(),
        rect,
    };
    Layout {
        outputs: vec![output("DP-2", 1920), output("eDP-1", -1440)],
        workspaces: Vec::new(),
        windows: vec![
            window(42, None),
            window(
                371,
                Some(WindowRect {
                    x: -488.0,
                    y: 568.0,
                    width: 400.0,
                    height: 300.0,
                }),
            ),
        ],
        focused_output: Some("DP-2".to_owned()),
    }
}

#[test]
fn list_json_has_the_documented_shape() {
    let value = list_json(&layout());

    assert_eq!(value["version"], 1);
    assert_eq!(
        value["outputs"][1],
        json!({ "name": "eDP-1", "x": -1440, "y": 0, "width": 2304, "height": 1440, "scale": 1.25, "focused": false })
    );
    assert_eq!(value["outputs"][0]["focused"], true);
    assert_eq!(
        value["windows"][0],
        json!({ "id": 42, "app_id": "firefox", "title": "<b>&title</b>", "workspace": 8, "output": "eDP-1",
                "focused": true, "floating": false, "width": null, "height": null, "rect": null })
    );
    assert_eq!(
        serde_json::to_string(&value["windows"][1]["rect"]).expect("rect"),
        r#"{"height":300.0,"width":400.0,"x":-488.0,"y":568.0}"#
    );
    assert_eq!(
        serde_json::to_string(&value["windows"][1]["width"]).expect("width"),
        "400"
    );
    assert_eq!(
        serde_json::to_string(&value["windows"][1]["height"]).expect("height"),
        "300"
    );

    let mut fractional = layout();
    fractional.windows[1].rect = Some(WindowRect {
        x: 300.5,
        y: 195.5,
        width: 500.5,
        height: 399.5,
    });
    let rounded = list_json(&fractional);
    assert_eq!(
        serde_json::to_string(&rounded["windows"][1]["width"]).expect("width"),
        "501"
    );
    assert_eq!(
        serde_json::to_string(&rounded["windows"][1]["height"]).expect("height"),
        "400"
    );
    assert_eq!(
        serde_json::to_string(&rounded["windows"][1]["rect"]["width"]).expect("rect width"),
        "500.5"
    );
    assert_eq!(rounded["outputs"][1]["x"].to_string(), "-1440");
    assert_eq!(rounded["outputs"][1]["y"].to_string(), "0");
    assert!(rounded["windows"][1].get("x").is_none());
    assert!(rounded["windows"][1].get("y").is_none());
    assert_eq!(
        list_text(&fractional, false)
            .lines()
            .find(|line| line.starts_with("window\t371\t")),
        Some("window\t371\t300.5\t195.5\t501\t400\teDP-1\tfirefox\t<b>&title</b>")
    );
}

#[test]
fn list_text_is_tab_separated_off_a_terminal_and_aligned_on_one() {
    let layout = layout();
    let piped = list_text(&layout, false);
    assert_eq!(piped.lines().count(), 4);
    assert_eq!(
        piped.lines().next(),
        Some("output\tDP-2\t1920\t0\t2304\t1440\t1.25\t\t")
    );
    assert_eq!(
        piped.lines().nth(2),
        Some("window\t42\t-\t-\t-\t-\teDP-1\tfirefox\t<b>&title</b>")
    );
    assert_eq!(
        piped.lines().nth(3),
        Some("window\t371\t-488\t568\t400\t300\teDP-1\tfirefox\t<b>&title</b>")
    );

    let aligned = list_text(&layout, true);
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
