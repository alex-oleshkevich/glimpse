use glimpse_shot::doc::units::Scale;
use glimpse_wayland::capture::Frame;

use super::*;

fn at(x: i32, y: i32) -> Point<BufferPx> {
    Point {
        x: BufferPx(x),
        y: BufferPx(y),
    }
}

#[test]
fn a_logical_point_at_scale_one_and_a_quarter_lands_on_the_buffer_pixel() {
    let view = View::fit((2880.0, 1800.0), (2304.0, 1440.0), f64::INFINITY, 0.0);

    assert_eq!(view.to_buffer(100.0, 100.0), at(125, 125));
    assert_eq!(view.to_logical(at(125, 125)), (100.0, 100.0));
}

#[test]
fn a_small_window_is_centered_at_its_logical_size_and_a_large_one_is_fit() {
    let small = View::fit((800.0, 600.0), (2304.0, 1440.0), 1.0 / 1.25, 48.0);
    assert_eq!(small.factor, 0.8);
    assert_eq!((small.x, small.y), (832.0, 480.0));

    let large = View::fit((4000.0, 1000.0), (2304.0, 1440.0), 1.0 / 1.25, 48.0);
    assert_eq!(large.factor, (2304.0 - 96.0) / 4000.0);
    assert_eq!(large.x, 48.0);
}

#[test]
fn a_square_drag_takes_the_longer_side_in_the_dragged_direction() {
    assert_eq!(
        between(at(10, 10), at(0, 40), true),
        Rect {
            x: BufferPx(-20),
            y: BufferPx(10),
            width: BufferPx(30),
            height: BufferPx(30),
        }
    );
    assert_eq!(
        between(at(10, 10), at(0, 40), false),
        Rect {
            x: BufferPx(0),
            y: BufferPx(10),
            width: BufferPx(10),
            height: BufferPx(30),
        }
    );
}

#[test]
fn a_shifted_arrow_snaps_to_fifteen_degrees() {
    assert_eq!(snapped(at(0, 0), at(100, 3), true), at(100, 0));
    assert_eq!(snapped(at(0, 0), at(100, 3), false), at(100, 3));
}

fn crop(x: i32, y: i32, width: i32, height: i32) -> Rect<BufferPx> {
    Rect {
        x: BufferPx(x),
        y: BufferPx(y),
        width: BufferPx(width),
        height: BufferPx(height),
    }
}

#[test]
#[ignore = "needs a display"]
fn a_dragged_crop_waits_for_enter_and_applying_it_is_one_undo_step() {
    if gtk4::init().is_err() {
        return;
    }
    let mut editor = editor();
    editor.set_tool(Tool::Crop);
    assert_eq!(editor.pending_crop, None, "nothing to adjust yet");
    assert!(
        !editor.apply_crop(),
        "Enter keeps its meaning with nothing pending"
    );

    editor.press(0.0, 0.0);
    editor.motion(30.0, 100.0, false);
    editor.release(30.0, 100.0, false);
    assert_eq!(editor.pending_crop, Some(crop(0, 0, 30, 100)));
    assert_eq!(editor.doc.crop, None, "a drag alone applies nothing");
    assert!(!editor.history.can_undo());
    assert_eq!(editor.interaction(), Interaction::CropPending);

    assert!(editor.apply_crop(), "Enter applies a pending crop");
    assert_eq!(editor.doc.crop, Some(crop(0, 0, 30, 100)));
    assert_eq!(editor.pending_crop, None);
    assert_eq!(editor.shown(), crop(0, 0, 30, 100));
    editor.undo();
    assert_eq!(editor.doc.crop, None, "one undo takes the whole crop back");
    assert!(!editor.history.can_undo());
    editor.redo();

    editor.set_tool(Tool::Crop);
    assert_eq!(
        editor.pending_crop,
        Some(crop(0, 0, 30, 100)),
        "the crop reopens to adjust"
    );
    assert_eq!(
        editor.shown(),
        crop(0, 0, 200, 100),
        "adjusting shows the whole frame"
    );
    assert!(editor.apply_crop());
    editor.undo();
    assert_eq!(
        editor.doc.crop, None,
        "re-applying the same crop added no entry"
    );

    editor.pending_crop = Some(crop(5, 5, 10, 10));
    editor.doc.selected = Some(0);
    assert_eq!(
        editor.escape(),
        EscOutcome::DiscardCrop,
        "a pending crop goes before Deselect"
    );
    assert_eq!(editor.pending_crop, None);
    assert_eq!(editor.doc.crop, None);
    assert_eq!(editor.escape(), EscOutcome::Deselect);
}

