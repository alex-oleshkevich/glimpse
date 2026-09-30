use std::cell::{Cell, RefCell};
use std::io::{Read as _, Write as _};
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use adw::prelude::*;
use anyhow::{Context as _, bail};
use chrono::{DateTime, Local};
use gettextrs::gettext;
use glimpse_config::ScreenshotConfig;
use glimpse_shot::doc::document::{Document, Mark, MarkId, Source};
use glimpse_shot::doc::esc::EscOutcome;
use glimpse_shot::doc::naming::file_name;
use glimpse_shot::doc::units::{BufferPx, Rect};
use glimpse_shot::doc::wire::TargetV1;
use glimpse_shot::ports::Clipboard as _;
use glimpse_shot::session::{self, Effect, Event, Face, FinishKey, Outcome, Phase};
use glimpse_wayland::selection::Entry;
use glimpse_widgets::ShotResultPage;
use gtk4::{gdk, gio, glib};

use super::canvas::{
    Editor, Sheet, palette, palette_index, resize_text_entry, set_cursor, spring_tool, stroke,
    text_entry, tool_name, tool_named,
};
use super::overlay::{Ended, result_line};
use crate::adapters::clipboard::HoldClipboard;
use crate::adapters::files::Disk;
use crate::adapters::windows::{decode, frame_of};
use crate::cli::ScaleMode;
use crate::finish::{self, Destination};
use crate::headless;
use crate::render::{self, Png};

const LARGEST_SIDE: u32 = 16384;
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
const DRAG_FALLBACK: &str = "Screenshot";

static FACES: AtomicU64 = AtomicU64::new(0);

pub struct Setup {
    pub json: bool,
    pub output: Option<PathBuf>,
    pub scale: ScaleMode,
    pub config: ScreenshotConfig,
    pub config_path: Option<PathBuf>,
}

pub struct Opened {
    pub editor: Editor,
    pub target: Option<TargetV1>,
    pub taken: Option<DateTime<Local>>,
    pub saved: bool,
}

type Answer = Box<dyn FnOnce(Ended)>;
type Act = Box<dyn Fn(&Rc<ResultFace>)>;
type Marked = (Vec<(MarkId, Mark)>, Option<Rect<BufferPx>>);

pub struct ResultFace {
    app: adw::Application,
    window: adw::ApplicationWindow,
    page: ShotResultPage,
    picture: gtk4::Picture,
    sheet: Sheet,
    setup: Setup,
    clipboard: HoldClipboard,
    target: RefCell<Option<TargetV1>>,
    taken: DateTime<Local>,
    phase: RefCell<Phase>,
    png: RefCell<Option<Png>>,
    destination: RefCell<Option<Destination>>,
    folder: RefCell<Option<PathBuf>>,
    written: RefCell<Option<PathBuf>>,
    clean: RefCell<Option<Marked>>,
    closing: Cell<Option<bool>>,
    ask_app: Cell<bool>,
    enter_held: Cell<bool>,
    space_held: Cell<bool>,
    over: Cell<bool>,
    typing: RefCell<Option<gtk4::Popover>>,
    answer: RefCell<Option<Answer>>,
    id: u64,
    dragged: RefCell<Option<PathBuf>>,
}

pub fn open(
    app: &adw::Application,
    window: adw::ApplicationWindow,
    setup: Setup,
    opened: Opened,
    menu: &gio::MenuModel,
    answer: Option<Answer>,
) -> Rc<ResultFace> {
    let page = ShotResultPage::new();
    page.set_menu_model(Some(menu));
    page.toolbar().set_can_focus(false);
    let picture = page.picture();
    picture.set_focusable(true);
    picture.set_cursor_from_name(Some("crosshair"));
    let clean = opened.saved.then(|| marked(&opened.editor.doc));
    let sheet = Sheet::new(opened.editor);
    picture.set_paintable(Some(&sheet));
    let face = Rc::new(ResultFace {
        app: app.clone(),
        window,
        clipboard: HoldClipboard {
            config: setup.config_path.clone(),
        },
        setup,
        page,
        picture,
        sheet,
        target: RefCell::new(opened.target),
        taken: opened.taken.unwrap_or_else(Local::now),
        phase: RefCell::new(Phase::Editing {
            saved: None,
            copied: false,
            face: Face {
                headless: false,
                notify: false,
            },
        }),
        png: RefCell::default(),
        destination: RefCell::default(),
        folder: RefCell::default(),
        written: RefCell::default(),
        clean: RefCell::new(clean),
        closing: Cell::default(),
        ask_app: Cell::new(false),
        enter_held: Cell::new(false),
        space_held: Cell::new(false),
        over: Cell::new(false),
        typing: RefCell::default(),
        answer: RefCell::new(answer),
        id: FACES.fetch_add(1, Ordering::Relaxed),
        dragged: RefCell::default(),
    });
    face.place();
    face.fill_rows();
    face.wire();
    face.sync();
    face
}

