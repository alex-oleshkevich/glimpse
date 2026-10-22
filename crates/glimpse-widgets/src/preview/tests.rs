#[test]
fn compiled_ui_is_named_for_this_process() {
    let path = super::compiled_ui();
    let expected = format!("glimpse-preview-{}.ui", std::process::id());
    assert_eq!(
        path.file_name().and_then(|name| name.to_str()),
        Some(expected.as_str())
    );
    assert_eq!(path.parent(), Some(std::env::temp_dir().as_path()));
}
