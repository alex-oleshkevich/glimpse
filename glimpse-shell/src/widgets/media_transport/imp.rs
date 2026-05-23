use glib::subclass::Signal;
use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::sync::OnceLock;

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/media_transport.ui")]
pub struct MediaTransport {
    #[template_child]
    pub previous: TemplateChild<gtk4::Button>,
    #[template_child]
    pub play_pause: TemplateChild<gtk4::Button>,
    #[template_child]
    pub play_icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub next: TemplateChild<gtk4::Button>,
}

#[glib::object_subclass]
impl ObjectSubclass for MediaTransport {
    const NAME: &'static str = "MediaTransport";
    type Type = super::MediaTransport;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for MediaTransport {
    fn constructed(&self) {
        self.parent_constructed();

        let obj = self.obj().downgrade();
        self.previous.connect_clicked({
            let obj = obj.clone();
            move |_| {
                if let Some(transport) = obj.upgrade() {
                    transport.emit_by_name::<()>("previous-clicked", &[]);
                }
            }
        });
        self.play_pause.connect_clicked({
            let obj = obj.clone();
            move |_| {
                if let Some(transport) = obj.upgrade() {
                    transport.emit_by_name::<()>("play-pause-clicked", &[]);
                }
            }
        });
        self.next.connect_clicked(move |_| {
            if let Some(transport) = obj.upgrade() {
                transport.emit_by_name::<()>("next-clicked", &[]);
            }
        });
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("previous-clicked").build(),
                Signal::builder("play-pause-clicked").build(),
                Signal::builder("next-clicked").build(),
            ]
        })
    }
}

impl WidgetImpl for MediaTransport {}
impl BoxImpl for MediaTransport {}
