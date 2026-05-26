mod imp;

use gtk4::{
    CssProvider, STYLE_PROVIDER_PRIORITY_APPLICATION, glib, prelude::*,
    subclass::prelude::ObjectSubclassIsExt,
};

use super::css_color::sanitize_css_color;

glib::wrapper! {
    pub struct CircleBox(ObjectSubclass<imp::CircleBox>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl CircleBox {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_color(&self, color: &str) {
        #[allow(deprecated)]
        let ctx = self.style_context();
        let imp = self.imp();
        if let Some(provider) = imp.provider.borrow_mut().take() {
            #[allow(deprecated)]
            ctx.remove_provider(&provider);
        }
        let Some(value) = sanitize_css_color(color) else {
            return;
        };
        let provider = CssProvider::new();
        provider.load_from_string(&format!(".circle-box {{ background: {value}; }}"));
        #[allow(deprecated)]
        ctx.add_provider(&provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
        *imp.provider.borrow_mut() = Some(provider);
    }
}

impl Default for CircleBox {
    fn default() -> Self {
        Self::new()
    }
}
