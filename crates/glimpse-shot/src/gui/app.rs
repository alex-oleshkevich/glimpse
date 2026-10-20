use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::io::IsTerminal as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use anyhow::{Context as _, Result, anyhow, bail};
use gettextrs::gettext;
use glimpse_compositors::Snapshot;
use glimpse_config::{
    Config, DARK_STYLESHEET, PANEL_STYLESHEET, stylesheet, user_dark_stylesheet, user_stylesheet,
};
use glimpse_shot::doc::document::{Document, Mark, Style, Tool};
use glimpse_shot::ports::Notice;
use glimpse_shot::session::{Outcome, Target};
use glimpse_utils::init_translations;
use glimpse_widgets::color_scheme;
use glimpse_widgets::{Sheets, ShotCapturePage, Styles};
use gtk4::{gdk, gio, glib};
use tokio::signal::unix::{SignalKind, signal};

use super::overlay::{self, Ended, Handoff, Options};
use super::prefs;
use super::result::{self, Opened, ResultFace, Setup};
use super::{barrier, canvas};
use crate::adapters::clipboard::HoldClipboard;
use crate::adapters::screens::compositor_snapshot;
use crate::cli::{self, GuiArgs, ScaleMode, missing_directory, paired};
use crate::errors::{self, Busy, Cancelled, TimedOut, Usage};
use crate::headless;

const APP_ID: &str = "me.aresa.Glimpse.Shot";
const APP_ID_VARIABLE: &str = "GLIMPSE_SHOT_APP_ID";
const SERVICE_FLAG: &str = "--gapplication-service";
const TITLE_MAX: usize = 200;
const ICON: &str = APP_ID;
const CONFIG_VARIABLE: &str = "GLIMPSE_CONFIG_PATH";
const PORTAL_TIMEOUT_MS: i32 = 1000;
const PROJECT_URL: &str = "https://github.com/alex-oleshkevich/glimpse";
const STDIN_CHUNK: usize = 64 * 1024;

#[derive(Debug, thiserror::Error)]
#[error("exited with status {0}")]
pub struct Exited(pub u8);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub stdout: String,
    pub stderr: String,
    pub status: u8,
}

thread_local! {
    static SIGNALLED: Cell<Option<u8>> = const { Cell::new(None) };
}

pub fn gtk_args(gui: &GuiArgs, argv: Vec<OsString>) -> Vec<OsString> {
    if !gui.gapplication_service {
        return argv;
    }
    let program = argv
        .into_iter()
        .next()
        .unwrap_or_else(|| OsString::from("glimpse-shot"));
    vec![program, OsString::from(SERVICE_FLAG)]
}

fn settled(ended: Ended) -> Result<Option<String>> {
    match ended {
        Ended::Finished { outcome, json } => match outcome {
            Outcome::Saved { .. } | Outcome::Copied | Outcome::Shared(_) => Ok(json),
            Outcome::Cancelled => Err(Cancelled.into()),
            Outcome::TimedOut => Err(TimedOut.into()),
            Outcome::Failed(reason) => Err(anyhow!(reason)),
        },
        Ended::HandedOff(_) => Err(Cancelled.into()),
        Ended::Failed(error) => Err(error),
    }
}

pub fn reply(settled: Result<Option<String>>, json: bool) -> Reply {
    match settled {
        Ok(line) => Reply {
            stdout: line.map(|line| format!("{line}\n")).unwrap_or_default(),
            stderr: String::new(),
            status: 0,
        },
        Err(error) => {
            let mut stderr = Vec::new();
            let code = errors::report(&error, json, &mut stderr);
            Reply {
                stdout: String::new(),
                stderr: String::from_utf8_lossy(&stderr).into_owned(),
                status: code as u8,
            }
        }
    }
}

