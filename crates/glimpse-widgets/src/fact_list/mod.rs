mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::Row;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Fact {
    pub label: String,
    pub value: String,
}

impl Fact {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

glib::wrapper! {
    pub struct FactList(ObjectSubclass<imp::FactList>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for FactList {
    fn default() -> Self {
        Self::new()
    }
}

impl FactList {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_facts(&self, facts: &[Fact]) {
        let imp = self.imp();
        if imp.facts.borrow().as_slice() == facts {
            return;
        }
        imp.facts.replace(facts.to_vec());

        let mut rows = imp.rows.borrow_mut();
        for (index, fact) in facts.iter().enumerate() {
            if rows.len() == index {
                let row = Row::new();
                row.set_activatable(false);
                row.insert_after(self, rows.last());
                rows.push(row);
            }
            rows[index].set_title(Some(fact.label.as_str()));
            rows[index].set_value(Some(fact.value.as_str()));
        }
        for row in rows.split_off(facts.len()) {
            row.unparent();
        }
    }
}
