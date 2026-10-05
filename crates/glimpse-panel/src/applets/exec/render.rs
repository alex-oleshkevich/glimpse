use crate::app::{AppInput, SessionDialog};
use crate::applet::{Button, Direction, Pointer};
use glimpse_services::{
    Align, ClassName, Element, Ellipsize, ExecBarRequest as BarRequest, IndicatorProps,
    SessionVerb, Tree,
};
use glimpse_services::{SessionAction, SessionActionsState};
use glimpse_widgets::{IndicatorSpec, Severity};
use gtk4::prelude::*;
use gtk4::{gdk, gio};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

const CHIP_TEXT_CAP: usize = 32;

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
        label: props
            .text
            .as_deref()
            .map(|text| glimpse_utils::clean(text, CHIP_TEXT_CAP)),
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
        ClassName::Column => "column",
        ClassName::NetworkPopoverRule => "network-popover__rule",
        ClassName::BluetoothPopover => "bluetooth-popover",
        ClassName::NetworkPopover => "network-popover",
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
mod tests;