pub fn run(gui: &GuiArgs, config_path: Option<&Path>) -> Result<()> {
    gui.target.resolve(false).map_err(Usage)?;
    if let Some(dir) = missing_directory(gui.output.as_deref()) {
        bail!("the directory {} does not exist", dir.display());
    }
    let (document, unreadable) = load_config(config_path);
    init_translations(document.regional.language());
    if let Some(error) = unreadable {
        tracing::warn!("running on defaults: {error:#}");
    }

    let app_id = std::env::var(APP_ID_VARIABLE).unwrap_or_else(|_| APP_ID.to_owned());
    let _session = register_with_portal(&app_id);
    gtk4::init().context("cannot initialize GTK")?;
    adw::init().context("cannot initialize libadwaita")?;
    glimpse_widgets::register_resources()?;
    let appearance = &document.appearance;
    let styles = Styles::install(color_scheme(appearance.color_scheme));
    styles.load(&Sheets {
        theme: stylesheet(&appearance.theme, PANEL_STYLESHEET),
        theme_dark: stylesheet(&appearance.theme, DARK_STYLESHEET),
        dropin: user_stylesheet(),
        dropin_dark: user_dark_stylesheet(),
    });
    styles.set_variant(&appearance.theme_variant);
    styles.set_animation_speed(appearance.animation_speed);

    let app = adw::Application::builder()
        .application_id(&app_id)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE | gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    let shell = Shell::new(&app, config_path);
    let started = Rc::clone(&shell);
    app.connect_startup(move |_| started.startup());
    let lines = Rc::clone(&shell);
    app.connect_command_line(move |_, line| lines.command_line(line));
    let opened = Rc::clone(&shell);
    app.connect_open(move |_, files, _| {
        for path in files.iter().filter_map(gio::File::path) {
            opened.open_file(&path, None, opened.config_path.as_deref(), None);
        }
    });
    let activated = Rc::clone(&shell);
    app.connect_activate(move |_| activated.present(None));
    if app
        .register(gio::Cancellable::NONE)
        .is_ok_and(|()| !app.is_remote())
    {
        listen_for_signals();
    }
    let argv: Vec<String> = gtk_args(gui, std::env::args_os().collect())
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let code = app.run_with_args(&argv);

    if let Some(code) = SIGNALLED.get() {
        return Err(Exited(code).into());
    }
    match shell.local.get().unwrap_or(code.get()) {
        0 => Ok(()),
        status => Err(Exited(status).into()),
    }
}

fn register_with_portal(app_id: &str) -> Option<gio::DBusConnection> {
    let connection = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>)
        .inspect_err(|error| tracing::debug!("no session bus to register on: {error}"))
        .ok()?;
    let registered = connection.call_sync(
        Some("org.freedesktop.portal.Desktop"),
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.host.portal.Registry",
        "Register",
        Some(&glib::Variant::tuple_from_iter([
            app_id.to_variant(),
            glib::VariantDict::new(None).end(),
        ])),
        None,
        gio::DBusCallFlags::NONE,
        PORTAL_TIMEOUT_MS,
        None::<&gio::Cancellable>,
    );
    if let Err(error) = registered {
        tracing::debug!("the portal does not know this app by id: {error}");
    }
    Some(connection)
}

fn load_config(config_path: Option<&Path>) -> (Config, Option<anyhow::Error>) {
    match glimpse_config::load(config_path) {
        Ok(document) => (document, None),
        Err(error) => (Config::default(), Some(error.into())),
    }
}

struct Main {
    window: adw::ApplicationWindow,
    page: ShotCapturePage,
    toasts: adw::ToastOverlay,
    mode: RefCell<String>,
    delay: Cell<u32>,
    cursor: Cell<bool>,
    snapshot: RefCell<Option<Snapshot>>,
}

struct Shell {
    app: adw::Application,
    config_path: Option<PathBuf>,
    menu: gio::Menu,
    main: RefCell<Option<Rc<Main>>>,
    running: RefCell<Option<overlay::Running>>,
    faces: RefCell<Vec<Rc<ResultFace>>>,
    local: Cell<Option<u8>>,
}

type Answer = Box<dyn FnOnce(Result<Option<String>>)>;

impl Shell {
    fn new(app: &adw::Application, config_path: Option<&Path>) -> Rc<Self> {
        let open = gio::Menu::new();
        open.append(Some(&gettext("_Open Image…")), Some("app.open"));
        let rest = gio::Menu::new();
        rest.append(Some(&gettext("_Preferences")), Some("app.preferences"));
        rest.append(Some(&gettext("_Keyboard Shortcuts")), Some("app.shortcuts"));
        rest.append(Some(&gettext("_About Screenshot")), Some("app.about"));
        let menu = gio::Menu::new();
        menu.append_section(None, &open);
        menu.append_section(None, &rest);
        Rc::new(Self {
            app: app.clone(),
            config_path: config_path.map(Path::to_path_buf),
            menu,
            main: RefCell::default(),
            running: RefCell::default(),
            faces: RefCell::default(),
            local: Cell::default(),
        })
    }

