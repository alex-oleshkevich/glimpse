use glimpse_compositors::{Logical, Output, WindowId, Workspace, WorkspaceId};

use super::*;

fn mimes(list: &[&str]) -> Vec<String> {
    list.iter().map(|mime| (*mime).to_owned()).collect()
}

#[test]
fn the_clipboard_is_restored_only_when_niri_alone_replaced_it() {
    assert!(restore(&[mimes(&["image/png"])]));
    assert!(!restore(&[]));
    assert!(!restore(&[mimes(&["image/png"]), mimes(&["image/png"])]));
    assert!(!restore(&[mimes(&["text/plain"]), mimes(&["image/png"])]));
    assert!(!restore(&[mimes(&["text/plain"])]));
    assert!(!restore(&[mimes(&["image/png", "text/plain"])]));
    assert!(!restore(&[mimes(&[])]));
}

fn window(app_id: &str, title: &str) -> Window {
    Window {
        id: WindowId(42),
        title: Some(title.to_owned()),
        app_id: Some(app_id.to_owned()),
        pid: None,
        workspace_id: Some(WorkspaceId(8)),
        is_focused: false,
        is_floating: true,
        is_urgent: false,
        layout_order: None,
        size: Some((400, 300)),
        rect: Some(WindowRect {
            x: 952.0,
            y: 568.0,
            width: 400.0,
            height: 300.0,
        }),
        output: Some("eDP-1".to_owned()),
    }
}

fn toplevel(identifier: &str, app_id: &str, title: &str) -> Toplevel {
    Toplevel {
        identifier: identifier.to_owned(),
        app_id: Some(app_id.to_owned()),
        title: Some(title.to_owned()),
    }
}

#[test]
fn a_window_matches_a_toplevel_only_when_exactly_one_has_its_app_id_and_title() {
    let wanted = window("firefox", "Docs");
    let one = [
        toplevel("a", "firefox", "Mail"),
        toplevel("b", "firefox", "Docs"),
        toplevel("c", "kitty", "Docs"),
    ];
    assert_eq!(correlate(&wanted, &one), Some("b".to_owned()));

    let two = [
        toplevel("a", "firefox", "Docs"),
        toplevel("b", "firefox", "Docs"),
    ];
    assert_eq!(correlate(&wanted, &two), None);
    assert_eq!(correlate(&wanted, &one[..1]), None);
    assert_eq!(correlate(&wanted, &[]), None);
}

#[test]
fn titles_and_app_ids_are_compared_as_the_compositor_cleans_them() {
    let sanitized = Window {
        app_id: None,
        ..window("unused", "Docs")
    };
    let raw_bidi = Toplevel {
        identifier: "bidi".to_owned(),
        app_id: Some(String::new()),
        title: Some("\u{202e}Docs".to_owned()),
    };
    let clean = Toplevel {
        identifier: "clean".to_owned(),
        app_id: None,
        title: Some("Docs".to_owned()),
    };

    assert_eq!(
        correlate(&sanitized, std::slice::from_ref(&raw_bidi)),
        Some("bidi".to_owned())
    );
    assert_eq!(correlate(&sanitized, &[raw_bidi, clean]), None);
}

fn snapshot(active: bool) -> Snapshot {
    Snapshot {
        outputs: vec![Output {
            connector: "eDP-1".to_owned(),
            make: None,
            model: None,
            serial: None,
            description: None,
            logical: Some(Logical {
                x: -1440,
                y: 0,
                width: 2304,
                height: 1440,
                scale: 1.25,
            }),
            current_mode: None,
            enabled: true,
            built_in: true,
        }],
        workspaces: vec![Workspace {
            id: WorkspaceId(8),
            idx: Some(1),
            name: None,
            output: Some("eDP-1".to_owned()),
            is_active: active,
            is_focused: active,
            is_urgent: false,
            active_window_id: None,
        }],
        ..Snapshot::default()
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
fn a_floating_window_is_its_output_relative_rect_times_the_scale() {
    let floating = window("firefox", "Docs");

    assert_eq!(
        visible_crop(&floating, &snapshot(true)),
        Some(buffer(1190, 710, 500, 375))
    );
    assert_eq!(visible_crop(&floating, &snapshot(false)), None);
    let tiled = Window {
        rect: None,
        ..floating
    };
    assert_eq!(visible_crop(&tiled, &snapshot(true)), None);
}

#[test]
fn a_crop_copies_the_rows_it_covers_and_stays_inside_the_frame() {
    let frame = Frame {
        connector: "eDP-1".to_owned(),
        width: 4,
        height: 3,
        pixels: glib::Bytes::from_owned((0u8..12).flat_map(|i| [i, 0, 0, 255]).collect::<Vec<_>>()),
        layout: Layout::Rgbx,
        alpha: false,
    };
    let reds = |frame: &Frame| frame.pixels.chunks(4).map(|p| p[0]).collect::<Vec<_>>();

    let inner = cropped(&frame, buffer(1, 1, 2, 2));
    assert_eq!((inner.width, inner.height), (2, 2));
    assert_eq!(reds(&inner), [5, 6, 9, 10]);

    let past = cropped(&frame, buffer(3, 2, 10, 10));
    assert_eq!((past.width, past.height), (1, 1));
    assert_eq!(reds(&past), [11]);
}

#[test]
fn a_crop_preserves_bgr_layout_and_color_sampling() {
    let frame = Frame {
        connector: "eDP-1".to_owned(),
        width: 2,
        height: 1,
        pixels: glib::Bytes::from_owned(vec![30, 20, 10, 0, 60, 50, 40, 0]),
        layout: Layout::Bgrx,
        alpha: false,
    };
    let crop = cropped(&frame, buffer(1, 0, 1, 1));
    assert_eq!(crop.layout, Layout::Bgrx);
    assert_eq!(crop.pixel(0, 0), [40, 50, 60]);
    assert!(!crop.alpha);
}

#[test]
fn a_floating_or_unscreenshottable_window_is_cropped_and_the_rest_use_the_compositor() {
    let crop = buffer(1, 2, 3, 4);
    assert_eq!(route(true, true, Some(crop)), Route::Crop(crop));
    assert_eq!(route(true, false, Some(crop)), Route::Screenshot);
    assert_eq!(route(true, false, None), Route::Screenshot);
    assert_eq!(route(false, false, Some(crop)), Route::Crop(crop));
    assert_eq!(route(false, false, None), Route::Hidden);
}
