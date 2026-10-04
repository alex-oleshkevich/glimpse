use gtk4::prelude::*;
use std::{cell::Cell, rc::Rc};

const LEAVING: &str = "leaving";

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
            leave(parent.upcast_ref(), widget.upcast());
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

fn leave(parent: &gtk4::Widget, widget: gtk4::Widget) {
    if !widget.is_mapped() || crate::duration_ms() == 0 {
        widget.unparent();
        return;
    }
    close_drawers(&widget);
    let section = std::iter::successors(Some(parent.clone()), |node| node.parent())
        .find_map(|node| node.downcast::<crate::Section>().ok())
        .map(|section| section.downgrade());
    let previous = widget.prev_sibling();
    let revealer = gtk4::Revealer::new();
    revealer.add_css_class(LEAVING);
    revealer.set_reveal_child(true);
    widget.unparent();
    revealer.set_child(Some(&widget));
    revealer.insert_after(parent, previous.as_ref());
    revealer.set_transition_type(gtk4::RevealerTransitionType::SlideUp);
    revealer.set_transition_duration(crate::duration_ms());
    let parent = parent.downgrade();
    let fading_parent = parent.clone();
    let weak_revealer = revealer.downgrade();
    let faded = Rc::new(Cell::new(false));
    let finish = Rc::new({
        let faded = faded.clone();
        move || {
            let Some(revealer) = weak_revealer.upgrade() else {
                return;
            };
            if !faded.get() || revealer.is_child_revealed() {
                return;
            }
            if let Some(parent) = parent.upgrade()
                && revealer.parent().as_ref() == Some(&parent)
            {
                let child = revealer.child();
                revealer.set_child(None::<&gtk4::Widget>);
                revealer.unparent();
                if let Some(child) = child {
                    child.set_opacity(1.0);
                }
            }
            if let Some(section) = section.as_ref().and_then(|section| section.upgrade()) {
                section.exit_finished();
            }
        }
    });
    revealer.connect_child_revealed_notify({
        let finish = finish.clone();
        move |_| finish()
    });
    revealer.set_reveal_child(false);
    let collapsing = revealer.downgrade();
    let leaving = widget.clone();
    crate::fade_out(&leaving.clone(), vec![widget], move || {
        if let Some(revealer) = collapsing.upgrade()
            && let Some(parent) = fading_parent.upgrade()
            && revealer.parent().as_ref() == Some(&parent)
            && leaving.parent().as_ref() == Some(revealer.upcast_ref())
        {
            leaving.set_opacity(0.0);
        }
        faded.set(true);
        finish();
    });
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
        .find(|sibling| !sibling.has_css_class(LEAVING))
}
