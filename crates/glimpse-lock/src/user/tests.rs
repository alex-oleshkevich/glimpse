use super::*;

const PASSWD: &str =
    "root:x:0:0::/root:/usr/bin/bash\nalex:x:1000:1000:Alex:/home/alex:/usr/bin/fish\n";

#[test]
fn user_comes_from_passwd_by_uid_first_then_the_environment() {
    assert_eq!(
        resolve(Some("root"), 1000, Some(PASSWD)),
        Ok("alex".into()),
        "$USER cannot point the prompt at another account"
    );
    assert_eq!(resolve(None, 1000, Some(PASSWD)), Ok("alex".into()));
    assert_eq!(
        resolve(Some("ldap-user"), 1001, Some(PASSWD)),
        Ok("ldap-user".into()),
        "an account passwd does not list, such as a directory user, falls back to $USER"
    );
    assert!(resolve(Some(""), 1001, Some(PASSWD)).is_err());
    assert!(resolve(None, 1000, None).is_err());
}

#[test]
fn the_display_name_is_the_real_name_else_the_user_name() {
    assert_eq!(
        display_name("Alex Doe", "alex").as_deref(),
        Some("Alex Doe")
    );
    assert_eq!(display_name("", "alex").as_deref(), Some("alex"));
    assert_eq!(display_name(" \t", "alex").as_deref(), Some("alex"));
    assert_eq!(display_name("", ""), None);
    assert_eq!(
        display_name("A\u{202e}lex\n", "alex").as_deref(),
        Some("A lex"),
        "a hostile character in RealName never reaches the label"
    );
}

#[test]
fn a_name_outside_the_portable_set_refuses() {
    for name in ["../x", "a/b", "-x", "", ".", "..", "al ex", "émile"] {
        let passwd = format!("{name}:x:1000:1000::/:/bin/sh\n");
        assert!(
            resolve(None, 1000, Some(&passwd)).is_err(),
            "{name:?} must be refused"
        );
    }
    assert!(resolve(Some("a/b"), 1000, None).is_err());
    assert_eq!(resolve(Some("a.b_c-1"), 1000, None), Ok("a.b_c-1".into()));
}
