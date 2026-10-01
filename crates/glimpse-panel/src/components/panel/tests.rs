use super::*;

#[test]
fn attached_id_uses_the_ordinary_widget_path() {
    let regional = Regional::default();
    let zones = [
        (Zone::Start, &[][..]),
        (Zone::Center, &[][..]),
        (Zone::End, &[][..]),
    ];
    let attached = [(Zone::End, "me.x.A".to_owned())];
    let result = desired(zones, &BTreeMap::new(), &regional, &attached);
    assert_eq!(result.len(), 1);
    assert_eq!((result[0].0, result[0].1.as_str()), (Zone::End, "me.x.A"));
    assert_eq!(
        result[0].2,
        Some(std::mem::discriminant(
            &AppletConfig::from_name("me.x.A").unwrap().kind
        ))
    );
}

#[test]
fn configured_exec_takes_the_offer_without_a_second_widget() {
    let regional = Regional::default();
    let mut applets = BTreeMap::new();
    applets.insert(
        "custom".to_owned(),
        AppletConfig::from_name("me.x.A").unwrap(),
    );
    let left = ["custom".to_owned()];
    let zones = [
        (Zone::Start, left.as_slice()),
        (Zone::Center, &[][..]),
        (Zone::End, &[][..]),
    ];
    let result = desired(
        zones,
        &applets,
        &regional,
        &[(Zone::End, "me.x.A".to_owned())],
    );
    assert_eq!(result.len(), 1);
    assert_eq!((result[0].0, result[0].1.as_str()), (Zone::Start, "custom"));
}
