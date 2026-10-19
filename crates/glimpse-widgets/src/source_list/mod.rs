mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Row, SliderRow, none_if_empty};

pub(crate) const CHANGED: &str = "changed";
const BRIGHTNESS_ICON: &str = "display-brightness-symbolic";

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Source {
    pub key: String,
    pub name: String,
    pub icon: String,
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
        let sources = imp.sources.borrow();
        let mut rows = imp.rows.borrow_mut();

        for (index, source) in sources.iter().enumerate() {
            if rows.len() == index {
                let (row, slider) = self.build_row(index as u32);
                row.insert_after(self, rows.last().map(|(_, slider)| slider));
                slider.insert_after(self, Some(&row));
                rows.push((row, slider));
            }
            let (row, slider) = &rows[index];
            row.set_title(none_if_empty(&source.name));
            slider.set_lead_icon(none_if_empty(&source.icon).unwrap_or(BRIGHTNESS_ICON));
            slider.set_range(source.floor, source.maximum);
            slider.set_value(source.value);
            slider.set_tooltip_text(Some(&crate::percent_text(crate::percent_of(
                source.value,
                source.maximum,
            ))));
        }

        for (row, slider) in rows.split_off(sources.len()) {
            row.unparent();
            slider.unparent();
        }
    }

    fn build_row(&self, index: u32) -> (Row, SliderRow) {
        let row = Row::new();

        let slider = SliderRow::new();
        slider.connect_changed(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_, value| list.report(index, value)
        ));

        (row, slider)
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