    fn startup(self: &Rc<Self>) {
        let action = |name: &str, act: fn(&Rc<Shell>)| {
            let entry = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            entry.connect_activate(move |_, _| {
                if let Some(shell) = weak.upgrade() {
                    act(&shell);
                }
            });
            self.app.add_action(&entry);
        };
        action("open", |shell| shell.choose_image());
        action("preferences", |shell| shell.preferences());
        action("shortcuts", |shell| shell.shortcuts());
        action("about", |shell| shell.about());
        action("quit", |shell| shell.quit());
        action("new", |shell| shell.present(None));
        action("screen", |shell| shell.present(Some("screen")));
        action("window", |shell| shell.present(Some("window")));
        action("region", |shell| shell.present(Some("region")));
        self.app
            .set_accels_for_action("app.preferences", &["<Control>comma"]);
        self.app
            .set_accels_for_action("app.shortcuts", &["<Control>question"]);
        self.app.set_accels_for_action("app.quit", &["<Control>q"]);
        self.app.set_accels_for_action("app.open", &["<Control>o"]);
    }

    fn command_line(self: &Rc<Self>, line: &gio::ApplicationCommandLine) -> glib::ExitCode {
        let immediate = Rc::new(Cell::new(None));
        let arguments = line.arguments();
        let json = arguments.iter().any(|arg| arg == "--json");
        let answer = self.answer(line, json, Rc::clone(&immediate));
        match cli::parse_forwarded(arguments) {
            Ok(parsed) if parsed.command.is_some() => answer(Err(Usage(
                "a subcommand never runs inside the app".to_owned(),
            )
            .into())),
            Ok(parsed) => {
                let cwd = line.cwd();
                let config = caller_config(
                    parsed.config.config,
                    line.getenv(CONFIG_VARIABLE).map(PathBuf::from),
                    cwd.as_deref(),
                );
                let gui = anchored(parsed.gui, cwd.as_deref());
                self.dispatch(&gui, config, line, answer);
            }
            Err(error) => {
                let mut rendered = Vec::new();
                let replied = match error.use_stderr() {
                    true => {
                        let status = errors::clap_failure(&error, json, &mut rendered) as u8;
                        Reply {
                            stdout: String::new(),
                            stderr: String::from_utf8_lossy(&rendered).into_owned(),
                            status,
                        }
                    }
                    false => Reply {
                        stdout: error.render().to_string(),
                        stderr: String::new(),
                        status: 0,
                    },
                };
                self.deliver(line, &replied);
                immediate.set(Some(replied.status));
            }
        }
        glib::ExitCode::from(immediate.get().unwrap_or(0))
    }

    fn answer(
        self: &Rc<Self>,
        line: &gio::ApplicationCommandLine,
        json: bool,
        immediate: Rc<Cell<Option<u8>>>,
    ) -> Answer {
        let weak = Rc::downgrade(self);
        let line = line.clone();
        let guard = self.app.hold();
        Box::new(move |settled| {
            let replied = reply(settled, json);
            if let Some(shell) = weak.upgrade() {
                shell.deliver(&line, &replied);
            }
            immediate.set(Some(replied.status));
            unsafe {
                gio::ffi::g_application_command_line_set_exit_status(
                    line.as_ptr(),
                    i32::from(replied.status),
                );
            }
            drop(line);
            drop(guard);
        })
    }

    fn deliver(&self, line: &gio::ApplicationCommandLine, replied: &Reply) {
        if !replied.stdout.is_empty() {
            line.print_literal(&replied.stdout);
        }
        if !replied.stderr.is_empty() {
            line.printerr_literal(&replied.stderr);
        }
        if !line.is_remote() {
            self.local.set(Some(replied.status));
        }
    }

