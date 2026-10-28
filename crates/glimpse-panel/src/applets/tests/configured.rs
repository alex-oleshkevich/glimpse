use super::*;

#[test]
fn every_name_in_the_default_zones_resolves_to_a_kind() {
    let panel = glimpse_config::Panel::default();
    for name in panel.left.iter().chain(&panel.center).chain(&panel.right) {
        assert!(
            glimpse_config::resolve_applet(name, &BTreeMap::new()).is_some(),
            "`{name}` ships in a default zone, so an untouched installation must resolve it"
        );
    }
}

#[test]
fn extends_names_the_kind_so_one_kind_can_have_several_instances() {
    let applets = custom("pulse", AppletKind::Heartbeat {}.into());
    assert!(
        glimpse_config::resolve_applet("pulse", &applets).is_some(),
        "`pulse` is not a kind; `extends` is what says which one it is"
    );
    assert!(
        glimpse_config::resolve_applet("pulse", &BTreeMap::new()).is_none(),
        "without the entry the same name is just unknown"
    );
}
