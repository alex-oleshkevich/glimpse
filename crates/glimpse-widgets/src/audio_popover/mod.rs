mod imp;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{
    ExpanderRow, Row, SliderRow, ToggleControl, ToggleRow, none_if_empty, reconcile,
    set_action_label,
};

pub use imp::{Block, Details, Entry};

const MUTED: &str = "audio-popover__header--muted";

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
    }

    pub fn set_inputs(&self, entries: &[Entry]) {
        let imp = self.imp();
        if imp.inputs_list.borrow().as_slice() == entries {
            return;
        }
        imp.inputs_list.replace(entries.to_vec());
        self.render_devices();
    }

    pub fn set_apps(&self, entries: &[Entry]) {
        let imp = self.imp();
        if imp.apps_list.borrow().as_slice() == entries {
            return;
        }
        imp.apps_list.replace(entries.to_vec());
        self.render_apps();
    }

    pub fn set_output_level(&self, volume: f64, muted: bool, icon: Option<&str>) {
        let imp = self.imp();
        dress_level(&imp.output, &imp.output_mute, volume, muted, icon);
    }

    pub fn set_input_level(&self, volume: f64, muted: bool, icon: Option<&str>) {
        let imp = self.imp();
        dress_level(&imp.input, &imp.input_mute, volume, muted, icon);
    }

    /// The header follows the output: its glyph, the device it plays on, and the warning colour
    /// while it is muted — the same muted glyph and colour every muted fader and app carries.
    pub fn set_heading(&self, icon: &str, device: Option<&str>, muted: bool) {
        let shell = &self.imp().shell;
        shell.set_icon_name(Some(icon));
        shell.set_subtitle(device);
        crate::set_css_class(&**shell, MUTED, muted);
    }

    pub fn set_details(&self, details: &[Details]) {
        let imp = self.imp();
        if imp.details.borrow().as_slice() == details {
            return;
        }
        imp.details.replace(details.to_vec());
        self.fill_all();
    }

    /// Closes a card once the choice made in it succeeded: `output` and `input` are the device
    /// cards, anything else an app's.
    pub fn collapse(&self, id: &str) {
        let imp = self.imp();
        match id {
            "output" => imp.output_device.set_open(false),
            "input" => imp.input_device.set_open(false),
            app => {
                if let Some((_, holder)) =
                    imp.app_held.borrow().iter().find(|(held, _)| held == app)
                {
                    holder.set_open(false);
                }
            }
        }
    }

    pub fn set_more_apps(&self, apps: Option<&str>) {
        set_action_label(&self.imp().more_apps, apps);
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
        for (list, current) in [
            (&imp.outputs_list, &imp.output_device),
            (&imp.inputs_list, &imp.input_device),
        ] {
            let list = list.borrow();
            let chosen = list.iter().find(|entry| entry.selected).or(list.first());
            current.set_title(chosen.map(|entry| entry.title.as_str()));
        }
    }

    fn build_device(&self, dir: &str, id: &str) -> ToggleRow {
        let dir = dir.to_owned();
        let key = id.to_owned();
        choice_row(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move || popover.emit_by_name::<()>("device-selected", &[&dir, &key])
        ))
    }

    fn render_apps(&self) {
        let imp = self.imp();
        let entries = imp.apps_list.borrow().clone();
        reconcile::by_key(
            &*imp.app_rows,
            &mut imp.app_held.borrow_mut(),
            &entries,
            |entry| entry.id.clone(),
            |_| ExpanderRow::new(),
            dress_app,
        );
        imp.apps.set_visible(!entries.is_empty());
        let live: Vec<String> = entries.iter().map(|entry| entry.id.clone()).collect();
        imp.blocks.borrow_mut().retain(|id, _| live.contains(id));
        imp.block_devices
            .borrow_mut()
            .retain(|key, _| live.iter().any(|id| key.starts_with(&format!("{id}/"))));
        self.fill_all();
    }

    fn fill_all(&self) {
        let held = self.imp().app_held.borrow().clone();
        for (id, row) in held {
            self.fill(&id, &row);
        }
    }

    fn fill(&self, id: &str, row: &ExpanderRow) {
        let imp = self.imp();
        let details = imp
            .details
            .borrow()
            .iter()
            .find(|details| details.id == id)
            .cloned();
        row.set_openable(details.is_some());
        let blocks = details.map(|details| details.blocks).unwrap_or_default();
        let mut held = imp.blocks.borrow_mut();
        reconcile::by_key(
            &row.details(),
            held.entry(id.to_owned()).or_default(),
            &blocks,
            |block| block.dir.clone(),
            |block| self.build_block(id, block),
            |widget, block| self.dress_block(widget, id, block),
        );
    }

    fn build_block(&self, app_id: &str, block: &Block) -> gtk4::Box {
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

        let heading = gtk4::Label::new(None);
        heading.set_xalign(0.0);
        heading.set_visible(false);
        heading.add_css_class("heading");
        container.append(&heading);

        let slider = SliderRow::new();
        slider.set_range(0.0, 100.0);
        let mute = gtk4::Button::new();
        mute.set_valign(gtk4::Align::Center);
        mute.add_css_class("flat");
        slider.set_start(&mute);
        container.append(&slider);

        let devices = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        container.append(&devices);

        let app = app_id.to_owned();
        let dir = block.dir.clone();
        slider.connect_changed(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_, value| {
                popover.emit_by_name::<()>("app-level-changed", &[&app, &dir, &value])
            }
        ));

        let app = app_id.to_owned();
        let dir = block.dir.clone();
        mute.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            #[weak]
            slider,
            move |_| {
                let muted = !slider.inactive();
                popover.emit_by_name::<()>("app-level-toggled", &[&app, &dir, &muted])
            }
        ));

        container
    }

    fn dress_block(&self, container: &gtk4::Box, app_id: &str, block: &Block) {
        let heading = heading_of(container);
        crate::set_text(&heading, block.heading.as_deref());

        let slider = slider_of(container);
        let mute = mute_of(&slider);
        dress_level(
            &slider,
            &mute,
            block.volume,
            block.muted,
            block.icon.as_deref(),
        );
        if !block.adjustable {
            slider.set_inactive(true);
            slider.set_subtitle(None::<String>);
        }
        mute.set_visible(block.adjustable);

        let devices = devices_of(container);
        let app = app_id.to_owned();
        let dir = block.dir.clone();
        let mut held = self.imp().block_devices.borrow_mut();
        reconcile::by_key(
            &devices,
            held.entry(format!("{app}/{dir}")).or_default(),
            &block.devices,
            |entry| entry.id.clone(),
            |entry| self.build_block_device(&app, &dir, &entry.id),
            dress_row,
        );
    }

    fn build_block_device(&self, app_id: &str, dir: &str, id: &str) -> ToggleRow {
        let app = app_id.to_owned();
        let dir = dir.to_owned();
        let key = id.to_owned();
        choice_row(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move || popover.emit_by_name::<()>("app-moved", &[&app, &dir, &key])
        ))
    }
}

