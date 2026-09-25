mod imp;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

const PROMPT_MAX: usize = 500;
const APP_MAX: usize = 200;

glib::wrapper! {
    pub struct ShotBanner(ObjectSubclass<imp::ShotBanner>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ShotBanner {
    fn default() -> Self {
        Self::new()
    }
}

impl ShotBanner {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_prompt(&self, prompt: &str) {
        let text: String = prompt.chars().take(PROMPT_MAX).collect();
        let imp = self.imp();
        if !imp.portal.get() && imp.body.text().as_str() == text {
            return;
        }
        imp.portal.set(false);
        imp.body.set_text(&text);
        if imp.keys.get_visible() {
            imp.keys.set_visible(false);
        }
    }

    pub fn set_portal_request(&self, app_name: &str) {
        let text = portal_text(app_name);
        let imp = self.imp();
        if imp.portal.get() && imp.body.text().as_str() == text {
            return;
        }
        imp.portal.set(true);
        imp.body.set_text(&text);
        if !imp.keys.get_visible() {
            imp.keys.set_visible(true);
        }
    }
}

fn portal_text(app_name: &str) -> String {
    let cleaned = glimpse_utils::text::clean(app_name, APP_MAX);
    let app = if cleaned.is_empty() {
        gettext("An application")
    } else {
        cleaned
    };
    gettext("“{app}” asked for a screenshot").replace("{app}", &app)
}
