use super::*;

pub(super) fn check() {
    let built = gtk4::Builder::from_string(
        r#"<interface>
                 <object class="Expandable" id="expandable">
                   <child><object class="GtkButton" id="head"/></child>
                   <child type="details"><object class="GtkLabel" id="details"/></child>
                 </object>
               </interface>"#,
    );
    let expandable: Expandable = built.object("expandable").expect("an Expandable");
    assert_eq!(
        expandable.head::<gtk4::Button>(),
        built.object::<gtk4::Button>("head"),
        "an unnamed child is the row that opens it"
    );
    assert_eq!(
        expandable.details::<gtk4::Label>(),
        built.object::<gtk4::Label>("details"),
        "and [details] is what it reveals"
    );
    assert_eq!(
        expandable
            .last_child()
            .and_downcast::<gtk4::Revealer>()
            .map(|r| r.transition_type()),
        Some(gtk4::RevealerTransitionType::SlideDown),
        "the row stays first and the details unfold under it"
    );

    let notified = Rc::new(Cell::new(0));
    expandable.connect_expanded_notify({
        let notified = Rc::clone(&notified);
        move |_| notified.set(notified.get() + 1)
    });
    expandable.set_expanded(true);
    assert!(
        expandable.has_css_class("card") && expandable.has_css_class("open"),
        "opening makes the row and its details one card"
    );
    expandable.set_expanded(true);
    assert_eq!(notified.get(), 1, "an unchanged expanded writes nothing");
    expandable.set_expanded(false);
    assert!(
        !expandable.has_css_class("card") && !expandable.has_css_class("open"),
        "an unmapped drawer closes at once, so the card goes with it"
    );
    let head: gtk4::Button = built.object("head").expect("a head button");
    head.emit_by_name::<()>("clicked", &[]);
    assert!(expandable.expanded(), "a plain row opens its own details");
    head.emit_by_name::<()>("clicked", &[]);
    assert!(!expandable.expanded(), "and the same row closes them");
}
