use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{Hero, Placeholder, PopoverShell, Row, Section};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    #[default]
    Networks,
    Known,
    Wired,
    Vpn,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub icon: String,
    pub place: Place,
    pub secured: bool,
    pub selected: bool,
    pub busy: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Line {
    pub action: String,
    pub title: String,
    pub value: String,
    pub toggle: Option<bool>,
    pub activates: bool,
    pub busy: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Entered {
    #[default]
    Secret,
    Passphrase,
    Name,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Ask {
    pub key: String,
    pub network: String,
    pub question: String,
    pub entered: Entered,
    pub accept: String,
    pub choices: Vec<String>,
    pub open_choice: Option<u32>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Details {
    pub id: String,
    pub lines: Vec<Line>,
}

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/network_popover.ui")]
pub struct NetworkPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub wifi: TemplateChild<gtk4::Switch>,
    #[template_child]
    pub networks: TemplateChild<Section>,
    #[template_child]
    pub network_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub more: TemplateChild<Row>,
    #[template_child]
    pub looking: TemplateChild<Placeholder>,
    #[template_child]
    pub known: TemplateChild<Section>,
    #[template_child]
    pub known_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub wired: TemplateChild<Section>,
    #[template_child]
    pub wired_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub vpn: TemplateChild<Section>,
    #[template_child]
    pub vpn_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub hidden: TemplateChild<Row>,
    #[template_child]
    pub pages: TemplateChild<gtk4::Stack>,
    #[template_child]
    pub prompt_network: TemplateChild<gtk4::Label>,
    #[template_child]
    pub prompt_ask: TemplateChild<gtk4::Label>,
    #[template_child]
    pub prompt_security: TemplateChild<gtk4::DropDown>,

    #[template_child]
    pub prompt_secret: TemplateChild<gtk4::PasswordEntry>,
    #[template_child]
    pub prompt_name: TemplateChild<gtk4::Entry>,
    #[template_child]
    pub prompt_cancel: TemplateChild<gtk4::Button>,
    #[template_child]
    pub prompt_accept: TemplateChild<gtk4::Button>,
    #[template_child]
    pub footer: TemplateChild<Row>,

    pub entries: RefCell<Vec<Entry>>,
    pub details: RefCell<Option<Details>>,
    pub network_held: RefCell<Vec<(String, gtk4::Box)>>,
    pub known_held: RefCell<Vec<(String, gtk4::Box)>>,
    pub wired_held: RefCell<Vec<(String, gtk4::Box)>>,
    pub vpn_held: RefCell<Vec<(String, gtk4::Box)>>,
    pub lines: RefCell<Vec<(String, Row)>>,
    pub prompt: RefCell<Option<Ask>>,
    pub quiet: std::cell::Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for NetworkPopover {
    const NAME: &'static str = "NetworkPopover";
    type Type = super::NetworkPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for NetworkPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("wifi-toggled")
                    .param_types([bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder("activated")
                    .param_types([String::static_type(), bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder("selected")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("acted")
                    .param_types([String::static_type(), String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("toggled")
                    .param_types([
                        String::static_type(),
                        String::static_type(),
                        bool::static_type(),
                    ])
                    .build(),
                glib::subclass::Signal::builder("expanded").build(),
                glib::subclass::Signal::builder("hidden-network").build(),
                glib::subclass::Signal::builder("answered")
                    .param_types([bool::static_type(), String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("footer-activated").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let object = self.obj();

        self.wifi.connect_state_set(glib::clone!(
            #[weak]
            object,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, on| {
                if !object.imp().quiet.get() {
                    object.emit_by_name::<()>("wifi-toggled", &[&on]);
                }
                glib::Propagation::Proceed
            }
        ));

        self.more.connect_clicked(glib::clone!(
            #[weak]
            object,
            move |_| object.emit_by_name::<()>("expanded", &[])
        ));

        self.hidden.connect_clicked(glib::clone!(
            #[weak]
            object,
            move |_| object.emit_by_name::<()>("hidden-network", &[])
        ));

        self.prompt_security.connect_selected_notify(glib::clone!(
            #[weak]
            object,
            move |_| object.imp().revalidate()
        ));

        for entry in [
            self.prompt_secret.upcast_ref::<gtk4::Widget>(),
            self.prompt_name.upcast_ref(),
        ] {
            entry.connect_notify_local(
                Some("text"),
                glib::clone!(
                    #[weak]
                    object,
                    move |_, _| object.imp().revalidate()
                ),
            );
        }

        self.prompt_cancel.connect_clicked(glib::clone!(
            #[weak]
            object,
            move |_| object.answer(false)
        ));

        self.prompt_accept.connect_clicked(glib::clone!(
            #[weak]
            object,
            move |_| object.answer(true)
        ));

        self.footer.connect_clicked(glib::clone!(
            #[weak]
            object,
            move |_| object.emit_by_name::<()>("footer-activated", &[])
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl NetworkPopover {
    pub fn typed(&self) -> String {
        match self.prompt.borrow().as_ref().map(|ask| ask.entered) {
            Some(Entered::Name) => self.prompt_name.text().to_string(),
            _ => self.prompt_secret.text().to_string(),
        }
    }

    pub fn open_chosen(&self) -> bool {
        self.prompt
            .borrow()
            .as_ref()
            .and_then(|ask| ask.open_choice)
            .is_some_and(|open| open == self.prompt_security.selected())
    }

    pub fn revalidate(&self) {
        let kind = self
            .prompt
            .borrow()
            .as_ref()
            .map(|ask| ask.entered)
            .unwrap_or_default();
        let open = self.open_chosen();
        self.prompt_secret
            .set_visible(kind != Entered::Name && !open);
        self.prompt_accept
            .set_sensitive(open || accepts(&self.typed(), kind));
    }
}

pub const ENTRY_MAX: usize = 256;
pub const SSID_OCTETS: usize = 32;
pub const PASSPHRASE_MIN: usize = 8;
pub const PASSPHRASE_MAX: usize = 63;
pub const PASSPHRASE_HEX: usize = 64;

/// What NetworkManager will accept, checked before it is asked rather than after it refuses.
/// A WPA key is a passphrase of 8 to 63 characters or the 64 hex digits of the key itself; an SSID
/// is at most 32 octets; everything else — a VPN token, a WEP key — is only bounded.
pub fn accepts(entered: &str, kind: Entered) -> bool {
    let characters = entered.chars().count();
    match kind {
        Entered::Name => !entered.is_empty() && entered.len() <= SSID_OCTETS,
        Entered::Passphrase => {
            (PASSPHRASE_MIN..=PASSPHRASE_MAX).contains(&characters)
                || (characters == PASSPHRASE_HEX
                    && entered.chars().all(|one| one.is_ascii_hexdigit()))
        }
        Entered::Secret => !entered.is_empty() && characters <= ENTRY_MAX,
    }
}

impl WidgetImpl for NetworkPopover {}