    fn dispatch(
        self: &Rc<Self>,
        gui: &GuiArgs,
        config_path: Option<PathBuf>,
        line: &gio::ApplicationCommandLine,
        answer: Answer,
    ) {
        if self.running.borrow().is_some() {
            answer(Err(Busy.into()));
            return;
        }
        let checked = cli::validate_gui(
            gui,
            cli::Terminal {
                stdin: false,
                stdout: false,
            },
        )
        .map_err(Usage)
        .and_then(|()| gui.target.resolve(false).map_err(Usage));
        let target = match checked {
            Ok(target) => target,
            Err(error) => return answer(Err(error.into())),
        };
        if gui.portal.is_some() && !portal_target_allowed(&target) {
            return answer(Err(Usage(gettext("--portal needs a screen target")).into()));
        }
        if line.is_remote() && gui.output.as_deref() == Some(Path::new("-")) {
            return answer(Err(Usage(
                "-o - needs the editor to run in this terminal's own process".to_owned(),
            )
            .into()));
        }
        if let Some(dir) = missing_directory(gui.output.as_deref()) {
            return answer(Err(anyhow!(
                "the directory {} does not exist",
                dir.display()
            )));
        }
        if let Some(file) = &gui.file {
            self.open_file(file, Some(answer), config_path.as_deref(), gui.tool);
            return;
        }
        if target == Target::App {
            self.present(None);
            answer(Ok(None));
            return;
        }
        match gui.annotate.as_deref() {
            Some(path) if path == Path::new("-") => {
                let weak = Rc::downgrade(self);
                let gui = gui.clone();
                read_stdin(
                    line,
                    Box::new(move |text| {
                        let Some(shell) = weak.upgrade() else {
                            return;
                        };
                        match text.and_then(|text| headless::parse_marks(&text)) {
                            Ok(marks) => shell.launch(&gui, target, config_path, marks, answer),
                            Err(error) => answer(Err(error)),
                        }
                    }),
                );
            }
            Some(path) => match headless::read_marks(path) {
                Ok(marks) => self.launch(gui, target, config_path, marks, answer),
                Err(error) => answer(Err(error)),
            },
            None => self.launch(gui, target, config_path, Vec::new(), answer),
        }
    }

    fn launch(
        self: &Rc<Self>,
        gui: &GuiArgs,
        target: Target,
        config_path: Option<PathBuf>,
        marks: Vec<Mark>,
        answer: Answer,
    ) {
        match self.overlay_options(gui, target, config_path, marks) {
            Ok(options) => self.start(options, answer),
            Err(error) => answer(Err(error)),
        }
    }

