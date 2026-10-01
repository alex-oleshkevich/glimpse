use super::*;

pub(super) fn check() {
    let split = SplitRow::new();
    split.set_title(Some("DP-1 · DELL U2720Q"));
    split.set_value(Some("144 Hz"));
    assert_eq!(
        split.row().title().as_deref(),
        Some("DP-1 · DELL U2720Q"),
        "a forwarded property reaches the row rather than living twice"
    );

    let built = gtk4::Builder::from_string(
        r#"<interface><object class="SplitRow" id="split">
                 <child type="trail"><object class="GtkSwitch" id="knob"/></child>
               </object></interface>"#,
    );
    let declared: SplitRow = built.object("split").expect("SplitRow builds from a .ui");
    let knob: gtk4::Switch = built.object("knob").expect("the trail child builds");
    assert_eq!(
        knob.ancestor(Row::static_type()),
        Some(declared.row().upcast::<gtk4::Widget>()),
        "a [trail] child lands inside the row, so the detail button stays last"
    );
    assert_eq!(
        split.detail().parent().as_ref(),
        Some(split.upcast_ref::<gtk4::Widget>()),
        "the detail button is the wrapper's own child, never the row's"
    );

    let fired: Rc<RefCell<Vec<&str>>> = Rc::new(RefCell::new(Vec::new()));
    split.connect_activated({
        let fired = Rc::clone(&fired);
        move |_| fired.borrow_mut().push("activated")
    });
    split.connect_details({
        let fired = Rc::clone(&fired);
        move |_| fired.borrow_mut().push("details")
    });

    split.row().emit_clicked();
    assert_eq!(
        *fired.borrow(),
        ["activated"],
        "the row body is the primary action and says nothing about details"
    );
    split.detail().emit_clicked();
    assert_eq!(
        *fired.borrow(),
        ["activated", "details"],
        "the detail button is the only way in, and does not also act"
    );
}
