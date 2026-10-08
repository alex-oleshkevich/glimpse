use super::*;

use gtk4::{graphene, gsk};

fn node(arrow: &PopoverArrow) -> gsk::RenderNode {
    let snapshot = gtk4::Snapshot::new();
    arrow
        .parent()
        .expect("a parent")
        .snapshot_child(arrow, &snapshot);
    snapshot.to_node().expect("a render node")
}

fn inside(fill: &gsk::FillNode, x: f32, y: f32) -> bool {
    fill.path()
        .in_fill(&graphene::Point::new(x, y), gsk::FillRule::Winding)
}

fn settle(until: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(300);
    while std::time::Instant::now() < deadline && !until() {
        glib::MainContext::default().iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
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

    let invalidated = Rc::new(Cell::new(0));
    let paintable = gtk4::WidgetPaintable::new(Some(&arrow));
    paintable.connect_invalidate_contents({
        let invalidated = Rc::clone(&invalidated);
        move |_| invalidated.set(invalidated.get() + 1)
    });

    arrow.set_tip(Tip::Up);
    settle(|| invalidated.get() > 0);
    assert_eq!(invalidated.get(), 0);
    arrow.set_tip(Tip::Down);
    settle(|| invalidated.get() > 0);
    assert!(invalidated.get() > 0);
    assert_eq!(arrow.tip(), Tip::Down);

    for (tip, solid, empty) in [
        (Tip::Up, (w / 2.0, h * 0.75), (1.0, 1.0)),
        (Tip::Down, (w / 2.0, h * 0.25), (1.0, h - 1.0)),
        (Tip::Left, (w * 0.75, h / 2.0), (1.0, 1.0)),
        (Tip::Right, (w * 0.25, h / 2.0), (w - 1.0, 1.0)),
    ] {
        arrow.set_tip(tip);
        let node = node(&arrow);
        assert_eq!(node.node_type(), gsk::RenderNodeType::FillNode, "{tip:?}");
        let fill = node.downcast::<gsk::FillNode>().expect("a fill node");
        assert!(inside(&fill, solid.0, solid.1), "{tip:?}");
        assert!(!inside(&fill, empty.0, empty.1), "{tip:?}");
        assert!(
            matches!(
                fill.child().node_type(),
                gsk::RenderNodeType::ColorNode | gsk::RenderNodeType::ContainerNode
            ),
            "{tip:?}"
        );
    }
    window.destroy();
}