    fn overlay_options(
        &self,
        gui: &GuiArgs,
        target: Target,
        config_path: Option<PathBuf>,
        marks: Vec<Mark>,
    ) -> Result<Options> {
        let (document, unreadable) = load_config(config_path.as_deref());
        if let Some(error) = unreadable {
            tracing::warn!("running on defaults: {error:#}");
            if gui.portal.is_none() {
                report_failure(&error, config_path.as_deref());
            }
        }
        let snapshot = match compositor_snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) if matches!(target, Target::Screen(_)) => {
                tracing::warn!("continuing without the compositor: {error:#}");
                None
            }
            Err(error) => return Err(error),
        };
        let config = document.screenshot;
        let portal = gui
            .portal
            .as_deref()
            .map(|app| glimpse_utils::text::clean(app, 200));
        Ok(Options {
            in_window: gui.in_window(config.editor),
            delay: gui.delay.unwrap_or(config.delay).min(60),
            cursor: paired(gui.cursor, gui.no_cursor).unwrap_or(config.cursor),
            flash: paired(gui.flash, gui.no_flash).unwrap_or(config.flash),
            notify: portal.is_none() && paired(gui.notify, gui.no_notify).unwrap_or(config.notify),
            style: Style {
                color: config.color,
                stroke: config.stroke,
            },
            target,
            prompt: gui.prompt.clone(),
            portal: portal.clone(),
            timeout: gui.timeout,
            json: portal.is_some() || gui.json,
            scale: gui.scale,
            marks,
            output: if portal.is_some() {
                None
            } else {
                gui.output.clone()
            },
            config,
            config_path,
            snapshot,
            tool: gui.tool,
        })
    }

    fn start(self: &Rc<Self>, options: Options, answer: Answer) {
        if self.running.borrow().is_some() {
            answer(Err(Busy.into()));
            return;
        }
        let setup = setup_of(&options);
        let weak = Rc::downgrade(self);
        let ended_now = Rc::new(Cell::new(false));
        let ended_flag = Rc::clone(&ended_now);
        let run = overlay::start(&self.app, options, move |ended| {
            ended_flag.set(true);
            let Some(shell) = weak.upgrade() else {
                return;
            };
            shell.running.replace(None);
            match ended {
                Ended::HandedOff(handoff) => shell.window_for(*handoff, setup, answer),
                other => answer(settled(other)),
            }
        });
        if !ended_now.get() {
            self.running.replace(Some(run));
        }
    }

    fn window_for(self: &Rc<Self>, handoff: Handoff, setup: Setup, answer: Answer) {
        let finished: Box<dyn FnOnce(Ended)> = Box::new(move |ended| answer(settled(ended)));
        let window = self.result_window();
        self.keep(result::open(
            &self.app,
            window,
            setup,
            opened(handoff),
            self.menu.upcast_ref(),
            Some(finished),
        ));
    }

    fn result_window(&self) -> adw::ApplicationWindow {
        let size = result::remembered_size();
        adw::ApplicationWindow::builder()
            .application(&self.app)
            .title(gettext("Screenshot"))
            .icon_name(ICON)
            .default_width(size.width)
            .default_height(size.height)
            .maximized(size.maximized)
            .build()
    }

    fn keep(&self, face: Rc<ResultFace>) {
        let mut faces = self.faces.borrow_mut();
        faces.retain(|face| !face.is_over());
        faces.push(face);
    }

    fn open_file(
        self: &Rc<Self>,
        path: &Path,
        answer: Option<Answer>,
        config_path: Option<&Path>,
        tool: Option<Tool>,
    ) {
        self.open_document(result::load(path), true, answer, config_path, tool);
    }

    fn paste(self: &Rc<Self>) {
        let main = self.main();
        let weak = Rc::downgrade(self);
        main.window
            .clipboard()
            .read_texture_async(None::<&gio::Cancellable>, move |read| {
                let Some(shell) = weak.upgrade() else {
                    return;
                };
                let loaded = match read {
                    Ok(Some(texture)) => result::pasted(&texture),
                    _ => Err(anyhow!(gettext("The clipboard holds no image"))),
                };
                let config_path = shell.config_path.clone();
                shell.open_document(loaded, false, None, config_path.as_deref(), None);
            });
    }

    fn open_document(
        self: &Rc<Self>,
        loaded: Result<Document>,
        saved: bool,
        answer: Option<Answer>,
        config_path: Option<&Path>,
        tool: Option<Tool>,
    ) {
        let (document, _) = load_config(config_path);
        match loaded {
            Ok(doc) => {
                let style = Style {
                    color: document.screenshot.color,
                    stroke: document.screenshot.stroke,
                };
                let setup = Setup {
                    json: false,
                    output: None,
                    scale: ScaleMode::Buffer,
                    config: document.screenshot,
                    config_path: config_path.map(Path::to_path_buf),
                };
                let window = self.result_window();
                self.keep(result::open(
                    &self.app,
                    window,
                    setup,
                    Opened {
                        editor: canvas::Editor::new(doc, style).starting_with(tool),
                        target: None,
                        taken: None,
                        saved,
                    },
                    self.menu.upcast_ref(),
                    None,
                ));
                self.close_main();
                if let Some(answer) = answer {
                    answer(Ok(None));
                }
            }
            Err(error) => match answer {
                Some(answer) => answer(Err(error)),
                None => {
                    let main = self.main();
                    toast(&main.toasts, &format!("{error:#}"));
                    main.window.present();
                }
            },
        }
    }

    fn main(self: &Rc<Self>) -> Rc<Main> {
        if let Some(main) = self.main.borrow().as_ref() {
            return Rc::clone(main);
        }
        let (document, _) = load_config(self.config_path.as_deref());
        let config = document.screenshot;
        let page = ShotCapturePage::new();
        page.set_menu_model(Some(self.menu.upcast_ref()));
        page.set_delay(config.delay);
        page.set_cursor(config.cursor);
        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&page));
        let window = adw::ApplicationWindow::builder()
            .application(&self.app)
            .title(gettext("Screenshot"))
            .icon_name(ICON)
            .default_width(480)
            .default_height(560)
            .content(&toasts)
            .build();
        let main = Rc::new(Main {
            window,
            page,
            toasts,
            mode: RefCell::new("screen".to_owned()),
            delay: Cell::new(config.delay),
            cursor: Cell::new(config.cursor),
            snapshot: RefCell::default(),
        });
        self.wire_main(&main);
        self.main.replace(Some(Rc::clone(&main)));
        main
    }

    fn wire_main(self: &Rc<Self>, main: &Rc<Main>) {
        let weak = Rc::downgrade(main);
        main.page.connect_mode_changed(move |_, mode| {
            if let Some(main) = weak.upgrade() {
                main.mode.replace(mode);
                main.refresh();
            }
        });
        let weak = Rc::downgrade(main);
        main.page.connect_delay_changed(move |_, seconds| {
            if let Some(main) = weak.upgrade() {
                main.delay.set(seconds);
            }
        });
        let weak = Rc::downgrade(main);
        main.page.connect_cursor_changed(move |_, shown| {
            if let Some(main) = weak.upgrade() {
                main.cursor.set(shown);
            }
        });
        let weak = Rc::downgrade(self);
        main.page.connect_take(move |_| {
            if let Some(shell) = weak.upgrade() {
                shell.take();
            }
        });
        let shortcuts = gtk4::ShortcutController::new();
        let weak = Rc::downgrade(self);
        shortcuts.add_shortcut(gtk4::Shortcut::new(
            gtk4::ShortcutTrigger::parse_string("Return|KP_Enter"),
            Some(gtk4::CallbackAction::new(move |_, _| {
                if let Some(shell) = weak.upgrade() {
                    shell.take();
                }
                glib::Propagation::Stop
            })),
        ));
        let weak = Rc::downgrade(self);
        shortcuts.add_shortcut(gtk4::Shortcut::new(
            gtk4::ShortcutTrigger::parse_string("<Control>v"),
            Some(gtk4::CallbackAction::new(move |_, _| {
                if let Some(shell) = weak.upgrade() {
                    shell.paste();
                }
                glib::Propagation::Stop
            })),
        ));
        main.page.add_controller(shortcuts);
        let drop = gtk4::DropTarget::new(gio::File::static_type(), gdk::DragAction::COPY);
        let weak = Rc::downgrade(self);
        drop.connect_drop(move |_, value, _, _| {
            let (Some(shell), Some(path)) = (
                weak.upgrade(),
                value.get::<gio::File>().ok().and_then(|file| file.path()),
            ) else {
                return false;
            };
            let config_path = shell.config_path.clone();
            shell.open_file(&path, None, config_path.as_deref(), None);
            true
        });
        main.toasts.add_controller(drop);
        let weak = Rc::downgrade(self);
        main.window.connect_close_request(move |window| {
            if let Some(shell) = weak.upgrade() {
                shell
                    .main
                    .borrow_mut()
                    .take_if(|main| &main.window == window);
            }
            glib::Propagation::Proceed
        });
    }

    fn close_main(&self) {
        let main = self.main.take();
        if let Some(main) = main {
            main.window.destroy();
        }
    }

    fn present(self: &Rc<Self>, mode: Option<&str>) {
        if self.running.borrow().is_some() {
            return;
        }
        let main = self.main();
        if let Some(mode) = mode {
            main.page.set_mode(mode);
            main.mode.replace(mode.to_owned());
        }
        main.refresh();
        main.window.present();
    }

    fn take(self: &Rc<Self>) {
        if self.running.borrow().is_some() {
            return;
        }
        let main = self.main();
        let snapshot = main.snapshot.borrow().clone();
        let target = match main.mode.borrow().as_str() {
            "window" => match main.page.selected_window() {
                Some(id) => Target::Window(id),
                None => {
                    toast(&main.toasts, &gettext("There is no window to capture"));
                    return;
                }
            },
            "region" => Target::PickRegion,
            _ => match main.page.selected_output().as_str() {
                "current" => Target::CurrentScreen,
                name => Target::Screen(name.to_owned()),
            },
        };
        let (document, _) = load_config(self.config_path.as_deref());
        let config = document.screenshot;
        let options = Options {
            target,
            in_window: true,
            delay: main.delay.get(),
            cursor: main.cursor.get(),
            flash: config.flash,
            notify: false,
            style: Style {
                color: config.color,
                stroke: config.stroke,
            },
            prompt: None,
            portal: None,
            timeout: None,
            json: false,
            scale: ScaleMode::Buffer,
            marks: Vec::new(),
            output: None,
            config,
            config_path: self.config_path.clone(),
            snapshot,
            tool: None,
        };
        let setup = setup_of(&options);
        main.window.set_visible(false);
        barrier();
        let weak = Rc::downgrade(self);
        let ended_now = Rc::new(Cell::new(false));
        let ended_flag = Rc::clone(&ended_now);
        let run = overlay::start(&self.app, options, move |ended| {
            ended_flag.set(true);
            if let Some(shell) = weak.upgrade() {
                shell.running.replace(None);
                shell.taken(ended, setup);
            }
        });
        if !ended_now.get() {
            self.running.replace(Some(run));
        }
    }

    fn taken(self: &Rc<Self>, ended: Ended, setup: Setup) {
        let main = self.main();
        match ended {
            Ended::HandedOff(handoff) => {
                self.close_main();
                self.keep(result::open(
                    &self.app,
                    self.result_window(),
                    setup,
                    opened(*handoff),
                    self.menu.upcast_ref(),
                    None,
                ));
            }
            Ended::Failed(error) => {
                let text = match error.downcast_ref::<errors::NotFound>() {
                    Some(_) if main.mode.borrow().as_str() == "window" => {
                        gettext("That window has closed")
                    }
                    _ => format!("{error:#}"),
                };
                main.refresh();
                toast(&main.toasts, &text);
                main.window.present();
            }
            Ended::Finished { .. } => {
                main.refresh();
                main.window.present();
            }
        }
    }

    fn choose_image(self: &Rc<Self>) {
        let filter = gtk4::FileFilter::new();
        filter.set_name(Some(&gettext("PNG Images")));
        filter.add_mime_type("image/png");
        let filters = gio::ListStore::new::<gtk4::FileFilter>();
        filters.append(&filter);
        let dialog = gtk4::FileDialog::builder()
            .title(gettext("Open Image"))
            .modal(true)
            .filters(&filters)
            .build();
        let parent = self.app.active_window();
        let weak = Rc::downgrade(self);
        dialog.open(parent.as_ref(), None::<&gio::Cancellable>, move |chosen| {
            let Some(shell) = weak.upgrade() else {
                return;
            };
            if let Some(path) = chosen.ok().and_then(|file| file.path()) {
                shell.open_file(&path, None, shell.config_path.as_deref(), None);
            }
        });
    }

    fn preferences(self: &Rc<Self>) {
        let window = self.app.active_window().unwrap_or_else(|| {
            let main = self.main();
            main.window.present();
            main.window.clone().upcast()
        });
        prefs::present(&window, self.config_path.as_deref());
    }

    fn shortcuts(self: &Rc<Self>) {
        let window = self.app.active_window().unwrap_or_else(|| {
            let main = self.main();
            main.window.present();
            main.window.clone().upcast()
        });
        glimpse_widgets::shot_shortcuts().present(Some(&window));
    }

    fn about(self: &Rc<Self>) {
        let dialog = adw::AboutDialog::builder()
            .application_name(gettext("Screenshot"))
            .application_icon(ICON)
            .version(cli::build::VERSION)
            .developer_name("Alex Oleshkevich")
            .website(PROJECT_URL)
            .issue_url(format!("{PROJECT_URL}/issues"))
            .license_type(gtk4::License::Bsd3)
            .build();
        let window = self.app.active_window();
        dialog.present(window.as_ref());
    }

    fn quit(self: &Rc<Self>) {
        let running = self.running.take();
        if let Some(running) = running {
            running.cancel();
        }
        for window in self.app.windows() {
            window.close();
        }
    }
}

