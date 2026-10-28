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
