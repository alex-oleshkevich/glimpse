const DISPLAY_NAME_MAX_CHARS: usize = 64;

pub fn resolve(env_user: Option<&str>, uid: u32, passwd: Option<&str>) -> Result<String, String> {
    let name = passwd
        .and_then(|passwd| name_of(passwd, uid))
        .or_else(|| {
            env_user
                .filter(|user| !user.is_empty())
                .map(ToOwned::to_owned)
        })
        .ok_or_else(|| format!("no /etc/passwd entry for uid {uid} and no $USER"))?;
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
mod tests;
