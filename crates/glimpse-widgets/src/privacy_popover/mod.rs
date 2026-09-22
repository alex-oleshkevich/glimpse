mod imp;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Row, SplitRow, none_if_empty, reconcile};

pub use imp::{Action, Usage};

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

    pub fn connect_muted<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "muted",
            false,
            glib::closure_local!(move |popover: Self, id: String| f(&popover, &id)),
        )
    }

    pub fn connect_stop_requested<F: Fn(&Self, &str) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "stop-requested",
            false,
            glib::closure_local!(move |popover: Self, id: String| f(&popover, &id)),
        )
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
            |usage| (usage.id.clone(), usage.action),
            |_| self.build_usage_cell(),
            |cell, usage| self.apply_usage_cell(cell, usage),
        );
        imp.usages.set_empty(usages.is_empty());
    }

    fn build_usage_cell(&self) -> gtk4::Box {
        gtk4::Box::new(gtk4::Orientation::Vertical, 0)
    }

    fn apply_usage_cell(&self, cell: &gtk4::Box, usage: &Usage) {
        let row = self.ensure_usage_body(cell, usage);
        row.set_title(none_if_empty(&usage.title));
        row.set_subtitle(usage.detail.as_deref());
        row.set_lead_icon(none_if_empty(&usage.icon));
        row.set_activatable(false);
        row.set_busy(usage.busy);
    }

    fn ensure_usage_body(&self, cell: &gtk4::Box, usage: &Usage) -> Row {
        match usage.action {
            Some(action) => {
                if let Some(existing) = cell.first_child().and_downcast::<SplitRow>() {
                    return existing.row();
                }
                if let Some(old) = cell.first_child() {
                    old.unparent();
                }
                let split_row = SplitRow::new();
                let (icon, tooltip, signal) = detail_for(action);
                split_row.set_detail_icon(icon.to_owned());
                split_row.set_detail_tooltip(Some(tooltip));
                let key = usage.id.clone();
                split_row.connect_details(glib::clone!(
                    #[weak(rename_to = popover)]
                    self,
                    move |_| popover.emit_by_name::<()>(signal, &[&key])
                ));
                cell.prepend(&split_row);
                split_row.row()
            }
            None => {
                if let Some(existing) = cell.first_child().and_downcast::<Row>() {
                    return existing;
                }
                if let Some(old) = cell.first_child() {
                    old.unparent();
                }
                let row = Row::new();
                cell.prepend(&row);
                row
            }
        }
    }
}

