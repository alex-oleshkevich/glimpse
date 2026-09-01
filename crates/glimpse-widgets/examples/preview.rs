use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};

const CHECKERBOARD: &str = "
window {
  background-color: #888888;
  background-image:
    linear-gradient(45deg, #6f6f6f 25%, transparent 25%, transparent 75%, #6f6f6f 75%),
    linear-gradient(45deg, #6f6f6f 25%, transparent 25%, transparent 75%, #6f6f6f 75%);
  background-size: 24px 24px;
  background-position: 0 0, 12px 12px;
}
window > * { background-color: transparent; }
";

const SETTLE: Duration = Duration::from_millis(40);

fn main() -> glib::ExitCode {
    let Some(blueprint) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!("usage: preview <blueprint.blp>");
        return glib::ExitCode::FAILURE;
    };

    let app = adw::Application::builder()
        .application_id(format!(
            "me.aresa.WidgetPreview.{}",
            blueprint.file_stem().unwrap_or_default().to_string_lossy()
        ))
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();

    app.connect_activate(move |app| activate(app, &blueprint));
    app.run_with_args::<&str>(&["preview"])
}

fn activate(app: &adw::Application, blueprint: &Path) {
    let blueprint = &resolve(blueprint);
    glimpse_widgets::register_resources().expect("widget resources");
    ensure_types();

    let sheets = [
        (resolve(&builtin_css()), provider()),
        (resolve(&theme_css()), provider()),
    ];
    let checkerboard = provider();
    checkerboard.load_from_string(CHECKERBOARD);

    if let Some(display) = gdk::Display::default() {
        let priorities = [
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            gtk4::STYLE_PROVIDER_PRIORITY_USER,
        ];
        for ((_, provider), priority) in sheets.iter().zip(priorities) {
            gtk4::style_context_add_provider_for_display(&display, provider, priority);
        }
        gtk4::style_context_add_provider_for_display(
            &display,
            &checkerboard,
            gtk4::STYLE_PROVIDER_PRIORITY_USER + 2,
        );
    }

    let slot = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .title(blueprint.file_name().unwrap_or_default().to_string_lossy())
        .child(&slot)
        .build();

    let keys = gtk4::EventControllerKey::new();
    let closing = window.clone();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape {
            closing.close();
        }
        glib::Propagation::Proceed
    });
    window.add_controller(keys);
    window.present();

    load_styles(&sheets);
    build(&slot, blueprint);

    let monitors = watch(blueprint, &slot, sheets);
    unsafe { window.set_data("preview-monitors", monitors) };
}

/// A watched path must be spelled the way the file monitor spells it back. A relative argument or
/// a `..` component compares unequal to the absolute, resolved path GIO reports, and every event is
/// then discarded — the watch arms, stays silent, and looks exactly like a tool that does not
/// reload.
fn resolve(path: &Path) -> PathBuf {
    std::fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn provider() -> gtk4::CssProvider {
    let provider = gtk4::CssProvider::new();
    provider.connect_parsing_error(|_, section, error| {
        eprintln!("stylesheet {}: {error}", section.to_str());
    });
    provider
}

fn load_styles(sheets: &[(PathBuf, gtk4::CssProvider)]) {
    for (path, provider) in sheets {
        match std::fs::read_to_string(path) {
            Ok(css) => provider.load_from_string(&css),
            Err(error) => eprintln!("{}: {error}", path.display()),
        }
    }
}

/// Watches each file's **directory**, not the file, and treats a rename onto the path as a change.
///
/// An editor that saves by writing a temporary file and renaming it over the original destroys the
/// inode a file monitor holds, and GIO then reports the write on a two-second timer. Watching the
/// directory sees the rename immediately — but reports it as `RENAMED`, whose `file` argument is
/// the *temporary* path and whose `other_file` is the one that was asked for, so matching only the
/// first argument silently ignores every such save.
fn watch(
    blueprint: &Path,
    slot: &gtk4::Box,
    sheets: [(PathBuf, gtk4::CssProvider); 2],
) -> Vec<gio::FileMonitor> {
    let sheets = Rc::new(sheets);
    let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let mut monitors = Vec::new();

    let watched: Vec<PathBuf> = [blueprint.to_path_buf()]
        .into_iter()
        .chain(sheets.iter().map(|(path, _)| path.clone()))
        .collect();

    for target in watched {
        let Some(directory) = target.parent() else {
            continue;
        };
        let Ok(monitor) = gio::File::for_path(directory)
            .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
        else {
            continue;
        };

        let slot = slot.clone();
        let blueprint = blueprint.to_path_buf();
        let sheets = sheets.clone();
        let pending = pending.clone();
        monitor.connect_changed(move |_, file, renamed_to, _| {
            let touched = [Some(file.clone()), renamed_to.cloned()]
                .into_iter()
                .flatten()
                .any(|file| file.path().is_some_and(|path| path == target));
            if !touched {
                return;
            }
            if let Some(source) = pending.borrow_mut().take() {
                source.remove();
            }
            let (slot, blueprint, sheets) = (slot.clone(), blueprint.clone(), sheets.clone());
            let fired = pending.clone();
            let source = glib::timeout_add_local_once(SETTLE, move || {
                fired.borrow_mut().take();
                load_styles(sheets.as_ref());
                build(&slot, &blueprint);
            });
            pending.replace(Some(source));
        });
        monitors.push(monitor);
    }
    monitors
}

fn ensure_types() {
    use glimpse_widgets::{Hero, Indicator, IndicatorGroup, Panel, PopoverShell, Row};

    for widget in [
        Hero::static_type(),
        PopoverShell::static_type(),
        Panel::static_type(),
        Indicator::static_type(),
        IndicatorGroup::static_type(),
        Row::static_type(),
    ] {
        let _ = widget;
    }
}

fn build(slot: &gtk4::Box, blueprint: &Path) {
    while let Some(child) = slot.first_child() {
        slot.remove(&child);
    }

    let ui = match compile(blueprint) {
        Ok(ui) => ui,
        Err(message) => {
            slot.append(&error_label(&message));
            return;
        }
    };

    let builder = gtk4::Builder::new();
    if let Err(error) = builder.add_from_file(&ui) {
        slot.append(&error_label(error.message()));
        return;
    }

    let widget = builder
        .objects()
        .into_iter()
        .filter_map(|object| object.downcast::<gtk4::Widget>().ok())
        .find(|widget| widget.parent().is_none());

    match widget {
        Some(widget) => {
            widget.set_halign(gtk4::Align::Center);
            widget.set_valign(gtk4::Align::Center);
            slot.append(&widget);
        }
        None => slot.append(&error_label(
            "nothing in this file builds a widget; an example is a top-level object, not a template",
        )),
    }
}

fn compile(blueprint: &Path) -> Result<PathBuf, String> {
    let ui = std::env::temp_dir().join("glimpse-preview.ui");
    let output = std::process::Command::new("blueprint-compiler")
        .args(["compile", "--output"])
        .args([ui.as_os_str(), blueprint.as_os_str()])
        .output()
        .map_err(|error| error.to_string())?;

    if output.status.success() {
        return Ok(ui);
    }
    let message = if output.stderr.is_empty() {
        output.stdout
    } else {
        output.stderr
    };
    Err(String::from_utf8_lossy(&message).trim().to_string())
}

fn error_label(message: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(message));
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.add_css_class("error");
    label
}

fn builtin_css() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("styles/glimpse.css")
}

fn theme_css() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/themes/adwaita/panel.css")
}
