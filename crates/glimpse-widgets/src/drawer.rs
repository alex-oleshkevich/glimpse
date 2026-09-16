use gtk4::prelude::*;

pub const OPEN: &str = "open";
pub const RECEDED: &str = "receded";

pub fn set(drawer: &gtk4::Revealer, open: bool) {
    if drawer.reveals_child() != open {
        drawer.set_reveal_child(open);
    }
}

pub fn toggle(drawer: &gtk4::Revealer) {
    set(drawer, !drawer.reveals_child());
}

pub fn holder(head: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    let holder = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    holder.append(head);
    holder.append(
        &gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideDown)
            .reveal_child(false)
            .build(),
    );
    holder
}

pub fn head<T: IsA<gtk4::Widget>>(holder: &gtk4::Box) -> Option<T> {
    holder.first_child().and_downcast::<T>()
}

pub fn panel(holder: &gtk4::Box) -> Option<gtk4::Revealer> {
    holder.last_child().and_downcast::<gtk4::Revealer>()
}