fn detail_for(action: Action) -> (&'static str, String, &'static str) {
    match action {
        Action::Mute => ("audio-volume-muted-symbolic", gettext("Mute"), "muted"),
        Action::StopSharing => (
            "media-playback-stop-symbolic",
            gettext("Stop sharing"),
            "stop-requested",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn usage(id: &str, title: &str, action: Option<Action>) -> Usage {
        Usage {
            id: id.to_owned(),
            icon: "camera-web-symbolic".into(),
            title: title.to_owned(),
            detail: Some("Chrome · since 14:02".into()),
            action,
            busy: false,
        }
    }

    fn cells(rows: &gtk4::Box) -> Vec<gtk4::Box> {
        let mut cells = Vec::new();
        let mut child = rows.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Ok(cell) = widget.downcast::<gtk4::Box>() {
                cells.push(cell);
            }
        }
        cells
    }

    fn split_of(cell: &gtk4::Box) -> Option<SplitRow> {
        cell.first_child().and_downcast::<SplitRow>()
    }

    fn row_of(cell: &gtk4::Box) -> Row {
        match split_of(cell) {
            Some(split) => split.row(),
            None => cell.first_child().and_downcast::<Row>().expect("a row"),
        }
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

        popover.set_usages(&[usage("camera", "Camera", None)]);
        assert!(!imp.empty_usages.is_visible());
        assert!(imp.usage_rows.is_visible());
        let camera_cells = cells(&imp.usage_rows);
        assert_eq!(camera_cells.len(), 1);
        assert!(
            split_of(&camera_cells[0]).is_none(),
            "no action means a plain $Row, never a $SplitRow"
        );
        let row = row_of(&camera_cells[0]);
        assert_eq!(row.title().as_deref(), Some("Camera"));
        assert_eq!(row.subtitle().as_deref(), Some("Chrome · since 14:02"));
        assert!(!row.activatable());

        let renders_after_first = imp.renders.get();
        popover.set_usages(&[usage("camera", "Camera", None)]);
        assert_eq!(
            imp.renders.get(),
            renders_after_first,
            "an unchanged usage list must never re-enter render_usages at all"
        );
        let same_cells = cells(&imp.usage_rows);
        assert_eq!(
            camera_cells[0], same_cells[0],
            "an unchanged usage list reuses its cell rather than rebuilding it"
        );

        popover.set_usages(&[Usage {
            detail: None,
            ..usage("location", "Location", None)
        }]);
        let location_cells = cells(&imp.usage_rows);
        let plain = row_of(&location_cells[0]);
        assert_eq!(
            plain.subtitle(),
            None,
            "a usage with no detail renders with no subtitle at all"
        );
        assert!(split_of(&location_cells[0]).is_none());

        let muted = Rc::new(RefCell::new(Vec::new()));
        popover.connect_muted({
            let muted = Rc::clone(&muted);
            move |_, id| muted.borrow_mut().push(id.to_owned())
        });
        popover.set_usages(&[usage("mic", "Microphone", Some(Action::Mute))]);
        let mic_cells = cells(&imp.usage_rows);
        let mic_split = split_of(&mic_cells[0]).expect("a mute action becomes a $SplitRow");
        assert_eq!(mic_split.detail_icon(), "audio-volume-muted-symbolic");
        assert_eq!(mic_split.detail_tooltip().as_deref(), Some("Mute"));
        assert!(!mic_split.row().activatable());
        mic_split.detail().emit_by_name::<()>("clicked", &[]);
        assert_eq!(*muted.borrow(), ["mic".to_owned()]);
        assert!(!imp.screen_notice.get_visible());

        let stopped = Rc::new(RefCell::new(Vec::new()));
        popover.connect_stop_requested({
            let stopped = Rc::clone(&stopped);
            move |_, id| stopped.borrow_mut().push(id.to_owned())
        });
        popover.set_usages(&[Usage {
            detail: Some("OBS Studio · sharing DP-1 since 13:41".into()),
            ..usage("screen", "Screen", Some(Action::StopSharing))
        }]);
        assert!(
            !imp.screen_notice.get_visible(),
            "the banner is never inferred from a usage's action, only from set_screen_shared"
        );
        let screen_cells = cells(&imp.usage_rows);
        let screen_split = split_of(&screen_cells[0]).expect("stop-sharing becomes a $SplitRow");
        assert_eq!(screen_split.detail_icon(), "media-playback-stop-symbolic");
        assert_eq!(
            screen_split.detail_tooltip().as_deref(),
            Some("Stop sharing")
        );
        screen_split.detail().emit_by_name::<()>("clicked", &[]);
        assert_eq!(*stopped.borrow(), ["screen".to_owned()]);

        popover.set_screen_shared(Some("OBS Studio · sharing DP-1 since 13:41"));
        assert!(
            imp.screen_notice.get_visible(),
            "Some(detail) shows the notice"
        );
        assert_eq!(
            imp.screen_notice.subtitle().as_deref(),
            Some("OBS Studio · sharing DP-1 since 13:41")
        );

        popover.set_screen_shared(None);
        assert!(!imp.screen_notice.get_visible(), "None hides the notice");

        popover.set_usages(&[Usage {
            action: None,
            ..usage("screen", "Screen", None)
        }]);
        let demoted_cells = cells(&imp.usage_rows);
        assert!(
            split_of(&demoted_cells[0]).is_none(),
            "a usage that loses its action is rebuilt as a plain row, not left as a stale $SplitRow"
        );

        popover.set_usages(&[]);
        assert!(imp.empty_usages.is_visible());
        assert!(imp.usage_rows.first_child().is_none());
    }

    #[test]
    #[ignore = "needs a display"]
    fn privacy_popover_reuses_split_row_on_changed_detail() {
        if gtk4::init().is_err() {
            return;
        }
        crate::register_resources().expect("resources");

        let popover = PrivacyPopover::new();
        let imp = popover.imp();

        let muted = Rc::new(RefCell::new(Vec::new()));
        popover.connect_muted({
            let muted = Rc::clone(&muted);
            move |_, id| muted.borrow_mut().push(id.to_owned())
        });

        popover.set_usages(&[usage("mic", "Microphone", Some(Action::Mute))]);
        let first_cells = cells(&imp.usage_rows);
        let first_split = split_of(&first_cells[0]).expect("a mute action becomes a $SplitRow");

        popover.set_usages(&[Usage {
            detail: Some("Discord · since 14:10".into()),
            ..usage("mic", "Microphone", Some(Action::Mute))
        }]);
        let second_cells = cells(&imp.usage_rows);
        assert_eq!(
            first_cells[0], second_cells[0],
            "an id+action pair unchanged across a detail change keeps its cell"
        );
        let second_split = split_of(&second_cells[0]).expect("still a $SplitRow");
        assert_eq!(
            first_split, second_split,
            "the same SplitRow instance is reused, not rebuilt"
        );
        assert_eq!(
            second_split.row().subtitle().as_deref(),
            Some("Discord · since 14:10"),
            "the reused row still applies the changed detail"
        );

        second_split.detail().emit_by_name::<()>("clicked", &[]);
        assert_eq!(
            *muted.borrow(),
            ["mic".to_owned()],
            "the handler attached on first build fires exactly once, not once per rebuild"
        );
    }

    #[test]
    #[ignore = "needs a display"]
    fn privacy_popover_rebuilds_split_row_on_changed_action() {
        if gtk4::init().is_err() {
            return;
        }
        crate::register_resources().expect("resources");

        let popover = PrivacyPopover::new();
        let imp = popover.imp();

        let muted = Rc::new(RefCell::new(Vec::new()));
        popover.connect_muted({
            let muted = Rc::clone(&muted);
            move |_, id| muted.borrow_mut().push(id.to_owned())
        });
        let stopped = Rc::new(RefCell::new(Vec::new()));
        popover.connect_stop_requested({
            let stopped = Rc::clone(&stopped);
            move |_, id| stopped.borrow_mut().push(id.to_owned())
        });

        popover.set_usages(&[usage("device", "Device", Some(Action::Mute))]);
        let first_cells = cells(&imp.usage_rows);
        let first_split = split_of(&first_cells[0]).expect("a mute action becomes a $SplitRow");
        assert_eq!(first_split.detail_icon(), "audio-volume-muted-symbolic");
        assert_eq!(first_split.detail_tooltip().as_deref(), Some("Mute"));

        popover.set_usages(&[usage("device", "Device", Some(Action::StopSharing))]);
        let second_cells = cells(&imp.usage_rows);
        let second_split =
            split_of(&second_cells[0]).expect("a stop-sharing action is still a $SplitRow");
        assert_eq!(
            second_split.detail_icon(),
            "media-playback-stop-symbolic",
            "a changed action rebuilds the control with the new icon"
        );
        assert_eq!(
            second_split.detail_tooltip().as_deref(),
            Some("Stop sharing"),
            "a changed action rebuilds the control with the new tooltip"
        );

        second_split.detail().emit_by_name::<()>("clicked", &[]);
        assert!(
            muted.borrow().is_empty(),
            "the stale Mute handler must not still be attached"
        );
        assert_eq!(
            *stopped.borrow(),
            ["device".to_owned()],
            "a click after the action changed emits the NEW action's signal"
        );
    }
}
