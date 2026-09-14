use std::process::ExitCode;

use zbus::DBusError as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    Ok = 0,
    Failed = 1,
    Unreachable = 3,
    Rejected = 4,
    Timeout = 5,
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        Self::from(exit as u8)
    }
}

pub fn exit(error: &anyhow::Error) -> Exit {
    if let Some(error) = error.downcast_ref::<zbus::fdo::Error>() {
        return named(error.name().as_str());
    }
    match error.downcast_ref::<zbus::Error>() {
        Some(zbus::Error::MethodError(name, _, _)) => named(name.as_str()),
        Some(zbus::Error::FDO(error)) => named(error.name().as_str()),
        Some(zbus::Error::InputOutput(_) | zbus::Error::Connection(_, _)) => Exit::Unreachable,
        _ => Exit::Failed,
    }
}

fn named(name: &str) -> Exit {
    match name {
        "org.freedesktop.DBus.Error.ServiceUnknown"
        | "org.freedesktop.DBus.Error.NameHasNoOwner"
        | "org.freedesktop.DBus.Error.Disconnected" => Exit::Unreachable,

        "org.freedesktop.DBus.Error.NoReply"
        | "org.freedesktop.DBus.Error.Timeout"
        | "org.freedesktop.DBus.Error.TimedOut" => Exit::Timeout,

        "me.aresa.Glimpse.NightLight1.Error.InvalidSchedule"
        | "me.aresa.Glimpse.Weather1.Error.InvalidPlace"
        | "me.aresa.Glimpse.Notifications1.Error.InvalidAction"
        | "org.freedesktop.DBus.Error.InvalidArgs"
        | "org.freedesktop.DBus.Error.UnknownMethod"
        | "org.freedesktop.DBus.Error.UnknownObject"
        | "org.freedesktop.DBus.Error.UnknownInterface" => Exit::Rejected,

        _ => Exit::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
