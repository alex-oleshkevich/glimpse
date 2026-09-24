use std::sync::OnceLock;

use gtk4::{AccessibleRole, CompositeTemplate, glib, prelude::*, subclass::prelude::*};

use crate::{ClipboardList, Hero, Notice, PopoverShell, Row, Section};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/clipboard_popover.ui")]
pub struct ClipboardPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub trouble: TemplateChild<Notice>,
    #[template_child]
    pub search: TemplateChild<gtk4::SearchEntry>,
    #[template_child]
    pub pinned_section: TemplateChild<Section>,
    #[template_child]
    pub pinned: TemplateChild<ClipboardList>,
    #[template_child]
    pub recent_section: TemplateChild<Section>,
    #[template_child]
    pub recent: TemplateChild<ClipboardList>,
    #[template_child]
    pub more: TemplateChild<Row>,
    #[template_child]
    pub clear: TemplateChild<Row>,
    #[template_child]
    pub footer: TemplateChild<Row>,
}

#[glib::object_subclass]
impl ObjectSubclass for ClipboardPopover {
    const NAME: &'static str = "ClipboardPopover";
    type Type = super::ClipboardPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        // A Rust GType registers lazily, so a `$Name` nothing has instantiated is an unknown class
        // to the template builder.
        PopoverShell::static_type();
        Hero::static_type();
        Notice::static_type();
        Section::static_type();
        ClipboardList::static_type();
        Row::static_type();
        klass.set_layout_manager_type::<gtk4::BinLayout>();
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for ClipboardPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("restored")
                    .param_types([u64::static_type()])
                    .build(),
                glib::subclass::Signal::builder("pinned")
                    .param_types([u64::static_type(), bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder("removed")
                    .param_types([u64::static_type()])
                    .build(),
                glib::subclass::Signal::builder("acted")
                    .param_types([u64::static_type(), String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("searched")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("more").build(),
                glib::subclass::Signal::builder("cleared").build(),
                glib::subclass::Signal::builder("footer-activated").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        for list in [&*self.pinned, &*self.recent] {
            list.connect_restored(glib::clone!(
                #[weak]
                obj,
                move |_, id| obj.emit_by_name::<()>("restored", &[&id])
            ));
            list.connect_pinned(glib::clone!(
                #[weak]
                obj,
                move |_, id, pinned| obj.emit_by_name::<()>("pinned", &[&id, &pinned])
            ));
            list.connect_removed(glib::clone!(
                #[weak]
                obj,
                move |_, id| obj.emit_by_name::<()>("removed", &[&id])
            ));
            list.connect_acted(glib::clone!(
                #[weak]
                obj,
                move |_, id, key| obj.emit_by_name::<()>("acted", &[&id, &key])
            ));
        }
        self.search.connect_search_changed(glib::clone!(
            #[weak]
            obj,
            move |entry| obj.emit_by_name::<()>("searched", &[&entry.text().to_string()])
        ));
        self.more.connect_clicked(glib::clone!(
            #[weak]
            obj,
            move |_| obj.emit_by_name::<()>("more", &[])
        ));
        self.clear.connect_clicked(glib::clone!(
            #[weak]
            obj,
            move |_| obj.emit_by_name::<()>("cleared", &[])
        ));
        self.footer.connect_clicked(glib::clone!(
            #[weak]
            obj,
            move |_| obj.emit_by_name::<()>("footer-activated", &[])
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for ClipboardPopover {}
