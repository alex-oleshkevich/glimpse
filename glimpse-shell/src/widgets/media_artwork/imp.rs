use glib::subclass::Signal;
use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::cell::Cell;
use std::sync::OnceLock;

const DEFAULT_ARTWORK_SIZE: i32 = 48;

#[derive(CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/media_artwork.ui")]
pub struct MediaArtwork {
    #[template_child]
    pub picture: TemplateChild<gtk4::Picture>,
    #[template_child]
    pub fallback_icon: TemplateChild<gtk4::Image>,
    pub size: Cell<i32>,
}

impl Default for MediaArtwork {
    fn default() -> Self {
        Self {
            picture: TemplateChild::default(),
            fallback_icon: TemplateChild::default(),
            size: Cell::new(DEFAULT_ARTWORK_SIZE),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for MediaArtwork {
    const NAME: &'static str = "MediaArtwork";
    type Type = super::MediaArtwork;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for MediaArtwork {
    fn constructed(&self) {
        self.parent_constructed();

        // GtkBox installs a GtkBoxLayout by default, and
        // `gtk_widget_measure()` delegates to the layout manager — so a plain
        // `WidgetImpl::measure` override on a Box subclass is silently
        // bypassed. We need our own LayoutManager (defined below) that
        // reports a constant size regardless of the Picture's huge texture.
        self.obj().set_layout_manager(Some(layout::FixedSizeLayout::new()));

        let click = gtk4::GestureClick::new();
        click.set_button(1);
        let obj = self.obj().downgrade();
        click.connect_pressed(move |_, _, _, _| {
            if let Some(artwork) = obj.upgrade() {
                artwork.emit_by_name::<()>("activated", &[]);
            }
        });
        self.obj().add_controller(click);
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| vec![Signal::builder("activated").build()])
    }
}

impl WidgetImpl for MediaArtwork {}
impl BoxImpl for MediaArtwork {}

mod layout {
    use gtk4::{
        Allocation, LayoutManager, Orientation, SizeRequestMode, Widget, glib, prelude::*,
        subclass::prelude::*,
    };

    glib::wrapper! {
        pub struct FixedSizeLayout(ObjectSubclass<imp::FixedSizeLayout>)
            @extends LayoutManager;
    }

    impl FixedSizeLayout {
        pub fn new() -> Self {
            glib::Object::builder().build()
        }
    }

    impl Default for FixedSizeLayout {
        fn default() -> Self {
            Self::new()
        }
    }

    mod imp {
        use super::*;
        use crate::widgets::media_artwork::MediaArtwork;

        #[derive(Default)]
        pub struct FixedSizeLayout;

        #[glib::object_subclass]
        impl ObjectSubclass for FixedSizeLayout {
            const NAME: &'static str = "MediaArtworkFixedSizeLayout";
            type Type = super::FixedSizeLayout;
            type ParentType = LayoutManager;
        }

        impl ObjectImpl for FixedSizeLayout {}

        impl LayoutManagerImpl for FixedSizeLayout {
            fn request_mode(&self, _widget: &Widget) -> SizeRequestMode {
                SizeRequestMode::ConstantSize
            }

            fn measure(
                &self,
                widget: &Widget,
                _orientation: Orientation,
                _for_size: i32,
            ) -> (i32, i32, i32, i32) {
                let size = widget
                    .downcast_ref::<MediaArtwork>()
                    .map(|w| w.imp().size.get().max(1))
                    .unwrap_or(1);
                (size, size, -1, -1)
            }

            fn allocate(&self, widget: &Widget, width: i32, height: i32, baseline: i32) {
                let Some(media) = widget.downcast_ref::<MediaArtwork>() else {
                    return;
                };
                let allocation = Allocation::new(0, 0, width, height);
                if media.imp().picture.is_visible() {
                    media.imp().picture.size_allocate(&allocation, baseline);
                }
                if media.imp().fallback_icon.is_visible() {
                    media.imp().fallback_icon.size_allocate(&allocation, baseline);
                }
            }
        }
    }
}
