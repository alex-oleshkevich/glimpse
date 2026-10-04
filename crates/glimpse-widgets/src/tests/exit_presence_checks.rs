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
    section_presence(false, false);
    section_presence(true, false);
    section_presence(false, true);
    unmapped_section();

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
    let deadline = Instant::now() + Duration::from_secs(3);
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
    let exit = b
        .parent()
        .and_downcast::<gtk4::Revealer>()
        .expect("the leaving row stays inside a collapse wrapper");
    assert_eq!(exit.parent(), Some(list.clone().upcast()));
    assert!(b.has_css_class("leaving") && exit.has_css_class("leaving"));
    assert_eq!(
        exit.transition_type(),
        gtk4::RevealerTransitionType::SlideUp
    );
    assert_eq!(exit.transition_duration(), duration_ms());
    assert!(!exit.reveals_child() && exit.is_child_revealed());
    assert!(!expandable.expanded());
    assert_eq!(list.first_child(), Some(a.clone().upcast()));
    assert_eq!(a.next_sibling(), Some(exit.clone().upcast()));
    assert_eq!(exit.next_sibling(), Some(c.clone().upcast()));
    assert_eq!(reparents.get(), 0);
    let height = exit.measure(gtk4::Orientation::Vertical, -1).1;
    wait_until(|| {
        let opacity = b.opacity();
        opacity > 0.0 && opacity < 1.0 && exit.measure(gtk4::Orientation::Vertical, -1).1 < height
    });
    assert!(exit.parent().is_some());
    wait_until(|| exit.parent().is_none());
    assert!(b.parent().is_none());
    assert_eq!(a.next_sibling(), Some(c.upcast()));
    assert_eq!(reparents.get(), 0);
    window.close();
}

fn section_presence(cancel_hide: bool, hide_first: bool) {
    let section = Section::new();
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    section.set_content(Some(&list));
    let mut held = Vec::new();
    update(&list, &mut held, &[1]);
    let row = held[0].1.clone();
    let window = mapped(&section);
    if hide_first {
        section.set_visible(false);
    }
    update(&list, &mut held, &[]);
    let exit = list.first_child().expect("the exit remains parented");
    if !hide_first {
        section.set_visible(false);
    }
    glib::MainContext::default().iteration(false);
    assert!(section.get_visible());
    assert!(section.is_mapped() && row.is_mapped());
    assert_eq!(exit.parent(), Some(list.clone().upcast()));
    if cancel_hide {
        section.set_visible(true);
    }
    wait_until(|| exit.parent().is_none());
    assert_eq!(section.get_visible(), cancel_hide);
    assert!(row.parent().is_none());
    window.close();
}

fn unmapped_section() {
    let section = Section::new();
    let row = gtk4::Label::new(Some("Unmapped exit"));
    row.add_css_class("leaving");
    section.set_content(Some(&row));
    assert!(!section.is_mapped());
    section.set_visible(false);
    assert!(!section.get_visible());
}