impl Main {
    fn refresh(&self) {
        let snapshot = compositor_snapshot().ok().flatten();
        if let Some(snapshot) = &snapshot {
            let outputs: Vec<(String, String)> = snapshot
                .outputs
                .iter()
                .filter(|output| output.enabled)
                .map(|output| {
                    let label = [output.make.as_deref(), output.model.as_deref()]
                        .into_iter()
                        .flatten()
                        .map(str::trim)
                        .filter(|part| !part.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let label = if label.is_empty() {
                        output.connector.clone()
                    } else {
                        format!("{label} ({})", output.connector)
                    };
                    (output.connector.clone(), label)
                })
                .collect();
            let current = snapshot.focused_output.clone().unwrap_or_default();
            self.page.set_outputs_labeled(&outputs, &current);
            let mut windows: Vec<_> = snapshot.windows.iter().collect();
            windows.sort_by_key(|window| !window.is_focused);
            let entries: Vec<(u64, String)> = windows
                .into_iter()
                .map(|window| {
                    (
                        window.id.0,
                        window_label(window.app_id.as_deref(), window.title.as_deref()),
                    )
                })
                .collect();
            self.page.set_windows(&entries);
        }
        self.snapshot.replace(snapshot);
    }
}

fn window_label(app_id: Option<&str>, title: Option<&str>) -> String {
    let name = app_id
        .and_then(super::desktop_entry)
        .map(|info| info.name().to_string())
        .or_else(|| app_id.map(str::to_owned));
    let clean = |text: &str| glimpse_utils::text::clean(text, TITLE_MAX);
    match (name, title.filter(|title| !title.is_empty())) {
        (Some(name), Some(title)) => format!("{} — {}", clean(&name), clean(title)),
        (Some(name), None) => clean(&name),
        (None, Some(title)) => clean(title),
        (None, None) => gettext("Untitled window"),
    }
}

fn setup_of(options: &Options) -> Setup {
    Setup {
        json: options.json,
        output: options.output.clone(),
        scale: options.scale,
        config: options.config.clone(),
        config_path: options.config_path.clone(),
    }
}

fn opened(handoff: Handoff) -> Opened {
    Opened {
        editor: handoff.editor,
        target: handoff.target,
        taken: handoff.taken,
        saved: false,
    }
}

fn anchored(mut gui: GuiArgs, cwd: Option<&Path>) -> GuiArgs {
    let Some(cwd) = cwd else {
        return gui;
    };
    let anchor = |path: &mut Option<PathBuf>| {
        if let Some(relative) = path
            .as_ref()
            .filter(|path| path.is_relative() && *path != Path::new("-"))
        {
            *path = Some(cwd.join(relative));
        }
    };
    anchor(&mut gui.output);
    anchor(&mut gui.annotate);
    anchor(&mut gui.file);
    gui
}

fn caller_config(
    explicit: Option<PathBuf>,
    environment: Option<PathBuf>,
    cwd: Option<&Path>,
) -> Option<PathBuf> {
    let path = explicit.or(environment)?;
    Some(match cwd {
        Some(cwd) if path.is_relative() => cwd.join(path),
        _ => path,
    })
}

type Read = Box<dyn FnOnce(Result<String>)>;

fn read_stdin(line: &gio::ApplicationCommandLine, done: Read) {
    match line.stdin() {
        Some(stream) => read_more(stream, Vec::new(), done),
        None => done(Err(anyhow!(
            "the calling process gave no stdin to read annotations from"
        ))),
    }
}

fn read_more(stream: gio::InputStream, mut read: Vec<u8>, done: Read) {
    let next = stream.clone();
    stream.read_bytes_async(
        STDIN_CHUNK,
        glib::Priority::DEFAULT,
        None::<&gio::Cancellable>,
        move |chunk| match chunk {
            Err(error) => done(Err(
                anyhow::Error::new(error).context("cannot read annotations from stdin")
            )),
            Ok(chunk) if chunk.is_empty() => done(headless::capped_text(read)),
            Ok(chunk) => {
                read.extend_from_slice(&chunk);
                match read.len() > headless::ANNOTATIONS_CAP {
                    true => done(headless::capped_text(read)),
                    false => read_more(next, read, done),
                }
            }
        },
    );
}

fn toast(toasts: &adw::ToastOverlay, text: &str) {
    let toast = adw::Toast::new(text);
    toast.set_use_markup(false);
    toasts.add_toast(toast);
}

fn portal_target_allowed(target: &Target) -> bool {
    matches!(target, Target::CurrentScreen | Target::Screen(_))
}

fn report_failure(error: &anyhow::Error, config: Option<&Path>) {
    if std::io::stderr().is_terminal() {
        return;
    }
    let notice = Notice {
        summary: gettext("Screenshot failed"),
        body: format!("{error:#}")
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned(),
        image: None,
        actions: false,
    };
    let holder = HoldClipboard {
        config: config.map(Path::to_path_buf),
    };
    if let Err(error) = holder.notify(notice) {
        tracing::warn!("{error:#}");
    }
}

fn listen_for_signals() {
    std::thread::spawn(|| {
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
        else {
            return;
        };
        let code = runtime.block_on(async {
            let mut terminate = signal(SignalKind::terminate()).ok()?;
            let mut interrupt = signal(SignalKind::interrupt()).ok()?;
            tokio::select! {
                _ = terminate.recv() => Some(143),
                _ = interrupt.recv() => Some(130),
            }
        });
        if let Some(code) = code {
            glib::MainContext::default().invoke(move || interrupted(code));
        }
    });
}

fn interrupted(code: u8) {
    SIGNALLED.set(Some(code));
    let Some(app) = gio::Application::default() else {
        return;
    };
    if let Some(app) = app.downcast_ref::<gtk4::Application>() {
        for window in app.windows() {
            window.destroy();
        }
    }
    app.quit();
}

#[cfg(test)]
mod tests;