#[test]
#[ignore = "needs a display"]
fn an_applied_output_crop_is_centered_and_undo_restores_the_full_view() {
    if gtk4::init().is_err() {
        return;
    }
    let mut editor = editor();
    editor.pending_crop = Some(crop(50, 20, 50, 20));
    assert!(editor.apply_crop());
    assert_eq!(editor.view().to_logical(at(50, 20)), (0.0, 10.0));
    assert_eq!(editor.doc.exported_rect(), crop(50, 20, 50, 20));
    editor.undo();
    assert_eq!(editor.view().to_logical(at(0, 0)), (0.0, 0.0));
}

#[test]
#[ignore = "needs a display"]
fn each_new_drag_is_one_undoable_mark() {
    if gtk4::init().is_err() {
        return;
    }
    for tool in [Tool::Line, Tool::Freehand, Tool::Highlighter] {
        let mut editor = editor();
        editor.set_tool(tool);
        editor.press(80.0, 20.0);
        editor.motion(90.0, 30.0, false);
        editor.release(100.0, 40.0, false);
        assert_eq!(editor.doc.marks.len(), 2);
        if tool == Tool::Freehand || tool == Tool::Highlighter {
            assert!(
                matches!(&editor.doc.marks[1].1, Mark::Path { points, .. } if points.len() == 3)
            );
        }
        editor.undo();
        assert_eq!(editor.doc.marks.len(), 1);
        assert!(!editor.history.can_undo());
    }
}

#[test]
fn a_toolbar_name_is_the_command_line_name() {
    for tool in <Tool as clap::ValueEnum>::value_variants() {
        assert_eq!(tool_named(tool_name(*tool)), Some(*tool));
    }
    assert_eq!(tool_named("pen"), None);
}

#[test]
fn a_crop_outside_the_frame_is_clipped_and_an_empty_one_shows_the_frame() {
    let whole = crop(0, 0, 200, 100);
    assert_eq!(clipped(crop(-10, 50, 40, 90), whole), crop(0, 50, 30, 50));
    assert_eq!(clipped(crop(300, 0, 10, 10), whole), whole);
}

fn rendered(editor: &mut Editor, width: usize, height: usize) -> Vec<u8> {
    let snapshot = gtk4::Snapshot::new();
    editor.draw(&snapshot, width as f32, height as f32);
    let node = snapshot.to_node().expect("a node");
    let renderer = gsk::CairoRenderer::new();
    renderer.realize(None).expect("a renderer");
    let texture = renderer.render_texture(
        &node,
        Some(&graphene::Rect::new(0.0, 0.0, width as f32, height as f32)),
    );
    renderer.unrealize();
    let mut bytes = vec![0u8; width * height * 4];
    texture.download(&mut bytes, width * 4);
    bytes
}

fn pixel(bytes: &[u8], width: usize, x: usize, y: usize) -> &[u8] {
    let at = (y * width + x) * 4;
    &bytes[at..at + 4]
}

