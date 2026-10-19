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
    assert_eq!(signature::<u32>(), "u");
    assert_eq!(format!("{states}{states}"), "asas");
    assert_eq!(
        signature::<Vec<super::Systemd1UnitEntry>>(),
        "a(ssssssouso)"
    );
    assert_eq!(signature::<Vec<(String, u32, String)>>(), "a(sus)");
    assert_eq!(format!("{name}{name}{}", signature::<i32>()), "ssi");
    assert_eq!(signature::<u64>(), "t");
    assert_eq!(
        format!(
            "{}{}{name}{name}",
            signature::<u32>(),
            signature::<zbus::zvariant::OwnedObjectPath>()
        ),
        "uoss"
    );
}

#[test]
fn transient_service_and_signal_signatures_match_introspection() {
    use zbus::zvariant::{Fd, OwnedObjectPath};
    assert_eq!(
        format!(
            "{}{}{}{}",
            signature::<u32>(),
            signature::<OwnedObjectPath>(),
            signature::<String>(),
            signature::<String>()
        ),
        "uoss"
    );
    assert_eq!(
        format!(
            "{}{}",
            signature::<String>(),
            signature::<OwnedObjectPath>()
        ),
        "so"
    );
    assert_eq!(signature::<Vec<(&str, Vec<&str>, bool)>>(), "a(sasb)");
    assert_eq!(signature::<Fd<'_>>(), "h");
    assert_eq!(signature::<Vec<&str>>(), "as");
}

#[tokio::test]
async fn unit_by_pid_uses_the_systemd_acronym_and_wire_types() {
    struct Manager;
    #[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
    impl Manager {
        #[zbus(name = "GetUnitByPID")]
        fn get_unit_by_pid(&self, pid: u32) -> zbus::fdo::Result<zbus::zvariant::OwnedObjectPath> {
            assert_eq!(pid, 42);
            Ok("/org/freedesktop/systemd1/unit/test".try_into().unwrap())
        }
    }
    let bus = crate::testing::PrivateBus::start();
    let server = bus.connection().await;
    crate::own_name(&server, "org.freedesktop.systemd1")
        .await
        .unwrap();
    server
        .object_server()
        .at("/org/freedesktop/systemd1", Manager)
        .await
        .unwrap();
    let client = bus.connection().await;
    let proxy = super::Systemd1ManagerProxy::new(&client).await.unwrap();
    assert_eq!(
        proxy.get_unit_by_pid(42).await.unwrap().as_str(),
        "/org/freedesktop/systemd1/unit/test"
    );
}
