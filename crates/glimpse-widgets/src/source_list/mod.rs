mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Fader, Row, none_if_empty};

pub(crate) const CHANGED: &str = "changed";
const BRIGHTNESS_ICON: &str = "display-brightness-symbolic";

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Source {
    pub key: String,
    pub name: String,
    pub value: f64,
    pub maximum: f64,
    pub floor: f64,
}

glib::wrapper! {
    pub struct SourceList(ObjectSubclass<imp::SourceList>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for SourceList {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceList {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_sources(&self, sources: &[Source]) {
        let imp = self.imp();
        if imp.sources.borrow().as_slice() == sources {
            return;
        }
        imp.sources.replace(sources.to_vec());
        self.render();
    }

    pub fn connect_changed<F: Fn(&Self, String, f64) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            CHANGED,
            false,
            glib::closure_local!(move |list: Self, key: String, value: f64| f(&list, key, value)),
        )
    }

    fn render(&self) {
        let imp = self.imp();
        #[cfg(test)]
        imp.renders.set(imp.renders.get() + 1);
        let sources = imp.sources.borrow();
        let mut rows = imp.rows.borrow_mut();

        for (index, source) in sources.iter().enumerate() {
            if rows.len() == index {
                let (row, fader) = self.build_row(index as u32);
                row.insert_after(self, rows.last().map(|(_, fader)| fader));
                fader.insert_after(self, Some(&row));
                rows.push((row, fader));
            }
            let (row, fader) = &rows[index];
            row.set_title(none_if_empty(&source.name));
            fader.set_maximum(source.maximum);
            fader.set_floor(source.floor);
            fader.set_value(source.value);
            fader.set_tooltip_text(Some(&crate::percent_text(crate::percent_of(
                source.value,
                source.maximum,
            ))));
        }

        for (row, fader) in rows.split_off(sources.len()) {
            row.unparent();
            fader.unparent();
        }
    }

    fn build_row(&self, index: u32) -> (Row, Fader) {
        let row = Row::new();
        row.set_activatable(false);

        let fader = Fader::new();
        fader.set_toggleable(false);
        fader.set_icon_name(Some(BRIGHTNESS_ICON));
        fader.connect_changed(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_, value| list.report(index, value)
        ));

        (row, fader)
    }

    fn report(&self, index: u32, value: f64) {
        let key = self
            .imp()
            .sources
            .borrow()
            .get(index as usize)
            .map(|source| source.key.clone());

        if let Some(key) = key {
            self.emit_by_name::<()>(CHANGED, &[&key, &value]);
        }
    }
}
