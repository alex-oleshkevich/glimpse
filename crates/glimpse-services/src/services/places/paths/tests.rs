use super::*;

fn home() -> PathBuf {
    PathBuf::from("/home/alex")
}

#[test]
fn a_non_standard_key_outside_the_closed_glib_enum_is_still_read() {
    let text = "XDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_PROJECTS_DIR=\"$HOME/Projects\"\n";
    let entries = parse_user_dirs(text, &home());

    assert!(
        entries
            .iter()
            .any(|(key, path)| key == "projects" && path == &home().join("Projects")),
        "a key outside glib::UserDirectory's eight variants must still be read"
    );
}

#[test]
fn a_value_resolves_home_relative_absolute_and_backslash_escaped() {
    let text = "XDG_DOCUMENTS_DIR=\"$HOME/Documents\"\n\
                     XDG_DOWNLOAD_DIR=\"/mnt/downloads\"\n\
                     XDG_MUSIC_DIR=\"$HOME/Bob\\\"s Songs\"\n";
    let entries = parse_user_dirs(text, &home());

    assert_eq!(
        entries
            .iter()
            .find(|(key, _)| key == "documents")
            .map(|(_, path)| path.clone()),
        Some(home().join("Documents"))
    );
    assert_eq!(
        entries
            .iter()
            .find(|(key, _)| key == "download")
            .map(|(_, path)| path.clone()),
        Some(PathBuf::from("/mnt/downloads"))
    );
    assert_eq!(
        entries
            .iter()
            .find(|(key, _)| key == "music")
            .map(|(_, path)| path.clone()),
        Some(home().join("Bob\"s Songs")),
        "a backslash inside the quotes escapes the character that follows it"
    );
}

#[test]
fn a_comment_and_a_blank_line_are_skipped() {
    let text = "# a user comment\n\nXDG_DESKTOP_DIR=\"$HOME/Desktop\"\n";
    assert_eq!(parse_user_dirs(text, &home()).len(), 1);
}

#[test]
fn a_bookmark_line_is_split_into_a_uri_an_optional_label_and_a_decoded_target() {
    let entries = parse_bookmarks(
        "file:///home/alex/Downloads Downloads\n\
             file:///home/alex/projects\n\
             file:///home/alex/My%20Folder\n",
    );

    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].1.as_deref(), Some("Downloads"));
    assert_eq!(entries[0].2, PathBuf::from("/home/alex/Downloads"));
    assert_eq!(entries[1].1, None, "a line with no label carries none");
    assert_eq!(entries[1].2, PathBuf::from("/home/alex/projects"));
    assert_eq!(
        entries[2].2,
        PathBuf::from("/home/alex/My Folder"),
        "a percent-encoded uri must be decoded"
    );
}

#[test]
fn a_non_file_scheme_bookmark_is_not_read_as_a_place() {
    assert!(parse_bookmarks("sftp://example.test/path Remote\n").is_empty());
}
