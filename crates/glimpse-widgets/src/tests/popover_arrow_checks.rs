use super::*;

use gtk4::{cairo, graphene, gsk};

fn node(arrow: &PopoverArrow) -> gsk::RenderNode {
    let snapshot = gtk4::Snapshot::new();
    arrow
        .parent()
        .expect("a parent")
        .snapshot_child(arrow, &snapshot);
    snapshot.to_node().expect("a render node")
}

fn rendered_alpha(node: &gsk::RenderNode, points: [graphene::Point; 2]) -> [u8; 2] {
    let bounds = node.bounds();
    let (width, height) = (bounds.width().ceil() as i32, bounds.height().ceil() as i32);
    let mut surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, width, height).expect("image surface");
    let context = cairo::Context::new(&surface).expect("render context");
    context.translate(-f64::from(bounds.x()), -f64::from(bounds.y()));
    node.draw(&context);
    drop(context);
    surface.flush();
    let stride = surface.stride() as usize;
    let pixels = surface.data().expect("rendered pixels");
    points.map(|point| {
        let (x, y) = (
            (point.x() - bounds.x()).floor() as i32,
            (point.y() - bounds.y()).floor() as i32,
        );
        if x < 0 || y < 0 || x >= width || y >= height {
            return 0;
        }
        let offset = y as usize * stride + x as usize * 4;
        (u32::from_ne_bytes(pixels[offset..offset + 4].try_into().expect("ARGB pixel")) >> 24) as u8
    })
}

pub(super) fn check() {
    let arrow = PopoverArrow::new(Tip::Up);
    let window = gtk4::Window::new();
    window.set_child(Some(&arrow));
    arrow.add_css_class("applet-popover__arrow");
    arrow.set_halign(gtk4::Align::Start);
    arrow.set_valign(gtk4::Align::Start);
    window.present();
    while glib::MainContext::default().iteration(false) {}
    let (width, height) = (arrow.width(), arrow.height());
    assert!(width > 0 && height > 0);
    let (w, h) = (width as f32, height as f32);

    for (tip, solid, empty) in [
        (Tip::Up, (w * 0.2, h * 0.8), (w * 0.2, h * 0.2)),
        (Tip::Down, (w * 0.2, h * 0.2), (w * 0.2, h * 0.8)),
        (Tip::Left, (w * 0.8, h * 0.2), (w * 0.2, h * 0.2)),
        (Tip::Right, (w * 0.2, h * 0.2), (w * 0.8, h * 0.2)),
    ] {
        arrow.set_tip(tip);
        let node = node(&arrow);
        let parent = arrow.parent().expect("a parent");
        let points = [solid, empty].map(|(x, y)| {
            arrow
                .compute_point(&parent, &graphene::Point::new(x, y))
                .expect("point in parent coordinates")
        });
        let [solid_alpha, empty_alpha] = rendered_alpha(&node, points);
        assert!(solid_alpha > 0, "{tip:?}");
        assert_eq!(empty_alpha, 0, "{tip:?}");
    }
    window.destroy();
}
