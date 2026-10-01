#[test]
fn a_zero_poll_interval_is_refused() {
    for key in ["poll-active", "poll-idle"] {
        let document = format!("[printing]\n{key} = 0\n");
        toml::from_str::<crate::Config>(&document)
            .expect_err("a zero interval has no timer to tick on");
    }
}
