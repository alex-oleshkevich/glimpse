use super::*;

fn unit(name: &str, active_state: &str, before: &[&str]) -> BoundUnit {
    BoundUnit {
        name: name.to_owned(),
        active_state: active_state.to_owned(),
        before: before.iter().map(|target| (*target).to_owned()).collect(),
    }
}

#[test]
fn an_active_compositor_service_ordered_before_the_target_stops_it() {
    assert!(stops_graphical_session(&[unit(
        "niri.service",
        "active",
        &["graphical-session.target", "xdg-desktop-autostart.target"]
    )]));
}

#[test]
fn a_uwsm_compositor_unit_stops_the_target() {
    assert!(stops_graphical_session(&[unit(
        "wayland-wm@hyprland.desktop.service",
        "active",
        &["graphical-session.target"]
    )]));
}

#[test]
fn a_sidecar_service_alone_does_not_stop_the_target() {
    assert!(!stops_graphical_session(&[unit(
        "xwayland-satellite.service",
        "active",
        &[]
    )]));
}

#[test]
fn a_compositor_beside_a_sidecar_stops_the_target() {
    assert!(stops_graphical_session(&[
        unit("xwayland-satellite.service", "active", &[]),
        unit("niri.service", "active", &["graphical-session.target"]),
    ]));
}

#[test]
fn an_activating_compositor_does_not_stop_the_target() {
    assert!(!stops_graphical_session(&[unit(
        "niri.service",
        "activating",
        &["graphical-session.target"]
    )]));
}

#[test]
fn a_bound_target_does_not_stop_the_target() {
    assert!(!stops_graphical_session(&[unit(
        "hyprland-session.target",
        "active",
        &["graphical-session.target"]
    )]));
}

#[test]
fn nothing_bound_does_not_stop_the_target() {
    assert!(!stops_graphical_session(&[]));
}
