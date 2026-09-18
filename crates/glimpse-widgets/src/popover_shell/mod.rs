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
        self.watch(hero);
        self.settle();
    }

    pub fn clear_hero(&self) {
        clear_children(&self.imp().hero_box);
        self.settle();
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
        self.watch(widget);
        self.settle();
    }

    pub fn clear_footer(&self) {
        clear_children(&self.imp().footer_box);
        self.settle();
    }

    pub fn set_footer_separated(&self, separated: bool) {
        self.imp().footer_rule_suppressed.set(!separated);
        self.settle();
    }

    fn watch(&self, widget: &impl IsA<gtk4::Widget>) {
        widget.as_ref().connect_visible_notify(glib::clone!(
            #[weak(rename_to = shell)]
            self,
            move |_| shell.settle()
        ));
    }

    fn settle(&self) {
        let imp = self.imp();
        let hero_shown = shows_anything(&imp.hero_box);
        imp.hero_box.set_visible(hero_shown);
        imp.hero_rule.set_visible(hero_shown);

        let footer_shown = shows_anything(&imp.footer_box);
        imp.footer_box.set_visible(footer_shown);
        imp.footer_rule
            .set_visible(footer_shown && !imp.footer_rule_suppressed.get());
    }
}

fn shows_anything(container: &gtk4::Box) -> bool {
    std::iter::successors(container.first_child(), |child| child.next_sibling())
        .any(|child| child.get_visible())
}
