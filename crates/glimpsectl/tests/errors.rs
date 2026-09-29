use glimpsectl::errors::{DEADLINE, Exit, TimedOut, exit, message};

fn method_error(name: &'static str) -> anyhow::Error {
    anyhow::Error::new(zbus::Error::MethodError(
        name.try_into().expect("a valid error name"),
        None,
        zbus::message::Message::method_call("/", "Whatever")
            .expect("a builder")
            .destination("me.aresa.Test")
            .expect("a destination")
            .build(&())
            .expect("a message"),
    ))
}

#[test]
fn context_does_not_change_the_code_a_script_sees() {
    let error = method_error("org.freedesktop.DBus.Error.ServiceUnknown")
        .context("while reading the night light");
    assert_eq!(exit(&error), Exit::Unreachable);
}

#[test]
fn an_absent_provider_and_a_refused_argument_have_different_codes() {
    assert_eq!(
        exit(&method_error("org.freedesktop.DBus.Error.ServiceUnknown")),
        Exit::Unreachable
    );
    assert_eq!(
        exit(&method_error(
            "me.aresa.Glimpse.NightLight1.Error.InvalidSchedule"
        )),
        Exit::Rejected
    );
    assert_eq!(
        exit(&method_error(
            "me.aresa.Glimpse.NightLight1.Error.InvalidTemperature"
        )),
        Exit::Rejected
    );
    assert_eq!(
        exit(&method_error("org.freedesktop.DBus.Error.NoReply")),
        Exit::Timeout
    );
}

#[test]
fn an_absent_name_is_unreachable_however_zbus_wraps_it() {
    let wrapped = anyhow::Error::new(zbus::Error::FDO(Box::new(
        zbus::fdo::Error::ServiceUnknown("The name is not activatable".to_owned()),
    )));
    assert_eq!(exit(&wrapped), Exit::Unreachable);

    let bare = anyhow::Error::new(zbus::fdo::Error::ServiceUnknown("gone".to_owned()));
    assert_eq!(exit(&bare), Exit::Unreachable);
}

#[test]
fn an_unlisted_name_and_a_plain_error_are_both_one() {
    assert_eq!(
        exit(&method_error(
            "me.aresa.Glimpse.Weather1.Error.LimitExceeded"
        )),
        Exit::Failed
    );
    assert_eq!(exit(&anyhow::anyhow!("something else")), Exit::Failed);
}

#[test]
fn a_provider_that_never_answers_has_its_own_code() {
    let bare = anyhow::Error::new(TimedOut);
    assert_eq!(exit(&bare), Exit::Timeout);

    let wrapped = anyhow::Error::new(TimedOut).context("cannot read the night light");
    assert_eq!(
        exit(&wrapped),
        Exit::Timeout,
        "a script must see 5 however the call site describes the failure"
    );
}

#[test]
fn giving_up_says_how_long_it_waited() {
    let printed = message(&anyhow::Error::new(TimedOut));

    assert!(
        printed.contains(&DEADLINE.as_secs().to_string()),
        "{printed}"
    );
    assert!(printed.contains("owns its name"), "{printed}");
}

#[test]
fn a_wrapper_that_repeats_its_source_is_printed_once() {
    let wrapped = anyhow::Error::new(zbus::Error::FDO(Box::new(
        zbus::fdo::Error::ServiceUnknown("The name is not provided".to_owned()),
    )))
    .context("cannot read the night light");

    let printed = message(&wrapped);

    assert_eq!(printed.matches("The name is not provided").count(), 1);
    assert!(printed.starts_with("cannot read the night light: "));
}

#[test]
fn every_layer_that_says_something_new_is_kept() {
    let error = anyhow::anyhow!("the socket refused")
        .context("cannot reach the provider")
        .context("while reading the night light");

    assert_eq!(
        message(&error),
        "while reading the night light: cannot reach the provider: the socket refused"
    );
}
