use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use glimpse_widgets::{ChipGroup, IndicatorSpec, LockStage, MessageKind, Track};
use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};

const PASSWORD: &str = "glimpse";

#[derive(Parser, Clone)]
#[command(about = "The real lock stage in a window, with stub data.")]
struct Cli {
    #[arg(long, default_value = "1600")]
    width: i32,
    #[arg(long, default_value = "900")]
    height: i32,
    #[arg(long)]
    wallpaper: Option<PathBuf>,
    #[arg(long)]
    avatar: Option<PathBuf>,
    #[arg(long)]
    art: Option<PathBuf>,
    #[arg(long)]
    awake: bool,
    #[arg(long)]
    sheet: bool,
    #[arg(long)]
    snapshot: Option<PathBuf>,
}

fn main() -> glib::ExitCode {
    let cli = Cli::parse();
    let app = adw::Application::builder()
        .application_id("me.aresa.WidgetPreview.Lock")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(move |app| activate(app, &cli));
    app.run_with_args::<&str>(&["lock-preview"])
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn activate(app: &adw::Application, cli: &Cli) {
    glimpse_utils::init_translations(None);
    glimpse_widgets::register_resources().expect("widget resources");
    let display = gdk::Display::default().expect("display");
    gtk4::IconTheme::for_display(&display).add_resource_path("/me/aresa/GlimpseShell/icons");
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);

    for (sheet, priority) in [
        ("crates/glimpse-widgets/styles/glimpse.css", 0),
        ("data/themes/adwaita/dark.css", 1),
        ("data/themes/adwaita/lock.css", 2),
    ] {
        let provider = gtk4::CssProvider::new();
        provider.load_from_path(root().join(sheet));
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_USER + priority,
        );
    }

    let stage = LockStage::new();
    stub(&stage, cli);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("glimpse-lock preview")
        .default_width(cli.width)
        .default_height(cli.height)
        .content(&stage)
        .css_classes(["lock-surface"])
        .build();
    window.present();
    stage.grab_focus();

    if let Some(path) = cli.snapshot.clone() {
        let window = window.clone();
        glib::timeout_add_local_once(Duration::from_millis(1500), move || {
            save(&window, &path);
            window.close();
        });
    }
}

fn spec(icon: &str, label: &str, tooltip: &str) -> IndicatorSpec {
    IndicatorSpec {
        icon: Some(gio::ThemedIcon::new(icon).upcast()),
        label: Some(label.to_owned()),
        tooltip: Some(tooltip.to_owned()),
        ..Default::default()
    }
}

fn stub(stage: &LockStage, cli: &Cli) {
    let wallpaper = cli
        .wallpaper
        .clone()
        .unwrap_or_else(|| root().join("data/themes/adwaita/default.jpg"));
    match gdk::Texture::from_filename(&wallpaper) {
        Ok(texture) => stage.set_background(Some(&texture)),
        Err(error) => eprintln!("wallpaper {}: {error}", wallpaper.display()),
    }
    stage.set_dim(0.35);
    stage.set_interactive(true);

    let clock = stage.clock();
    clock.set_formats("%H:%M", "%A, {day} %B");
    if let Ok(now) = glib::DateTime::now_local() {
        clock.set_time(&now);
    }

    stage.set_user(Some("Alex Aresa"));
    stage.set_avatar(
        cli.avatar
            .as_deref()
            .and_then(|path| glimpse_widgets::artwork(path, 176, glimpse_widgets::Formats::Strict))
            .as_ref(),
    );
    stage.set_battery(Some(&spec(
        "battery-level-80-symbolic",
        "82%",
        "Battery 82%",
    )));
    let shown = glimpse_widgets::SessionActionState {
        visible: true,
        enabled: true,
        subtitle: None,
    };
    stage.set_session_actions(
        [glimpse_widgets::SUSPEND, "reboot", "power-off"]
            .into_iter()
            .map(|action| (action, &shown)),
    );
    stage.connect_session_action(|stage, action| {
        eprintln!("session action requested: {action}");
        stage.close_session();
    });

    stage.set_notifications(&[
        ChipGroup {
            key: "chat".into(),
            app: "Fractal".into(),
            icon: Some(gio::ThemedIcon::new("chat-message-new-symbolic").upcast()),
            count: 2,
        },
        ChipGroup {
            key: "mail".into(),
            app: "Geary".into(),
            icon: Some(gio::ThemedIcon::new("mail-unread-symbolic").upcast()),
            count: 1,
        },
    ]);

    let track = std::rc::Rc::new(std::cell::RefCell::new(Track {
        title: "Teardrop".into(),
        artist: "Massive Attack — Mezzanine".into(),
        playing: true,
        can_play_pause: true,
        can_next: true,
        art: cli
            .art
            .as_deref()
            .and_then(|path| glimpse_widgets::artwork(path, 128, glimpse_widgets::Formats::Strict)),
    }));
    stage.track().set_track(Some(&track.borrow()));
    stage.track().connect_action(move |card, action| {
        let mut track = track.borrow_mut();
        match action {
            glimpse_widgets::TransportAction::PlayPause => track.playing = !track.playing,
            glimpse_widgets::TransportAction::Next => track.title = "Angel".into(),
            _ => {}
        }
        card.set_track(Some(&track));
    });

    stage.prompt().connect_submitted(|prompt| {
        prompt.set_busy(true);
        let prompt = prompt.clone();
        glib::timeout_add_local_once(Duration::from_millis(700), move || {
            prompt.set_busy(false);
            let text = prompt.take_text();
            if text.as_str() == PASSWORD {
                prompt.set_message(Some("Unlocked (preview)"), MessageKind::Info);
            } else {
                prompt.set_message(Some("Wrong password"), MessageKind::Error);
                prompt.shake();
            }
            prompt.grab_focus();
        });
    });
    stage.prompt().connect_edited(|prompt| {
        prompt.set_message(None, MessageKind::Info);
    });

    if cli.awake {
        stage.set_awake(true);
    }
    if cli.sheet {
        stage.set_awake(true);
        stage.status().emit_by_name::<()>("session-toggled", &[]);
    }
}

fn save(window: &adw::ApplicationWindow, path: &std::path::Path) {
    let paintable = gtk4::WidgetPaintable::new(Some(window));
    let (width, height) = (window.width(), window.height());
    let snapshot = gtk4::Snapshot::new();
    paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
    let Some(node) = snapshot.to_node() else {
        eprintln!("nothing rendered");
        return;
    };
    let Some(renderer) = window.renderer() else {
        eprintln!("no renderer");
        return;
    };
    let texture = renderer.render_texture(&node, None);
    match texture.save_to_png(path) {
        Ok(()) => eprintln!("saved {}", path.display()),
        Err(error) => eprintln!("save {}: {error}", path.display()),
    }
}
