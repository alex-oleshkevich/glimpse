mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::SplitRow;

const CARD: &str = "card";
pub const OPENER: &str = "expandable__opener";

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
        let handler = self.toggle_from(head);
        if let Some((old, handler)) = imp.head.replace(Some((head.clone(), handler))) {
            if let Some((source, handler)) = handler {
                source.disconnect(handler);
            }
            old.unparent();
        }
        head.insert_before(self, Some(&imp.drawer));
    }

    pub(crate) fn connect_late_opener(&self) {
        let mut held = self.imp().head.borrow_mut();
        if let Some((head, handler @ None)) = held.as_mut() {
            *handler = self.toggle_from(head);
        }
    }

    fn toggle_from(&self, head: &gtk4::Widget) -> Option<imp::Toggle> {
        let toggle = glib::clone!(
            #[weak(rename_to = expandable)]
            self,
            move || expandable.set_expanded(!expandable.expanded())
        );
        match (
            head.downcast_ref::<SplitRow>(),
            head.downcast_ref::<gtk4::Button>(),
            opener(head),
        ) {
            (Some(split), _, _) => Some((
                split.clone().upcast(),
                split.connect_details(move |_| toggle()),
            )),
            (None, Some(row), _) => {
                Some((row.clone().upcast(), row.connect_clicked(move |_| toggle())))
            }
            (None, None, Some(badge)) => Some((
                badge.clone().upcast(),
                badge.connect_clicked(move |_| toggle()),
            )),
            (None, None, None) => None,
        }
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
        if details.is_none() {
            self.set_expanded(false);
        }
    }

    pub fn details<T: IsA<gtk4::Widget>>(&self) -> Option<T> {
        self.imp().drawer.child().and_downcast()
    }

    pub(crate) fn opens_from(&self, target: &gtk4::Widget) -> bool {
        let Some(head) = self.head::<gtk4::Widget>() else {
            return false;
        };
        let opener = match head.downcast_ref::<SplitRow>() {
            Some(split) => split.detail().upcast(),
            None if head.is::<gtk4::Button>() => head,
            None => match opener(&head) {
                Some(badge) => badge.upcast(),
                None => return false,
            },
        };
        target == &opener || target.is_ancestor(&opener)
    }
}

fn opener(head: &gtk4::Widget) -> Option<gtk4::Button> {
    let mut child = head.first_child();
    while let Some(widget) = child {
        if widget.has_css_class(OPENER)
            && let Some(button) = widget.downcast_ref::<gtk4::Button>()
        {
            return Some(button.clone());
        }
        if let Some(found) = opener(&widget) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}
