use std::cell::RefCell;

use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};

use crate::services::clock::WorldClockTime;

use super::row::WorldClockRow;

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/world_clock.ui")]
pub struct WorldClock {
    #[template_child]
    rows: TemplateChild<gtk4::Box>,
    row_widgets: RefCell<Vec<WorldClockRow>>,
}

#[glib::object_subclass]
impl ObjectSubclass for WorldClock {
    const NAME: &'static str = "WorldClock";
    type Type = super::WorldClock;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for WorldClock {}
impl WidgetImpl for WorldClock {}
impl BoxImpl for WorldClock {}

impl WorldClock {
    pub(super) fn set_rows(&self, rows: &[WorldClockTime]) {
        let mut widgets = self.row_widgets.borrow_mut();

        while widgets.len() < rows.len() {
            let row = WorldClockRow::new();
            self.rows.append(&row);
            widgets.push(row);
        }
        while widgets.len() > rows.len() {
            if let Some(row) = widgets.pop() {
                self.rows.remove(&row);
            }
        }

        for (widget, data) in widgets.iter().zip(rows) {
            widget.set_name(&data.name);
            widget.set_day(data.day_label);
            widget.set_time(&data.time);
            widget.set_offset(&data.offset);
            widget.set_tooltip_text(Some(&data.timezone));
        }

        self.obj().set_visible(!rows.is_empty());
    }
}
