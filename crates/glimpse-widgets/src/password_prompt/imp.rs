use gtk4::{CompositeTemplate, TemplateChild, accessible, glib, prelude::*, subclass::prelude::*};
use std::cell::Cell;
use std::sync::OnceLock;

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/password_prompt.ui")]
pub struct PasswordPrompt {
    #[template_child]
    pub user: TemplateChild<gtk4::Label>,
    #[template_child]
    pub entry_row: TemplateChild<gtk4::Overlay>,
    #[template_child]
    pub entry: TemplateChild<gtk4::PasswordEntry>,
    #[template_child]
    pub spinner: TemplateChild<gtk4::Spinner>,
    #[template_child]
    pub message: TemplateChild<gtk4::Label>,
    #[template_child]
    pub caps: TemplateChild<gtk4::Label>,

    pub busy: Cell<bool>,
    pub mirrored: Cell<bool>,
    pub unavailable: Cell<bool>,
    pub shaking: Cell<bool>,
}

impl PasswordPrompt {
    fn refused(&self) -> bool {
        self.busy.get() || self.mirrored.get() || self.unavailable.get()
    }

    pub(super) fn sync_editable(&self) {
        let editable = !self.refused();
        if self.entry.is_editable() != editable {
            self.entry.set_editable(editable);
        }
    }

    pub(super) fn text(&self) -> Option<gtk4::Text> {
        self.entry.delegate().and_downcast::<gtk4::Text>()
    }

    pub(super) fn conceal(&self) {
        if let Some(text) = self.text()
            && text.property::<bool>("visibility")
        {
            text.set_visibility(false);
        }
    }

    pub(super) fn release(&self) {
        self.entry.set_text("");
        self.conceal();
        let Some(root) = self.obj().root() else {
            return;
        };
        if root
            .focus()
            .is_some_and(|focus| focus.is_ancestor(&*self.entry))
        {
            root.set_focus(None::<&gtk4::Widget>);
        }
    }

    fn submit(&self) {
        let empty = self.text().is_none_or(|text| text.text_length() == 0);
        if self.refused() || empty {
            return;
        }
        self.obj().emit_by_name::<()>("submitted", &[]);
    }
}

#[glib::object_subclass]
impl ObjectSubclass for PasswordPrompt {
    const NAME: &'static str = "PasswordPrompt";
    type Type = super::PasswordPrompt;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for PasswordPrompt {
    fn constructed(&self) {
        self.parent_constructed();
        self.caps.set_child_visible(false);
        let message: &gtk4::Accessible = self.message.upcast_ref();
        let user: &gtk4::Accessible = self.user.upcast_ref();
        self.entry.update_relation(&[
            accessible::Relation::DescribedBy(&[message]),
            accessible::Relation::LabelledBy(&[user]),
        ]);

        let prompt = self.obj().downgrade();
        self.entry.connect_activate(move |_| {
            if let Some(prompt) = prompt.upgrade() {
                prompt.imp().submit();
            }
        });
        let prompt = self.obj().downgrade();
        self.entry.connect_changed(move |_| {
            if let Some(prompt) = prompt.upgrade() {
                prompt.emit_by_name::<()>("edited", &[]);
            }
        });
    }

    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("submitted").build(),
                glib::subclass::Signal::builder("edited").build(),
            ]
        })
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for PasswordPrompt {
    fn grab_focus(&self) -> bool {
        self.entry.grab_focus()
    }
}