#[test]
#[ignore = "needs a display"]
fn a_pending_crop_dims_only_the_image_and_an_applied_one_is_all_that_shows() {
    if gtk4::init().is_err() {
        return;
    }
    let mut doc = document();
    if let Source::Output { frame, .. } = &mut doc.source {
        frame.pixels = glib::Bytes::from_owned(vec![0x80u8; 200 * 100 * 4]);
    }
    let style = Style {
        color: PaletteColor::Red,
        stroke: Stroke::Medium,
    };
    let mut editor = Editor::new(doc, style);
    editor.fitted = true;
    let plain = rendered(&mut editor, 200, 100);

    editor.pending_crop = Some(crop(0, 0, 30, 100));
    let pending = rendered(&mut editor, 200, 100);
    let same = |x, y| pixel(&plain, 200, x, y) == pixel(&pending, 200, x, y);
    assert!(same(10, 30), "a mark inside the crop keeps its color");
    assert!(same(50, 30), "a mark outside the crop keeps its color");
    assert!(same(20, 80), "the image inside the crop is untouched");
    assert!(!same(150, 80), "the image outside the crop is dimmed");

    assert!(editor.apply_crop());
    let sheet = Sheet::new(editor);
    assert_eq!(
        (sheet.intrinsic_width(), sheet.intrinsic_height()),
        (30, 100),
        "an applied crop is the whole picture"
    );
    let applied = sheet
        .with(|editor| rendered(editor, 30, 100))
        .expect("an editor");
    assert_eq!(pixel(&applied, 30, 10, 30), pixel(&plain, 200, 10, 30));
    assert_eq!(
        pixel(&applied, 30, 20, 80),
        pixel(&plain, 200, 20, 80),
        "no dim after apply"
    );
}

#[test]
#[ignore = "needs a display"]
fn a_picked_window_replaces_the_screen_and_only_the_scrim_surrounds_it() {
    if gtk4::init().is_err() {
        return;
    }
    let mut doc = document();
    if let Source::Output { frame, .. } = &mut doc.source {
        frame.pixels = glib::Bytes::from_owned(vec![0x80u8; 200 * 100 * 4]);
    }
    let mut editor = Editor::new(
        doc,
        Style {
            color: PaletteColor::Red,
            stroke: Stroke::Medium,
        },
    );
    let screen = rendered(&mut editor, 400, 300);
    assert_ne!(pixel(&screen, 400, 2, 2), [0, 0, 0, 153]);
    editor.replace_source(Source::Window {
        id: 1,
        app_id: None,
        title: None,
        scale: Scale(1.0),
        frame: Frame {
            connector: "eDP-1".to_owned(),
            width: 40,
            height: 20,
            pixels: glib::Bytes::from_owned(vec![0xffu8; 40 * 20 * 4]),
            alpha: true,
            layout: glimpse_wayland::capture::Layout::Rgbx,
        },
    });
    let picked = rendered(&mut editor, 400, 300);
    assert_eq!(
        pixel(&picked, 400, 2, 2),
        [0, 0, 0, 153],
        "the scrim, not the frozen screen"
    );
    assert_eq!(
        pixel(&picked, 400, 200, 150),
        [255, 255, 255, 255],
        "the window itself"
    );
}

#[test]
fn grabbing_a_crop_edge_moves_only_that_side() {
    let crop = Rect {
        x: BufferPx(10),
        y: BufferPx(10),
        width: BufferPx(100),
        height: BufferPx(50),
    };
    assert_eq!(side_near(crop, at(61, 58), 4), Some(Side::Bottom));
    assert_eq!(side_near(crop, at(8, 36), 4), Some(Side::Left));
    assert_eq!(side_near(crop, at(60, 30), 4), None);
    assert_eq!(
        resized(crop, Side::Right, at(150, 999)),
        Rect {
            width: BufferPx(140),
            ..crop
        }
    );
    assert_eq!(
        resized(crop, Side::Top, at(0, 80)),
        Rect {
            x: BufferPx(10),
            y: BufferPx(60),
            width: BufferPx(100),
            height: BufferPx(20),
        },
        "dragging an edge past its opposite flips the crop"
    );
}

#[test]
fn grabbing_a_crop_corner_anchors_the_opposite_one() {
    let crop = Rect {
        x: BufferPx(10),
        y: BufferPx(10),
        width: BufferPx(100),
        height: BufferPx(50),
    };
    assert_eq!(corner_near(crop, at(112, 58), 4), Some(at(10, 10)));
    assert_eq!(corner_near(crop, at(60, 30), 4), None);
}

