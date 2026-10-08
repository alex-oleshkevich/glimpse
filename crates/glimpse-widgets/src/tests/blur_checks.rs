use super::*;

use gtk4::gsk;

fn settle() {
    while gtk4::glib::MainContext::default().iteration(false) {}
}

fn node_of(widget: &gtk4::Widget) -> gsk::RenderNode {
    let paintable = gtk4::WidgetPaintable::new(Some(widget));
    let snapshot = gtk4::Snapshot::new();
    paintable.snapshot(&snapshot, 200.0, 100.0);
    snapshot.to_node().expect("a painted widget")
}

fn children(node: &gsk::RenderNode) -> Vec<gsk::RenderNode> {
    if let Some(container) = node.downcast_ref::<gsk::ContainerNode>() {
        return (0..container.n_children())
            .map(|index| container.child(index))
            .collect();
    }
    let child = if let Some(node) = node.downcast_ref::<gsk::TransformNode>() {
        node.child()
    } else if let Some(node) = node.downcast_ref::<gsk::ClipNode>() {
        node.child()
    } else if let Some(node) = node.downcast_ref::<gsk::RoundedClipNode>() {
        node.child()
    } else if let Some(node) = node.downcast_ref::<gsk::CopyNode>() {
        node.child()
    } else if let Some(node) = node.downcast_ref::<gsk::OpacityNode>() {
        node.child()
    } else if let Some(node) = node.downcast_ref::<gsk::DebugNode>() {
        node.child()
    } else {
        return Vec::new();
    };
    vec![child]
}

fn widest_blur(node: &gsk::RenderNode) -> f32 {
    let own = node
        .downcast_ref::<gsk::BlurNode>()
        .map_or(0.0, |blur| blur.radius());
    children(node).iter().map(widest_blur).fold(own, f32::max)
}

fn panel_in_window(blurred: bool) -> (gtk4::Window, gtk4::Box) {
    let window = gtk4::Window::new();
    let panel = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    panel.add_css_class("panel");
    panel.set_size_request(200, 100);
    window.set_child(Some(&panel));
    crate::blur::apply_with(&window, blurred);
    assert_eq!(window.has_css_class("blurred"), blurred);
    window.present();
    settle();
    (window, panel)
}

pub(super) fn check() {
    let (blurred, panel) = panel_in_window(true);
    let radius = widest_blur(&node_of(panel.upcast_ref()));
    assert!(
        radius >= 20.0,
        "a blurred panel carries a blur node GTK reports a region for, found radius {radius}"
    );
    crate::blur::apply_with(&blurred, false);
    assert!(!blurred.has_css_class("blurred"));
    blurred.destroy();

    let (plain, panel) = panel_in_window(false);
    assert_eq!(
        widest_blur(&node_of(panel.upcast_ref())),
        0.0,
        "an unblurred panel paints no blur"
    );
    plain.destroy();
}
