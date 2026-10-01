use super::*;

pub(super) fn check() {
    let notice = Notice::new();
    let notice_icon = child_named::<gtk4::Image>(&notice, "notice__icon");
    let notice_chevron = child_named::<gtk4::Image>(&notice, "notice__chevron");
    assert!(
        !notice.can_target() && !notice_chevron.get_visible(),
        "a notice that only states something takes no click and promises none"
    );
    notice.set_activatable(true);
    assert!(
        notice.can_target() && notice_chevron.get_visible(),
        "and one that leads somewhere shows the chevron that says so"
    );

    notice.set_title(Some("Thunderstorm warning until 21:00"));
    notice.set_icon_name(Some("dialog-warning-symbolic"));
    assert_eq!(notice.severity(), Severity::Info);
    assert!(!notice.has_css_class("notice--warning"));
    notice.set_severity(Severity::Warning);
    assert!(notice.has_css_class("notice--warning") && !notice.has_css_class("notice--error"));
    notice.set_severity(Severity::Error);
    assert!(
        notice.has_css_class("notice--error") && !notice.has_css_class("notice--warning"),
        "severity is one state, not a set of flags that can disagree"
    );
    notice.set_severity(Severity::Info);
    assert!(!notice.has_css_class("notice--error"));

    let notice_changes = Rc::new(Cell::new(0u32));
    notice_icon.connect_icon_name_notify({
        let notice_changes = Rc::clone(&notice_changes);
        move |_| notice_changes.set(notice_changes.get() + 1)
    });
    notice.set_icon_name(Some("dialog-warning-symbolic"));
    assert_eq!(notice_changes.get(), 0, "an equal icon is not reapplied");
}