fn document() -> Document {
    Document {
        source: Source::Output {
            name: "eDP-1".to_owned(),
            scale: Scale(1.0),
            frame: Frame {
                connector: "eDP-1".to_owned(),
                width: 200,
                height: 100,
                pixels: glib::Bytes::from_owned(vec![0u8; 200 * 100 * 4]),
                alpha: false,
                layout: glimpse_wayland::capture::Layout::Rgbx,
            },
        },
        crop: None,
        marks: vec![(
            0,
            Mark::Rect {
                rect: Rect {
                    x: BufferPx(10),
                    y: BufferPx(10),
                    width: BufferPx(40),
                    height: BufferPx(40),
                },
                style: Style {
                    color: PaletteColor::Red,
                    stroke: Stroke::Medium,
                },
                filled: false,
            },
        )],
        selected: None,
    }
}

fn editor() -> Editor {
    let mut editor = Editor::new(
        document(),
        Style {
            color: PaletteColor::Red,
            stroke: Stroke::Medium,
        },
    );
    editor.area = (200.0, 100.0);
    editor
}

#[test]
#[ignore = "needs a display"]
fn resize_starts_at_the_handle_without_a_jump_and_undoes_once() {
    if gtk4::init().is_err() {
        return;
    }
    let mut editor = editor();
    editor.doc.selected = Some(0);
    let original = editor.doc.marks[0].1.clone();
    assert!(!editor.press_resize(100.0, 70.0));
    assert!(editor.press_resize(50.0, 50.0));
    editor.release(50.0, 50.0, false);
    assert_eq!(editor.doc.marks[0].1, original);
    assert!(!editor.history.can_undo());

    assert!(editor.press_resize(50.0, 50.0));
    editor.motion(70.0, 70.0, false);
    assert_ne!(editor.preview().doc.marks[0].1, original);
    editor.release(70.0, 70.0, false);
    let resized = editor.doc.marks[0].1.clone();
    assert_ne!(resized, original);
    editor.undo();
    assert_eq!(editor.doc.marks[0].1, original);
    assert!(!editor.history.can_undo());
    editor.redo();
    assert_eq!(editor.doc.marks[0].1, resized);
}

#[test]
#[ignore = "needs a display"]
fn wheel_steps_only_the_size_bearing_tools() {
    if gtk4::init().is_err() {
        return;
    }
    let mut editor = editor();
    assert!(editor.scroll_stroke(-1.0));
    assert_eq!(editor.style.stroke, Stroke::Thick);
    assert!(editor.scroll_stroke(-1.0));
    assert_eq!(editor.style.stroke, Stroke::Thick);
    editor.set_tool(Tool::Text);
    assert!(editor.scroll_stroke(1.0));
    assert_eq!(editor.style.stroke, Stroke::Medium);
    editor.set_tool(Tool::Number);
    assert!(editor.scroll_stroke(1.0));
    assert_eq!(editor.style.stroke, Stroke::Thin);
    editor.set_tool(Tool::Line);
    assert!(editor.scroll_stroke(-1.0));
    assert_eq!(editor.style.stroke, Stroke::Medium);
    editor.set_tool(Tool::Blur);
    assert!(editor.scroll_stroke(-1.0));
    assert_eq!(editor.style.stroke, Stroke::Thick);
    editor.set_tool(Tool::Rect);
    assert!(!editor.scroll_stroke(-1.0));
    assert_eq!(editor.style.stroke, Stroke::Thick);
}

