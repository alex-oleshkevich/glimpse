use super::*;
use std::time::{Duration, Instant};

pub(super) fn check(styles: &Styles) {
    let settings = gtk4::Settings::default().expect("GTK settings");
    let animations = settings.property::<bool>("gtk-enable-animations");
    let duration = duration_ms();
    settings.set_property("gtk-enable-animations", true);
    styles.set_animation_speed(0.25);

    synchronous_removal();
    animated_removal();
    section_exit();
    unmapped_section();
    idle_section_leaves_with_its_last_row();

    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let mut held = Vec::new();
    update(&list, &mut held, &[1]);
    let row = held[0].1.clone();
    let window = mapped(&list);
    styles.set_animation_speed(0.0);
    assert_eq!(duration_ms(), 0);
    update(&list, &mut held, &[]);
    assert!(row.parent().is_none());
    assert!(list.first_child().is_none());
    window.close();

    styles.set_animation_speed(if duration == 0 {
        0.0
    } else {
        150.0 / f64::from(duration)
    });
    settings.set_property("gtk-enable-animations", animations);
}

fn update(list: &gtk4::Box, held: &mut Vec<(u8, gtk4::Box)>, wanted: &[u8]) {
    reconcile::by_key(
        list,
        held,
        wanted,
        |key| *key,
        |key| {
            let row = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            let expandable = Expandable::new(&gtk4::Button::with_label(&key.to_string()));
            expandable.set_details(Some(&gtk4::Label::new(Some("Details\nMore details"))));
            row.append(&expandable);
            row
        },
        |_, _| {},
    );
}

fn mapped(widget: &impl IsA<gtk4::Widget>) -> gtk4::Window {
    let window = gtk4::Window::new();
    window.set_child(Some(widget));
    window.present();
    wait_until(|| widget.is_mapped() && widget.height() > 0);
    window
}

fn wait_until(ready: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let context = glib::MainContext::default();
    while !ready() {
        assert!(Instant::now() < deadline, "exit presence timed out");
        context.iteration(false);
    }
}

fn synchronous_removal() {
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let mut held = Vec::new();
    update(&list, &mut held, &[1]);
    let row = held[0].1.clone();
    assert!(!row.is_mapped());
    assert!(duration_ms() > 0);
    update(&list, &mut held, &[]);
    assert!(row.parent().is_none());
    assert!(list.first_child().is_none());
}

fn animated_removal() {
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let mut held = Vec::new();
    update(&list, &mut held, &[1, 2, 3]);
    let a = held[0].1.clone();
    let b = held[1].1.clone();
    let c = held[2].1.clone();
    let expandable = b
        .first_child()
        .and_downcast::<Expandable>()
        .expect("nested drawer");
    expandable.set_expanded(true);
    let window = mapped(&list);
    let reparents = Rc::new(Cell::new(0));
    c.connect_parent_notify({
        let reparents = reparents.clone();
        move |_| reparents.set(reparents.get() + 1)
    });

    update(&list, &mut held, &[1, 3]);
    assert!(b.parent().is_none(), "the real row leaves at once");
    assert!(
        !expandable.expanded(),
        "its drawer is closed so the dimming lifts"
    );
    let exit = a
        .next_sibling()
        .and_downcast::<gtk4::Revealer>()
        .expect("a picture of the row holds its place");
    assert!(exit.has_css_class("exiting"));
    assert!(
        exit.child()
            .is_some_and(|child| child.is::<gtk4::Picture>() && child.opacity() > 0.5),
        "the picture starts visible and fades from there"
    );
    assert_eq!(exit.next_sibling(), Some(c.clone().upcast()));
    assert_eq!(reparents.get(), 0);
    let height = exit.measure(gtk4::Orientation::Vertical, -1).1;
    assert!(height > 0);
    wait_until(|| {
        exit.parent().is_some() && exit.measure(gtk4::Orientation::Vertical, -1).1 < height
    });
    wait_until(|| exit.parent().is_none());
    assert_eq!(a.next_sibling(), Some(c.upcast()));
    assert_eq!(reparents.get(), 0);
    window.close();
}

fn section_exit() {
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let section = Section::new();
    section.set_content(Some(&gtk4::Label::new(Some("Row"))));
    column.append(&section);
    let window = mapped(&column);
    section.set_visible(false);
    assert!(
        section.get_visible(),
        "the section waits for a painted frame"
    );
    wait_until(|| !section.get_visible());
    let exit = section
        .next_sibling()
        .and_downcast::<gtk4::Revealer>()
        .expect("a picture of the section collapses in its place");
    assert!(exit.has_css_class("exiting"));
    wait_until(|| exit.parent().is_none());
    window.close();
}

fn unmapped_section() {
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let section = Section::new();
    column.append(&section);
    section.set_visible(false);
    assert!(!section.get_visible());
    assert!(section.next_sibling().is_none());
}

fn idle_section_leaves_with_its_last_row() {
    let idle = IdlePopover::new();
    idle.set_inhibitors(&[inhibitor(1, "Zoom", true)]);
    let window = mapped(&idle);
    let others = idle.imp().others.get();
    wait_until(|| others.is_mapped());
    idle.set_inhibitors(&[]);
    wait_until(|| !others.get_visible());
    let exit = others
        .next_sibling()
        .filter(|sibling| sibling.has_css_class("exiting"))
        .expect("the section and its last row collapse as one picture");
    wait_until(|| exit.parent().is_none());
    window.close();
}
