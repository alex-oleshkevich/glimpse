pub fn set(drawer: &gtk4::Revealer, open: bool) {
    if drawer.reveals_child() != open {
        drawer.set_reveal_child(open);
    }
}

pub fn toggle(drawer: &gtk4::Revealer) {
    set(drawer, !drawer.reveals_child());
}
