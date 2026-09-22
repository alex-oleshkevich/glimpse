mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Row, none_if_empty, reconcile};

pub use imp::Usage;

glib::wrapper! {
    pub struct PrivacyPopover(ObjectSubclass<imp::PrivacyPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for PrivacyPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl PrivacyPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_usages(&self, usages: &[Usage]) {
        let imp = self.imp();
        if imp.usage_data.borrow().as_slice() == usages {
            return;
        }
        imp.usage_data.replace(usages.to_vec());
        self.render_usages();
    }

    pub fn set_screen_shared(&self, detail: Option<&str>) {
        let imp = self.imp();
        if imp.screen_notice.get_visible() == detail.is_some()
            && imp.screen_notice.subtitle().as_deref() == detail
        {
            return;
        }
        imp.screen_notice.set_visible(detail.is_some());
        imp.screen_notice.set_subtitle(detail);
    }

    fn render_usages(&self) {
        let imp = self.imp();
        #[cfg(test)]
        imp.renders.set(imp.renders.get() + 1);
        let usages = imp.usage_data.borrow();
        reconcile::by_key(
            &*imp.usage_rows,
            &mut imp.usage_held.borrow_mut(),
            &usages,
            |usage| usage.id.clone(),
            |_| Row::new(),
            apply_usage_row,
        );
        imp.usages.set_empty(usages.is_empty());
    }
}

fn apply_usage_row(row: &Row, usage: &Usage) {
    row.set_title(none_if_empty(&usage.title));
    row.set_subtitle(usage.detail.as_deref());
    row.set_lead_icon(none_if_empty(&usage.icon));
    row.set_activatable(false);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(id: &str, title: &str) -> Usage {
        Usage {
            id: id.to_owned(),
            icon: "camera-web-symbolic".into(),
            title: title.to_owned(),
            detail: Some("Chrome \u{b7} since 14:02".into()),
        }
    }

    fn rows(parent: &gtk4::Box) -> Vec<Row> {
        let mut rows = Vec::new();
        let mut child = parent.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Ok(row) = widget.downcast::<Row>() {
                rows.push(row);
            }
        }
        rows
    }

    #[test]
    #[ignore = "needs a display"]
    fn privacy_popover_states() {
        if gtk4::init().is_err() {
            return;
        }
        crate::register_resources().expect("resources");

        let popover = PrivacyPopover::new();
        let imp = popover.imp();

        assert!(
            imp.empty_usages.is_visible(),
            "an untouched popover shows the empty state"
        );
        assert!(!imp.usage_rows.is_visible());
        assert!(!imp.screen_notice.get_visible());

        popover.set_usages(&[usage("camera", "Camera")]);
        assert!(!imp.empty_usages.is_visible());
        assert!(imp.usage_rows.is_visible());
        let camera = rows(&imp.usage_rows);
        assert_eq!(camera.len(), 1);
        assert_eq!(camera[0].title().as_deref(), Some("Camera"));
        assert_eq!(
            camera[0].subtitle().as_deref(),
            Some("Chrome \u{b7} since 14:02")
        );
        assert!(
            !camera[0].activatable(),
            "a usage row reports and is never pressed"
        );

        let renders_after_first = imp.renders.get();
        popover.set_usages(&[usage("camera", "Camera")]);
        assert_eq!(
            imp.renders.get(),
            renders_after_first,
            "an unchanged usage list must never re-enter render_usages at all"
        );
        assert_eq!(
            camera[0],
            rows(&imp.usage_rows)[0],
            "an unchanged usage list reuses its row rather than rebuilding it"
        );

        popover.set_usages(&[Usage {
            detail: Some("Discord \u{b7} since 14:10".into()),
            ..usage("camera", "Camera")
        }]);
        let changed = rows(&imp.usage_rows);
        assert_eq!(
            camera[0], changed[0],
            "an id unchanged across a detail change keeps its row"
        );
        assert_eq!(
            changed[0].subtitle().as_deref(),
            Some("Discord \u{b7} since 14:10"),
            "the reused row still applies the changed detail"
        );

        popover.set_usages(&[Usage {
            detail: None,
            ..usage("location", "Location")
        }]);
        assert_eq!(
            rows(&imp.usage_rows)[0].subtitle(),
            None,
            "a usage with no detail renders with no subtitle at all"
        );

        popover.set_screen_shared(Some("OBS Studio \u{b7} sharing DP-1 since 13:41"));
        assert!(
            imp.screen_notice.get_visible(),
            "Some(detail) shows the notice"
        );
        assert_eq!(
            imp.screen_notice.subtitle().as_deref(),
            Some("OBS Studio \u{b7} sharing DP-1 since 13:41")
        );

        popover.set_screen_shared(None);
        assert!(!imp.screen_notice.get_visible(), "None hides the notice");

        popover.set_usages(&[]);
        assert!(imp.empty_usages.is_visible());
        assert!(imp.usage_rows.first_child().is_none());
    }
}
