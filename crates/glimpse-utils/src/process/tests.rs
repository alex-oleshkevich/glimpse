use super::{app_ids_of_cgroup, parse_stat};

#[test]
fn a_cgroup_yields_app_ids_most_specific_first() {
    let slice = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/";
    let cases: [(String, Vec<&str>); 10] = [
        (
            format!("{slice}app-com.mitchellh.ghostty.service"),
            vec!["com.mitchellh.ghostty"],
        ),
        (
            format!("{slice}app-com.google.Chrome-4525.scope"),
            vec!["com.google.Chrome"],
        ),
        (format!("{slice}app-slack-4533.scope"), vec!["slack"]),
        (
            format!("{slice}app-ghostty-surface-transient-82829.scope"),
            vec![
                "ghostty-surface-transient",
                "surface-transient",
                "transient",
            ],
        ),
        (
            format!("{slice}app-flatpak-org.mozilla.firefox-12345.scope"),
            vec!["flatpak-org.mozilla.firefox", "org.mozilla.firefox"],
        ),
        (
            format!("{slice}app-gnome-google\\x2dchrome-9876.scope"),
            vec!["gnome-google-chrome", "google-chrome"],
        ),
        (format!("{slice}app-foo@abc.service"), vec!["foo"]),
        (format!("{slice}app-evil\\x2f-path-1.scope"), vec!["path"]),
        ("0::/system.slice/packagekit.service".to_owned(), vec![]),
        ("1:name=systemd:/user.slice".to_owned(), vec![]),
    ];
    for (cgroup, expected) in cases {
        assert_eq!(app_ids_of_cgroup(&cgroup), expected, "{cgroup}");
    }
}

#[test]
fn a_stat_line_is_parsed_after_its_last_parenthesis() {
    let stat = parse_stat("123 (a) (b c) S 45 123 123 34816 1 2 3").expect("parses");
    assert_eq!(stat.parent, 45);
    assert!(stat.terminal);
    let stat = parse_stat("123 (a) (b c) S 45 123 123 0 1 2 3").expect("parses");
    assert!(!stat.terminal);
    assert!(parse_stat("garbage").is_none());
}
