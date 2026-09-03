use gtk4::prelude::*;

pub(crate) fn by_key<T, W, K>(
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
        widget.unparent();
    }

    let mut previous: Option<W> = None;
    for (_, widget) in &next {
        let expected = previous.clone().map(Cast::upcast::<gtk4::Widget>);
        if widget.parent().is_none() || widget.prev_sibling() != expected {
            if widget.parent().is_some() {
                widget.unparent();
            }
            widget.insert_after(parent, previous.as_ref());
        }
        previous = Some(widget.clone());
    }

    *held = next;
}
