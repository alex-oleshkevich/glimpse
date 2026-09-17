mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Fader, Row, drawer, none_if_empty, reconcile, set_footer_row};

pub use imp::{Block, Details, Entry};

const DEVICE: &str = "audio-popover__device";
const APP: &str = "audio-popover__app";
const BLOCK: &str = "audio-popover__block";
const DETAIL: &str = "detail-card";
const HEADING: &str = "section__title";

glib::wrapper! {
    pub struct AudioPopover(ObjectSubclass<imp::AudioPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for AudioPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_outputs(&self, entries: &[Entry]) {
        let imp = self.imp();
        if imp.outputs_list.borrow().as_slice() == entries {
            return;
        }
        imp.outputs_list.replace(entries.to_vec());
        self.render_devices();
        self.render_details();
    }

    pub fn set_inputs(&self, entries: &[Entry]) {
        let imp = self.imp();
        if imp.inputs_list.borrow().as_slice() == entries {
            return;
        }
        imp.inputs_list.replace(entries.to_vec());
        self.render_devices();
        self.render_details();
    }

    pub fn set_apps(&self, entries: &[Entry]) {
        let imp = self.imp();
        if imp.apps_list.borrow().as_slice() == entries {
            return;
        }
        imp.apps_list.replace(entries.to_vec());
        self.render_apps();
        self.render_details();
    }

    pub fn set_output_level(&self, volume: f64, muted: bool, icon: Option<&str>) {
        let imp = self.imp();
        imp.output.set_value(volume);
        imp.output.set_muted(muted);
        imp.output.set_icon_name(icon);
    }

    pub fn set_input_level(&self, volume: f64, muted: bool, icon: Option<&str>) {
        let imp = self.imp();
        imp.input.set_value(volume);
        imp.input.set_muted(muted);
        imp.input.set_icon_name(icon);
    }

    pub fn set_details(&self, details: Option<&Details>) {
        let imp = self.imp();
        if imp.details.borrow().as_ref() == details {
            return;
        }
        imp.details.replace(details.cloned());
        self.render_details();
    }

    pub fn set_footer(&self, label: Option<&str>) {
        set_footer_row(&self.imp().footer, label);
    }

    pub fn set_readout(&self, value: Option<&str>) {
        let readout = &self.imp().readout;
        readout.set_value(value);
        readout.set_visible(value.is_some());
    }

    pub fn set_overflow(&self, outputs: Option<&str>, inputs: Option<&str>, apps: Option<&str>) {
        let imp = self.imp();
        set_footer_row(&imp.more_outputs, outputs);
        set_footer_row(&imp.more_inputs, inputs);
        set_footer_row(&imp.more_apps, apps);
    }

    pub fn connect_level_changed<F: Fn(&Self, &str, f64) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "level-changed",
            false,
            glib::closure_local!(move |popover: Self, dir: String, value: f64| f(
                &popover, &dir, value
            )),
        )
    }

    pub fn connect_level_moved<F: Fn(&Self, &str, f64) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "level-moved",
            false,
            glib::closure_local!(move |popover: Self, dir: String, value: f64| f(
                &popover, &dir, value
            )),
        )
    }

    pub fn connect_level_toggled<F: Fn(&Self, &str, bool) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "level-toggled",
            false,
            glib::closure_local!(move |popover: Self, dir: String, muted: bool| f(
                &popover, &dir, muted
            )),
        )
    }

    pub fn connect_device_selected<F: Fn(&Self, &str, &str) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "device-selected",
            false,
            glib::closure_local!(move |popover: Self, dir: String, id: String| f(
                &popover, &dir, &id
            )),
        )
    }

    pub fn connect_app_selected<F: Fn(&Self, &str) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "app-selected",
            false,
            glib::closure_local!(move |popover: Self, id: String| f(&popover, &id)),
        )
    }

    pub fn connect_app_level_changed<F: Fn(&Self, &str, &str, f64) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "app-level-changed",
            false,
            glib::closure_local!(
                move |popover: Self, app: String, dir: String, value: f64| f(
                    &popover, &app, &dir, value
                )
            ),
        )
    }

    pub fn connect_app_level_toggled<F: Fn(&Self, &str, &str, bool) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "app-level-toggled",
            false,
            glib::closure_local!(
                move |popover: Self, app: String, dir: String, muted: bool| f(
                    &popover, &app, &dir, muted
                )
            ),
        )
    }

    pub fn connect_app_moved<F: Fn(&Self, &str, &str, &str) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "app-moved",
            false,
            glib::closure_local!(
                move |popover: Self, app: String, dir: String, device: String| f(
                    &popover, &app, &dir, &device
                )
            ),
        )
    }

    pub fn connect_expanded<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "expanded",
            false,
            glib::closure_local!(move |popover: Self, place: String| f(&popover, &place)),
        )
    }

    pub fn connect_footer_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "footer-activated",
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }

    fn render_devices(&self) {
        let imp = self.imp();
        for (dir, list, section, parent, held) in [
            (
                "output",
                &imp.outputs_list,
                &imp.outputs,
                &imp.output_rows,
                &imp.output_held,
            ),
            (
                "input",
                &imp.inputs_list,
                &imp.inputs,
                &imp.input_rows,
                &imp.input_held,
            ),
        ] {
            let entries = list.borrow().clone();
            reconcile::by_key(
                &**parent,
                &mut held.borrow_mut(),
                &entries,
                |entry| entry.id.clone(),
                |entry| self.build_device(dir, &entry.id),
                dress_row,
            );
            section.set_visible(!entries.is_empty());
        }
    }

    fn build_device(&self, dir: &str, id: &str) -> Row {
        let row = Row::new();
        row.add_css_class(DEVICE);
        row.set_selectable(true);
        let dir = dir.to_owned();
        let key = id.to_owned();
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>("device-selected", &[&dir, &key])
        ));
        row
    }

    fn render_apps(&self) {
        let imp = self.imp();
        let entries = imp.apps_list.borrow().clone();
        reconcile::by_key(
            &*imp.app_rows,
            &mut imp.app_held.borrow_mut(),
            &entries,
            |entry| entry.id.clone(),
            |entry| drawer::holder(&self.build_app(&entry.id)),
            |holder, entry| {
                if let Some(row) = drawer::head::<Row>(holder) {
                    dress_row(&row, entry);
                }
            },
        );
        imp.apps.set_empty(entries.is_empty());
    }

    fn build_app(&self, id: &str) -> Row {
        let row = Row::new();
        row.add_css_class(APP);
        row.set_activatable(true);
        let key = id.to_owned();
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>("app-selected", &[&key])
        ));
        row
    }

    fn render_details(&self) {
        let details = self.imp().details.borrow().clone();
        let wanted = details.as_ref().map(|details| details.id.as_str());
        let held = self.imp().app_held.borrow().clone();
        let open = held.into_iter().find(|(id, _)| wanted == Some(id.as_str()));

        for (id, holder) in self.imp().app_held.borrow().iter() {
            if let Some(panel) = drawer::panel(holder) {
                drawer::set(&panel, open.as_ref().is_some_and(|(open, _)| open == id));
            }
        }

        if let (Some(details), Some((_, holder))) = (details.as_ref(), open.as_ref()) {
            self.fill(holder, details);
        }

        self.recede(open.as_ref().map(|(id, _)| id.as_str()));
    }

    fn fill(&self, holder: &gtk4::Box, details: &Details) {
        let Some(panel) = drawer::panel(holder) else {
            return;
        };
        let card = match panel.child().and_downcast::<gtk4::Box>() {
            Some(card) => card,
            None => {
                let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                card.add_css_class(DETAIL);
                panel.set_child(Some(&card));
                card
            }
        };
        let id = details.id.clone();
        reconcile::by_key(
            &card,
            &mut self.imp().blocks.borrow_mut(),
            &details.blocks,
            |block| format!("{id}/{}", block.dir),
            |block| self.build_block(&id, block),
            |widget, block| self.dress_block(widget, &id, block),
        );
    }

    fn build_block(&self, app_id: &str, block: &Block) -> gtk4::Box {
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        container.add_css_class(BLOCK);

        let heading = gtk4::Label::new(None);
        heading.set_xalign(0.0);
        heading.set_visible(false);
        heading.add_css_class(HEADING);
        container.append(&heading);

        let fader = Fader::new();
        container.append(&fader);

        let devices = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        container.append(&devices);

        let app = app_id.to_owned();
        let dir = block.dir.clone();
        fader.connect_changed(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_, value| {
                popover.emit_by_name::<()>("app-level-changed", &[&app, &dir, &value])
            }
        ));

        let app = app_id.to_owned();
        let dir = block.dir.clone();
        fader.connect_toggled(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_, muted| {
                popover.emit_by_name::<()>("app-level-toggled", &[&app, &dir, &muted])
            }
        ));

        container
    }

    fn dress_block(&self, container: &gtk4::Box, app_id: &str, block: &Block) {
        let heading = heading_of(container);
        crate::set_text(&heading, block.heading.as_deref());

        let fader = fader_of(container);
        fader.set_value(block.volume);
        fader.set_muted(block.muted);
        fader.set_icon_name(block.icon.as_deref());
        fader.set_sensitive(block.adjustable);

        let devices = devices_of(container);
        let app = app_id.to_owned();
        let dir = block.dir.clone();
        reconcile::by_key(
            &devices,
            &mut self.imp().block_devices.borrow_mut(),
            &block.devices,
            |entry| format!("{app}/{dir}/{}", entry.id),
            |entry| self.build_block_device(&app, &dir, &entry.id),
            dress_row,
        );
    }

    fn build_block_device(&self, app_id: &str, dir: &str, id: &str) -> Row {
        let row = Row::new();
        row.set_selectable(true);
        let app = app_id.to_owned();
        let dir = dir.to_owned();
        let key = id.to_owned();
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>("app-moved", &[&app, &dir, &key])
        ));
        row
    }

    fn recede(&self, open: Option<&str>) {
        let imp = self.imp();
        for (id, holder) in imp.app_held.borrow().iter() {
            let Some(head) = holder.first_child() else {
                continue;
            };
            crate::set_css_class(&head, drawer::RECEDED, open.is_some_and(|open| open != id));
            crate::set_css_class(&head, drawer::OPEN, open == Some(id.as_str()));
        }

        let outputs = imp.output_held.borrow();
        let inputs = imp.input_held.borrow();
        for (_, row) in outputs.iter().chain(inputs.iter()) {
            crate::set_css_class(row, drawer::RECEDED, open.is_some());
        }
        drop(outputs);
        drop(inputs);

        for widget in [
            imp.output.upcast_ref::<gtk4::Widget>(),
            imp.input.upcast_ref(),
            imp.more_outputs.upcast_ref(),
            imp.more_inputs.upcast_ref(),
            imp.more_apps.upcast_ref(),
            imp.footer.upcast_ref(),
        ] {
            crate::set_css_class(widget, drawer::RECEDED, open.is_some());
        }
        crate::set_css_class(&*imp.hero, drawer::RECEDED, open.is_some());
    }
}

fn heading_of(container: &gtk4::Box) -> gtk4::Label {
    container
        .first_child()
        .and_downcast()
        .expect("a block leads with its heading")
}

fn fader_of(container: &gtk4::Box) -> Fader {
    heading_of(container)
        .next_sibling()
        .and_downcast()
        .expect("a block carries a fader after its heading")
}

fn devices_of(container: &gtk4::Box) -> gtk4::Box {
    fader_of(container)
        .next_sibling()
        .and_downcast()
        .expect("a block carries its device rows after the fader")
}

fn dress_row(row: &Row, entry: &Entry) {
    row.set_title(none_if_empty(&entry.title));
    row.set_lead_icon(entry.icon.as_deref());
    row.set_value(entry.value.as_deref());
    row.set_selected(entry.selected);
}