#[test]
#[ignore = "needs a display"]
fn tool_keys_tap_to_select_and_hold_to_borrow() {
    if gtk4::init().is_err() {
        return;
    }
    let mut editor = editor();
    assert_eq!(spring_tool(gdk::Key::n), Some(Tool::Number));
    assert_eq!(spring_tool(gdk::Key::b), Some(Tool::Blur));
    assert_eq!(spring_tool(gdk::Key::r), Some(Tool::Rect));

    assert!(editor.tool_key_down(Tool::Number));
    assert_eq!(editor.tool, Tool::Number);
    assert!(!editor.tool_key_up(Tool::Number));
    assert_eq!(editor.tool, Tool::Number);

    editor.set_tool(Tool::Arrow);
    assert!(editor.tool_key_down(Tool::Blur));
    assert_eq!(editor.tool, Tool::Blur);
    editor.press(10.0, 10.0);
    assert!(!editor.tool_key_up(Tool::Blur));
    assert_eq!(editor.tool, Tool::Blur);
    editor.release(10.0, 10.0, false);
    assert_eq!(editor.tool, Tool::Arrow);

    assert!(editor.tool_key_down(Tool::Rect));
    if let Some(held) = editor.temporary_tool.as_mut() {
        held.since -= TOOL_HOLD;
    }
    assert!(editor.tool_key_up(Tool::Rect));
    assert_eq!(editor.tool, Tool::Arrow);

    editor.set_tool(Tool::Crop);
    assert!(editor.tool_key_down(Tool::Number));
    assert!(!editor.tool_key_up(Tool::Number));
    assert_eq!(editor.tool, Tool::Number);
    editor.set_tool(Tool::Text);
    assert!(editor.tool_key_down(Tool::Blur));
    assert!(!editor.tool_key_up(Tool::Blur));
    assert_eq!(editor.tool, Tool::Blur);

    editor.set_tool(Tool::Line);
    assert!(editor.tool_key_down(Tool::Number));
    editor.set_tool(Tool::Oval);
    assert!(!editor.tool_key_up(Tool::Number));
    assert_eq!(editor.tool, Tool::Oval);

    assert!(editor.tool_key_down(Tool::Rect));
    assert!(editor.cancel_tool_key());
    assert_eq!(editor.tool, Tool::Oval);
}

#[test]
#[ignore = "needs a display"]
fn hover_cursors_match_crop_and_annotation_handles() {
    if gtk4::init().is_err() {
        return;
    }
    let mut editor = editor();
    editor.doc.selected = Some(0);
    assert_eq!(editor.cursor_at(10.0, 10.0), "nwse-resize");
    assert_eq!(editor.cursor_at(50.0, 10.0), "nesw-resize");
    assert_eq!(editor.cursor_at(30.0, 10.0), "ns-resize");
    assert_eq!(editor.cursor_at(50.0, 30.0), "ew-resize");
    if let Mark::Rect { filled, .. } = &mut editor.doc.marks[0].1 {
        *filled = true;
    }
    assert_eq!(editor.cursor_at(30.0, 30.0), "move");
    assert_eq!(editor.hover(30.0, 30.0), "move");
    editor.doc.selected = None;
    assert_eq!(editor.cursor(), "crosshair");
    assert_eq!(editor.cursor_at(100.0, 70.0), "crosshair");

    editor.set_tool(Tool::Crop);
    editor.pending_crop = Some(Rect {
        x: BufferPx(10),
        y: BufferPx(10),
        width: BufferPx(40),
        height: BufferPx(40),
    });
    assert_eq!(editor.cursor_at(10.0, 10.0), "nwse-resize");
    assert_eq!(editor.cursor_at(30.0, 10.0), "ns-resize");
    assert_eq!(editor.cursor_at(30.0, 30.0), "move");
}

#[test]
#[ignore = "needs a display"]
fn a_starting_tool_is_active_at_once_and_survives_a_picked_window() {
    if gtk4::init().is_err() {
        return;
    }
    assert_eq!(editor().tool, Tool::Arrow);
    assert_eq!(editor().starting_with(None).tool, Tool::Arrow);
    let mut started = editor().starting_with(Some(Tool::Crop));
    assert_eq!(started.tool, Tool::Crop);
    let source = started.doc.source.clone();
    started.replace_source(source.clone());
    assert_eq!(started.tool, Tool::Crop, "--tool was given");
    let mut plain = editor();
    plain.set_tool(Tool::Blur);
    plain.replace_source(source);
    assert_eq!(
        plain.tool,
        Tool::Arrow,
        "no --tool: a picked window starts on the arrow"
    );
}

