use gtk4::{glib, prelude::*};

const EXITING: &str = "exiting";

pub fn by_key<T, W, K>(
    parent: &impl IsA<gtk4::Widget>,
    held: &mut Vec<(K, W)>,
    wanted: &[T],
    key: impl Fn(&T) -> K,
    build: impl Fn(&T) -> W,
    apply: impl Fn(&W, &T),
) where
    K: PartialEq,
    W: IsA<gtk4::Widget> + Clone,
{
    let mut next: Vec<(K, W)> = Vec::with_capacity(wanted.len());

    for item in wanted {
        let key = key(item);
        let widget = match held.iter().position(|(held, _)| *held == key) {
            Some(at) => held.remove(at).1,
            None => build(item),
        };
        apply(&widget, item);
        next.push((key, widget));
    }

    for (_, widget) in held.drain(..) {
        if widget.parent().as_ref() == Some(parent.upcast_ref()) {
            leave(widget.upcast());
        }
    }

    let mut previous: Option<W> = None;
    for (_, widget) in &next {
        let expected = previous.clone().map(Cast::upcast::<gtk4::Widget>);
        if widget.parent().as_ref() != Some(parent.upcast_ref())
            || staying_before(widget.upcast_ref()) != expected
        {
            if widget.parent().is_some() {
                widget.unparent();
            }
            widget.insert_after(parent, previous.as_ref());
        }
        previous = Some(widget.clone());
    }

    *held = next;
}

fn leave(widget: gtk4::Widget) {
    close_drawers(&widget);
    exit(&widget);
    widget.unparent();
}

pub(crate) fn exit(widget: &gtk4::Widget) {
    use adw::prelude::*;

    let duration = crate::duration_ms() * 2;
    let Some(parent) = widget.parent() else {
        return;
    };
    if !widget.is_mapped()
        || duration == 0
        || !parent
            .layout_manager()
            .is_some_and(|layout| layout.is::<gtk4::BoxLayout>())
    {
        return;
    }
    let ghost =
        gtk4::Picture::for_paintable(&gtk4::WidgetPaintable::new(Some(widget)).current_image());
    ghost.set_can_shrink(false);
    let exit = gtk4::Revealer::builder()
        .transition_type(gtk4::RevealerTransitionType::SlideUp)
        .transition_duration(0)
        .reveal_child(true)
        .child(&ghost)
        .build();
    exit.add_css_class(EXITING);
    exit.insert_after(&parent, Some(widget));
    exit.connect_child_revealed_notify(|exit| {
        if exit.reveals_child() || exit.is_child_revealed() {
            return;
        }
        let exit = exit.clone();
        glib::idle_add_local_once(move || {
            if exit.parent().is_some() {
                exit.unparent();
            }
        });
    });
    exit.set_transition_duration(duration);
    exit.set_reveal_child(false);
    let fading = ghost.downgrade();
    let target = adw::CallbackAnimationTarget::new(move |value| {
        if let Some(ghost) = fading.upgrade() {
            ghost.set_opacity(value);
        }
    });
    let fade = adw::TimedAnimation::new(&parent, 1.0, 0.0, duration, target);
    fade.set_easing(adw::Easing::EaseOutCubic);
    fade.play();
}

fn close_drawers(widget: &gtk4::Widget) {
    if let Some(expandable) = widget.downcast_ref::<crate::Expandable>()
        && expandable.expanded()
    {
        expandable.set_expanded(false);
    }
    let mut child = widget.first_child();
    while let Some(node) = child {
        close_drawers(&node);
        child = node.next_sibling();
    }
}

fn staying_before(widget: &gtk4::Widget) -> Option<gtk4::Widget> {
    std::iter::successors(widget.prev_sibling(), |sibling| sibling.prev_sibling())
        .find(|sibling| !sibling.has_css_class(EXITING))
}
