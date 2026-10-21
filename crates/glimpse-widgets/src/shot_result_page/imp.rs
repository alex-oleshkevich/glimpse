use std::cell::{Cell, RefCell};

use adw::prelude::*;
use gtk4::{CompositeTemplate, TemplateChild, gdk, gio, glib, subclass::prelude::*};

use crate::ShotToolbar;

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/shot_result_page.ui")]
pub struct ShotResultPage {
    #[template_child]
    pub heading: TemplateChild<adw::WindowTitle>,
    #[template_child]
    pub toolbar: TemplateChild<ShotToolbar>,
    #[template_child]
    pub picture: TemplateChild<gtk4::Picture>,
    #[template_child]
    pub menu: TemplateChild<gtk4::MenuButton>,
    #[template_child]
    pub undo: TemplateChild<gtk4::Button>,
    #[template_child]
    pub redo: TemplateChild<gtk4::Button>,
    #[template_child]
    pub zoom: TemplateChild<adw::ToggleGroup>,
    #[template_child]
    pub hand: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub scroller: TemplateChild<gtk4::ScrolledWindow>,
    #[template_child]
    pub toasts: TemplateChild<adw::ToastOverlay>,
    #[template_child]
    pub name: TemplateChild<adw::EntryRow>,
    #[template_child]
    pub collision: TemplateChild<gtk4::Label>,
    #[template_child]
    pub copy: TemplateChild<adw::SplitButton>,
    #[template_child]
    pub new_shot: TemplateChild<gtk4::Button>,

    pub colliding: Cell<bool>,
    pub level: Cell<Option<f64>>,
    pub pointer: Cell<Option<(f64, f64)>>,
    pub committed_name: RefCell<String>,
    pub actions: gio::SimpleActionGroup,
}

#[glib::object_subclass]
impl ObjectSubclass for ShotResultPage {
    const NAME: &'static str = "ShotResultPage";
    type Type = super::ShotResultPage;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        ShotToolbar::static_type();
        klass.set_layout_manager_type::<gtk4::BinLayout>();
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for ShotResultPage {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: std::sync::OnceLock<Vec<glib::subclass::Signal>> =
            std::sync::OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("copy").build(),
                glib::subclass::Signal::builder("save").build(),
                glib::subclass::Signal::builder("open-with").build(),
                glib::subclass::Signal::builder("save-as").build(),
                glib::subclass::Signal::builder("new").build(),
                glib::subclass::Signal::builder("name-changed").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        self.toolbar.set_target_visible(false);
        self.toolbar.set_finish_visible(false);
        self.toolbar.set_history_visible(false);
        let obj = self.obj();
        self.zoom.connect_active_name_notify(glib::clone!(
            #[weak]
            obj,
            move |_| obj.apply_zoom()
        ));
        let motion = gtk4::EventControllerMotion::new();
        motion.connect_motion(glib::clone!(
            #[weak]
            obj,
            move |_, x, y| obj.imp().pointer.set(Some((x, y)))
        ));
        motion.connect_leave(glib::clone!(
            #[weak]
            obj,
            move |_| obj.imp().pointer.set(None)
        ));
        self.scroller.add_controller(motion);
        let wheel = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
        wheel.set_propagation_phase(gtk4::PropagationPhase::Capture);
        wheel.connect_scroll(glib::clone!(
            #[weak]
            obj,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |controller, _, dy| {
                if !controller
                    .current_event_state()
                    .contains(gdk::ModifierType::CONTROL_MASK)
                {
                    return glib::Propagation::Proceed;
                }
                obj.zoom_by(-dy);
                glib::Propagation::Stop
            }
        ));
        self.scroller.add_controller(wheel);
        self.undo.connect_clicked(glib::clone!(
            #[weak]
            obj,
            move |button| emit_history(&obj, button, "undo")
        ));
        self.redo.connect_clicked(glib::clone!(
            #[weak]
            obj,
            move |button| emit_history(&obj, button, "redo")
        ));
        self.copy.connect_clicked(glib::clone!(
            #[weak]
            obj,
            move |_| obj.emit_by_name::<()>("copy", &[])
        ));
        for (name, signal) in [
            ("save", "save"),
            ("save-as", "save-as"),
            ("open-with", "open-with"),
        ] {
            let action = gio::SimpleAction::new(name, None);
            action.connect_activate(glib::clone!(
                #[weak]
                obj,
                move |_, _| obj.emit_by_name::<()>(signal, &[])
            ));
            self.actions.add_action(&action);
        }
        obj.insert_action_group("result", Some(&self.actions));
        self.new_shot.connect_clicked(glib::clone!(
            #[weak]
            obj,
            move |_| obj.emit_by_name::<()>("new", &[])
        ));
        self.name.connect_apply(glib::clone!(
            #[weak]
            obj,
            move |row| obj.apply_name(&row.text())
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
        while let Some(child) = self.obj().first_child() {
            child.unparent();
        }
    }
}

impl WidgetImpl for ShotResultPage {}

fn emit_history(page: &super::ShotResultPage, button: &gtk4::Button, action: &str) {
    if !button.is_sensitive() {
        return;
    }
    page.imp()
        .toolbar
        .emit_by_name::<()>("action", &[&action.to_string()]);
}
