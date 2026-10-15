use super::*;

const PASSWD: &str =
    "root:x:0:0::/root:/usr/bin/bash\nsam:x:4242:4242:Sam:/home/sam:/usr/bin/fish\n";

#[test]
fn user_comes_from_passwd_by_uid_first_then_the_environment() {
    assert_eq!(
        resolve(Some("root"), 4242, Some(PASSWD)),
        Ok("sam".into()),
        "$USER cannot point the prompt at another account"
    );
    assert_eq!(resolve(None, 4242, Some(PASSWD)), Ok("sam".into()));
    assert_eq!(
        resolve(Some("ldap-user"), 4243, Some(PASSWD)),
        Ok("ldap-user".into()),
        "an account passwd does not list, such as a directory user, falls back to $USER"
    );
    assert!(resolve(Some(""), 4243, Some(PASSWD)).is_err());
    assert!(resolve(None, 4242, None).is_err());
}

#[test]
fn the_display_name_is_the_real_name_else_the_user_name() {
    assert_eq!(display_name("Sam Doe", "sam").as_deref(), Some("Sam Doe"));
    assert_eq!(display_name("", "sam").as_deref(), Some("sam"));
    assert_eq!(display_name(" \t", "sam").as_deref(), Some("sam"));
    assert_eq!(display_name("", ""), None);
    assert_eq!(
        display_name("S\u{202e}am\n", "sam").as_deref(),
        Some("S am"),
        "a hostile character in RealName never reaches the label"
    );
}

#[test]
fn a_name_outside_the_portable_set_refuses() {
    for name in ["../x", "a/b", "-x", "", ".", "..", "sa m", "émile"] {
        let passwd = format!("{name}:x:4242:4242::/:/bin/sh\n");
        assert!(
            resolve(None, 4242, Some(&passwd)).is_err(),
            "{name:?} must be refused"
        );
    }
    assert!(resolve(Some("a/b"), 4242, None).is_err());
    assert_eq!(resolve(Some("a.b_c-1"), 4242, None), Ok("a.b_c-1".into()));
}
