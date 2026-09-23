const DISPLAY_NAME_MAX_CHARS: usize = 64;

pub fn resolve(env_user: Option<&str>, uid: u32, passwd: Option<&str>) -> Result<String, String> {
    let name = match env_user.filter(|user| !user.is_empty()) {
        Some(user) => user.to_owned(),
        None => passwd
            .and_then(|passwd| name_of(passwd, uid))
            .ok_or_else(|| format!("no $USER and no /etc/passwd entry for uid {uid}"))?,
    };
    if portable(&name) {
        Ok(name)
    } else {
        Err(format!(
            "username {:?} is outside the POSIX portable set",
            glimpse_utils::clean(&name, 64)
        ))
    }
}

pub fn current() -> Result<String, String> {
    let uid = glimpse_dbus::login1::current_uid().map_err(|error| error.to_string())?;
    let env_user = std::env::var("USER").ok();
    let passwd = std::fs::read_to_string("/etc/passwd").ok();
    resolve(env_user.as_deref(), uid, passwd.as_deref())
}

pub fn display_name(real_name: &str, user_name: &str) -> Option<String> {
    [real_name, user_name]
        .into_iter()
        .map(|name| glimpse_utils::clean(name, DISPLAY_NAME_MAX_CHARS))
        .find(|name| !name.is_empty())
}

fn name_of(passwd: &str, uid: u32) -> Option<String> {
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        let name = fields.next()?;
        let entry_uid = fields.nth(1)?.parse::<u32>().ok()?;
        (entry_uid == uid).then(|| name.to_owned())
    })
}

fn portable(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSWD: &str =
        "root:x:0:0::/root:/usr/bin/bash\nalex:x:1000:1000:Alex:/home/alex:/usr/bin/fish\n";

    #[test]
    fn user_comes_from_the_environment_first_then_passwd_by_uid() {
        assert_eq!(resolve(Some("alex"), 0, Some(PASSWD)), Ok("alex".into()));
        assert_eq!(resolve(None, 1000, Some(PASSWD)), Ok("alex".into()));
        assert_eq!(resolve(Some(""), 1000, Some(PASSWD)), Ok("alex".into()));
        assert!(resolve(None, 1001, Some(PASSWD)).is_err());
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
        assert!(resolve(Some("a/b"), 1000, Some(PASSWD)).is_err());
        assert_eq!(resolve(Some("a.b_c-1"), 1000, None), Ok("a.b_c-1".into()));
    }
}
