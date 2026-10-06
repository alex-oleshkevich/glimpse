mod imp;

use gettextrs::{gettext, ngettext};
use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

use crate::reconcile::by_key;
use crate::{ActionRow, ExpanderRow, Row, Swatch, clear_children, none_if_empty};

const FOLDED_ROWS: usize = 8;

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
        let shown = match imp.show_all.get() {
            true => shades.len(),
            false => shades.len().min(FOLDED_ROWS),
        };
        by_key(
            &imp.rows,
            &mut imp.holders.borrow_mut(),
            &shades[..shown],
            |shade| shade.id,
            |shade| self.build(shade.id),
            |holder, shade| self.apply(holder, shade),
        );
        let hidden = shades.len() - shown;
        imp.more.set_visible(hidden > 0);
        if hidden > 0 {
            imp.more.upcast_ref::<Row>().set_title(Some(
                ngettext("{count} more color", "{count} more colors", hidden as u32)
                    .replace("{count}", &hidden.to_string())
                    .as_str(),
            ));
        }
    }

    fn build(&self, id: u64) -> ExpanderRow {
        let row = ExpanderRow::new();
        row.connect_toggled(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_, open| {
                if open {
                    list.fill(id);
                }
            }
        ));
        row
    }

    fn apply(&self, row: &ExpanderRow, shade: &Shade) {
        row.set_title(none_if_empty(&shade.title));
        let swatch = Swatch::default();
        swatch.set_valign(gtk4::Align::Center);
        swatch.set_color(Some(&shade.color));
        row.set_end(&swatch);
        let details = row.details();
        if details.first_child().is_some() {
            clear_children(&details);
            self.drawer(&details, shade);
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
        if let (Some(holder), Some(shade)) = (holder, shade) {
            let details = holder.details();
            if details.first_child().is_none() {
                self.drawer(&details, &shade);
            }
        }
    }

    fn drawer(&self, panel: &gtk4::Box, shade: &Shade) {
        let id = shade.id;
        let strip = Swatch::default();
        strip.set_color(Some(&shade.color));
        strip.add_css_class("color-card__swatch");
        panel.append(&strip);
        for notation in &shade.notations {
            let row = ActionRow::new();
            row.add_css_class("color-notation");
            let base: &Row = row.upcast_ref();
            base.set_title(Some(notation.value.as_str()));
            base.set_value(Some(notation.label.as_str()));
            let key = notation.key.clone();
            row.connect_activated(glib::clone!(
                #[weak(rename_to = list)]
                self,
                move |_| list.emit_by_name::<()>("copied", &[&id, &key])
            ));
            panel.append(&row);
        }
        let copy = ActionRow::new();
        copy.upcast_ref::<Row>()
            .set_title(Some(gettext("Copy").as_str()));
        copy.connect_activated(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("activated", &[&id])
        ));
        panel.append(&copy);
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
