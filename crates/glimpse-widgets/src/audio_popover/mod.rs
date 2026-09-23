mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Expandable, Fader, Row, none_if_empty, reconcile, set_footer_row};

pub use imp::{Block, Details, Entry};

const DEVICE: &str = "audio-popover__device";
const APP: &str = "audio-popover__app";
const BLOCK: &str = "audio-popover__block";
const MUTED: &str = "audio-popover__hero--muted";
const MUTED_GLYPH: &str = "audio-popover__muted";
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

    /// The header follows the output: its glyph, the device it plays on, and the warning colour
    /// while it is muted — the same muted glyph and colour every muted fader and app carries.
    pub fn set_heading(&self, icon: &str, device: Option<&str>, muted: bool) {
        let hero = &self.imp().hero;
        hero.set_icon_name(Some(icon));
        hero.set_subtitle(device);
        crate::set_css_class(&**hero, MUTED, muted);
    }

    /// Every listed app's detail, keyed by `Details::id`. A card is built the first time its app
    /// is opened and refreshed from here afterwards; an app whose detail is gone closes.
    pub fn set_details(&self, details: &[Details]) {
        let imp = self.imp();
        if imp.details.borrow().as_slice() == details {
            return;
        }
        imp.details.replace(details.to_vec());
        let held = imp.app_held.borrow().clone();
        for (id, holder) in held {
            if holder.details::<gtk4::Widget>().is_some() {
                self.fill(&id, &holder);
            }
        }
    }

    /// Closes a card once the choice made in it succeeded: `output` and `input` are the device
    /// cards, anything else an app's.
    pub fn collapse(&self, id: &str) {
        let imp = self.imp();
        match id {
            "output" => imp.output_device.set_expanded(false),
            "input" => imp.input_device.set_expanded(false),
            app => {
                if let Some((_, holder)) =
                    imp.app_held.borrow().iter().find(|(held, _)| held == app)
                {
                    holder.set_expanded(false);
                }
            }
        }
    }

    pub fn set_footer(&self, label: Option<&str>) {
        set_footer_row(&self.imp().footer, label);
    }

    pub fn set_more_apps(&self, apps: Option<&str>) {
        set_footer_row(&self.imp().more_apps, apps);
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
        for (list, current) in [
            (&imp.outputs_list, &imp.output_current),
            (&imp.inputs_list, &imp.input_current),
        ] {
            let list = list.borrow();
            let chosen = list.iter().find(|entry| entry.selected).or(list.first());
            current.set_title(chosen.map(|entry| entry.title.as_str()));
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
            |entry| Expandable::new(&self.build_app(&entry.id)),
            |holder, entry| {
                if let Some(row) = holder.head::<Row>() {
                    dress_app(&row, entry);
                }
            },
        );
        imp.apps.set_visible(!entries.is_empty());
        let live: Vec<String> = entries.iter().map(|entry| entry.id.clone()).collect();
        imp.blocks.borrow_mut().retain(|id, _| live.contains(id));
        imp.block_devices
            .borrow_mut()
            .retain(|key, _| live.iter().any(|id| key.starts_with(&format!("{id}/"))));
    }

    /// An app's whole row opens its card; the trail carries the muted glyph beside the chevron,
    /// so a muted app is marked the way a muted fader is.
    fn build_app(&self, id: &str) -> Row {
        let row = Row::new();
        row.add_css_class(APP);
        let trail = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let muted = gtk4::Image::new();
        muted.set_visible(false);
        muted.add_css_class(MUTED_GLYPH);
        trail.append(&muted);
        let chevron = gtk4::Image::from_icon_name("go-next-symbolic");
        chevron.set_accessible_role(gtk4::AccessibleRole::Presentation);
        chevron.add_css_class("drawer-chevron");
        trail.append(&chevron);
        row.set_trail(&trail);
        let key = id.to_owned();
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.open(&key)
        ));
        row
    }

    fn open(&self, id: &str) {
        let holder = self
            .imp()
            .app_held
            .borrow()
            .iter()
            .find(|(held, _)| held == id)
            .map(|(_, holder)| holder.clone());
        if let Some(holder) = holder
            && holder.details::<gtk4::Widget>().is_none()
        {
            self.fill(id, &holder);
        }
    }

    fn fill(&self, id: &str, holder: &Expandable) {
        let imp = self.imp();
        let Some(details) = imp
            .details
            .borrow()
            .iter()
            .find(|details| details.id == id)
            .cloned()
        else {
            holder.set_details(None::<&gtk4::Widget>);
            return;
        };
        let card = holder.details::<gtk4::Box>().unwrap_or_else(|| {
            let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            holder.set_details(Some(&card));
            card
        });
        let mut blocks = imp.blocks.borrow_mut();
        reconcile::by_key(
            &card,
            blocks.entry(id.to_owned()).or_default(),
            &details.blocks,
            |block| block.dir.clone(),
            |block| self.build_block(id, block),
            |widget, block| self.dress_block(widget, id, block),
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

fn dress_app(row: &Row, entry: &Entry) {
    dress_row(row, entry);
    let muted = row
        .trail()
        .and_then(|trail| trail.first_child())
        .and_downcast::<gtk4::Image>();
    if let Some(muted) = muted {
        muted.set_visible(entry.muted_icon.is_some());
        if let Some(icon) = &entry.muted_icon
            && muted.icon_name().as_deref() != Some(icon.as_str())
        {
            muted.set_icon_name(Some(icon));
        }
    }
}

fn dress_row(row: &Row, entry: &Entry) {
    row.set_title(none_if_empty(&entry.title));
    row.set_lead_icon(entry.icon.as_deref());
    row.set_value(entry.value.as_deref());
    row.set_selected(entry.selected);
}
