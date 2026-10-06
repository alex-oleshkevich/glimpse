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

#[test]
fn a_bare_renamed_applet_beside_its_new_name_is_dropped() {
    let regional = Regional::default();
    let right = ["display".to_owned(), "brightness".to_owned()];
    let zones = [
        (Zone::Start, &["brightness".to_owned()][..]),
        (Zone::Center, &[][..]),
        (Zone::End, &right[..]),
    ];
    let result = desired(zones, &BTreeMap::new(), &regional, &[]);
    let names: Vec<_> = result
        .iter()
        .map(|(zone, name, _)| (*zone, name.as_str()))
        .collect();
    assert_eq!(names, [(Zone::End, "display")]);

    for (old, new) in [("brightness", "display"), ("battery", "power")] {
        let left = [old.to_owned()];
        let right = [new.to_owned()];
        let zones = [
            (Zone::Start, &left[..]),
            (Zone::Center, &[][..]),
            (Zone::End, &right[..]),
        ];
        let result = desired(zones, &BTreeMap::new(), &regional, &[]);
        assert_eq!(result.len(), 1);
        assert_eq!((result[0].0, result[0].1.as_str()), (Zone::End, new));

        let applets = BTreeMap::from([(old.to_owned(), AppletConfig::from_name(old).unwrap())]);
        let result = desired(zones, &applets, &regional, &[]);
        assert_eq!(result.len(), 1);
        assert_eq!((result[0].0, result[0].1.as_str()), (Zone::Start, old));

        let both = BTreeMap::from([
            (old.to_owned(), AppletConfig::from_name(old).unwrap()),
            (new.to_owned(), AppletConfig::from_name(new).unwrap()),
        ]);
        assert_eq!(desired(zones, &both, &regional, &[]).len(), 2);
        let zones = [
            (Zone::Start, &[][..]),
            (Zone::Center, &[][..]),
            (Zone::End, &right[..]),
        ];
        assert_eq!(desired(zones, &applets, &regional, &[])[0].1, new);
    }
}
