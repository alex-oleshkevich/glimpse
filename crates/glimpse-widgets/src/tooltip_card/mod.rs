mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::icons_equal;

glib::wrapper! {
    pub struct TooltipCard(ObjectSubclass<imp::TooltipCard>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

pub(crate) const TITLE_MAX_CHARS: usize = 128;
pub(crate) const BODY_MAX_CHARS: usize = 512;
pub(crate) const BODY_MAX_LINES: usize = 6;

impl Default for TooltipCard {
    fn default() -> Self {
        Self::new()
    }
}

impl TooltipCard {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_icon(&self, icon: Option<&gio::Icon>) {
        let imp = self.imp();
        if icons_equal(imp.gicon.borrow().as_ref(), icon) {
            return;
        }
        imp.gicon.replace(icon.cloned());
        match icon {
            Some(icon) => imp.icon.set_from_gicon(icon),
            None => imp.icon.clear(),
        }
        imp.icon.set_visible(icon.is_some());
        self.sync_visible();
    }

    fn sync_visible(&self) {
        let imp = self.imp();
        let any = imp.icon.get_visible()
            || imp.title.get_visible()
            || imp.body.get_visible()
            || imp.status.get_visible();
        imp.text.set_visible(
            imp.title.get_visible() || imp.body.get_visible() || imp.status.get_visible(),
        );
        if self.get_visible() != any {
            self.set_visible(any);
        }
    }
}

pub(crate) fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

/// A tooltip body is another application's string: capped by characters so a multi-byte one cannot
/// be sliced mid-character, and by lines so a sync log cannot grow the tooltip past the screen.
pub(crate) fn clamp_body(value: &str) -> String {
    let capped = truncate(value, BODY_MAX_CHARS);
    capped
        .lines()
        .take(BODY_MAX_LINES)
        .collect::<Vec<&str>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::{BODY_MAX_CHARS, BODY_MAX_LINES, clamp_body, truncate};

    #[test]
    fn a_multibyte_body_is_cut_by_characters_rather_than_bytes() {
        let body = "é".repeat(BODY_MAX_CHARS * 2);
        assert_eq!(clamp_body(&body).chars().count(), BODY_MAX_CHARS);
    }

    #[test]
    fn a_body_of_many_lines_keeps_only_the_first_few() {
        let body = (0..40)
            .map(|line| line.to_string())
            .collect::<Vec<String>>()
            .join("\n");
        assert_eq!(clamp_body(&body).lines().count(), BODY_MAX_LINES);
    }

    #[test]
    fn a_short_body_is_left_alone() {
        assert_eq!(clamp_body("Synced\nJust now"), "Synced\nJust now");
        assert_eq!(truncate("Synced", 128), "Synced");
    }
}
