use super::*;

#[test]
fn slug_is_a_package_name() {
    assert_eq!(slug("me.example.HelloWorld_2"), "hello-world-2");
    assert_eq!(slug("me.example.123Thing"), "applet-123-thing");
    assert_eq!(slug("me.example._"), "applet");
}

#[test]
fn template_resolution_uses_the_templates_root_or_installed_templates() {
    assert_eq!(
        template_dir(AppletLanguage::Go, Some(PathBuf::from("/tmp/templates")),),
        PathBuf::from("/tmp/templates/go")
    );
    assert!(
        template_dir(AppletLanguage::Rs, Some(PathBuf::new())).ends_with("sdk/templates/rs")
    );
    assert!(template_dir(AppletLanguage::Py, None).ends_with("sdk/templates/py"));
}

#[test]
fn template_collection_is_recursive_sorted_and_rejects_symlinks() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("nested")).unwrap();
    fs::write(root.path().join("z.txt"), "z").unwrap();
    fs::write(root.path().join("nested/a.txt"), "a").unwrap();
    let mut files = Vec::new();
    collect_template_files(root.path(), root.path(), &mut files).unwrap();
    assert_eq!(
        files,
        [PathBuf::from("nested/a.txt"), PathBuf::from("z.txt")]
    );
    std::os::unix::fs::symlink("z.txt", root.path().join("link.txt")).unwrap();
    let mut files = Vec::new();
    assert!(collect_template_files(root.path(), root.path(), &mut files).is_err());
}
