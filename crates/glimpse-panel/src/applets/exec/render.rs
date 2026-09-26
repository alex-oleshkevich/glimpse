use crate::app::{AppInput, SessionDialog};
use crate::applet::{Button, Direction, Pointer};
use glimpse_services::Placement;
use glimpse_services::{
    Align, ClassName, Element, Ellipsize, ExecBarRequest as BarRequest, IndicatorProps,
    SessionVerb, Tree,
};
use glimpse_services::{SessionAction, SessionActionsState};
use glimpse_widgets::{IndicatorSpec, Severity};
use gtk4::prelude::*;
use gtk4::{gdk, gio};
use serde_json::{Number, Value};
use std::collections::HashMap;
use std::sync::Arc;

pub fn indicators(
    tree: &Tree,
    icons: &mut impl FnMut(&str) -> gio::Icon,
) -> Vec<(u32, IndicatorSpec)> {
    tree.children(0)
        .iter()
        .filter_map(|id| {
            let Element::Indicator(props) = &tree.node(*id)?.element else {
                return None;
            };
            Some((*id, indicator(props, icons)))
        })
        .collect()
}

pub fn first_child_of_kind(
    tree: &Tree,
    parent: u32,
    pred: impl Fn(&Element) -> bool,
) -> Option<u32> {
    tree.children(parent)
        .iter()
        .copied()
        .find(|id| tree.node(*id).is_some_and(|node| pred(&node.element)))
}

pub fn cached_icon(icons: &mut HashMap<String, gio::Icon>, name: &str) -> gio::Icon {
    if icons.len() > 64 {
        icons.clear();
    }
    icons
        .entry(name.to_owned())
        .or_insert_with(|| gio::ThemedIcon::new(name).upcast())
        .clone()
}

pub fn place_placement(current: &mut Placement, next: Placement) -> bool {
    if *current == next {
        false
    } else {
        *current = next;
        true
    }
}

fn indicator(props: &IndicatorProps, icons: &mut impl FnMut(&str) -> gio::Icon) -> IndicatorSpec {
    IndicatorSpec {
        icon: props
            .icon
            .as_deref()
            .filter(|s| valid_icon(s))
            .map(&mut *icons),
        overlay: props
            .overlay
            .as_deref()
            .filter(|s| valid_icon(s))
            .map(icons),
        dot: props.dot.as_deref().and_then(|s| gdk::RGBA::parse(s).ok()),
        extension: None,
        label: props.text.clone(),
        tooltip: props.tooltip.clone(),
        badge: props.badge.clone(),
        attention: props.attention,
        notice: props.notice,
        severity: props.severity.map(|s| match s {
            glimpse_services::Severity::Info => Severity::Info,
            glimpse_services::Severity::Warning => Severity::Warning,
            glimpse_services::Severity::Error => Severity::Error,
        }),
        class: props.class_name.map(|s| class_name(s).to_owned()),
    }
}

pub fn class_name(class: ClassName) -> &'static str {
    match class {
        ClassName::DimLabel => "dim-label",
        ClassName::Caption => "caption",
        ClassName::Heading => "heading",
        ClassName::Title1 => "title-1",
        ClassName::Title2 => "title-2",
        ClassName::Title3 => "title-3",
        ClassName::Title4 => "title-4",
        ClassName::Numeric => "numeric",
        ClassName::Accent => "accent",
        ClassName::Success => "success",
        ClassName::Warning => "warning",
        ClassName::Error => "error",
        ClassName::Flat => "flat",
        ClassName::Pill => "pill",
        ClassName::Circular => "circular",
    }
}

