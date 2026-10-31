mod imp;

#[cfg(test)]
mod tests;

use gettextrs::gettext;
use gtk4::glib;

const LABEL_MAX_CHARS: usize = 121;
const OVER_CLASS: &str = "osd-view--over";

glib::wrapper! {
    pub struct OsdView(ObjectSubclass<imp::OsdView>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for OsdView {
    fn default() -> Self {
        Self::new()
    }
}

impl OsdView {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn announce(&self) {
        let label = self.label();
        let message = match (!label.is_empty(), self.has_value()) {
            (true, true) => gettext("{label}, {percent} percent")
                .replace("{percent}", &percent(self.value()).to_string())
                .replace("{label}", &label),
            (false, true) => gettext("{percent} percent")
                .replace("{percent}", &percent(self.value()).to_string()),
            (true, false) => label,
            (false, false) => return,
        };
        gtk4::prelude::AccessibleExt::announce(
            self,
            &message,
            gtk4::AccessibleAnnouncementPriority::Medium,
        );
    }
}

fn percent(value: f64) -> i64 {
    (value * 100.0).round() as i64
}
