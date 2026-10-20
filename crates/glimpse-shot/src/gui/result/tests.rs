use glimpse_config::{PaletteColor, Stroke};
use glimpse_shot::doc::document::Style;
use glimpse_shot::doc::units::Point;
use glimpse_wayland::capture::Frame;

use super::*;

fn header(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = PNG_SIGNATURE.to_vec();
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes
}

#[test]
fn a_folder_under_home_is_shown_from_the_tilde() {
    let home = Path::new("/home/user");
    assert_eq!(
        home_relative(Path::new("/home/user/Pictures/Screenshots"), home),
        "~/Pictures/Screenshots"
    );
    assert_eq!(home_relative(Path::new("/srv/shots"), home), "/srv/shots");
}

#[test]
fn a_png_larger_than_the_limit_is_refused_from_its_header_alone() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let huge = dir.path().join("huge.png");
    std::fs::write(&huge, header(20_000, 10)).expect("writes");
    let refused = format!("{:#}", load(&huge).expect_err("too large"));
    assert!(refused.contains("20000 × 10"), "{refused}");

    let text = dir.path().join("text.png");
    std::fs::write(&text, b"not an image, but long enough to hold a header").expect("writes");
    assert!(format!("{:#}", load(&text).expect_err("no signature")).contains("not a PNG"));

    assert_eq!(png_size(&header(16384, 16384)), Some((16384, 16384)));
    assert_eq!(png_size(&header(1, 1)[..20]), None);
}

fn document(marks: Vec<(MarkId, Mark)>) -> Document {
    Document {
        source: Source::File {
            path: PathBuf::from("/x.png"),
            frame: Frame {
                connector: String::new(),
                width: 4,
                height: 4,
                pixels: glib::Bytes::from_owned(vec![0u8; 64]),
                alpha: true,
                layout: glimpse_wayland::capture::Layout::Rgbx,
            },
        },
        crop: None,
        marks,
        selected: None,
    }
}

#[test]
fn a_drag_writes_one_private_file_named_after_the_row_under_the_runtime_directory() {
    let runtime = Path::new("/fixture/runtime/glimpse");
    let pid = std::process::id();
    let dir = runtime.join("shot").join(format!("drag-{pid}-7"));
    assert_eq!(
        drag_file(runtime, 7, "  Screenshot from today "),
        dir.join("Screenshot from today.png")
    );
    for hostile in ["", "   ", "../escape", "a/b", ".hidden", "nul\0byte"] {
        assert_eq!(
            drag_file(runtime, 7, hostile),
            dir.join("Screenshot.png"),
            "{hostile:?}"
        );
    }

    let scratch = tempfile::tempdir().expect("a temporary directory");
    let path = drag_file(scratch.path(), 1, "shot");
    write_private(&path, b"first").expect("writes");
    write_private(&path, b"2nd").expect("overwrites");
    assert_eq!(std::fs::read(&path).expect("reads"), b"2nd");
    use std::os::unix::fs::PermissionsExt as _;
    let mode = |path: &Path| {
        std::fs::metadata(path)
            .expect("exists")
            .permissions()
            .mode()
            & 0o777
    };
    assert_eq!(mode(&path), 0o600);
    assert_eq!(mode(path.parent().expect("a directory")), 0o700);
}

#[test]
fn a_drag_offers_the_file_and_the_png_bytes() {
    let provider = drag_content(Path::new("/fixture/runtime/glimpse/shot/x.png"), vec![1, 2]);
    let formats = provider.formats();
    assert!(formats.contain_mime_type("image/png"), "{formats}");
    assert!(
        formats.contains_type(gdk::FileList::static_type()),
        "{formats}"
    );
}

