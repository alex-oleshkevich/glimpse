use std::cell::{Cell, RefCell};
use std::marker::PhantomData;

use glib::Properties;
use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::PopoverSize;

#[derive(Debug, Default, CompositeTemplate, Properties)]
#[properties(wrapper_type = super::PopoverShell)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/popover_shell.ui")]
pub struct PopoverShell {
    #[template_child]
    pub card: TemplateChild<crate::AppletPopover>,
    #[template_child]
    pub header: TemplateChild<crate::Hero>,
    #[template_child]
    pub hero_box: TemplateChild<gtk4::Box>,
    #[template_child]
    pub hero_rule: TemplateChild<gtk4::Separator>,
    #[template_child]
    pub scroller: TemplateChild<gtk4::ScrolledWindow>,
    #[template_child]
    pub body: TemplateChild<gtk4::Box>,
    #[template_child]
    pub footer_rule: TemplateChild<gtk4::Separator>,
    #[template_child]
    pub footer_box: TemplateChild<gtk4::Box>,
    entered: RefCell<Option<(gtk4::gdk::Surface, glib::SignalHandlerId)>>,
    body_children: RefCell<Option<gtk4::gio::ListModel>>,

    #[property(name = "icon-name", get = Self::icon_name, set = Self::set_icon_name, nullable)]
    icon_name: PhantomData<Option<String>>,
    #[property(name = "title", get = Self::title, set = Self::set_title, nullable)]
    title: PhantomData<Option<String>>,
    #[property(name = "subtitle", get = Self::subtitle, set = Self::set_subtitle, nullable)]
    subtitle: PhantomData<Option<String>>,
    #[property(get, set = Self::set_size, builder(PopoverSize::Regular).explicit_notify())]
    size: Cell<PopoverSize>,
}

impl PopoverShell {
    fn icon_name(&self) -> Option<String> {
        self.header.icon_name().map(|name| name.to_string())
    }

    fn set_icon_name(&self, icon_name: Option<String>) {
        if self.header.icon_name().as_deref() == icon_name.as_deref() {
            return;
        }
        self.header.set_icon_name(icon_name.as_deref());
        self.obj().settle();
    }

    fn title(&self) -> Option<String> {
        self.header.title().map(|title| title.to_string())
    }

    fn set_title(&self, title: Option<String>) {
        let changed = self.header.title().as_deref() != title.as_deref();
        if changed {
            self.header.set_title(title.as_deref());
        }
        let visible = title.is_some();
        let visibility_changed = self.header.get_visible() != visible;
        if visibility_changed {
            self.header.set_visible(visible);
        }
        if changed || visibility_changed {
            self.obj().settle();
        }
    }

    fn subtitle(&self) -> Option<String> {
        self.header.subtitle().map(|subtitle| subtitle.to_string())
    }

    fn set_subtitle(&self, subtitle: Option<String>) {
        if self.header.subtitle().as_deref() == subtitle.as_deref() {
            return;
        }
        self.header.set_subtitle(subtitle.as_deref());
        self.obj().settle();
    }

    fn set_size(&self, size: PopoverSize) {
        if self.size.get() == size {
            return;
        }
        self.size.set(size);
        if let Some(card) = self.card.try_get() {
            card.set_size(size);
        }
        self.obj().notify_size();
    }
}

#[glib::object_subclass]
impl ObjectSubclass for PopoverShell {
    const NAME: &'static str = "PopoverShell";
    type Type = super::PopoverShell;
    type ParentType = gtk4::Widget;
    type Interfaces = (gtk4::Buildable,);

    fn class_init(klass: &mut Self::Class) {
        crate::Hero::static_type();
        crate::AppletPopover::static_type();
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for PopoverShell {
    fn constructed(&self) {
        self.parent_constructed();
        let shell = self.obj();
        let press = gtk4::GestureClick::new();
        press.set_button(0);
        press.set_propagation_phase(gtk4::PropagationPhase::Capture);
        press.connect_pressed(glib::clone!(
            #[weak]
            shell,
            move |press, _, x, y| {
                if shell.dismiss(x, y) {
                    press.set_state(gtk4::EventSequenceState::Claimed);
                }
            }
        ));
        shell.add_controller(press);
        let size = self.size.get();
        if self.card.size() != size {
            self.card.set_size(size);
        }
        let shell = self.obj().downgrade();
        let children = self.body.observe_children();
        children.connect_items_changed(move |_, _, _, _| {
            if let Some(shell) = shell.upgrade() {
                shell.settle();
            }
        });
        self.body_children.replace(Some(children));
        self.obj().settle();
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for PopoverShell {
    fn map(&self) {
        self.parent_map();
        let obj = self.obj();
        obj.apply_cap(None);
        if let Some(surface) = obj.native().and_then(|native| native.surface()) {
            let handler = surface.connect_enter_monitor(glib::clone!(
                #[weak]
                obj,
                move |_, monitor| obj.apply_cap(Some(monitor))
            ));
            if let Some((old, handler)) = self.entered.replace(Some((surface, handler))) {
                old.disconnect(handler);
            }
        }
    }

    fn unmap(&self) {
        if let Some((surface, handler)) = self.entered.take() {
            surface.disconnect(handler);
        }
        self.parent_unmap();
    }
}

impl BuildableImpl for PopoverShell {
    fn add_child(&self, builder: &gtk4::Builder, child: &glib::Object, kind: Option<&str>) {
        let own_template = self.body.try_get().is_none();
        let shell = self.obj();
        match (kind, child.downcast_ref::<gtk4::Widget>()) {
            _ if own_template => self.parent_add_child(builder, child, kind),
            (Some("hero"), Some(widget)) => shell.set_hero(widget),
            (Some("footer"), Some(widget)) => shell.append_to_footer(widget),
            (Some("addon"), Some(widget)) => shell.set_addon(Some(widget)),
            (None, Some(widget)) => {
                shell.imp().body.append(widget);
                shell.watch(widget);
                shell.settle();
            }
            _ => self.parent_add_child(builder, child, kind),
        }
    }
}
