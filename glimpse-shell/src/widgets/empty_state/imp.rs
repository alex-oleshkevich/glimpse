use gtk4::{glib, prelude::*, subclass::prelude::*};

pub struct EmptyState {
    pub(super) title: gtk4::Label,
    pub(super) subtitle: gtk4::Label,
}

impl Default for EmptyState {
    fn default() -> Self {
        Self {
            title: gtk4::Label::new(None),
            subtitle: gtk4::Label::new(None),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for EmptyState {
    const NAME: &'static str = "GlimpseEmptyState";
    type Type = super::EmptyState;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for EmptyState {
    fn constructed(&self) {
        self.parent_constructed();

        let obj = self.obj();
        obj.add_css_class("empty-state");
        obj.set_orientation(gtk4::Orientation::Vertical);
        obj.set_spacing(4);
        obj.set_hexpand(true);
        obj.set_vexpand(true);
        obj.set_halign(gtk4::Align::Fill);
        obj.set_valign(gtk4::Align::Center);

        self.title.add_css_class("empty-state__title");
        self.title.set_hexpand(true);
        self.title.set_halign(gtk4::Align::Fill);
        self.title.set_xalign(0.5);

        self.subtitle.add_css_class("empty-state__subtitle");
        self.subtitle.set_hexpand(true);
        self.subtitle.set_halign(gtk4::Align::Fill);
        self.subtitle.set_xalign(0.5);
        self.subtitle.set_visible(false);

        obj.append(&self.title);
        obj.append(&self.subtitle);
    }
}

impl WidgetImpl for EmptyState {}
impl BoxImpl for EmptyState {}
