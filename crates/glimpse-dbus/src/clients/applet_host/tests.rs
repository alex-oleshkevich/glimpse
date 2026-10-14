#[test]
fn panel_app_id_defaults_when_unset() {
    assert_eq!(super::panel_app_id_from(None), "me.aresa.GlimpsePanel");
    assert_eq!(
        super::panel_app_id_from(Some("test.Panel".to_owned())),
        "test.Panel"
    );
}

#[test]
fn attach_signature_matches_the_interface() {
    use zbus::zvariant::{Fd, Type};
    let env = <std::collections::HashMap<&str, &str>>::SIGNATURE;
    let line = format!(
        "{}{}{}{}{}{}{}",
        <&str>::SIGNATURE,
        u32::SIGNATURE,
        <&str>::SIGNATURE,
        <&str>::SIGNATURE,
        <&str>::SIGNATURE,
        env,
        Fd::SIGNATURE
    );
    assert_eq!(line, "susssa{ss}h");
}
