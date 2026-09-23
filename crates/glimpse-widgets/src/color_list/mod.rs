mod imp;

use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

use crate::reconcile::by_key;
use crate::{Expandable, Row, SplitRow, Swatch, none_if_empty};

glib::wrapper! {
    pub struct ColorList(ObjectSubclass<imp::ColorList>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shade {
    pub id: u64,
    pub color: gdk::RGBA,
    pub title: String,
    pub notations: Vec<Notation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notation {
    pub key: String,
    pub label: String,
    pub value: String,
}

impl Default for ColorList {
    fn default() -> Self {
        Self::new()
    }
}

impl ColorList {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_shades(&self, shades: &[Shade]) {
        if self.imp().shades.borrow().as_slice() == shades {
            return;
        }
        self.imp().shades.replace(shades.to_vec());
        self.render();
    }

    fn render(&self) {
        let imp = self.imp();
        let shades = imp.shades.borrow();
        by_key(
            self,
            &mut imp.holders.borrow_mut(),
            &shades,
            |shade| shade.id,
            |shade| self.build(shade.id),
            |holder, shade| self.apply(holder, shade),
        );
    }

    fn build(&self, id: u64) -> Expandable {
        let split = SplitRow::new();
        split.set_property("detail-icon", "pan-end-symbolic");
        split.connect_activated(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("activated", &[&id])
        ));
        split.connect_details(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.fill(id)
        ));
        Expandable::new(&split)
    }

    fn apply(&self, holder: &Expandable, shade: &Shade) {
        if let Some(split) = holder.head::<SplitRow>() {
            let row = split.row();
            row.set_title(none_if_empty(&shade.title));
            match row.lead().and_downcast::<Swatch>() {
                Some(swatch) => swatch.set_color(Some(&shade.color)),
                None => {
                    let swatch = Swatch::default();
                    swatch.set_valign(gtk4::Align::Center);
                    swatch.set_color(Some(&shade.color));
                    row.set_lead(&swatch);
                }
            }
        }
        if holder.details::<gtk4::Widget>().is_some() {
            holder.set_details(Some(&self.panel(shade)));
        }
    }

    /// The notations are built the first time a shade is opened, and rebuilt only for a shade that
    /// has been: a palette of a dozen shades would otherwise carry a dozen cards nobody asked for.
    fn fill(&self, id: u64) {
        let imp = self.imp();
        let holder = imp
            .holders
            .borrow()
            .iter()
            .find(|(held, _)| *held == id)
            .map(|(_, holder)| holder.clone());
        let shade = imp
            .shades
            .borrow()
            .iter()
            .find(|shade| shade.id == id)
            .cloned();
        if let (Some(holder), Some(shade)) = (holder, shade)
            && holder.details::<gtk4::Widget>().is_none()
        {
            holder.set_details(Some(&self.panel(&shade)));
        }
    }

    fn panel(&self, shade: &Shade) -> gtk4::Box {
        let panel = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        for notation in &shade.notations {
            let row = Row::new();
            row.add_css_class("color-notation");
            row.set_title(Some(notation.value.as_str()));
            row.set_value(Some(notation.label.as_str()));
            let (id, key) = (shade.id, notation.key.clone());
            row.connect_clicked(glib::clone!(
                #[weak(rename_to = list)]
                self,
                move |_| list.emit_by_name::<()>("copied", &[&id, &key])
            ));
            panel.append(&row);
        }
        panel
    }

    pub fn connect_activated<F: Fn(&Self, u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |list: Self, id: u64| f(&list, id)),
        )
    }

    pub fn connect_copied<F: Fn(&Self, u64, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "copied",
            false,
            glib::closure_local!(move |list: Self, id: u64, key: String| f(&list, id, &key)),
        )
    }
}
