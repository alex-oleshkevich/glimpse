mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::clear_children;

glib::wrapper! {
    pub struct PopoverShell(ObjectSubclass<imp::PopoverShell>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for PopoverShell {
    fn default() -> Self {
        Self::new()
    }
}

impl PopoverShell {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_hero(&self, hero: &impl IsA<gtk4::Widget>) {
        let imp = self.imp();
        clear_children(&imp.hero_box);
        imp.hero_box.append(hero);
        self.show_hero(true);
    }

    pub fn clear_hero(&self) {
        clear_children(&self.imp().hero_box);
        self.show_hero(false);
    }

    pub fn set_content(&self, content: &impl IsA<gtk4::Widget>) {
        let content_box = &self.imp().content_box;
        clear_children(content_box);
        content_box.append(content);
    }

    pub fn clear_content(&self) {
        clear_children(&self.imp().content_box);
    }

    pub fn append_to_footer(&self, widget: &impl IsA<gtk4::Widget>) {
        self.imp().footer_box.append(widget);
        widget.as_ref().connect_visible_notify(glib::clone!(
            #[weak(rename_to = shell)]
            self,
            move |_| shell.settle_footer()
        ));
        self.settle_footer();
    }

    pub fn clear_footer(&self) {
        clear_children(&self.imp().footer_box);
        self.settle_footer();
    }

    fn settle_footer(&self) {
        self.show_footer(shows_anything(&self.imp().footer_box));
    }

    fn show_hero(&self, visible: bool) {
        let imp = self.imp();
        imp.hero_box.set_visible(visible);
        imp.hero_rule.set_visible(visible);
    }

    fn show_footer(&self, visible: bool) {
        let imp = self.imp();
        imp.footer_box.set_visible(visible);
        imp.footer_rule.set_visible(visible);
    }
}

fn shows_anything(container: &gtk4::Box) -> bool {
    let mut child = container.first_child();
    while let Some(widget) = child {
        if widget.get_visible() {
            return true;
        }
        child = widget.next_sibling();
    }
    false
}