#[test]
#[ignore = "needs a display"]
fn a_sheet_is_its_logical_size_and_maps_the_picture_back_to_buffer_pixels() {
    if gtk4::init().is_err() {
        return;
    }
    let mut doc = document();
    doc.source = Source::Window {
        id: 1,
        app_id: None,
        title: None,
        scale: Scale(2.0),
        frame: Frame {
            connector: "eDP-1".to_owned(),
            width: 200,
            height: 100,
            pixels: glib::Bytes::from_owned(vec![0u8; 200 * 100 * 4]),
            alpha: true,
            layout: glimpse_wayland::capture::Layout::Rgbx,
        },
    };
    let style = Style {
        color: PaletteColor::Red,
        stroke: Stroke::Medium,
    };
    let sheet = Sheet::new(Editor::new(doc, style));
    assert_eq!(
        (sheet.intrinsic_width(), sheet.intrinsic_height()),
        (100, 50)
    );
    let snapshot = gtk4::Snapshot::new();
    sheet.snapshot(&snapshot, 100.0, 50.0);
    assert_eq!(sheet.origin(300, 50), (100.0, 0.0));
    let view = sheet.read(Editor::view).expect("an editor");
    assert_eq!(
        view,
        View {
            x: 0.0,
            y: 0.0,
            factor: 0.5
        },
        "a sheet has no margin"
    );
    assert_eq!(view.to_buffer(50.0, 25.0), at(100, 50));
}

#[test]
#[ignore = "needs a display"]
fn escape_unwinds_the_innermost_thing_first() {
    if gtk4::init().is_err() {
        return;
    }
    let mut editor = editor();
    editor.list_open = true;
    editor.doc.selected = Some(0);
    editor.press(100.0, 80.0);
    editor.motion(150.0, 90.0, false);
    editor.typing = Some(at(5, 5));

    assert_eq!(editor.interaction(), Interaction::Typing { id: TYPING });
    assert_eq!(editor.escape(), EscOutcome::DiscardText);
    assert_eq!(editor.typing, None);
    assert_eq!(
        editor.interaction(),
        Interaction::Dragging { tool: Tool::Arrow }
    );
    assert_eq!(editor.escape(), EscOutcome::AbortDrag);
    editor.release(150.0, 90.0, false);
    assert_eq!(editor.doc.marks.len(), 1, "an aborted drag draws nothing");
    assert_eq!(editor.interaction(), Interaction::Selected { id: 0 });
    assert_eq!(editor.escape(), EscOutcome::Deselect);
    assert_eq!(editor.doc.selected, None);
    assert_eq!(editor.interaction(), Interaction::ListOpen);
    assert_eq!(editor.escape(), EscOutcome::CloseList);
    assert!(!editor.list_open);
    assert_eq!(editor.interaction(), Interaction::Idle);
    assert_eq!(editor.escape(), EscOutcome::Cancel);
    assert_eq!(editor.doc.marks.len(), 1, "escape never removes a mark");
}

#[test]
#[ignore = "needs a display"]
fn a_click_selects_a_drag_draws_and_undo_takes_it_back() {
    if gtk4::init().is_err() {
        return;
    }
    let mut editor = editor();
    editor.press(10.0, 30.0);
    editor.release(10.0, 30.0, false);
    assert_eq!(editor.doc.selected, Some(0), "a click on a mark selects it");
    editor.doc.selected = None;

    editor.tool = Tool::Blur;
    editor.press(20.0, 20.0);
    editor.release(80.0, 60.0, false);
    assert_eq!(editor.doc.marks.len(), 2, "a drag over a mark draws");
    assert!(matches!(editor.doc.marks[1].1, Mark::Pixelate { .. }));
    assert!(editor.history.can_undo());

    editor.undo();
    assert_eq!(editor.doc.marks.len(), 1);
    assert!(!editor.history.can_undo(), "the base mark is never undone");
    assert!(editor.history.can_redo());

    editor.tool = Tool::Number;
    editor.press(30.0, 30.0);
    editor.release(30.0, 30.0, false);
    assert!(
        matches!(editor.doc.marks[1].1, Mark::Number { .. }),
        "a number clicked inside a rectangle stamps"
    );
}

