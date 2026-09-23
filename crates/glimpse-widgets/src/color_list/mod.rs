mod imp;

use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

use crate::{Row, SplitRow, Swatch, drawer, none_if_empty};

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

    pub fn set_open(&self, open: Option<u64>) {
        if self.imp().open.replace(open) == open {
            return;
        }
        self.reveal();
    }

    fn render(&self) {
        let imp = self.imp();
        let shades = imp.shades.borrow();
        let mut holders = imp.holders.borrow_mut();

        for (index, shade) in shades.iter().enumerate() {
            if holders.len() == index {
                let holder = drawer::holder(&self.build_row(index as u32));
                holder.insert_after(self, holders.last());
                holders.push(holder);
            }
            if let Some(split) = drawer::head::<SplitRow>(&holders[index]) {
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
        }

        for holder in holders.split_off(shades.len()) {
            holder.unparent();
        }
        drop(shades);
        drop(holders);
        self.reveal();
    }

    fn build_row(&self, index: u32) -> SplitRow {
        let split = SplitRow::new();
        split.set_property("detail-icon", "pan-end-symbolic");
        split.connect_activated(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.report("activated", index)
        ));
        split.connect_details(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.report("detailed", index)
        ));
        split
    }

    fn panel(&self, shade: &Shade) -> gtk4::Box {
        let panel = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        panel.add_css_class("detail-card");
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

    fn reveal(&self) {
        let imp = self.imp();
        let open = imp.open.get();
        let shades = imp.shades.borrow();
        let holders = imp.holders.borrow();

        for (holder, shade) in holders.iter().zip(shades.iter()) {
            let Some(panel) = drawer::panel(holder) else {
                continue;
            };
            let wanted = open == Some(shade.id);
            let key = (shade.id, shade.notations.clone());
            if wanted {
                let stale = imp.built.borrow().as_ref() != Some(&key);
                if stale || panel.child().is_none() {
                    panel.set_child(Some(&self.panel(shade)));
                    imp.built.replace(Some(key));
                }
            } else {
                panel.set_child(gtk4::Widget::NONE);
            }
            drawer::set(&panel, wanted);
            if let Some(split) = drawer::head::<SplitRow>(holder) {
                crate::set_css_class(&split, drawer::RECEDED, open.is_some() && !wanted);
                crate::set_css_class(&split, drawer::OPEN, wanted);
            }
        }
    }

    fn report(&self, signal: &str, index: u32) {
        let id = self
            .imp()
            .shades
            .borrow()
            .get(index as usize)
            .map(|shade| shade.id);
        if let Some(id) = id {
            self.emit_by_name::<()>(signal, &[&id]);
        }
    }

    pub fn connect_activated<F: Fn(&Self, u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |list: Self, id: u64| f(&list, id)),
        )
    }

    pub fn connect_detailed<F: Fn(&Self, u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "detailed",
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