pub fn load(path: &Path) -> anyhow::Result<Document> {
    let mut header = [0u8; 24];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .with_context(|| format!("cannot read {}", path.display()))?;
    let Some((width, height)) = png_size(&header) else {
        bail!("{} is not a PNG image", path.display());
    };
    if width > LARGEST_SIDE || height > LARGEST_SIDE {
        bail!(
            "{} is {width} × {height}; the largest side opened is {LARGEST_SIDE} pixels",
            path.display()
        );
    }
    let frame = decode(path).map_err(anyhow::Error::new)?;
    Ok(Document {
        source: Source::File {
            path: path.to_path_buf(),
            frame,
        },
        crop: None,
        marks: Vec::new(),
        selected: None,
    })
}

pub fn pasted(texture: &gdk::Texture) -> anyhow::Result<Document> {
    let (width, height) = (
        texture.width().unsigned_abs(),
        texture.height().unsigned_abs(),
    );
    if width > LARGEST_SIDE || height > LARGEST_SIDE {
        bail!("the image is {width} × {height}; the largest side opened is {LARGEST_SIDE} pixels");
    }
    Ok(Document {
        source: Source::File {
            path: PathBuf::new(),
            frame: frame_of(texture),
        },
        crop: None,
        marks: Vec::new(),
        selected: None,
    })
}

fn png_size(header: &[u8]) -> Option<(u32, u32)> {
    if header.get(..8)? != PNG_SIGNATURE || header.get(12..16)? != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(header.get(16..20)?.try_into().ok()?);
    let height = u32::from_be_bytes(header.get(20..24)?.try_into().ok()?);
    Some((width, height))
}

fn marked(doc: &Document) -> Marked {
    (doc.marks.clone(), doc.crop)
}

fn dirty(clean: Option<&Marked>, doc: &Document) -> bool {
    clean.is_none_or(|(marks, crop)| *marks != doc.marks || *crop != doc.crop)
}

impl ResultFace {
    pub fn window(&self) -> &adw::ApplicationWindow {
        &self.window
    }

    pub fn is_over(&self) -> bool {
        self.over.get()
    }

    fn place(&self) {
        self.window.set_content(Some(&self.page));
        self.window.present();
        self.picture.grab_focus();
    }

