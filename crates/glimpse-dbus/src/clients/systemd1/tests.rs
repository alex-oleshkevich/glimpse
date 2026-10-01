use zbus::zvariant::{Type, Value};

fn signature<T: Type + ?Sized>() -> String {
    T::SIGNATURE.to_string()
}

#[test]
fn manager_and_scope_signatures_match_introspection() {
    let name = signature::<&str>();
    let properties = signature::<&[(&str, Value<'_>)]>();
    let aux = signature::<&[(&str, &[(&str, Value<'_>)])]>();
    let states = signature::<&[&str]>();
    assert_eq!(format!("{name}{name}{properties}{aux}"), "ssa(sv)a(sa(sv))");
    assert_eq!(signature::<zbus::zvariant::OwnedObjectPath>(), "o");
    assert_eq!(format!("{states}{states}"), "asas");
    assert_eq!(
        signature::<Vec<super::Systemd1UnitEntry>>(),
        "a(ssssssouso)"
    );
    assert_eq!(signature::<Vec<(String, u32, String)>>(), "a(sus)");
    assert_eq!(format!("{name}{name}{}", signature::<i32>()), "ssi");
    assert_eq!(signature::<u64>(), "t");
}
