use serde::{Serialize, de::DeserializeOwned};

use crate::types::*;

/// One invocable command. `Args` is the type itself, the way a topic's `Payload` is, so a command
/// is one named struct rather than a marker plus an argument type that can drift from it.
pub trait Command {
    const NAME: &'static str;
    type Args: Serialize + DeserializeOwned + Send + 'static;
    type Output: Serialize + DeserializeOwned + Send + 'static;
}

#[macro_export]
macro_rules! commands {
    ($(
        #[name = $name:literal]
        $(#[$meta:meta])*
        $vis:vis struct $ty:ident {
            $( $(#[$field_meta:meta])* $field:ident : $fty:ty ),* $(,)?
        } -> $output:ty;
    )*) => {
        $(
            $(#[$meta])*
            #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
            $vis struct $ty {
                $( $(#[$field_meta])* pub $field: $fty, )*
            }

            impl $crate::Command for $ty {
                const NAME: &'static str = $name;
                type Args = Self;
                type Output = $output;
            }
        )*

        /// Every command name the tree knows. See `ALL_TOPICS`.
        pub const ALL_COMMANDS: &[&str] = &[$($name),*];
    };
}

commands! {
    #[name = "heartbeat.reset"]
    pub struct HeartbeatReset {} -> ();

    #[name = "heartbeat.set_interval"]
    pub struct HeartbeatSetInterval { period_ms: u64 } -> HeartbeatInterval;

    #[name = "geolocation.refresh"]
    pub struct GeolocationRefresh {} -> ();

    #[name = "solar.refresh"]
    pub struct SolarRefresh {} -> ();
}
