use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

use chrono::DateTime;
use gtk4::gdk_pixbuf::{Colorspace, Pixbuf};

use super::*;

fn image(id: ClipboardEntryId, data: &[u8]) -> ClipboardEntry {
    ClipboardEntry {
        id,
        kind: ClipboardKind::Image,
        mime: "image/png".to_owned(),
        preview: String::new(),
        pinned: false,
        at: DateTime::from_timestamp(1_700_000_000, 0).expect("an instant"),
        data: std::sync::Arc::from(data),
    }
}

fn png() -> Vec<u8> {
    Pixbuf::new(Colorspace::Rgb, false, 8, 4, 4)
        .expect("a pixbuf")
        .save_to_bufferv("png", &[])
        .expect("png bytes")
}

#[test]
fn starting_the_services_forgets_every_spilled_image() {
    let runtime = tempfile::tempdir().expect("a runtime dir");
    let spilled = runtime.path().join("clipboard").join("123");
    std::fs::create_dir_all(&spilled).expect("a spill directory");
    std::fs::write(spilled.join("x.png"), b"png").expect("a spilled image");

    forget_spills(Some(runtime.path().to_path_buf()));
    forget_spills(Some(runtime.path().join("missing")));
    forget_spills(None);

    assert!(!runtime.path().join("clipboard").exists());
    assert!(runtime.path().exists());
}

#[tokio::test]
#[ignore = "sets XDG_RUNTIME_DIR, so it runs alone"]
async fn service_start_clears_old_spills_but_a_second_applet_keeps_live_files() {
    let runtime = tempfile::tempdir().expect("a runtime dir");
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", runtime.path()) };
    let spilled = runtime.path().join("glimpse/clipboard/123/x.png");
    std::fs::create_dir_all(spilled.parent().unwrap()).unwrap();
    std::fs::write(&spilled, b"png").unwrap();

    let config = glimpse_config::Config::default();
    let services = crate::services::PanelServices::start_with_buses(
        &config,
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    assert!(!spilled.parent().unwrap().exists());

    std::fs::create_dir_all(spilled.parent().unwrap()).unwrap();
    std::fs::write(&spilled, b"live image").unwrap();
    for _ in 0..2 {
        let _applet = Clipboard::start(
            services.handles.clipboard.clone(),
            services.handles.kdeconnect.clone(),
            services.handles.notifications.clone(),
        );
        assert_eq!(std::fs::read(&spilled).unwrap(), b"live image");
    }
    services.shutdown().await;

    unsafe { std::env::remove_var("XDG_RUNTIME_DIR") };
    let services = crate::services::PanelServices::start_with_buses(
        &config,
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    services.shutdown().await;
}

#[tokio::test]
#[ignore = "needs a display and a private XDG_RUNTIME_DIR"]
async fn images_decode_off_the_main_loop_and_fill_in_on_a_later_wake() {
    gtk4::init().expect("a display");
    glimpse_widgets::register_resources().expect("resources");
    let runtime = tempfile::tempdir().expect("a runtime dir");
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", runtime.path()) };
    let services = crate::services::PanelServices::start_with_buses(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    let (sender, inbox) = relm4::channel();
    let ctx = Ctx::new("clipboard".into(), None, sender.clone());
    let mut clipboard = Clipboard::start(
        services.handles.clipboard.clone(),
        services.handles.kdeconnect.clone(),
        services.handles.notifications.clone(),
    );
    let png = png();
    let mut entries: Vec<_> = (1..=10).map(|id| image(id, &png)).collect();
    entries.push(image(11, b"not an image"));
    clipboard.state.entries = entries.clone();
    clipboard.settings.visible = 50;
    let calls = Arc::new(AtomicUsize::new(0));
    let main_thread = std::thread::current().id();

    let context = glib::MainContext::new();
    let (first, later) = context
        .with_thread_default(|| {
            let opener = ctx.opener();
            for entry in &entries {
                let calls = calls.clone();
                assert!(
                    clipboard
                        .image(entry, &opener, move |entry| {
                            assert_ne!(std::thread::current().id(), main_thread);
                            calls.fetch_add(1, Ordering::SeqCst);
                            decode(entry)
                        })
                        .is_none()
                );
            }
            let shown = clipboard.popover(&Seat::new(sender)).expect("a popover");
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            let first: Vec<Clip> = entries
                .iter()
                .map(|entry| clipboard.clip(entry, &opener))
                .collect();
            let deadline = Instant::now() + Duration::from_secs(10);
            while clipboard.images.borrow().values().flatten().count() < 10
                && Instant::now() < deadline
            {
                context.iteration(false);
                std::thread::sleep(Duration::from_millis(5));
            }
            for _ in 0..20 {
                context.iteration(false);
            }
            loop {
                let input = inbox.recv_sync().expect("a completion wakes the applet");
                if matches!(input, crate::applet::runtime::HostInput::Woken) {
                    break;
                }
            }
            clipboard.dress(&shown.downcast::<ClipboardPopover>().unwrap(), &opener);
            let later: Vec<Clip> = entries
                .iter()
                .map(|entry| clipboard.clip(entry, &opener))
                .collect();
            (first, later)
        })
        .expect("a thread-default context");

    assert!(
        first
            .iter()
            .all(|clip| clip.image.is_none() && clip.file.is_none()),
        "nothing is decoded while the popover is being dressed"
    );
    assert!(later[..10].iter().all(|clip| clip.image.is_some()));
    assert_eq!(calls.load(Ordering::SeqCst), 11);
    assert!(later[..10].iter().all(|clip| {
        clip.file
            .as_ref()
            .and_then(gio::File::path)
            .is_some_and(|path| path.exists())
    }));
    assert!(later[10].image.is_none() && later[10].file.is_none());
    assert_eq!(later[10].icon, render::IMAGE_ICON);
    assert!(
        !runtime.path().join("glimpse/clipboard/11").exists(),
        "an image that will not decode is never written"
    );
    services.shutdown().await;
}

#[tokio::test]
#[ignore = "sets XDG_RUNTIME_DIR, so it runs alone"]
async fn a_decode_finishing_after_removal_cannot_restore_its_file_or_cache_entry() {
    let runtime = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", runtime.path()) };
    let services = crate::services::PanelServices::start_with_buses(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    let mut clipboard = Clipboard::start(
        services.handles.clipboard.clone(),
        services.handles.kdeconnect.clone(),
        services.handles.notifications.clone(),
    );
    let (sender, inbox) = relm4::channel();
    let ctx = Ctx::new("clipboard".into(), None, sender);
    let entry = image(123, &png());
    let context = glib::MainContext::new();
    crate::applet::testing::run_in(&context, || {
        assert!(clipboard.image(&entry, &ctx.opener(), decode).is_none());
        clipboard.handle(&ctx, &Input::Woken);
    });
    assert!(clipboard.images.borrow().is_empty());
    let completion = crate::applet::testing::driven(&context, inbox.recv()).await;
    assert!(matches!(
        completion,
        Some(crate::applet::runtime::HostInput::Woken)
    ));
    assert!(clipboard.images.borrow().is_empty());
    assert!(!runtime.path().join("glimpse/clipboard/123").exists());
    services.shutdown().await;
}
