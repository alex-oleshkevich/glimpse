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

pub use glimpse_dbus::DEADLINE;

#[derive(Debug)]
pub struct TimedOut;

impl std::fmt::Display for TimedOut {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "the provider owns its name but did not answer within {}s",
            DEADLINE.as_secs()
        )
    }
}

impl std::error::Error for TimedOut {}

#[derive(Debug)]
pub struct Refused(pub String);

impl std::fmt::Display for Refused {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for Refused {}

#[derive(Debug)]
pub struct Gone;

impl std::fmt::Display for Gone {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("the panel lost its bus owner")
    }
}

impl std::error::Error for Gone {}

pub fn exit(error: &anyhow::Error) -> Exit {
    if error.downcast_ref::<Refused>().is_some() {
        return Exit::Rejected;
    }
    if error.downcast_ref::<Gone>().is_some() {
        return Exit::Unreachable;
    }
    if error.downcast_ref::<TimedOut>().is_some() {
        return Exit::Timeout;
    }
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

pub fn message(error: &anyhow::Error) -> String {
    let mut parts: Vec<String> = Vec::new();
    for link in error.chain() {
        let text = link.to_string();
        if parts
            .last()
            .is_some_and(|previous| previous.contains(&text))
        {
            continue;
        }
        parts.push(text);
    }
    parts.join(": ")
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
        | "me.aresa.Glimpse.NightLight1.Error.InvalidTemperature"
        | "me.aresa.Glimpse.Weather1.Error.InvalidPlace"
        | "me.aresa.Glimpse.Notifications1.Error.InvalidAction"
        | "me.aresa.Glimpse.Clipboard1.Error.NoSuchEntry"
        | "org.freedesktop.DBus.Error.InvalidArgs"
        | "org.freedesktop.DBus.Error.UnknownMethod"
        | "org.freedesktop.DBus.Error.UnknownObject"
        | "org.freedesktop.DBus.Error.UnknownInterface" => Exit::Rejected,

        _ => Exit::Failed,
    }
}