pub fn valid_icon(icon: &str) -> bool {
    !icon.is_empty()
        && icon
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

pub fn keeps_newer_local_value(local: Option<u64>, incoming: Option<u64>) -> bool {
    matches!((local, incoming), (Some(local), Some(incoming)) if incoming < local)
}

pub fn number(value: f64) -> Option<Value> {
    Number::from_f64(value).map(Value::Number)
}

pub fn scale_value(value: f64, min: f64, max: f64) -> f64 {
    value.clamp(min, max)
}

pub fn scale_step(min: f64, max: f64, step: f64) -> f64 {
    if step > 0.0 {
        step
    } else {
        (max - min) / 100.0
    }
}

pub fn align(value: Align) -> gtk4::Align {
    match value {
        Align::Fill => gtk4::Align::Fill,
        Align::Start => gtk4::Align::Start,
        Align::End => gtk4::Align::End,
        Align::Center => gtk4::Align::Center,
        Align::Baseline => gtk4::Align::Baseline,
    }
}

pub fn ellipsize(value: Option<Ellipsize>) -> gtk4::pango::EllipsizeMode {
    match value.unwrap_or(Ellipsize::End) {
        Ellipsize::None => gtk4::pango::EllipsizeMode::None,
        Ellipsize::Start => gtk4::pango::EllipsizeMode::Start,
        Ellipsize::Middle => gtk4::pango::EllipsizeMode::Middle,
        Ellipsize::End => gtk4::pango::EllipsizeMode::End,
    }
}

pub fn action(request: &BarRequest) -> Option<SessionAction> {
    let BarRequest::Session(verb) = request else {
        return None;
    };
    Some(match verb {
        SessionVerb::Lock => SessionAction::Lock,
        SessionVerb::Suspend => SessionAction::Suspend,
        SessionVerb::Hibernate => SessionAction::Hibernate,
        SessionVerb::LogOut => SessionAction::LogOut,
        SessionVerb::Reboot => SessionAction::Reboot,
        SessionVerb::PowerOff => SessionAction::PowerOff,
    })
}

pub fn session_input(request: &BarRequest, state: &SessionActionsState) -> Option<AppInput> {
    let action = action(request)?;
    Some(
        match crate::applets::session::render::confirm(state, action.clone()) {
            Some((title, body, accept)) => AppInput::SessionConfirm(SessionDialog {
                title,
                body,
                accept,
                action,
            }),
            None => AppInput::SessionRun(action),
        },
    )
}

pub fn requests_before_tree_check(
    last: &mut u64,
    requests: &[(u64, BarRequest)],
    current: &Arc<Tree>,
    next: &Arc<Tree>,
) -> (Vec<BarRequest>, bool) {
    let mut fresh = Vec::new();
    for (serial, request) in requests {
        if *serial > *last {
            *last = *serial;
            fresh.push(request.clone());
        }
    }
    (fresh, Arc::ptr_eq(current, next))
}

pub fn pointer(pointer: Pointer) -> Option<(&'static str, Vec<Value>, bool)> {
    match pointer {
        Pointer::Press(Button::Left) => None,
        Pointer::Press(button) => Some((
            "onPress",
            vec![Value::from(match button {
                Button::Middle => 2,
                Button::Right => 3,
                Button::Other(code) => code,
                Button::Left => 1,
            })],
            true,
        )),
        Pointer::Scroll(direction) => {
            let (dx, dy) = match direction {
                Direction::Up => (0, -1),
                Direction::Down => (0, 1),
                Direction::Left => (-1, 0),
                Direction::Right => (1, 0),
            };
            Some(("onScroll", vec![Value::from(dx), Value::from(dy)], false))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_services::{Op, WireNode};
    use serde_json::json;

    #[test]
    fn keeps_newer_local_value_until_acknowledged() {
        assert!(keeps_newer_local_value(Some(5), Some(4)));
        assert!(!keeps_newer_local_value(Some(5), Some(5)));
        assert!(!keeps_newer_local_value(Some(5), None));
    }

    #[test]
    fn icon_cache_clears_after_64_entries() {
        let mut icons = HashMap::new();
        for index in 0..65 {
            cached_icon(&mut icons, &format!("icon-{index}"));
        }
        assert_eq!(icons.len(), 65);
        cached_icon(&mut icons, "next");
        assert_eq!(icons.len(), 1);
    }

    #[test]
    fn orientation_and_position_change_send_one_place() {
        let mut current = super::super::placement(
            glimpse_config::Position::Top,
            32,
            None,
            crate::components::panel::Zone::Start,
        );
        let next = super::super::placement(
            glimpse_config::Position::Left,
            32,
            None,
            crate::components::panel::Zone::Start,
        );
        let mut sends = 0;
        if place_placement(&mut current, next.clone()) {
            sends += 1;
        }
        if place_placement(&mut current, next) {
            sends += 1;
        }
        assert_eq!(sends, 1);
    }

    #[test]
    fn number_rejects_non_finite_values() {
        assert!(number(f64::NAN).is_none());
        assert!(number(f64::INFINITY).is_none());
        assert_eq!(number(2.5), Some(json!(2.5)));
    }

    #[test]
    fn a_continuous_scale_gets_a_usable_gtk_step() {
        assert_eq!(scale_step(0.0, 100.0, 0.0), 1.0);
        assert_eq!(scale_step(0.0, 100.0, 0.5), 0.5);
    }

    #[test]
    fn session_requests_map_to_dialog_or_direct_action() {
        let state = SessionActionsState::default();
        assert!(matches!(
            session_input(&BarRequest::Session(SessionVerb::Reboot), &state),
            Some(AppInput::SessionConfirm(SessionDialog {
                action: SessionAction::Reboot,
                ..
            }))
        ));
        assert!(matches!(
            session_input(&BarRequest::Session(SessionVerb::Lock), &state),
            Some(AppInput::SessionRun(SessionAction::Lock))
        ));
    }

    #[test]
    fn requests_survive_an_unchanged_tree() {
        let tree = Arc::new(Tree::default());
        let mut serial = 0;
        let (requests, unchanged) =
            requests_before_tree_check(&mut serial, &[(1, BarRequest::ClosePopover)], &tree, &tree);
        assert!(unchanged);
        assert_eq!(requests, vec![BarRequest::ClosePopover]);
        assert_eq!(serial, 1);
    }

    #[test]
    fn pointer_events_keep_the_wire_names_and_arguments() {
        assert_eq!(pointer(Pointer::Press(Button::Left)), None);
        assert_eq!(
            pointer(Pointer::Press(Button::Right)),
            Some(("onPress", vec![json!(3)], true))
        );
        assert_eq!(
            pointer(Pointer::Scroll(Direction::Up)),
            Some(("onScroll", vec![json!(0), json!(-1)], false))
        );
    }

    #[test]
    fn tree_maps_every_chip_field_and_drops_bad_icons_and_dots() {
        let tree = Tree::default()
            .apply(vec![Op::Insert {
            parent: 0,
            node: WireNode { id: 1, kind: "indicator".into(), props: serde_json::from_value(json!({
                "icon": "view-list-symbolic", "overlay": "emblem-symbolic", "dot": "#e01b24",
                "text": "Todo", "tooltip": "Tasks", "badge": "3", "severity": "warning",
                "attention": true, "notice": true, "className": "pill"
            })).expect("props"), children: vec![] },
            before: None,
        }])
            .expect("tree");
        let chips = indicators(&tree, &mut |name| gio::ThemedIcon::new(name).upcast());
        let chip = &chips[0].1;
        assert!(chip.icon.is_some());
        assert!(chip.overlay.is_some());
        assert!(chip.dot.is_some());
        assert_eq!(chip.label.as_deref(), Some("Todo"));
        assert_eq!(chip.tooltip.as_deref(), Some("Tasks"));
        assert_eq!(chip.badge.as_deref(), Some("3"));
        assert_eq!(chip.severity, Some(Severity::Warning));
        assert!(chip.attention && chip.notice);
        assert_eq!(chip.class.as_deref(), Some("pill"));
        let bad = Tree::default()
            .apply(vec![Op::Insert {
                parent: 0,
                node: WireNode {
                    id: 2,
                    kind: "indicator".into(),
                    props: serde_json::from_value(json!({"icon":"bad/icon","dot":"bad"}))
                        .expect("props"),
                    children: vec![],
                },
                before: None,
            }])
            .expect("tree");
        let bad = &indicators(&bad, &mut |name| gio::ThemedIcon::new(name).upcast())[0].1;
        assert!(bad.icon.is_none());
        assert!(bad.dot.is_none());
    }
}