fn choice_row(chosen: impl Fn() + 'static) -> ToggleRow {
    let row: ToggleRow = glib::Object::builder()
        .property("control", ToggleControl::Check)
        .build();
    row.connect_toggled(move |row, active| match active {
        true => chosen(),
        false => row.set_active(true),
    });
    row
}

fn dress_level(
    slider: &SliderRow,
    mute: &gtk4::Button,
    volume: f64,
    muted: bool,
    icon: Option<&str>,
) {
    slider.set_value(volume);
    slider.set_inactive(muted);
    mute.set_icon_name(icon.unwrap_or_default());
    mute.set_tooltip_text(Some(&match muted {
        true => gettext("Unmute"),
        false => gettext("Mute"),
    }));
}

fn heading_of(container: &gtk4::Box) -> gtk4::Label {
    container
        .first_child()
        .and_downcast()
        .expect("a block leads with its heading")
}

fn slider_of(container: &gtk4::Box) -> SliderRow {
    heading_of(container)
        .next_sibling()
        .and_downcast()
        .expect("a block carries a slider after its heading")
}

fn mute_of(slider: &SliderRow) -> gtk4::Button {
    slider
        .imp()
        .start
        .first_child()
        .and_downcast()
        .expect("a block's slider carries its mute button")
}

fn devices_of(container: &gtk4::Box) -> gtk4::Box {
    slider_of(container)
        .next_sibling()
        .and_downcast()
        .expect("a block carries its device rows after the slider")
}

fn dress_app(row: &ExpanderRow, entry: &Entry) {
    row.set_title(none_if_empty(&entry.title));
    row.set_icon_name(entry.icon.as_deref());
    row.set_value(entry.value.as_deref());
    match &entry.muted_icon {
        Some(icon) => {
            let glyph = gtk4::Image::from_icon_name(icon);
            glyph.add_css_class("warning");
            row.set_end(&glyph);
        }
        None => row.clear_end(),
    }
}

fn dress_row(row: &ToggleRow, entry: &Entry) {
    let row_ref = row.upcast_ref::<Row>();
    row_ref.set_title(none_if_empty(&entry.title));
    row_ref.set_icon_name(entry.icon.as_deref());
    row_ref.set_value(entry.value.as_deref());
    row.set_active(entry.selected);
}