#[test]
#[ignore = "needs a display"]
fn blur_paints_a_path_and_click_paints_one_round_stamp() {
    if gtk4::init().is_err() {
        return;
    }
    let mut editor = editor();
    editor.set_tool(Tool::Blur);
    editor.press(70.0, 70.0);
    editor.motion(85.0, 70.0, false);
    assert!(
        matches!(&editor.preview().doc.marks[1].1, Mark::Pixelate { points, .. } if points.len() == 2)
    );
    editor.release(100.0, 70.0, false);
    assert!(matches!(&editor.doc.marks[1].1, Mark::Pixelate { points, .. } if points.len() == 3));
    editor.press(120.0, 70.0);
    editor.release(120.0, 70.0, false);
    assert!(matches!(&editor.doc.marks[2].1, Mark::Pixelate { points, .. } if points.len() == 1));
    editor.undo();
    assert_eq!(editor.doc.marks.len(), 2);
    editor.undo();
    assert_eq!(editor.doc.marks.len(), 1);
}

#[test]
#[ignore = "needs a display"]
fn a_pixelation_stroke_moves_and_resizes_with_undo() {
    if gtk4::init().is_err() {
        return;
    }
    let mut editor = editor();
    editor.set_tool(Tool::Blur);
    editor.press(70.0, 70.0);
    editor.release(100.0, 70.0, false);
    let original = editor.doc.marks[1].1.clone();
    editor.doc.selected = Some(1);
    editor.set_tool(Tool::Arrow);
    editor.press(85.0, 70.0);
    editor.release(95.0, 80.0, false);
    let moved = editor.doc.marks[1].1.clone();
    assert_ne!(moved, original);
    let bounds = outline_rect(render::outline(&editor.doc, &moved, &editor.cache));
    let corner = editor.view().to_logical(Point {
        x: bounds.x,
        y: bounds.y,
    });
    assert!(editor.press_resize(corner.0, corner.1));
    editor.release(corner.0 - 10.0, corner.1 - 10.0, false);
    assert_ne!(editor.doc.marks[1].1, moved);
    editor.undo();
    assert_eq!(editor.doc.marks[1].1, moved);
    editor.undo();
    assert_eq!(editor.doc.marks[1].1, original);
}

#[test]
#[ignore = "needs a display"]
fn a_selected_mark_moves_by_drag_and_delete_removes_it() {
    if gtk4::init().is_err() {
        return;
    }
    let mut editor = editor();
    editor.doc.selected = Some(0);
    editor.press(12.0, 20.0);
    editor.release(22.0, 25.0, false);
    let Mark::Rect { rect, .. } = &editor.doc.marks[0].1 else {
        panic!("still a rectangle");
    };
    assert_eq!((rect.x, rect.y), (BufferPx(20), BufferPx(15)));

    editor.delete();
    assert!(editor.doc.marks.is_empty());
    assert_eq!(editor.doc.selected, None);
}

#[test]
#[ignore = "needs a display"]
fn text_commits_capped_and_blank_text_is_dropped() {
    if gtk4::init().is_err() {
        return;
    }
    let mut editor = editor();
    editor.tool = Tool::Text;
    editor.press(150.0, 80.0);
    editor.release(150.0, 80.0, false);
    assert_eq!(editor.typing, Some(at(150, 80)));
    editor.commit_text("   ");
    assert_eq!(editor.doc.marks.len(), 1);

    editor.typing = Some(at(150, 80));
    editor.commit_text(&"x".repeat(MAX_TEXT_CHARS + 10));
    let Mark::Text { text, .. } = &editor.doc.marks[1].1 else {
        panic!("a text mark");
    };
    assert_eq!(text.chars().count(), MAX_TEXT_CHARS);
}