    fn fill_rows(&self) {
        let (folder, name) =
            match Destination::of(self.setup.output.as_deref(), false, &self.setup.config) {
                Ok(Destination::File(path)) => (
                    path.parent().map(Path::to_path_buf),
                    path.file_stem()
                        .map(|stem| stem.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                ),
                Ok(Destination::Directory(dir)) => (Some(dir), self.default_name()),
                _ => (None, self.default_name()),
            };
        self.page.set_name(&name);
        self.show_name();
        self.folder.replace(folder);
    }

    fn show_name(&self) {
        let title = format!("{}.png", self.page.name());
        if self.window.title().as_deref() != Some(title.as_str()) {
            self.window.set_title(Some(&title));
        }
    }

    fn default_name(&self) -> String {
        let name = file_name(&self.setup.config.filename, self.taken);
        name.strip_suffix(".png").unwrap_or(&name).to_owned()
    }

    fn wire(self: &Rc<Self>) {
        let toolbar = self.page.toolbar();
        let weak = Rc::downgrade(self);
        toolbar.connect_tool_selected(move |_, name| {
            if let (Some(face), Some(tool)) = (weak.upgrade(), tool_named(&name)) {
                face.page.hand().set_active(false);
                face.edit(|editor| editor.set_tool(tool));
            }
        });
        let weak = Rc::downgrade(self);
        self.page.hand().connect_toggled(move |_| {
            if let Some(face) = weak.upgrade() {
                face.sync();
            }
        });
        let weak = Rc::downgrade(self);
        toolbar.connect_color_selected(move |_, color| {
            if let Some(face) = weak.upgrade() {
                face.edit(|editor| editor.style.color = palette(color));
            }
        });
        let weak = Rc::downgrade(self);
        toolbar.connect_stroke_selected(move |_, width| {
            if let Some(face) = weak.upgrade() {
                face.edit(|editor| editor.style.stroke = stroke(width));
            }
        });
        let weak = Rc::downgrade(self);
        toolbar.connect_action(move |_, action| {
            let Some(face) = weak.upgrade() else {
                return;
            };
            match action.as_str() {
                "undo" => face.edit(Editor::undo),
                "redo" => face.edit(Editor::redo),
                "apply-crop" => face.edit(|editor| {
                    editor.apply_crop();
                }),
                _ => {}
            }
        });

        let weak = Rc::downgrade(self);
        self.page.connect_copy(move |_| {
            if let Some(face) = weak.upgrade() {
                face.finish(FinishKey::Copy);
            }
        });
        let weak = Rc::downgrade(self);
        self.page.connect_save(move |_| {
            if let Some(face) = weak.upgrade() {
                face.finish(FinishKey::Save);
            }
        });
        let weak = Rc::downgrade(self);
        self.page.connect_open_with(move |_| {
            if let Some(face) = weak.upgrade() {
                face.open_with();
            }
        });
        let weak = Rc::downgrade(self);
        self.page.connect_save_as(move |_| {
            if let Some(face) = weak.upgrade() {
                face.save_as();
            }
        });
        let weak = Rc::downgrade(self);
        self.page.connect_new(move |_| {
            if let Some(face) = weak.upgrade() {
                face.request_close(true);
            }
        });
        let weak = Rc::downgrade(self);
        self.page.connect_name_changed(move |_| {
            if let Some(face) = weak.upgrade() {
                face.show_name();
                face.check_collision();
            }
        });
        let weak = Rc::downgrade(self);
        self.window
            .connect_close_request(move |_| match weak.upgrade() {
                Some(face) if !face.over.get() => {
                    face.request_close(false);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            });

        self.wire_pointer();
        self.wire_keys();
    }

    fn wire_pointer(self: &Rc<Self>) {
        let pan = gtk4::GestureDrag::new();
        pan.set_button(gdk::BUTTON_PRIMARY);
        pan.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let scroller = self.page.scroller();
        let start = Rc::new(Cell::new((0.0, 0.0)));
        let weak = Rc::downgrade(self);
        let origin = Rc::clone(&start);
        let adjust = scroller.clone();
        pan.connect_drag_begin(move |gesture, _, _| {
            let Some(face) = weak.upgrade().filter(|face| face.panning()) else {
                gesture.set_state(gtk4::EventSequenceState::Denied);
                return;
            };
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            origin.set((adjust.hadjustment().value(), adjust.vadjustment().value()));
            set_cursor(&face.picture, "grabbing");
        });
        let adjust = scroller.clone();
        pan.connect_drag_update(move |_, dx, dy| {
            let (x, y) = start.get();
            adjust.hadjustment().set_value(x - dx);
            adjust.vadjustment().set_value(y - dy);
        });
        let weak = Rc::downgrade(self);
        pan.connect_drag_end(move |_, _, _| {
            if let Some(face) = weak.upgrade() {
                face.sync();
            }
        });
        scroller.add_controller(pan);

        let drag = gtk4::GestureDrag::new();
        drag.set_button(gdk::BUTTON_PRIMARY);
        let weak = Rc::downgrade(self);
        drag.connect_drag_begin(move |gesture, x, y| {
            if held(gesture, gdk::ModifierType::CONTROL_MASK) {
                gesture.set_state(gtk4::EventSequenceState::Denied);
                return;
            }
            if let Some(face) = weak.upgrade() {
                face.picture.grab_focus();
                let (x, y) = face.local(x, y);
                face.edit(|editor| editor.press(x, y));
            }
        });
        let weak = Rc::downgrade(self);
        drag.connect_drag_update(move |gesture, dx, dy| {
            let shift = shifted(gesture);
            if let (Some(face), Some((x, y))) = (weak.upgrade(), gesture.start_point()) {
                let (x, y) = face.local(x + dx, y + dy);
                face.sheet.with(|editor| editor.motion(x, y, shift));
            }
        });
        let weak = Rc::downgrade(self);
        drag.connect_drag_end(move |gesture, dx, dy| {
            let shift = shifted(gesture);
            if let (Some(face), Some((x, y))) = (weak.upgrade(), gesture.start_point()) {
                let (x, y) = face.local(x + dx, y + dy);
                face.edit(|editor| editor.release(x, y, shift));
            }
        });
        self.picture.add_controller(drag);

        let hover = gtk4::EventControllerMotion::new();
        let weak = Rc::downgrade(self);
        hover.connect_motion(move |controller, x, y| {
            if controller
                .current_event_state()
                .intersects(gdk::ModifierType::BUTTON1_MASK | gdk::ModifierType::BUTTON3_MASK)
            {
                return;
            }
            if let Some(face) = weak.upgrade() {
                let (x, y) = face.local(x, y);
                let name = match face.panning() {
                    true => "grab",
                    false => face.sheet.hover(x, y).unwrap_or("crosshair"),
                };
                set_cursor(&face.picture, name);
            }
        });
        hover.connect_leave(|controller| {
            if let Some(widget) = controller.widget() {
                set_cursor(&widget, "crosshair");
            }
        });
        self.picture.add_controller(hover);

        let resize = gtk4::GestureDrag::new();
        resize.set_button(gdk::BUTTON_SECONDARY);
        let weak = Rc::downgrade(self);
        resize.connect_drag_begin(move |gesture, x, y| {
            if shifted(gesture)
                && let Some(face) = weak.upgrade()
            {
                face.picture.grab_focus();
                let (x, y) = face.local(x, y);
                face.edit(|editor| {
                    editor.press_resize(x, y);
                });
            }
        });
        let weak = Rc::downgrade(self);
        resize.connect_drag_update(move |gesture, dx, dy| {
            if let (Some(face), Some((x, y))) = (weak.upgrade(), gesture.start_point()) {
                let (x, y) = face.local(x + dx, y + dy);
                face.sheet.with(|editor| {
                    if editor.resizing_mark() {
                        editor.motion(x, y, false);
                    }
                });
            }
        });
        let weak = Rc::downgrade(self);
        resize.connect_drag_end(move |gesture, dx, dy| {
            if let (Some(face), Some((x, y))) = (weak.upgrade(), gesture.start_point()) {
                let (x, y) = face.local(x + dx, y + dy);
                face.edit(|editor| {
                    if editor.resizing_mark() {
                        editor.release(x, y, false);
                    }
                });
            }
        });
        self.picture.add_controller(resize.clone());

        let undo = gtk4::GestureClick::new();
        undo.set_button(gdk::BUTTON_SECONDARY);
        let weak = Rc::downgrade(self);
        undo.connect_pressed(move |gesture, _, _, _| {
            if gesture
                .current_event_state()
                .contains(gdk::ModifierType::SHIFT_MASK)
            {
                return;
            }
            if let Some(face) = weak.upgrade() {
                face.edit(Editor::undo);
            }
        });
        self.picture.add_controller(undo.clone());
        undo.group_with(&resize);

        let source = gtk4::DragSource::new();
        source.set_actions(gdk::DragAction::COPY);
        let weak = Rc::downgrade(self);
        source.connect_prepare(move |source, _, _| {
            if !held(source, gdk::ModifierType::CONTROL_MASK) {
                return None;
            }
            weak.upgrade()?.drag_out()
        });
        self.picture.add_controller(source);
    }

    fn drag_out(&self) -> Option<gdk::ContentProvider> {
        let doc = self.doc()?;
        let png = render::export(&doc, headless::factor(self.setup.scale, &doc))
            .inspect_err(|error| tracing::warn!("cannot render the drag: {error:#}"))
            .ok()?;
        let runtime = glimpse_config::runtime_dir()?;
        let path = drag_file(&runtime, self.id, &self.page.name());
        if let Some(previous) = self.dragged.replace(Some(path.clone()))
            && previous != path
        {
            let _ = std::fs::remove_file(previous);
        }
        if let Err(error) = write_private(&path, &png.bytes) {
            tracing::warn!("cannot write {}: {error}", path.display());
            return None;
        }
        Some(drag_content(&path, png.bytes))
    }

    fn forget_drag(&self) {
        if let Some(path) = self.dragged.take() {
            let _ = std::fs::remove_file(&path);
            if let Some(dir) = path.parent() {
                let _ = std::fs::remove_dir(dir);
            }
        }
    }

    fn wire_keys(self: &Rc<Self>) {
        let shortcuts = gtk4::ShortcutController::new();
        let bind = |trigger: &str, act: Act| {
            let weak = Rc::downgrade(self);
            let action = gtk4::CallbackAction::new(move |_, _| {
                let Some(face) = weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                act(&face);
                glib::Propagation::Stop
            });
            shortcuts.add_shortcut(gtk4::Shortcut::new(
                gtk4::ShortcutTrigger::parse_string(trigger),
                Some(action),
            ));
        };
        for (key, tool) in [
            ("c", "crop"),
            ("a", "arrow"),
            ("l", "line"),
            ("p", "freehand"),
            ("h", "highlighter"),
            ("f", "filled-rect"),
            ("o", "oval"),
            ("t", "text"),
        ] {
            bind(
                key,
                Box::new(move |face| {
                    if let Some(tool) = tool_named(tool) {
                        face.page.hand().set_active(false);
                        face.edit(|editor| editor.set_tool(tool));
                    }
                }),
            );
        }
        for index in 0..5u32 {
            bind(
                &(index + 1).to_string(),
                Box::new(move |face| face.edit(|editor| editor.style.color = palette(index))),
            );
        }
        bind("bracketleft", Box::new(|face| face.step_stroke(-1)));
        bind("bracketright", Box::new(|face| face.step_stroke(1)));
        bind("<Control>z", Box::new(|face| face.edit(Editor::undo)));
        bind(
            "<Control><Shift>z",
            Box::new(|face| face.edit(Editor::redo)),
        );
        bind("Delete", Box::new(|face| face.edit(Editor::delete)));
        bind("Escape", Box::new(|face| face.escape()));
        bind("Return", Box::new(|face| face.enter()));
        bind("KP_Enter", Box::new(|face| face.enter()));
        bind(
            "<Shift>Return",
            Box::new(|face| face.finish(FinishKey::SaveAsClose)),
        );
        bind(
            "<Shift>KP_Enter",
            Box::new(|face| face.finish(FinishKey::SaveAsClose)),
        );
        bind("<Control><Shift>s", Box::new(|face| face.save_as()));
        bind("<Control>c", Box::new(|face| face.finish(FinishKey::Copy)));
        bind("<Control>s", Box::new(|face| face.finish(FinishKey::Save)));
        bind("<Control>0", Box::new(|face| face.page.set_zoom("fit")));
        bind("<Control>1", Box::new(|face| face.page.set_zoom("actual")));
        bind(
            "<Control>plus|<Control>equal|<Control>KP_Add",
            Box::new(|face| face.page.zoom_by(1.0)),
        );
        bind(
            "<Control>minus|<Control>KP_Subtract",
            Box::new(|face| face.page.zoom_by(-1.0)),
        );
        bind("<Control>w", Box::new(|face| face.request_close(false)));
        bind("<Control>n", Box::new(|face| face.request_close(true)));
        self.page.add_controller(shortcuts);

        let released = gtk4::EventControllerKey::new();
        released.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        released.connect_key_pressed(move |_, key, _, state| {
            if key == gdk::Key::space {
                let Some(face) = weak.upgrade().filter(|face| face.picture.has_focus()) else {
                    return glib::Propagation::Proceed;
                };
                if !face.space_held.replace(true) {
                    face.sync();
                }
                return glib::Propagation::Stop;
            }
            let Some(tool) = spring_tool(key) else {
                return glib::Propagation::Proceed;
            };
            if state.intersects(
                gdk::ModifierType::CONTROL_MASK
                    | gdk::ModifierType::ALT_MASK
                    | gdk::ModifierType::SHIFT_MASK
                    | gdk::ModifierType::SUPER_MASK
                    | gdk::ModifierType::META_MASK,
            ) {
                return glib::Propagation::Proceed;
            }
            let Some(face) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if face.typing.borrow().is_some() {
                return glib::Propagation::Proceed;
            }
            if face.sheet.with(|editor| editor.tool_key_down(tool)) == Some(true) {
                face.sync();
            }
            glib::Propagation::Stop
        });
        let weak = Rc::downgrade(self);
        released.connect_key_released(move |_, key, _, _| {
            if let Some(face) = weak.upgrade() {
                if matches!(key, gdk::Key::Return | gdk::Key::KP_Enter) {
                    face.enter_held.set(false);
                }
                if key == gdk::Key::space && face.space_held.replace(false) {
                    face.sync();
                }
                if let Some(tool) = spring_tool(key)
                    && face.sheet.with(|editor| editor.tool_key_up(tool)) == Some(true)
                {
                    face.sync();
                }
            }
        });
        self.page.add_controller(released);

        let weak = Rc::downgrade(self);
        self.window().connect_is_active_notify(move |window| {
            if !window.is_active()
                && let Some(face) = weak.upgrade()
            {
                let released = face.space_held.replace(false);
                if face.sheet.with(Editor::cancel_tool_key).unwrap_or(false) || released {
                    face.sync();
                }
            }
        });
    }

    fn open_with(self: &Rc<Self>) {
        self.ask_app.set(true);
        self.finish(FinishKey::Save);
    }

    fn panning(&self) -> bool {
        self.space_held.get() || self.page.hand().is_active()
    }

    fn local(&self, x: f64, y: f64) -> (f64, f64) {
        let (left, top) = self
            .sheet
            .origin(self.picture.width(), self.picture.height());
        (x - left, y - top)
    }

    fn step_stroke(self: &Rc<Self>, by: i32) {
        self.edit(|editor| editor.step_stroke(by));
    }

    fn edit(self: &Rc<Self>, change: impl FnOnce(&mut Editor)) {
        self.sheet.with(change);
        self.sync();
        if self.typing.borrow().is_none() {
            self.picture.grab_focus();
        }
    }

    fn enter(self: &Rc<Self>) {
        if self.enter_held.get() {
            return;
        }
        match self.sheet.with(Editor::apply_crop) {
            Some(true) => {
                self.enter_held.set(true);
                self.sync();
            }
            _ => self.finish(FinishKey::CopyClose),
        }
    }

    fn escape(self: &Rc<Self>) {
        match self.sheet.with(Editor::escape) {
            Some(EscOutcome::Cancel) | None => self.request_close(false),
            Some(_) => self.sync(),
        }
    }

    fn sync(self: &Rc<Self>) {
        let Some(state) = self.sheet.read(|editor| {
            let exported = editor.doc.exported_rect();
            (
                editor.tool,
                editor.style,
                editor.history.can_undo(),
                editor.history.can_redo(),
                editor.typing,
                format!("{} × {}", exported.width.0, exported.height.0),
                editor.pending_crop.is_some(),
                editor.cursor(),
            )
        }) else {
            return;
        };
        let (tool, style, undo, redo, typing, size, pending, cursor) = state;
        set_cursor(&self.picture, if self.panning() { "grab" } else { cursor });
        let toolbar = self.page.toolbar();
        toolbar.set_apply_visible(pending);
        toolbar.set_tool(tool_name(tool));
        toolbar.set_color(palette_index(style.color));
        toolbar.set_stroke(style.stroke as u32);
        self.page.set_can_undo(undo);
        self.page.set_can_redo(redo);
        self.page.set_size_text(&size);
        let open = self.typing.borrow().is_some();
        if typing.is_some() && !open {
            self.ask_text();
        }
    }

    fn ask_text(self: &Rc<Self>) {
        let Some(at) = self
            .sheet
            .read(|editor| editor.typing.map(|point| editor.view().to_logical(point)))
        else {
            return;
        };
        let Some((x, y)) = at else {
            return;
        };
        let (left, top) = self
            .sheet
            .origin(self.picture.width(), self.picture.height());
        let max_height = (self.picture.height() * 2 / 3).max(180);
        let (scroller, entry) = text_entry();
        entry.add_css_class("shot-text-entry");
        scroller.set_max_content_height(max_height);
        let popover = gtk4::Popover::builder().child(&scroller).build();
        popover.set_parent(&self.picture);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(
            (x + left).round() as i32,
            (y + top).round() as i32,
            1,
            1,
        )));
        entry.buffer().connect_changed(glib::clone!(
            #[weak]
            entry,
            #[weak]
            scroller,
            #[weak]
            popover,
            move |_| {
                let entry = entry.clone();
                let scroller = scroller.clone();
                let popover = popover.clone();
                glib::idle_add_local_once(move || {
                    if popover.is_visible() {
                        resize_text_entry(&entry, &scroller, max_height);
                        popover.present();
                    }
                });
            }
        ));
        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |controller, key, _, state| {
            let entered = matches!(key, gdk::Key::Return | gdk::Key::KP_Enter)
                && !state.contains(gdk::ModifierType::SHIFT_MASK);
            let (Some(face), true) = (weak.upgrade(), entered) else {
                return glib::Propagation::Proceed;
            };
            if let Some(view) = controller.widget().and_downcast::<gtk4::TextView>() {
                let buffer = view.buffer();
                let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
                face.sheet.with(|editor| editor.commit_text(&text));
            }
            face.enter_held.set(true);
            face.close_text();
            glib::Propagation::Stop
        });
        entry.add_controller(keys);
        let weak = Rc::downgrade(self);
        popover.connect_closed(move |_| {
            if let Some(face) = weak.upgrade() {
                face.sheet.with(|editor| editor.typing = None);
                face.close_text();
            }
        });
        self.typing.replace(Some(popover.clone()));
        popover.popup();
        entry.grab_focus();
    }

    fn close_text(self: &Rc<Self>) {
        let popover = self.typing.take();
        if let Some(popover) = popover {
            let weak = Rc::downgrade(self);
            glib::idle_add_local_once(move || {
                popover.unparent();
                if let Some(face) = weak.upgrade() {
                    face.picture.grab_focus();
                    face.sync();
                }
            });
        }
    }

    fn doc(&self) -> Option<Document> {
        self.sheet.read(|editor| editor.doc.clone())
    }

    fn is_dirty(&self) -> bool {
        self.sheet
            .read(|editor| dirty(self.clean.borrow().as_ref(), &editor.doc))
            .unwrap_or(false)
    }

    fn requested(&self) -> anyhow::Result<(Destination, PathBuf)> {
        let name = self.page.name();
        let name = name.trim();
        if name.is_empty() || name.contains('/') {
            bail!("{}", gettext("A file name cannot be empty or contain “/”"));
        }
        let Some(folder) = self.folder.borrow().clone() else {
            bail!(
                "{}",
                gettext("There is no Pictures folder to save into; choose a folder in Preferences")
            );
        };
        let path = folder.join(format!("{name}.png"));
        let own = self.written.borrow().clone();
        let explicit = self.setup.output.clone();
        Ok(
            match [own, explicit]
                .into_iter()
                .flatten()
                .find(|own| *own == path)
            {
                Some(own) => (Destination::File(own.clone()), own),
                None => (Destination::Directory(folder), path),
            },
        )
    }

    fn check_collision(&self) {
        let taken = self.requested().is_ok_and(|(destination, path)| {
            matches!(destination, Destination::Directory(_)) && path.exists()
        });
        self.page.set_collision(taken);
    }

    fn finish(self: &Rc<Self>, key: FinishKey) {
        let path = match key {
            FinishKey::Save | FinishKey::SaveAsClose => match self.requested() {
                Ok((destination, path)) => {
                    self.destination.replace(Some(destination));
                    Some(path)
                }
                Err(error) => {
                    self.ask_app.set(false);
                    self.closing.set(None);
                    self.toast(&format!("{error:#}"), None);
                    return;
                }
            },
            FinishKey::Copy | FinishKey::CopyClose if self.setup.json => match self.requested() {
                Ok((destination, path)) => {
                    self.destination.replace(Some(destination));
                    Some(path)
                }
                Err(_) => None,
            },
            _ => None,
        };
        self.send(Event::Finish(key, path));
    }

    fn save_as(self: &Rc<Self>) {
        let dialog = gtk4::FileDialog::builder()
            .title(gettext("Save Screenshot"))
            .modal(true)
            .initial_name(format!("{}.png", self.page.name().trim()))
            .build();
        if let Some(folder) = self.folder.borrow().as_ref().filter(|dir| dir.is_dir()) {
            dialog.set_initial_folder(Some(&gio::File::for_path(folder)));
        }
        let weak = Rc::downgrade(self);
        dialog.save(
            Some(self.window()),
            None::<&gio::Cancellable>,
            move |chosen| {
                let Some(face) = weak.upgrade() else {
                    return;
                };
                if let Some(path) = chosen.ok().and_then(|file| file.path()) {
                    face.destination
                        .replace(Some(Destination::File(path.clone())));
                    face.send(Event::Finish(FinishKey::SaveAsClose, Some(path)));
                }
            },
        );
    }

    pub fn request_close(self: &Rc<Self>, then_new: bool) {
        if self.over.get() {
            return;
        }
        if !self.is_dirty() {
            self.close(then_new);
            return;
        }
        let dialog = adw::AlertDialog::new(
            Some(&gettext("Discard this screenshot?")),
            Some(&gettext("It has not been saved or copied.")),
        );
        dialog.add_responses(&[
            ("cancel", &gettext("Cancel")),
            ("discard", &gettext("Discard")),
            ("save", &gettext("Save")),
        ]);
        dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));
        dialog.set_close_response("cancel");
        let weak = Rc::downgrade(self);
        dialog.choose(
            Some(self.window()),
            None::<&gio::Cancellable>,
            move |response| {
                let Some(face) = weak.upgrade() else {
                    return;
                };
                match response.as_str() {
                    "discard" => face.close(then_new),
                    "save" => {
                        face.closing.set(Some(then_new));
                        face.finish(FinishKey::Save);
                    }
                    _ => {}
                }
            },
        );
    }

    fn close(self: &Rc<Self>, then_new: bool) {
        self.send(Event::Esc);
        if then_new {
            self.app.activate_action("new", None);
        }
    }

    fn send(self: &Rc<Self>, event: Event) {
        if self.over.get() {
            return;
        }
        let phase = self.phase.replace(Phase::Done(Outcome::Cancelled));
        let (phase, effects) = session::step(phase, event);
        self.phase.replace(phase);
        let events: Vec<Event> = effects
            .into_iter()
            .filter_map(|effect| self.perform(effect))
            .collect();
        for event in events {
            self.send(event);
        }
        let done = match &*self.phase.borrow() {
            Phase::Done(outcome) => Some(outcome.clone()),
            _ => None,
        };
        match done {
            Some(outcome) => self.end(outcome),
            None => match self.closing.get() {
                Some(then_new) if !self.is_dirty() => self.close(then_new),
                _ => {}
            },
        }
    }

    fn perform(self: &Rc<Self>, effect: Effect) -> Option<Event> {
        match effect {
            Effect::Encode => {
                let encoded = self
                    .doc()
                    .ok_or_else(|| anyhow::anyhow!("nothing to encode"))
                    .and_then(|doc| render::export(&doc, headless::factor(self.setup.scale, &doc)));
                let result = encoded
                    .map(|png| drop(self.png.replace(Some(png))))
                    .map_err(|error| format!("{error:#}"));
                Some(Event::Effected(Effect::Encode, result))
            }
            Effect::Write(requested) => {
                let written = match (&*self.png.borrow(), &*self.destination.borrow()) {
                    (Some(png), Some(destination)) => finish::write(
                        destination,
                        &requested,
                        &Disk,
                        &png.bytes,
                        &mut std::io::sink(),
                    )
                    .map_err(|error| format!("{error:#}")),
                    _ => Err("nothing was encoded".to_owned()),
                };
                Some(match written {
                    Ok(actual) => {
                        self.saved(&actual);
                        Event::Effected(Effect::Write(actual), Ok(()))
                    }
                    Err(reason) => {
                        self.closing.set(None);
                        self.ask_app.set(false);
                        Event::Effected(Effect::Write(requested), Err(reason))
                    }
                })
            }
            Effect::Copy => {
                let entry = self.png.borrow().as_ref().map(|png| Entry {
                    mime: "image/png".to_owned(),
                    data: Arc::from(png.bytes.as_slice()),
                });
                let copied = match entry {
                    Some(entry) => self.clipboard.copy(vec![entry]),
                    None => Err("nothing was encoded".to_owned()),
                };
                if copied.is_ok() {
                    self.mark_clean();
                }
                Some(Event::Effected(Effect::Copy, copied))
            }
            Effect::ShowToast(text) => {
                match text.as_str() {
                    "Saved" => self.saved_toast(),
                    "Copied" => self.toast(&gettext("Copied to clipboard"), None),
                    other => {
                        self.closing.set(None);
                        self.toast(other, None);
                    }
                }
                None
            }
            Effect::Share(path) => Some(Event::Effected(
                Effect::Share(path),
                Err("the result window does not share".to_owned()),
            )),
            Effect::Capture(_) | Effect::Notify(_) | Effect::OpenInWindow => None,
        }
    }

    fn saved(&self, path: &Path) {
        self.written.replace(Some(path.to_path_buf()));
        self.mark_clean();
        let file = gio::File::for_path(path);
        gtk4::RecentManager::default().add_item(&file.uri());
        if let Some(stem) = path.file_stem() {
            self.page.set_name(&stem.to_string_lossy());
            self.show_name();
        }
        if let Some(folder) = path.parent() {
            self.folder.replace(Some(folder.to_path_buf()));
        }
        self.page.set_collision(false);
        if self.ask_app.replace(false) {
            open_with(path, Some(self.window.upcast_ref()));
        }
    }

    fn mark_clean(&self) {
        if let Some(clean) = self.sheet.read(|editor| marked(&editor.doc)) {
            self.clean.replace(Some(clean));
        }
    }

    fn saved_toast(&self) {
        let Some(path) = self.written.borrow().clone() else {
            return;
        };
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let title = gettext("Saved “{name}”").replace("{name}", &name);
        self.toast(&title, Some(path));
    }

    fn toast(&self, text: &str, show: Option<PathBuf>) {
        let toast = adw::Toast::new(text);
        toast.set_use_markup(false);
        if let Some(path) = show {
            toast.set_button_label(Some(&gettext("Show in Folder")));
            let window = self.window().clone();
            toast.connect_button_clicked(move |_| {
                gtk4::FileLauncher::new(Some(&gio::File::for_path(&path))).open_containing_folder(
                    Some(&window),
                    None::<&gio::Cancellable>,
                    |opened| {
                        if let Err(error) = opened {
                            tracing::warn!("cannot show the folder: {error}");
                        }
                    },
                );
            });
        }
        self.page.add_toast(&toast);
    }

    fn end(&self, outcome: Outcome) {
        if self.over.replace(true) {
            return;
        }
        self.forget_drag();
        let json = match (self.setup.json, self.doc(), self.png.borrow().as_ref()) {
            (true, Some(doc), Some(png)) => self
                .target
                .take()
                .and_then(|target| result_line(&outcome, &doc, png, target)),
            _ => None,
        };
        if let Some(answer) = self.answer.take() {
            answer(Ended::Finished { outcome, json });
        }
        remember_size(&self.window);
        self.window.destroy();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WindowSize {
    pub width: i32,
    pub height: i32,
    pub maximized: bool,
}

impl Default for WindowSize {
    fn default() -> Self {
        Self {
            width: 960,
            height: 720,
            maximized: false,
        }
    }
}

fn size_file() -> Option<PathBuf> {
    glimpse_config::state_dir().map(|dir| dir.join("shot-window.json"))
}

pub fn remembered_size() -> WindowSize {
    size_file()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<WindowSize>(&bytes).ok())
        .filter(|size| size.width > 0 && size.height > 0)
        .unwrap_or_default()
}

