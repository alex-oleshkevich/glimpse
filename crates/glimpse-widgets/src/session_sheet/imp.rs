use gtk4::{
    AccessibleRole, CompositeTemplate, StackTransitionType, TemplateChild, glib, prelude::*,
    subclass::prelude::*,
};
use std::sync::OnceLock;

use crate::{Row, SessionActionState};

const MENU_PAGE: &str = "menu";
const SUSPEND_PAGE: &str = "suspend";
const REBOOT_PAGE: &str = "reboot";
const POWER_OFF_PAGE: &str = "power-off";

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/session_sheet.ui")]
pub struct SessionSheet {
    #[template_child]
    pub revealer: TemplateChild<gtk4::Revealer>,
    #[template_child]
    pub stack: TemplateChild<gtk4::Stack>,
    #[template_child]
    pub suspend: TemplateChild<Row>,
    #[template_child]
    pub reboot: TemplateChild<Row>,
    #[template_child]
    pub power_off: TemplateChild<Row>,
    #[template_child]
    pub error: TemplateChild<gtk4::Label>,
    #[template_child]
    pub suspend_cancel: TemplateChild<gtk4::Button>,
    #[template_child]
    pub suspend_confirm: TemplateChild<gtk4::Button>,
    #[template_child]
    pub reboot_cancel: TemplateChild<gtk4::Button>,
    #[template_child]
    pub reboot_confirm: TemplateChild<gtk4::Button>,
    #[template_child]
    pub power_off_cancel: TemplateChild<gtk4::Button>,
    #[template_child]
    pub power_off_confirm: TemplateChild<gtk4::Button>,
}

impl SessionSheet {
    pub(super) fn focus_first_row(&self) {
        for row in [&self.suspend, &self.reboot, &self.power_off] {
            if row.get_visible() && row.is_sensitive() {
                row.grab_focus();
                return;
            }
        }
    }

    pub(super) fn has_actions(&self) -> bool {
        [&self.suspend, &self.reboot, &self.power_off]
            .into_iter()
            .any(|row| row.get_visible())
    }

    pub(super) fn return_to_menu_and_focus(&self) {
        self.stack
            .set_visible_child_full(MENU_PAGE, StackTransitionType::None);
        self.focus_first_row();
    }

    pub(super) fn close_confirm_if_unavailable(&self, action: &str, state: &SessionActionState) {
        let page = match action {
            crate::SUSPEND => SUSPEND_PAGE,
            crate::REBOOT => REBOOT_PAGE,
            crate::POWER_OFF => POWER_OFF_PAGE,
            _ => return,
        };
        let available = state.visible && state.enabled;
        if !available && self.stack.visible_child_name().as_deref() == Some(page) {
            self.stack
                .set_visible_child_full(MENU_PAGE, StackTransitionType::None);
        }
    }

    fn reset(&self) {
        self.stack
            .set_visible_child_full(MENU_PAGE, StackTransitionType::None);
        crate::set_text_capped(&self.error, None, crate::TEXT_MAX_CHARS);
    }
}

#[glib::object_subclass]
impl ObjectSubclass for SessionSheet {
    const NAME: &'static str = "SessionSheet";
    type Type = super::SessionSheet;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        Row::static_type();
        klass.set_layout_manager_type::<gtk4::BinLayout>();
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for SessionSheet {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("action-requested")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("closed").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let sheet = self.obj();

        self.suspend.connect_clicked(glib::clone!(
            #[weak]
            sheet,
            move |row| {
                if !row.is_sensitive() {
                    return;
                }
                let imp = sheet.imp();
                imp.stack.set_visible_child_name(SUSPEND_PAGE);
                imp.suspend_cancel.grab_focus();
            }
        ));
        self.suspend_cancel.connect_clicked(glib::clone!(
            #[weak]
            sheet,
            move |_| {
                let imp = sheet.imp();
                imp.stack.set_visible_child_name(MENU_PAGE);
                imp.suspend.grab_focus();
            }
        ));
        self.suspend_confirm.connect_clicked(glib::clone!(
            #[weak]
            sheet,
            move |_| {
                let imp = sheet.imp();
                if !(imp.suspend.get_visible() && imp.suspend.is_sensitive()) {
                    return;
                }
                sheet.emit_by_name::<()>("action-requested", &[&crate::SUSPEND]);
            }
        ));
        self.reboot.connect_clicked(glib::clone!(
            #[weak]
            sheet,
            move |row| {
                if !row.is_sensitive() {
                    return;
                }
                let imp = sheet.imp();
                imp.stack.set_visible_child_name(REBOOT_PAGE);
                imp.reboot_cancel.grab_focus();
            }
        ));
        self.power_off.connect_clicked(glib::clone!(
            #[weak]
            sheet,
            move |row| {
                if !row.is_sensitive() {
                    return;
                }
                let imp = sheet.imp();
                imp.stack.set_visible_child_name(POWER_OFF_PAGE);
                imp.power_off_cancel.grab_focus();
            }
        ));
        self.reboot_cancel.connect_clicked(glib::clone!(
            #[weak]
            sheet,
            move |_| {
                let imp = sheet.imp();
                imp.stack.set_visible_child_name(MENU_PAGE);
                imp.reboot.grab_focus();
            }
        ));
        self.reboot_confirm.connect_clicked(glib::clone!(
            #[weak]
            sheet,
            move |_| {
                let imp = sheet.imp();
                if !(imp.reboot.get_visible() && imp.reboot.is_sensitive()) {
                    return;
                }
                sheet.emit_by_name::<()>("action-requested", &[&crate::REBOOT]);
            }
        ));
        self.power_off_cancel.connect_clicked(glib::clone!(
            #[weak]
            sheet,
            move |_| {
                let imp = sheet.imp();
                imp.stack.set_visible_child_name(MENU_PAGE);
                imp.power_off.grab_focus();
            }
        ));
        self.power_off_confirm.connect_clicked(glib::clone!(
            #[weak]
            sheet,
            move |_| {
                let imp = sheet.imp();
                if !(imp.power_off.get_visible() && imp.power_off.is_sensitive()) {
                    return;
                }
                sheet.emit_by_name::<()>("action-requested", &[&crate::POWER_OFF]);
            }
        ));

        self.revealer.connect_reveal_child_notify(glib::clone!(
            #[weak]
            sheet,
            move |revealer| {
                let imp = sheet.imp();
                if revealer.reveals_child() {
                    imp.focus_first_row();
                } else {
                    imp.reset();
                    sheet.emit_by_name::<()>("closed", &[]);
                }
            }
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for SessionSheet {}
