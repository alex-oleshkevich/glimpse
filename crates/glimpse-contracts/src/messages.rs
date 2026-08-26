use std::collections::BTreeMap;

use serde::{Serialize, de::DeserializeOwned};

use crate::types::*;

pub trait Message {
    const NAME: &'static str;
    type Payload: Serialize + DeserializeOwned + PartialEq + Send + 'static;
}

#[macro_export]
macro_rules! topic {
    ($ty:ty, $name:literal) => {
        impl $crate::Message for $ty {
            const NAME: &'static str = $name;
            type Payload = Self;
        }
    };
}

#[macro_export]
macro_rules! topics {
    ($(
        #[name = $name:literal]
        $(#[$meta:meta])*
        $vis:vis struct $ty:ident {
            $( $(#[$field_meta:meta])* $field:ident : $fty:ty ),* $(,)?
        }
    )*) => {
        $(
            $(#[$meta])*
            #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
            $vis struct $ty {
                $( $(#[$field_meta])* pub $field: $fty, )*
            }

            $crate::topic!($ty, $name);
        )*
    };
}

topics! {
    #[name = "system.topics"]
    pub struct SystemTopics { topics: BTreeMap<String, TopicReport> }

    #[name = "system.services"]
    pub struct SystemServices { services: BTreeMap<String, ServiceState> }

    #[name = "solar.status"]
    pub struct SolarStatus { phase: SolarPhase }

    #[name = "geolocation.status"]
    pub struct GeolocationStatus { coordinates: Option<GeoCoordinates> }
}
