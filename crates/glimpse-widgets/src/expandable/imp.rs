use std::cell::RefCell;
use std::marker::PhantomData;

use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};

use super::CARD;
use crate::{PopoverShell, drawer};

#[derive(Debug, Default, glib::Properties)]
#[properties(wrapper_type = super::Expandable)]
pub struct Expandable {
    pub head: RefCell<Option<(gtk4::Widget, Option<glib::SignalHandlerId>)>>,
    pub drawer: gtk4::Revealer,

    #[property(name = "expanded", get = Self::expanded, set = Self::set_expanded, explicit_notify)]
    expanded: PhantomData<bool>,
}

impl Expandable {
    fn expanded(&self) -> bool {
        self.drawer.reveals_child()
    }

    fn set_expanded(&self, expanded: bool) {
        if self.drawer.reveals_child() == expanded || expanded && self.drawer.child().is_none() {
            return;
        }
        let obj = self.obj();
        if expanded {
            obj.add_css_class(CARD);
        }
        crate::set_css_class(&*obj, drawer::OPEN, expanded);
        drawer::set(&self.drawer, expanded);
        obj.notify_expanded();
        if let Some(shell) = self.shell() {
            shell.focus(&obj);
        }
    }

    fn shell(&self) -> Option<PopoverShell> {
        self.obj()
            .ancestor(PopoverShell::static_type())
            .and_downcast()
    }
}

#[glib::object_subclass]
impl ObjectSubclass for Expandable {
    const NAME: &'static str = "Expandable";
    type Type = super::Expandable;
    type ParentType = gtk4::Widget;
    type Interfaces = (gtk4::Buildable,);

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_css_name("expandable");
        klass.set_accessible_role(AccessibleRole::Group);
    }
}

#[glib::derived_properties]
impl ObjectImpl for Expandable {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        if let Some(layout) = obj.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_orientation(gtk4::Orientation::Vertical);
        }

        self.drawer
            .set_transition_type(gtk4::RevealerTransitionType::SlideDown);
        self.drawer.set_parent(&*obj);
        self.drawer.connect_child_revealed_notify(glib::clone!(
            #[weak]
            obj,
            move |drawer| {
                if !drawer.reveals_child() && !drawer.is_child_revealed() {
                    obj.remove_css_class(CARD);
                }
            }
        ));
    }

    fn dispose(&self) {
        if let Some((head, _)) = self.head.take() {
            head.unparent();
        }
        self.drawer.unparent();
    }
}

impl WidgetImpl for Expandable {
    fn root(&self) {
        self.parent_root();
        if let Some(shell) = self.drawer.reveals_child().then(|| self.shell()).flatten() {
            shell.focus(&self.obj());
        }
    }

    fn unroot(&self) {
        let shell = self.drawer.reveals_child().then(|| self.shell()).flatten();
        self.parent_unroot();
        if let Some(shell) = shell {
            shell.release(&self.obj());
        }
    }
}

impl BuildableImpl for Expandable {
    fn add_child(&self, builder: &gtk4::Builder, child: &glib::Object, kind: Option<&str>) {
        match (kind, child.downcast_ref::<gtk4::Widget>()) {
            (Some("details"), Some(details)) => self.obj().set_details(Some(details)),
            (None, Some(head)) => self.obj().set_head(head),
            _ => self.parent_add_child(builder, child, kind),
        }
    }
}
