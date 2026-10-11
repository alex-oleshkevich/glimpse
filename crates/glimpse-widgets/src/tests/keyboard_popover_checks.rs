use super::*;

pub(super) fn check() {
    let keyboard = KeyboardPopover::new();
    let layout = |name: &str, active: bool| KeyboardLayout {
        name: name.to_owned(),
        active,
    };
    keyboard.set_layouts(&[layout("English (US)", true), layout("Russian", false)]);
    keyboard.set_layouts(&[layout("English (US)", true), layout("Russian", false)]);
    let rows = keyboard.imp().widgets.borrow().clone();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].active() && !rows[1].active());
    assert_eq!(keyboard.imp().rows.observe_children().n_items(), 2);

    let activated = Rc::new(RefCell::new(Vec::new()));
    keyboard.connect_activated({
        let activated = Rc::clone(&activated);
        move |_, index| activated.borrow_mut().push(index)
    });
    rows[1].activate();
    assert!(
        !rows[0].active() && rows[1].active(),
        "picking a layout moves the check to it"
    );
    rows[1].activate();
    assert!(
        rows[1].active(),
        "picking the active layout again keeps its check"
    );
    assert_eq!(
        *activated.borrow(),
        vec![1],
        "only a real switch is reported"
    );

    keyboard.set_layouts(&[layout("English (US)", true)]);
    assert_eq!(keyboard.imp().rows.observe_children().n_items(), 1);

}
