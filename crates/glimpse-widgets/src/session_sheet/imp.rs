use gtk4::{
    AccessibleRole, CompositeTemplate, StackTransitionType, TemplateChild, glib, prelude::*,
    subclass::prelude::*,
};
use std::sync::OnceLock;

use crate::{ActionRow, SessionActionState};

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
    pub suspend: TemplateChild<ActionRow>,
    #[template_child]
    pub reboot: TemplateChild<ActionRow>,
    #[template_child]
    pub power_off: TemplateChild<ActionRow>,
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
        ActionRow::static_type();
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

        for (row, page, cancel, confirm, action) in [
            (
                self.suspend.get(),
                SUSPEND_PAGE,
                self.suspend_cancel.get(),
                self.suspend_confirm.get(),
                crate::SUSPEND,
            ),
            (
                self.reboot.get(),
                REBOOT_PAGE,
                self.reboot_cancel.get(),
                self.reboot_confirm.get(),
                crate::REBOOT,
            ),
            (
                self.power_off.get(),
                POWER_OFF_PAGE,
                self.power_off_cancel.get(),
                self.power_off_confirm.get(),
                crate::POWER_OFF,
            ),
        ] {
            row.connect_activated(glib::clone!(
                #[weak]
                sheet,
                #[strong]
                cancel,
                move |row| {
                    if !row.is_sensitive() {
                        return;
                    }
                    sheet.imp().stack.set_visible_child_name(page);
                    cancel.grab_focus();
                }
            ));
            cancel.connect_clicked(glib::clone!(
                #[weak]
                sheet,
                #[strong]
                row,
                move |_| {
                    sheet.imp().stack.set_visible_child_name(MENU_PAGE);
                    row.grab_focus();
                }
            ));
            confirm.connect_clicked(glib::clone!(
                #[weak]
                sheet,
                #[strong]
                row,
                move |_| {
                    if !(row.get_visible() && row.is_sensitive()) {
                        return;
                    }
                    sheet.emit_by_name::<()>("action-requested", &[&action]);
                }
            ));
        }

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
