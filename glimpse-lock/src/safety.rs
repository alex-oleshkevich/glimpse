pub fn no_new_privs_from_status(status: &str) -> Option<bool> {
    status.lines().find_map(|line| {
        let value = line.strip_prefix("NoNewPrivs:")?.trim();
        match value {
            "0" => Some(false),
            "1" => Some(true),
            _ => None,
        }
    })
}

pub fn current_no_new_privs() -> anyhow::Result<Option<bool>> {
    let status = std::fs::read_to_string("/proc/self/status")?;
    Ok(no_new_privs_from_status(&status))
}

#[cfg(test)]
mod tests {
    use super::no_new_privs_from_status;

    #[test]
    fn no_new_privs_parser_detects_disabled_state() {
        let status = "Name:\tglimpse-lock\nNoNewPrivs:\t0\n";

        assert_eq!(no_new_privs_from_status(status), Some(false));
    }

    #[test]
    fn no_new_privs_parser_detects_enabled_state() {
        let status = "Name:\tglimpse-lock\nNoNewPrivs:\t1\n";

        assert_eq!(no_new_privs_from_status(status), Some(true));
    }

    #[test]
    fn no_new_privs_parser_returns_none_when_missing() {
        assert_eq!(no_new_privs_from_status("Name:\tglimpse-lock\n"), None);
    }
}
