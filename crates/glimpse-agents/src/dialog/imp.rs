use std::cell::{Cell, RefCell};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use adw::{prelude::*, subclass::prelude::*};
use glimpse_utils::clean;
use gtk4::{CompositeTemplate, TemplateChild, gdk, glib, glib::subclass::Signal};
use zeroize::Zeroizing;

use super::{
    ACCOUNT_MAX, BODY_MAX, DETAIL_MAX, Field, Note, PROMPT_BODY_MAX, Response, TITLE_MAX, View,
};

const ARM_DELAY: Duration = Duration::from_millis(1000);
const GUARD: Duration = Duration::from_millis(400);

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/agents/agent_dialog.ui")]
pub struct AgentDialog {
    #[template_child]
    pub status: TemplateChild<adw::StatusPage>,
    #[template_child]
    pub rows: TemplateChild<gtk4::ListBox>,
    #[template_child]
    pub account: TemplateChild<adw::ComboRow>,
    #[template_child]
    pub accounts: TemplateChild<gtk4::StringList>,
    #[template_child]
    pub username: TemplateChild<adw::EntryRow>,
    #[template_child]
    pub secret: TemplateChild<adw::PasswordEntryRow>,
    #[template_child]
    pub spinner: TemplateChild<adw::Spinner>,
    #[template_child]
    pub autoconnect: TemplateChild<adw::SwitchRow>,
    #[template_child]
    pub details: TemplateChild<adw::ExpanderRow>,
    #[template_child]
    pub code: TemplateChild<gtk4::Label>,
    #[template_child]
    pub note: TemplateChild<gtk4::Label>,
    #[template_child]
    pub cancel: TemplateChild<gtk4::Button>,
    #[template_child]
    pub confirm: TemplateChild<gtk4::Button>,