#[test]
fn work_is_dirty_until_it_matches_what_was_last_saved_or_copied() {
    let mark = Mark::Number {
        at: Point {
            x: BufferPx(1),
            y: BufferPx(1),
        },
        style: Style {
            color: PaletteColor::Red,
            stroke: Stroke::Medium,
        },
    };
    let blank = document(Vec::new());
    let marked_doc = document(vec![(0, mark)]);

    assert!(dirty(None, &blank), "a fresh capture was never saved");
    let clean = marked(&blank);
    assert!(!dirty(Some(&clean), &blank), "an opened file is clean");
    assert!(dirty(Some(&clean), &marked_doc));
    let mut selected = blank.clone();
    selected.selected = Some(3);
    assert!(!dirty(Some(&clean), &selected), "selecting is not an edit");
    let mut cropped = blank.clone();
    cropped.crop = Some(Rect {
        x: BufferPx(0),
        y: BufferPx(0),
        width: BufferPx(2),
        height: BufferPx(2),
    });
    assert!(dirty(Some(&clean), &cropped));
}

#[test]
#[ignore = "needs a display"]
fn file_marks_remain_editable_and_answers_survive_failed_saves() {
    use glimpse_shot::doc::document::{Op, Tool};
    glib::set_application_name("Screenshot test");
    gtk4::init().expect("display");
    adw::init().expect("libadwaita");
    glimpse_widgets::register_resources().expect("resources");
    let app = adw::Application::builder()
        .application_id("me.aresa.Glimpse.Shot.FileTest")
        .build();
    app.register(gio::Cancellable::NONE).expect("register");
    let scratch = tempfile::tempdir().expect("scratch");
    let output = scratch.path().join("answer.png");
    let image = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/every_mark.png");
    let style = Style {
        color: PaletteColor::Red,
        stroke: Stroke::Medium,
    };
    let mark = Mark::Number {
        at: Point {
            x: BufferPx(1),
            y: BufferPx(1),
        },
        style,
    };
    let wait_for = |ready: &dyn Fn() -> bool| {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !ready() && std::time::Instant::now() < until {
            let context = glib::MainContext::default();
            while context.pending() {
                context.iteration(false);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(ready(), "operation settled");
    };
    for prefilled in [false, true] {
        let doc = load(&image).expect("PNG");
        let target = TargetV1::File {
            path: image.canonicalize().expect("canonical"),
        };
        let marks = if prefilled {
            vec![mark.clone()]
        } else {
            Vec::new()
        };
        let shot = headless::shot(doc.source, None, &marks, target, Local::now());
        let answer = Rc::new(RefCell::new(None));
        let replied = Rc::clone(&answer);
        let face = open(
            &app,
            adw::ApplicationWindow::builder().application(&app).build(),
            Setup {
                json: true,
                output: Some(output.clone()),
                scale: ScaleMode::Buffer,
                config: ScreenshotConfig::default(),
                config_path: None,
                prompt: Some("Circle <b>this</b> & that".into()),
                timeout: (!prefilled).then_some(0),
            },
            Opened {
                editor: Editor::new(shot.doc, style).starting_with(Some(Tool::Number)),
                target: Some(shot.target),
                taken: None,
                saved: !prefilled,
            },
            &gio::Menu::new().upcast::<gio::MenuModel>(),
            Some(Box::new(move |ended| {
                replied.replace(Some(ended));
            })),
        );
        assert_eq!(face.is_dirty(), prefilled);
        assert_eq!(face.sheet.read(|editor| editor.tool), Some(Tool::Number));
        let content = face
            .window
            .content()
            .and_downcast::<gtk4::Box>()
            .expect("content");
        let banner = content
            .first_child()
            .and_downcast::<ShotBanner>()
            .expect("prompt");
        let body = banner
            .first_child()
            .expect("banner box")
            .last_child()
            .expect("text box")
            .first_child()
            .and_downcast::<gtk4::Label>()
            .expect("prompt label");
        assert_eq!(body.text(), "Circle <b>this</b> & that");
        assert!(!body.uses_markup());
        if !prefilled {
            wait_for(&|| face.is_over());
            assert!(matches!(
                answer.borrow().as_ref(),
                Some(Ended::Finished {
                    outcome: Outcome::TimedOut,
                    ..
                })
            ));
            continue;
        }
        face.edit(|editor| {
            editor.apply(Op::Move(
                0,
                Point {
                    x: BufferPx(100),
                    y: BufferPx(100),
                },
            ));
            editor.apply(Op::Add(mark.clone()));
            assert_eq!(
                editor
                    .doc
                    .marks
                    .iter()
                    .map(|(id, _)| *id)
                    .collect::<Vec<_>>(),
                [0, 1]
            );
            editor.undo();
            assert_eq!(editor.doc.marks.len(), 1);
            editor.redo();
            assert_eq!(editor.doc.marks.len(), 2);
        });
        std::fs::create_dir(&output).expect("unwritable destination");
        face.finish(FinishKey::Save);
        wait_for(&|| {
            matches!(*face.phase.borrow(), Phase::Editing { .. }) && face.png.borrow().is_some()
        });
        assert!(face.is_dirty());
        assert!(answer.borrow().is_none());
        std::fs::remove_dir(&output).expect("allow retry");
        face.finish(FinishKey::Save);
        wait_for(&|| !face.is_dirty());
        let bytes = std::fs::read(&output).expect("saved");
        face.edit(|editor| editor.apply(Op::Remove(1)));
        face.finish(FinishKey::Save);
        wait_for(&|| !face.is_dirty());
        assert_ne!(std::fs::read(&output).expect("saved again"), bytes);
        face.send(Event::Esc);
        let answer = answer.take().expect("one reply");
        let Ended::Finished {
            json: Some(line), ..
        } = answer
        else {
            panic!("JSON answer")
        };
        let value: serde_json::Value = serde_json::from_str(&line).expect("JSON");
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
        assert_eq!(value["annotations"].as_array().expect("marks").len(), 1);
    }
}

#[test]
#[ignore = "needs a display"]
fn text_entry_has_a_readable_height_and_grows_with_lines() {
    gtk4::init().expect("nested display");
    adw::init().expect("libadwaita");
    assert!(glimpse_widgets::register_resources().is_ok());
    let _styles = glimpse_widgets::Styles::install(adw::ColorScheme::Default);
    let app = adw::Application::builder()
        .application_id("me.aresa.Glimpse.Shot.TextEntryTest")
        .build();
    app.register(gio::Cancellable::NONE)
        .expect("register application");
    let window = adw::ApplicationWindow::builder()
        .application(&app)
        .default_width(900)
        .default_height(700)
        .build();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/every_mark.png");
    let editor = Editor::new(
        load(&path).expect("sample image"),
        Style {
            color: PaletteColor::Red,
            stroke: Stroke::Medium,
        },
    );
    let menu = gio::Menu::new().upcast::<gio::MenuModel>();
    let face = open(
        &app,
        window,
        Setup {
            json: false,
            output: None,
            scale: ScaleMode::Buffer,
            config: ScreenshotConfig::default(),
            config_path: None,
            prompt: None,
            timeout: None,
        },
        Opened {
            editor,
            target: None,
            taken: None,
            saved: false,
        },
        &menu,
        None,
    );
    let settle = || {
        let context = glib::MainContext::default();
        for _ in 0..30 {
            while context.pending() {
                context.iteration(false);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    };
    settle();
    assert!(face.picture.is_mapped(), "result picture is mapped");
    face.sheet.with(|editor| {
        editor.typing = Some(Point {
            x: BufferPx(20),
            y: BufferPx(20),
        });
    });
    face.sync();
    let popover = face.typing.borrow().clone().expect("text popover");
    let scroller = popover
        .child()
        .and_downcast::<gtk4::ScrolledWindow>()
        .expect("text scroller");
    let entry = scroller
        .child()
        .and_downcast::<gtk4::TextView>()
        .expect("text view");
    settle();
    assert!(popover.is_mapped(), "text popover is mapped");
    let initial = scroller.height();
    entry.buffer().set_text("first\nsecond\nthird\nfourth");
    settle();
    let grown = scroller.height();
    assert!(initial >= 40, "text entry is only {initial} pixels tall");
    assert!(
        grown > initial + 10,
        "text entry did not grow: {initial} -> {grown}"
    );
    entry.buffer().set_text(&"line\n".repeat(100));
    settle();
    assert!(scroller.height() <= (face.picture.height() * 2 / 3).max(180));
    let adjustment = scroller.vadjustment();
    assert!(adjustment.upper() > adjustment.page_size());
}
