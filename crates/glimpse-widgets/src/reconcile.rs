use gtk4::prelude::*;

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
    let parent = parent.downgrade();
    let leaving = widget.clone();
    crate::fade_out(&leaving.clone(), vec![widget], move || {
        if leaving.parent() == parent.upgrade() {
            leaving.unparent();
        }
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