    shown: RefCell<Option<View>>,
    detail_rows: RefCell<Vec<adw::ActionRow>>,
    confirmable: Cell<bool>,
    armed: Cell<bool>,
    arm_timer: RefCell<Option<glib::SourceId>>,
    guard_until: Cell<Option<Instant>>,
    focus_handler: RefCell<Option<glib::SignalHandlerId>>,
    pub(super) account_hold: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for AgentDialog {
    const NAME: &'static str = "AgentDialog";
    type Type = super::AgentDialog;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(gtk4::AccessibleRole::Dialog);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for AgentDialog {
    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("response")
                    .param_types([bool::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let dialog = self.obj();
        dialog.connect_sensitive_notify(|dialog| {
            if !dialog.is_sensitive() {
                let imp = dialog.imp();
                imp.disarm();
                imp.revalidate();
            }
        });
        self.secret.connect_changed(glib::clone!(
            #[weak]
            dialog,
            move |_| dialog.imp().revalidate()
        ));
        for row in [
            self.username.upcast_ref::<adw::EntryRow>(),
            self.secret.upcast_ref::<adw::EntryRow>(),
        ] {
            row.connect_entry_activated(glib::clone!(
                #[weak]
                dialog,
                move |_| dialog.imp().submit()
            ));
        }
        self.confirm.connect_clicked(glib::clone!(
            #[weak]
            dialog,
            move |_| dialog.imp().submit()
        ));
        self.cancel.connect_clicked(glib::clone!(
            #[weak]
            dialog,
            move |_| dialog.imp().respond(false)
        ));

        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        keys.connect_key_pressed(glib::clone!(
            #[weak]
            dialog,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, _| match key {
                gdk::Key::Escape => {
                    dialog.imp().respond(false);
                    glib::Propagation::Stop
                }
                _ if dialog
                    .imp()
                    .guard_until
                    .get()
                    .is_some_and(|until| Instant::now() < until) =>
                {
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        ));
        dialog.add_controller(keys);
        propagate_content_height(dialog.upcast_ref());
        let imp = dialog.downgrade();
        dialog.connect_map(move |_| {
            if let Some(dialog) = imp.upgrade() {
                dialog.imp().place_focus();
            }
        });
    }

    fn dispose(&self) {
        self.disarm();
        self.dispose_template();
        while let Some(child) = self.obj().first_child() {
            child.unparent();
        }
    }
}

impl WidgetImpl for AgentDialog {
    fn root(&self) {
        self.parent_root();
        let dialog = self.obj();
        if let Some(window) = dialog.root().and_downcast::<gtk4::Window>() {
            self.focus_handler
                .replace(Some(window.connect_is_active_notify(glib::clone!(
                    #[weak]
                    dialog,
                    move |window| dialog.imp().focus_changed(window.is_active())
                ))));
        }
    }

    fn unroot(&self) {
        self.disarm();
        self.revalidate();
        if let Some(handler) = self.focus_handler.take()
            && let Some(window) = self.obj().root().and_downcast::<gtk4::Window>()
        {
            window.disconnect(handler);
        }
        self.parent_unroot();
    }
}

impl AgentDialog {
    pub fn body_label(&self) -> Option<gtk4::Label> {
        let mut pending = vec![self.status.get().upcast::<gtk4::Widget>()];
        while let Some(widget) = pending.pop() {
            if let Some(label) = widget.downcast_ref::<gtk4::Label>()
                && label.has_css_class("body")
            {
                return Some(label.clone());
            }
            let mut child = widget.first_child();
            while let Some(next) = child {
                child = next.next_sibling();
                pending.push(next);
            }
        }
        None
    }

    pub fn show(&self, view: &View) {
        let shown = self.shown.take();
        let before = shown.as_ref();
        if before.is_none() {
            self.guard_until.set(Some(Instant::now() + GUARD));
            self.arm();
        }

        let icon = view.icon.name();
        if self.status.icon_name().as_deref() != Some(icon) {
            self.status.set_icon_name(Some(icon));
        }
        let title = clean(&view.title, TITLE_MAX);
        if self.status.title() != title {
            self.status.set_title(&title);
        }
        let body = clean(&view.body, PROMPT_BODY_MAX);
        let description = (!body.is_empty()).then(|| {
            let accent = adw::StyleManager::default().accent_color_rgba();
            let tint = format!(
                "#{:02x}{:02x}{:02x}",
                (accent.red() * 255.0).round() as u8,
                (accent.green() * 255.0).round() as u8,
                (accent.blue() * 255.0).round() as u8,
            );
            super::description(&body, &tint)
        });
        if self.status.description().as_deref() != description.as_deref() {
            self.status.set_description(description.as_deref());
        }
        if let Some(label) = self.body_label() {
            label.set_wrap(true);
            label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
            label.set_ellipsize(gtk4::pango::EllipsizeMode::None);
            label.set_lines(0);
        }

        if before.map(|shown| &shown.accounts) != Some(&view.accounts) {
            self.account_hold.set(true);
            let accounts: Vec<String> = view
                .accounts
                .iter()
                .map(|account| clean(account, ACCOUNT_MAX))
                .collect();
            let accounts: Vec<&str> = accounts.iter().map(String::as_str).collect();
            self.accounts.splice(0, self.accounts.n_items(), &accounts);
            self.account.set_selected(0);
            self.account_hold.set(false);
        }
        visible(&*self.account, view.accounts.len() > 1);

        if before.map(|shown| &shown.username) != Some(&view.username) {
            self.username.set_text(&clean(
                view.username.as_deref().unwrap_or_default(),
                ACCOUNT_MAX,
            ));
        }
        visible(&*self.username, view.username.is_some());

        let was_busy = before.is_some_and(|shown| shown.busy);
        if before.map(|shown| &shown.secret) != Some(&view.secret) {
            self.secret.set_text("");
            if let Some((title, field)) = &view.secret {
                self.secret.set_title(&clean(title, TITLE_MAX));
                self.set_field(field);
            }
        } else if was_busy && !view.busy {
            self.secret.set_text("");
        }
        visible(&*self.secret, view.secret.is_some());
        visible(&*self.spinner, view.busy);
        if self.secret.is_sensitive() == view.busy {
            self.secret.set_sensitive(!view.busy);
        }
        css_class(
            &*self.secret,
            "error",
            matches!(view.note, Some(Note::Error(_))),
        );

        if before.map(|shown| shown.autoconnect) != Some(view.autoconnect) {
            self.autoconnect
                .set_active(view.autoconnect.unwrap_or_default());
        }
        visible(&*self.autoconnect, view.autoconnect.is_some());

        if before.map(|shown| &shown.details) != Some(&view.details) {
            self.fill_details(&view.details);
        }
        visible(&*self.details, !view.details.is_empty());

        visible(
            &*self.rows,
            view.accounts.len() > 1
                || view.username.is_some()
                || view.secret.is_some()
                || view.autoconnect.is_some()
                || !view.details.is_empty(),
        );

        text(&self.code, view.code.as_deref().unwrap_or_default());
        visible(&*self.code, view.code.is_some());

        let (note, error) = match &view.note {
            Some(Note::Info(note)) => (clean(note, BODY_MAX), false),
            Some(Note::Error(note) | Note::Unavailable(note)) => (clean(note, BODY_MAX), true),
            None => (String::new(), false),
        };
        text(&self.note, &note);
        css_class(&*self.note, "error", error);
        visible(&*self.note, view.note.is_some());

        if let Some(confirm) = &view.confirm
            && self.confirm.label().as_deref() != Some(confirm.as_str())
        {
            self.confirm.set_label(confirm);
        }
        visible(&*self.confirm, view.confirm.is_some());
        if self.cancel.label().as_deref() != Some(view.cancel.as_str()) {
            self.cancel.set_label(&view.cancel);
        }

        let blocked = matches!(view.note, Some(Note::Unavailable(_)));
        self.confirmable
            .set(view.confirm.is_some() && !view.busy && !blocked);
        self.shown.replace(Some(view.clone()));
        self.revalidate();
        self.place_focus();
    }

    fn place_focus(&self) {
        let (secret, confirm) = {
            let shown = self.shown.borrow();
            let Some(view) = shown.as_ref() else {
                return;
            };
            (
                view.secret.is_some() && !view.busy,
                view.secret.is_none() && view.confirm.is_some(),
            )
        };
        if secret && self.secret.is_sensitive() && self.secret.get_visible() {
            self.focus_secret();
            return;
        }
        if confirm && self.confirm.is_sensitive() && self.confirm.get_visible() {
            self.confirm.set_receives_default(true);
            self.confirm.grab_focus();
        }
    }

    fn focus_secret(&self) {
        if let Some(text) = self
            .secret
            .delegate()
            .and_then(|delegate| delegate.downcast::<gtk4::Text>().ok())
            && text.grab_focus()
        {
            return;
        }
        self.secret.grab_focus();
    }

    fn set_field(&self, field: &Field) {
        let purpose = match field {
            Field::Password | Field::WifiKey => gtk4::InputPurpose::Password,
            Field::Visible | Field::Pin => gtk4::InputPurpose::FreeForm,
            Field::Passkey => gtk4::InputPurpose::Digits,
        };
        self.secret.set_input_purpose(purpose);
        if let Some(text) = self
            .secret
            .delegate()
            .and_then(|delegate| delegate.downcast::<gtk4::Text>().ok())
        {
            text.set_visibility(*field == Field::Visible);
        }
    }

    fn fill_details(&self, details: &[(String, String)]) {
        for row in self.detail_rows.take() {
            self.details.remove(&row);
        }
        let rows = details
            .iter()
            .map(|(key, value)| {
                let row = adw::ActionRow::builder()
                    .use_markup(false)
                    .subtitle_selectable(true)
                    .subtitle_lines(0)
                    .css_classes(["property"])
                    .build();
                row.set_title(&clean(key, ACCOUNT_MAX));
                row.set_subtitle(&clean(value, DETAIL_MAX));
                self.details.add_row(&row);
                row
            })
            .collect();
        self.detail_rows.replace(rows);
    }

    pub fn revalidate(&self) {
        let field = self
            .shown
            .borrow()
            .as_ref()
            .and_then(|shown| shown.secret.as_ref().map(|(_, field)| field.clone()));
        let valid = field.is_none_or(|field| field.accepts(&self.secret.text()));
        let sensitive = self.armed.get() && self.confirmable.get() && valid;
        if self.confirm.is_sensitive() != sensitive {
            self.confirm.set_sensitive(sensitive);
        }
    }

    fn disarm(&self) {
        self.armed.set(false);
        if let Some(timer) = self.arm_timer.take() {
            timer.remove();
        }
    }

    fn arm(&self) {
        self.disarm();
        self.revalidate();
        let dialog = self.obj();
        self.arm_timer.replace(Some(glib::timeout_add_local_once(
            ARM_DELAY,
            glib::clone!(
                #[weak]
                dialog,
                move || {
                    let imp = dialog.imp();
                    imp.arm_timer.take();
                    imp.armed.set(true);
                    imp.revalidate();
                    imp.place_focus();
                }
            ),
        )));
    }

    pub(super) fn focus_changed(&self, active: bool) {
        if active && self.obj().is_sensitive() && self.shown.borrow().is_some() {
            self.arm();
        } else {
            self.disarm();
            self.revalidate();
        }
    }

    fn submit(&self) {
        if self.confirm.get_visible() && self.confirm.is_sensitive() {
            self.respond(true);
        }
    }

    fn respond(&self, confirmed: bool) {
        self.obj().emit_by_name::<()>("response", &[&confirmed]);
    }

    pub fn confirmation(&self) -> Response {
        Response::Confirm {
            account: self.account.selected() as usize,
            username: self.username.text().to_string(),
            secret: Zeroizing::new(self.secret.text().to_string()),
            autoconnect: self.autoconnect.is_active(),
        }
    }

    pub fn detail_rows(&self) -> Vec<adw::ActionRow> {
        self.detail_rows.borrow().clone()
    }
}

fn propagate_content_height(widget: &gtk4::Widget) {
    let mut next = widget.first_child();
    while let Some(child) = next {
        if let Some(scrolled) = child.downcast_ref::<gtk4::ScrolledWindow>() {
            scrolled.set_propagate_natural_height(true);
            scrolled.set_vscrollbar_policy(gtk4::PolicyType::Never);
        }
        propagate_content_height(&child);
        next = child.next_sibling();
    }
}

fn visible(widget: &impl IsA<gtk4::Widget>, on: bool) {
    if widget.get_visible() != on {
        widget.set_visible(on);
    }
}

fn text(label: &gtk4::Label, text: &str) {
    if label.text() != text {
        label.set_text(text);
    }
}

fn css_class(widget: &impl IsA<gtk4::Widget>, name: &str, on: bool) {
    if widget.has_css_class(name) != on {
        match on {
            true => widget.add_css_class(name),
            false => widget.remove_css_class(name),
        }
    }
}