fn remember_size(window: &adw::ApplicationWindow) {
    let (width, height) = window.default_size();
    let size = WindowSize {
        width,
        height,
        maximized: window.is_maximized(),
    };
    let Some(path) = size_file() else {
        return;
    };
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&path, serde_json::to_vec(&size).unwrap_or_default()));
    if let Err(error) = written {
        tracing::debug!(
            "cannot remember the window size in {}: {error}",
            path.display()
        );
    }
}

pub fn open_with(path: &Path, parent: Option<&gtk4::Window>) {
    let hold = gio::Application::default().map(|app| app.hold());
    let launcher = gtk4::FileLauncher::new(Some(&gio::File::for_path(path)));
    launcher.set_always_ask(true);
    launcher.launch(parent, None::<&gio::Cancellable>, move |result| {
        if let Err(error) = result {
            tracing::warn!("cannot open the screenshot: {error}");
        }
        drop(hold);
    });
}

fn shifted(gesture: &gtk4::GestureDrag) -> bool {
    held(gesture, gdk::ModifierType::SHIFT_MASK)
}

fn held(controller: &impl IsA<gtk4::EventController>, modifier: gdk::ModifierType) -> bool {
    controller.current_event_state().contains(modifier)
}

fn drag_file(runtime: &Path, face: u64, name: &str) -> PathBuf {
    let name = name.trim();
    let name = match name.is_empty() || name.starts_with('.') || name.contains(['/', '\0']) {
        true => DRAG_FALLBACK,
        false => name,
    };
    runtime
        .join("shot")
        .join(format!("drag-{}-{face}", std::process::id()))
        .join(format!("{name}.png"))
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?
        .write_all(bytes)
}

fn drag_content(path: &Path, png: Vec<u8>) -> gdk::ContentProvider {
    let files = gdk::FileList::from_array(&[gio::File::for_path(path)]);
    gdk::ContentProvider::new_union(&[
        gdk::ContentProvider::for_value(&files.to_value()),
        gdk::ContentProvider::for_bytes("image/png", &glib::Bytes::from_owned(png)),
    ])
}

pub fn home_relative(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
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
        let home = Path::new("/home/alex");
        assert_eq!(
            home_relative(Path::new("/home/alex/Pictures/Screenshots"), home),
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
                },
            },
            crop: None,
            marks,
            selected: None,
        }
    }

    #[test]
    fn a_drag_writes_one_private_file_named_after_the_row_under_the_runtime_directory() {
        let runtime = Path::new("/run/user/1000/glimpse");
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
        let provider = drag_content(Path::new("/run/user/1000/glimpse/shot/x.png"), vec![1, 2]);
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
}
