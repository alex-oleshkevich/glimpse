mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::SplitRow;

const CARD: &str = "card";

glib::wrapper! {
    pub struct Expandable(ObjectSubclass<imp::Expandable>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for Expandable {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Expandable {
    pub fn new(head: &impl IsA<gtk4::Widget>) -> Self {
        let expandable = Self::default();
        expandable.set_head(head);
        expandable
    }

    pub fn set_head(&self, head: &impl IsA<gtk4::Widget>) {
        let imp = self.imp();
        let head = head.upcast_ref::<gtk4::Widget>();
        if self.head::<gtk4::Widget>().as_ref() == Some(head) {
            return;
        }
        let toggle = glib::clone!(
            #[weak(rename_to = expandable)]
            self,
            move || expandable.set_expanded(!expandable.expanded())
        );
        let handler = match (
            head.downcast_ref::<SplitRow>(),
            head.downcast_ref::<gtk4::Button>(),
        ) {
            (Some(split), _) => Some(split.connect_details(move |_| toggle())),
            (None, Some(row)) => Some(row.connect_clicked(move |_| toggle())),
            (None, None) => None,
        };
        if let Some((old, handler)) = imp.head.replace(Some((head.clone(), handler))) {
            if let Some(handler) = handler {
                old.disconnect(handler);
            }
            old.unparent();
        }
        head.insert_before(self, Some(&imp.drawer));
    }

    pub fn head<T: IsA<gtk4::Widget>>(&self) -> Option<T> {
        self.imp()
            .head
            .borrow()
            .as_ref()
            .and_then(|(head, _)| head.clone().downcast().ok())
    }

    pub fn set_details(&self, details: Option<&impl IsA<gtk4::Widget>>) {
        let drawer = &self.imp().drawer;
        let details = details.map(|details| details.upcast_ref::<gtk4::Widget>());
        if drawer.child().as_ref() != details {
            drawer.set_child(details);
        }
    }

    pub fn details<T: IsA<gtk4::Widget>>(&self) -> Option<T> {
        self.imp().drawer.child().and_downcast()
    }
}
