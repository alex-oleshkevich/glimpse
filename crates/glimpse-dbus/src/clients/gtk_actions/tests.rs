use super::*;

#[test]
fn application_names_become_object_paths() {
    assert_eq!(
        object_path("me.aresa.GlimpsePanel"),
        "/me/aresa/GlimpsePanel"
    );
    assert_eq!(object_path("a.b-c"), "/a/b_c");
}
