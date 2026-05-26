use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};

use crate::widgets::status_dot::StatusDot;

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/events_row.ui")]
pub struct EventRow {
    #[template_child]
    pub(super) color_dot: TemplateChild<StatusDot>,
    #[template_child]
    pub(super) title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub(super) time: TemplateChild<gtk4::Label>,
}

#[glib::object_subclass]
impl ObjectSubclass for EventRow {
    const NAME: &'static str = "EventRow";
    type Type = super::EventRow;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for EventRow {}
impl WidgetImpl for EventRow {}
impl BoxImpl for EventRow {}

impl EventRow {
    pub(super) fn set_title(&self, text: &str, color: Option<&str>) {
        self.title.set_use_markup(false);
        self.title.set_label(&format_event_title(text));
        self.color_dot.set_visible(self.color_dot.set_color(color));
    }

    pub(super) fn set_time(&self, text: &str) {
        if text.is_empty() {
            self.time.set_visible(false);
        } else {
            self.time.set_label(text);
            self.time.set_visible(true);
        }
    }
}

fn format_event_title(text: &str) -> String {
    let mut chars = text.chars();
    let first: String = chars.by_ref().take(40).collect();
    let rest: String = chars.collect();

    if rest.is_empty() {
        first
    } else {
        format!("{first}\n{rest}")
    }
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::ObjectSubclassIsExt;

    use super::format_event_title;

    #[test]
    fn event_title_under_limit_is_unchanged() {
        assert_eq!(format_event_title("Short planning"), "Short planning");
    }

    #[test]
    fn event_title_over_limit_is_split_after_forty_chars() {
        assert_eq!(
            format_event_title("1234567890123456789012345678901234567890ABCDE"),
            "1234567890123456789012345678901234567890\nABCDE"
        );
    }

    #[test]
    fn event_title_split_uses_character_count_not_byte_count() {
        assert_eq!(
            format_event_title("ą234567890123456789012345678901234567890ABCDE"),
            "ą234567890123456789012345678901234567890\nABCDE"
        );
    }

    #[test]
    fn event_row_uses_status_dot_for_calendar_color() {
        if !crate::utils::test_support::gtk_available_on_this_thread() {
            return;
        }

        let row = super::super::EventRow::new();
        row.set_title("Standup", Some("#4285f4"));

        assert!(row.imp().color_dot.is_visible());
        assert_eq!(row.imp().title.label(), "Standup");
        assert!(!row.imp().title.uses_markup());
    }

    #[test]
    fn event_row_hides_status_dot_without_calendar_color() {
        if !crate::utils::test_support::gtk_available_on_this_thread() {
            return;
        }

        let row = super::super::EventRow::new();
        row.set_title("Standup", None);

        assert!(!row.imp().color_dot.is_visible());
        assert_eq!(row.imp().title.label(), "Standup");
    }
}
