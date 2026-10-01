#[test]
fn panel_app_id_defaults_when_unset() {
    assert_eq!(super::panel_app_id_from(None), "me.aresa.GlimpsePanel");
    assert_eq!(
        super::panel_app_id_from(Some("test.Panel".to_owned())),
        "test.Panel"
    );
}
